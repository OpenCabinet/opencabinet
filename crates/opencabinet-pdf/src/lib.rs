// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading PDF drawings for OpenCabinet Review (RFC-0004).
//!
//! - [`Document`] opens a PDF and knows its pages' sizes and how their own
//!   coordinates map to the page as shown.
//! - [`render`] draws part of a page into a [`Bitmap`], with hayro.
//! - [`annotations`] reads the markups a PDF holds and saves new ones as an
//!   incremental update, with lopdf: the original bytes are never changed,
//!   only appended to.
//!
//! Coordinates come in two spaces. **PDF space** is the page's own, in
//! points (1/72"), y up, as annotations store them. **Page space** is the
//! page as shown: points from its top left corner, y down, after the crop
//! box and the page's rotation. [`Page::to_page`] maps the first to the
//! second.

pub mod annotations;
mod render;
pub mod sample;
mod text;

use std::fmt;
use std::sync::Arc;

use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_syntax::Pdf;

pub use render::{Bitmap, View, render};

/// A point, in whichever space the context says.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// Across.
    pub x: f64,
    /// Up in PDF space, down in page space.
    pub y: f64,
}

impl Point {
    /// The point at `x`, `y`.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// The distance to `other`.
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

/// An affine transform, `[a b c d e f]` as PDF writes it: `x' = a x + c y +
/// e`, `y' = b x + d y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform(pub [f64; 6]);

#[allow(clippy::many_single_char_names, reason = "the names PDF gives them")]
impl Transform {
    /// Leaves points where they are.
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// Where `point` goes.
    #[must_use]
    pub fn apply(self, point: Point) -> Point {
        let [a, b, c, d, e, f] = self.0;
        Point::new(a * point.x + c * point.y + e, b * point.x + d * point.y + f)
    }

    /// The transform that undoes this one; `None` when it flattens the
    /// plane.
    #[must_use]
    pub fn invert(self) -> Option<Self> {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if det.abs() < f64::EPSILON {
            return None;
        }
        let (ia, ib, ic, id) = (d / det, -b / det, -c / det, a / det);
        Some(Self([
            ia,
            ib,
            ic,
            id,
            -(ia * e + ic * f),
            -(ib * e + id * f),
        ]))
    }
}

/// A page of a document.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Page {
    /// Width as shown, in points.
    pub width: f64,
    /// Height as shown, in points.
    pub height: f64,
    /// From PDF space to page space.
    pub to_page: Transform,
}

impl Page {
    /// From page space back to PDF space.
    #[must_use]
    pub fn to_pdf(&self) -> Transform {
        self.to_page.invert().unwrap_or(Transform::IDENTITY)
    }
}

/// Why a PDF could not be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// It is not a PDF, or too damaged to read.
    Invalid,
    /// It is encrypted; reading those is not supported yet.
    Encrypted,
    /// It has no pages.
    NoPages,
    /// Its markups could not be read or saved, with why.
    Annotations(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid => f.write_str("the file is not a PDF, or is too damaged to read"),
            Self::Encrypted => f.write_str("the PDF is protected with a password"),
            Self::NoPages => f.write_str("the PDF has no pages"),
            Self::Annotations(why) => write!(f, "the PDF's markups: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// An open PDF: its bytes and its pages.
#[derive(Clone, Debug)]
pub struct Document {
    bytes: Arc<Vec<u8>>,
    pages: Vec<Page>,
}

impl Document {
    /// Opens a PDF from its bytes.
    ///
    /// # Errors
    /// When it is not a readable PDF, is encrypted or has no pages.
    pub fn open(bytes: Vec<u8>) -> Result<Self, Error> {
        let bytes = Arc::new(bytes);
        let pdf = Pdf::new(Arc::clone(&bytes)).map_err(|e| match e {
            hayro::hayro_syntax::LoadPdfError::Decryption(_) => Error::Encrypted,
            hayro::hayro_syntax::LoadPdfError::Invalid => Error::Invalid,
        })?;
        let pages: Vec<Page> = pdf
            .pages()
            .iter()
            .map(|page| {
                let (width, height) = page.render_dimensions();
                let t = page.initial_transform(true).to_kurbo().as_coeffs();
                Page {
                    width: f64::from(width),
                    height: f64::from(height),
                    to_page: Transform(t),
                }
            })
            .collect();
        if pages.is_empty() {
            return Err(Error::NoPages);
        }
        Ok(Self { bytes, pages })
    }

    /// The file as read.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The pages, first to last.
    #[must_use]
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub(crate) fn shared_bytes(&self) -> Arc<Vec<u8>> {
        Arc::clone(&self.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transform_is_undone_by_its_inverse() {
        let t = Transform([0.0, 1.0, -1.0, 0.0, 30.0, 40.0]);
        let p = Point::new(3.0, 7.0);
        let back = t.invert().unwrap().apply(t.apply(p));
        assert!(back.distance(p) < 1e-9);
        assert_eq!(Transform([1.0, 0.0, 2.0, 0.0, 0.0, 0.0]).invert(), None);
    }

    #[test]
    fn what_is_not_a_pdf_is_refused() {
        assert_eq!(
            Document::open(b"hello".to_vec()).unwrap_err(),
            Error::Invalid
        );
    }
}
