// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! OpenCabinet Review's documents (RFC-0004): a PDF, the markups on it,
//! and each page's scale.
//!
//! A [`Review`] keeps the PDF's bytes as they were opened. The markups the
//! PDF already had become editable [`Markup`]s, and the page is drawn
//! without them, from a copy that leaves them out, so a markup moves
//! without leaving its old picture behind. Saving appends one incremental
//! update to the original bytes with every markup as a standard annotation
//! (see [`opencabinet_pdf::annotations`]).

mod markup;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use opencabinet_model::{Id, Length, Units};
use opencabinet_pdf::annotations::{self, Markups};
use opencabinet_pdf::{Document, Error, Point};

pub use markup::{Color, FONT_SIZE, Kind, Markup, Segment};

/// A page's scale: how long a real length is on the sheet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    /// Points on the sheet.
    pub points: f64,
    /// The real length they stand for.
    pub real: Length,
}

impl Scale {
    /// The scale that makes the line from `a` to `b` measure `real`;
    /// `None` when the points are the same or the length is not positive.
    #[must_use]
    pub fn from_line(a: Point, b: Point, real: Length) -> Option<Self> {
        let points = a.distance(b);
        (points > f64::EPSILON && real > Length::ZERO).then_some(Self { points, real })
    }

    /// The real length of the line from `a` to `b`.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "measured lengths are far inside both ranges"
    )]
    pub fn measure(&self, a: Point, b: Point) -> Length {
        Length::from_nm((a.distance(b) / self.points * self.real.nm() as f64).round() as i64)
    }
}

/// How a measured length is shown: to the nearest 1/16" or 0.1 mm.
#[must_use]
pub fn shown(length: Length, units: Units) -> String {
    let precision = match units {
        Units::Imperial => Length::from_inch_fraction(1, 16).unwrap_or(Length::from_mm(1)),
        Units::Metric => Length::from_nm(100_000),
    };
    length.show(units, precision).text
}

/// A PDF under review.
#[derive(Clone, Debug)]
pub struct Review {
    original: Document,
    shown: Document,
    markups: Vec<Markup>,
    scales: BTreeMap<usize, Scale>,
    modified: bool,
}

impl Review {
    /// Opens a PDF from its bytes.
    ///
    /// # Errors
    /// When it is not a PDF Review can read.
    pub fn open(bytes: Vec<u8>) -> Result<Self, Error> {
        let original = Document::open(bytes)?;
        let saved = annotations::read(original.bytes())?;
        let markups = saved
            .annotations
            .iter()
            .filter_map(Markup::from_annotation)
            .map(|mut m| {
                if m.id.is_empty() {
                    m.id = Id::new().to_string();
                }
                m
            })
            .collect();
        let scales = saved
            .pages
            .iter()
            .filter_map(|(&page, own)| {
                let points = own.get("ScalePoints")?.parse().ok()?;
                let real = Length::from_nm(own.get("ScaleNm")?.parse().ok()?);
                (points > 0.0 && real > Length::ZERO).then_some((page, Scale { points, real }))
            })
            .collect();
        // The page as drawn: the PDF without the markups Review edits.
        let shown = Document::open(annotations::save(original.bytes(), &Markups::default())?)?;
        Ok(Self {
            original,
            shown,
            markups,
            scales,
            modified: false,
        })
    }

    /// The PDF to draw pages from, without the editable markups.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.shown
    }

    /// The markups, in the order they were made.
    #[must_use]
    pub fn markups(&self) -> &[Markup] {
        &self.markups
    }

    /// The markup with `id`.
    #[must_use]
    pub fn markup(&self, id: &str) -> Option<&Markup> {
        self.markups.iter().find(|m| m.id == id)
    }

    /// Whether there are changes not saved.
    #[must_use]
    pub fn modified(&self) -> bool {
        self.modified
    }

    /// Adds a markup made by `author`, and returns its id.
    pub fn add(&mut self, mut markup: Markup, author: &str) -> String {
        markup.id = Id::new().to_string();
        author.clone_into(&mut markup.author);
        markup.modified = now();
        let id = markup.id.clone();
        self.markups.push(markup);
        self.modified = true;
        id
    }

    /// Removes the markup with `id`.
    pub fn remove(&mut self, id: &str) {
        let before = self.markups.len();
        self.markups.retain(|m| m.id != id);
        self.modified |= self.markups.len() != before;
    }

    /// Changes the markup with `id` through `change`.
    pub fn change(&mut self, id: &str, change: impl FnOnce(&mut Markup)) {
        if let Some(markup) = self.markups.iter_mut().find(|m| m.id == id) {
            let before = markup.clone();
            change(markup);
            if *markup != before {
                markup.modified = now();
                self.modified = true;
            }
        }
    }

    /// The topmost markup on `page` at `point`, within `tolerance` points.
    #[must_use]
    pub fn hit(&self, page: usize, point: Point, tolerance: f64) -> Option<&Markup> {
        self.markups
            .iter()
            .rev()
            .find(|m| m.page == page && m.hit(point, tolerance))
    }

    /// The scale of `page`, if it was calibrated.
    #[must_use]
    pub fn scale(&self, page: usize) -> Option<Scale> {
        self.scales.get(&page).copied()
    }

    /// Calibrates `page`.
    pub fn set_scale(&mut self, page: usize, scale: Scale) {
        if self.scales.insert(page, scale) != Some(scale) {
            self.modified = true;
        }
    }

    /// What a length markup measures, in `units`; `None` for other kinds
    /// or a page without a scale.
    #[must_use]
    pub fn measured(&self, markup: &Markup, units: Units) -> Option<String> {
        let [a, b] = markup.points[..] else {
            return None;
        };
        if markup.kind != Kind::Length {
            return None;
        }
        Some(match self.scale(markup.page) {
            Some(scale) => shown(scale.measure(a, b), units),
            None => "Not to scale".to_owned(),
        })
    }

    /// The PDF as opened, with an incremental update holding every markup
    /// and scale. Marks the review saved.
    ///
    /// # Errors
    /// When the PDF cannot be written.
    pub fn save(&mut self, units: Units) -> Result<Vec<u8>, Error> {
        let bytes = self.saved_bytes(units)?;
        self.modified = false;
        Ok(bytes)
    }

    fn saved_bytes(&self, units: Units) -> Result<Vec<u8>, Error> {
        let annotations = self
            .markups
            .iter()
            .map(|m| m.to_annotation(self.measured(m, units).as_deref()))
            .collect();
        let pages = self
            .scales
            .iter()
            .map(|(&page, scale)| {
                (
                    page,
                    BTreeMap::from([
                        ("ScalePoints".to_owned(), format!("{}", scale.points)),
                        ("ScaleNm".to_owned(), scale.real.nm().to_string()),
                    ]),
                )
            })
            .collect();
        annotations::save(self.original.bytes(), &Markups { annotations, pages })
    }

    /// The markup list for spreadsheets: one row per markup.
    #[must_use]
    pub fn csv(&self, units: Units) -> String {
        let mut out = String::from("Page,Type,Author,Modified,Text,Measurement\r\n");
        for markup in &self.markups {
            let fields = [
                (markup.page + 1).to_string(),
                markup.kind.label().to_owned(),
                markup.author.clone(),
                pdf_date_text(&markup.modified),
                markup.text.clone(),
                self.measured(markup, units).unwrap_or_default(),
            ];
            let row: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
            let _ = write!(out, "{}\r\n", row.join(","));
        }
        out
    }
}

/// Now, as a PDF date.
fn now() -> String {
    chrono::Utc::now().format("D:%Y%m%d%H%M%SZ").to_string()
}

/// A PDF date as `2026-11-21 15:30`, or as it was when it is not one.
#[must_use]
pub fn pdf_date_text(date: &str) -> String {
    let digits: String = date
        .trim_start_matches("D:")
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if digits.len() < 12 {
        return date.to_owned();
    }
    format!(
        "{}-{}-{} {}:{}",
        &digits[0..4],
        &digits[4..6],
        &digits[6..8],
        &digits[8..10],
        &digits[10..12]
    )
}

fn csv_field(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}
