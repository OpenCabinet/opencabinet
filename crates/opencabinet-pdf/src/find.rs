// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The text on a page, letter by letter with where each is, and finding
//! words in it.

use hayro::hayro_interpret::font::GlyphRun;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, RenderContent, SoftMask, interpret_page,
};

use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{Affine, BezPath, Point as KPoint, Rect};

use crate::{Document, Error, Point};

/// A letter on a page, in PDF space.
#[derive(Clone, Debug, PartialEq)]
pub struct Letter {
    /// What it says: usually one character, sometimes a ligature's few.
    pub text: String,
    /// Where its baseline starts.
    pub origin: Point,
    /// The font size, in points.
    pub size: f64,
}

/// Where a word was found: its page and its box in PDF space, left,
/// bottom, right, top.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// The page, from 0.
    pub page: usize,
    /// Its box.
    pub rect: [f64; 4],
}

#[derive(Default)]
struct Collector {
    letters: Vec<Letter>,
}

impl<'a> Device<'a> for Collector {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for glyph in run.glyphs() {
            let Some(text) = glyph.as_unicode() else {
                continue;
            };
            let text = match text {
                hayro::hayro_interpret::hayro_cmap::BfString::Char(c) => c.to_string(),
                hayro::hayro_interpret::hayro_cmap::BfString::String(s) => s,
            };
            let place = props.transform * glyph.transform();
            let origin = place * KPoint::ORIGIN;
            let [_, _, c, d, _, _] = place.as_coeffs();
            self.letters.push(Letter {
                text,
                origin: Point::new(origin.x, origin.y),
                // Glyph space is a thousandth of the font size.
                size: c.hypot(d) * 1000.0,
            });
        }
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

/// The letters on `page`, in the order the PDF shows them.
///
/// # Errors
/// When the page does not exist or the PDF can no longer be read.
pub fn letters(document: &Document, page: usize) -> Result<Vec<Letter>, Error> {
    let pdf = Pdf::new(document.shared_bytes()).map_err(|_| Error::Invalid)?;
    let pages = pdf.pages();
    let page = pages.get(page).ok_or(Error::Invalid)?;
    let settings = InterpreterSettings {
        render_content: RenderContent::PAGE,
        ..InterpreterSettings::default()
    };
    let cache = InterpreterCache::new();
    let mut context = Context::new(
        Affine::IDENTITY,
        Rect::new(-1e9, -1e9, 1e9, 1e9),
        &cache,
        page.xref(),
        settings,
    );
    let mut collector = Collector::default();
    interpret_page(page, &mut context, &mut collector);
    Ok(collector.letters)
}

/// Where `query` is on `page`, ignoring case and runs of spaces.
///
/// # Errors
/// When the page does not exist or the PDF can no longer be read.
pub fn find(document: &Document, page: usize, query: &str) -> Result<Vec<Found>, Error> {
    Ok(find_all(document, page, &[query], false)?
        .into_iter()
        .map(|(_, found)| found)
        .collect())
}

/// Where each of `queries` is on `page`, reading the page's text once;
/// with `whole`, only where no letter or digit is right before or after,
/// so `A-301` is not found in `A-3010`. Each place found comes with the
/// query it matches.
///
/// # Errors
/// When the page does not exist or the PDF can no longer be read.
pub fn find_all(
    document: &Document,
    page: usize,
    queries: &[&str],
    whole: bool,
) -> Result<Vec<(usize, Found)>, Error> {
    let queries: Vec<Vec<char>> = queries.iter().map(|q| squeeze(&q.to_lowercase())).collect();
    if queries.iter().all(Vec::is_empty) {
        return Ok(Vec::new());
    }
    let letters = letters(document, page)?;
    // The page's text, one char at a time, each with the letter it came
    // from; a space between letters far apart.
    let mut chars: Vec<(char, usize)> = Vec::new();
    for (index, letter) in letters.iter().enumerate() {
        if let Some(previous) = index.checked_sub(1).map(|i| &letters[i]) {
            let gap = letter.origin.distance(previous.origin);
            let line = (letter.origin.y - previous.origin.y).abs() > previous.size * 0.5;
            if line || gap > previous.size * 1.5 {
                chars.push((' ', index));
            }
        }
        for c in letter.text.to_lowercase().chars() {
            chars.push((c, index));
        }
    }
    let text: Vec<(char, usize)> = squeeze_indexed(&chars);
    let alone = |at: Option<&(char, usize)>| at.is_none_or(|&(c, _)| !c.is_alphanumeric());
    let mut found = Vec::new();
    for (which, query) in queries.iter().enumerate() {
        if query.is_empty() {
            continue;
        }
        let mut start = 0;
        while start + query.len() <= text.len() {
            let end = start + query.len();
            let matches = text[start..end]
                .iter()
                .zip(query)
                .all(|(&(c, _), q)| c == *q)
                && (!whole
                    || (alone(start.checked_sub(1).and_then(|i| text.get(i)))
                        && alone(text.get(end))));
            if matches {
                let first = text[start].1;
                let last = text[end - 1].1;
                found.push((
                    which,
                    Found {
                        page,
                        rect: bounds(&letters[first..=last]),
                    },
                ));
                start = end;
            } else {
                start += 1;
            }
        }
    }
    Ok(found)
}

fn squeeze(text: &str) -> Vec<char> {
    let mut out = Vec::new();
    for c in text.trim().chars() {
        if c.is_whitespace() {
            if out.last() != Some(&' ') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn squeeze_indexed(chars: &[(char, usize)]) -> Vec<(char, usize)> {
    let mut out: Vec<(char, usize)> = Vec::new();
    for &(c, index) in chars {
        if c.is_whitespace() {
            if out.last().is_some_and(|&(last, _)| last != ' ') {
                out.push((' ', index));
            }
        } else {
            out.push((c, index));
        }
    }
    out
}

/// The box around `letters`: from the first's baseline to a size above,
/// to past the last.
fn bounds(letters: &[Letter]) -> [f64; 4] {
    let mut rect = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for letter in letters {
        let (x, y, size) = (letter.origin.x, letter.origin.y, letter.size);
        rect[0] = rect[0].min(x);
        rect[1] = rect[1].min(y - size * 0.25);
        rect[2] = rect[2].max(x + size * 0.6);
        rect[3] = rect[3].max(y + size * 0.85);
    }
    rect
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample;

    #[test]
    fn the_sheet_number_is_read_where_it_is_written() {
        let document = Document::open(sample::sheets(2, 2592.0, 1728.0, 0)).unwrap();
        let letters = letters(&document, 1).unwrap();
        let text: String = letters.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(text, "A-202");
        // Written at 14 pt from x = width - 90, y = 30.
        assert!(
            letters[0].origin.distance(Point::new(2502.0, 30.0)) < 0.01,
            "{:?}",
            letters[0]
        );
        assert!((letters[0].size - 14.0).abs() < 0.01, "{}", letters[0].size);
        assert!(letters[1].origin.x > letters[0].origin.x);
    }

    #[test]
    fn a_word_is_found_whatever_its_case() {
        let document = Document::open(sample::sheets(2, 2592.0, 1728.0, 0)).unwrap();
        let found = find(&document, 1, "a-202").unwrap();
        assert_eq!(found.len(), 1);
        let [x0, y0, x1, y1] = found[0].rect;
        assert!(
            x0 <= 2502.0 && x1 > 2530.0 && y0 < 30.0 && y1 > 40.0,
            "{:?}",
            found[0]
        );
        assert_eq!(find(&document, 0, "a-202").unwrap().len(), 0);
        assert_eq!(find(&document, 0, "  ").unwrap().len(), 0);
        // Whole names only: "A-20" is in "A-202", but not as a name.
        assert_eq!(find_all(&document, 1, &["A-20"], false).unwrap().len(), 1);
        assert_eq!(
            find_all(&document, 1, &["A-20", "a-202"], true).unwrap()[0].0,
            1
        );
    }
}
