// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drawing part of a page with hayro.

use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{InterpreterSettings, RenderContent};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{Affine, Rect};
use hayro::vello_cpu::color::AlphaColor;
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{RenderCache, RenderSettings, render_into};

use crate::{Document, Error, Point};

/// The gray around a page.
const DESK: [u8; 4] = [0x80, 0x80, 0x80, 0xff];

/// The largest side of a bitmap hayro draws into.
const MAX_SIDE: u32 = u16::MAX as u32;

/// Which part of a page to draw, and how large.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The page, from 0.
    pub page: usize,
    /// Pixels per point: 1.0 shows the page at 72 dpi.
    pub zoom: f64,
    /// Where the bitmap's top left corner falls, in page space pixels: the
    /// page's own top left corner, times `zoom`, is at (0, 0).
    pub origin: Point,
    /// The bitmap's width, in pixels.
    pub width: u32,
    /// The bitmap's height, in pixels.
    pub height: u32,
    /// Quarter turns clockwise the page is seen turned, without changing
    /// the file (see [`crate::Page::turned`]).
    pub turn: u8,
}

/// An image: rows of RGBA pixels, 4 bytes each, top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 4` bytes.
    pub pixels: Vec<u8>,
}

impl Bitmap {
    /// The pixel at `x`, `y`, as RGBA.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[at],
            self.pixels[at + 1],
            self.pixels[at + 2],
            self.pixels[at + 3],
        ]
    }
}

/// Draws `view` of `document`: the page white, its content and its
/// annotations on it, and gray around it.
///
/// # Errors
/// When the page does not exist, the view is empty or too large, or the
/// document can no longer be read.
pub fn render(document: &Document, view: View) -> Result<Bitmap, Error> {
    if view.width == 0
        || view.height == 0
        || view.width > MAX_SIDE
        || view.height > MAX_SIDE
        || !(view.zoom.is_finite() && view.zoom > 0.0)
    {
        return Err(Error::Invalid);
    }
    let unturned = *document.pages().get(view.page).ok_or(Error::Invalid)?;
    let page_size = unturned.turned(view.turn);
    // From the page as the PDF shows it to the page as seen.
    let turn = unturned.to_pdf().then(page_size.to_page).0;
    let pdf = Pdf::new(document.shared_bytes()).map_err(|_| Error::Invalid)?;
    let pages = pdf.pages();
    let page = pages.get(view.page).ok_or(Error::Invalid)?;
    // Checked above: both sides fit in u16.
    let (width, height) = (
        u16::try_from(view.width).map_err(|_| Error::Invalid)?,
        u16::try_from(view.height).map_err(|_| Error::Invalid)?,
    );
    let mut ctx = RenderContext::new(width, height);
    let place = Affine::translate((-view.origin.x, -view.origin.y)) * Affine::scale(view.zoom);
    ctx.set_transform(place);
    ctx.set_paint(AlphaColor::from_rgba8(0xff, 0xff, 0xff, 0xff));
    ctx.fill_rect(&Rect::new(0.0, 0.0, page_size.width, page_size.height));
    let settings = InterpreterSettings {
        render_content: RenderContent::all(),
        ..InterpreterSettings::default()
    };
    let cache = RenderCache::new();
    render_into(
        page,
        &cache,
        &settings,
        &RenderSettings::default(),
        &mut ctx,
        place * Affine::new(turn) * page.initial_transform(true).to_kurbo(),
    );
    ctx.flush();
    let mut pixmap = Pixmap::new(width, height);
    let [r, g, b, a] = DESK;
    ctx.render_with(
        &mut pixmap,
        &mut Resources::default(),
        RasterizerSettings {
            target_init: TargetInit::Clear(AlphaColor::from_rgba8(r, g, b, a)),
            ..Default::default()
        },
    );
    let pixels = pixmap
        .data()
        .iter()
        .flat_map(|p| [p.r, p.g, p.b, p.a])
        .collect();
    Ok(Bitmap {
        width: view.width,
        height: view.height,
        pixels,
    })
}
