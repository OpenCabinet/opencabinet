// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The points of a drawing a markup can snap to: the ends and middles of
//! its lines, where two lines cross, and the nearest point on a line.
//!
//! [`Snaps::of`] reads a page's vector drawing once (its text and images
//! are left out) and files every line in a grid, so finding what is near
//! the mouse looks only at the few cells around it, even on a sheet with
//! hundreds of thousands of lines.

use std::collections::{BTreeMap, BTreeSet};

use hayro::hayro_interpret::font::GlyphRun;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, RenderContent, SoftMask, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{Affine, BezPath, PathEl, Point as KPoint, Rect};

use crate::{Document, Error, Point};

/// The side of a grid cell, in points.
const CELL: f64 = 24.0;
/// The most lines read from a page; past that, the rest are left out.
const MOST: usize = 400_000;
/// The most cells one line is filed in; longer lines go in a list of
/// their own, looked at every time.
const SPAN: i64 = 512;
/// Lines shorter than this, in points, are left out: they are the dots
/// and dashes of patterns, not edges.
const SHORTEST: f64 = 0.5;

/// What a snapped point is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Snap {
    /// The end of a line, or a corner.
    End,
    /// Where two lines cross.
    Intersection,
    /// The middle of a straight line.
    Middle,
    /// The point of a line nearest the mouse.
    OnLine,
}

impl Snap {
    /// What the window calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::End => "Endpoint",
            Self::Intersection => "Intersection",
            Self::Middle => "Midpoint",
            Self::OnLine => "On line",
        }
    }
}

/// A point snapped to, and what it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapped {
    /// Where, in PDF space.
    pub at: Point,
    /// What.
    pub kind: Snap,
}

type Cell = (i64, i64);

/// The lines of one page, filed for finding what is near a point.
#[derive(Clone, Debug, Default)]
pub struct Snaps {
    /// Every straight line, and the chord of every curve.
    lines: Vec<(Point, Point, bool)>,
    /// The lines in each cell.
    cells: BTreeMap<Cell, Vec<u32>>,
    /// Lines too long to file cell by cell.
    long: Vec<u32>,
}

#[derive(Default)]
struct Collector {
    lines: Vec<(Point, Point, bool)>,
}

impl Collector {
    fn add(&mut self, a: KPoint, b: KPoint, straight: bool) {
        if self.lines.len() < MOST && a.distance(b) >= SHORTEST {
            self.lines
                .push((Point::new(a.x, a.y), Point::new(b.x, b.y), straight));
        }
    }
}

impl<'a> Device<'a> for Collector {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, _: &DrawMode) {
        let place = props.transform;
        let mut start = KPoint::ORIGIN;
        let mut last = KPoint::ORIGIN;
        for element in path.elements() {
            match *element {
                PathEl::MoveTo(p) => {
                    start = place * p;
                    last = start;
                }
                PathEl::LineTo(p) => {
                    let p = place * p;
                    self.add(last, p, true);
                    last = p;
                }
                PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => {
                    let p = place * p;
                    self.add(last, p, false);
                    last = p;
                }
                PathEl::ClosePath => {
                    self.add(last, start, true);
                    last = start;
                }
            }
        }
    }
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, _: &GlyphRun<'_, 'a>, _: DrawProps<'a>, _: &DrawMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "cells of a page in points are far inside i64"
)]
fn cell_of(p: Point) -> Cell {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

/// Where segment `first` and segment `second` cross, if they do.
fn crossing(first: (Point, Point), second: (Point, Point)) -> Option<Point> {
    let (start, end) = first;
    let (other, other_end) = second;
    let (rx, ry) = (end.x - start.x, end.y - start.y);
    let (sx, sy) = (other_end.x - other.x, other_end.y - other.y);
    let denominator = rx * sy - ry * sx;
    if denominator.abs() < 1e-12 {
        return None;
    }
    let (qx, qy) = (other.x - start.x, other.y - start.y);
    let along_first = (qx * sy - qy * sx) / denominator;
    let along_second = (qx * ry - qy * rx) / denominator;
    let inside = -1e-9..=1.0 + 1e-9;
    (inside.contains(&along_first) && inside.contains(&along_second))
        .then(|| Point::new(start.x + along_first * rx, start.y + along_first * ry))
}

/// The point of segment `a`–`b` nearest `p`.
fn nearest_on(p: Point, a: Point, b: Point) -> Point {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx * dx + dy * dy;
    if length < 1e-12 {
        return a;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length).clamp(0.0, 1.0);
    Point::new(a.x + t * dx, a.y + t * dy)
}

impl Snaps {
    /// The lines `page` of `document` draws.
    ///
    /// # Errors
    /// When the page does not exist or the PDF can no longer be read.
    pub fn of(document: &Document, page: usize) -> Result<Self, Error> {
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
        Ok(Self::from_lines(collector.lines))
    }

    /// Snaps for these lines: two ends, and whether the line is straight
    /// (a curve's middle is not a snap).
    #[must_use]
    pub fn from_lines(lines: Vec<(Point, Point, bool)>) -> Self {
        let mut cells: BTreeMap<Cell, Vec<u32>> = BTreeMap::new();
        let mut long = Vec::new();
        for (index, &(a, b, _)) in lines.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                break;
            };
            let (ca, cb) = (cell_of(a), cell_of(b));
            let (x0, x1) = (ca.0.min(cb.0), ca.0.max(cb.0));
            let (y0, y1) = (ca.1.min(cb.1), ca.1.max(cb.1));
            if (x1 - x0 + 1) * (y1 - y0 + 1) > SPAN {
                long.push(index);
                continue;
            }
            for x in x0..=x1 {
                for y in y0..=y1 {
                    // Only the cells the line goes through.
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "cells of a page are small numbers"
                    )]
                    let low = Point::new(x as f64 * CELL, y as f64 * CELL);
                    let high = Point::new(low.x + CELL, low.y + CELL);
                    if passes(a, b, low, high) {
                        cells.entry((x, y)).or_default().push(index);
                    }
                }
            }
        }
        Self { lines, cells, long }
    }

    /// How many lines it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether it holds no lines: a scan, or an empty page.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The lines within `reach` of `p`.
    fn near(&self, p: Point, reach: f64) -> Vec<u32> {
        let low = cell_of(Point::new(p.x - reach, p.y - reach));
        let high = cell_of(Point::new(p.x + reach, p.y + reach));
        let mut found: BTreeSet<u32> = self.long.iter().copied().collect();
        for x in low.0..=high.0 {
            for y in low.1..=high.1 {
                if let Some(lines) = self.cells.get(&(x, y)) {
                    found.extend(lines);
                }
            }
        }
        found
            .into_iter()
            .filter(|&i| {
                let (a, b, _) = self.lines[i as usize];
                nearest_on(p, a, b).distance(p) <= reach
            })
            .collect()
    }

    /// The best point to snap to within `reach` points of `p`: an end or
    /// a crossing before a middle, and a middle before a point on a line;
    /// the nearest among equals.
    #[must_use]
    pub fn nearest(&self, mouse: Point, reach: f64) -> Option<Snapped> {
        let near = self.near(mouse, reach);
        let mut best: Option<(Snap, f64, Point)> = None;
        let mut offer = |kind: Snap, at: Point| {
            let distance = at.distance(mouse);
            if distance > reach {
                return;
            }
            // Ends and crossings rank alike: whichever is nearer.
            let rank = |k: Snap| match k {
                Snap::End | Snap::Intersection => 0,
                Snap::Middle => 1,
                Snap::OnLine => 2,
            };
            let better = best.is_none_or(|(k, d, _)| (rank(kind), distance) < (rank(k), d));
            if better {
                best = Some((kind, distance, at));
            }
        };
        for &i in &near {
            let (a, b, straight) = self.lines[i as usize];
            offer(Snap::End, a);
            offer(Snap::End, b);
            if straight {
                offer(
                    Snap::Middle,
                    Point::new(f64::midpoint(a.x, b.x), f64::midpoint(a.y, b.y)),
                );
            }
            offer(Snap::OnLine, nearest_on(mouse, a, b));
        }
        // Crossings among the few lines near the mouse.
        for (n, &i) in near.iter().enumerate().take(64) {
            let (first, first_end, _) = self.lines[i as usize];
            for &j in near.iter().skip(n + 1).take(64) {
                let (second, second_end, _) = self.lines[j as usize];
                if let Some(at) = crossing((first, first_end), (second, second_end)) {
                    // Two lines meeting end to end are an end, not a
                    // crossing.
                    let at_end = [first, first_end, second, second_end]
                        .iter()
                        .any(|e| e.distance(at) < 1e-6);
                    if !at_end {
                        offer(Snap::Intersection, at);
                    }
                }
            }
        }
        best.map(|(kind, _, at)| Snapped { at, kind })
    }
}

/// Whether segment `a`–`b` goes through the box from `low` to `high`.
fn passes(a: Point, b: Point, low: Point, high: Point) -> bool {
    let inside = |p: Point| p.x >= low.x && p.x <= high.x && p.y >= low.y && p.y <= high.y;
    if inside(a) || inside(b) {
        return true;
    }
    let corners = [
        low,
        Point::new(high.x, low.y),
        high,
        Point::new(low.x, high.y),
    ];
    (0..4).any(|i| crossing((a, b), (corners[i], corners[(i + 1) % 4])).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snaps() -> Snaps {
        // A square 100 points across, and a diagonal through it.
        Snaps::from_lines(vec![
            (Point::new(0.0, 0.0), Point::new(100.0, 0.0), true),
            (Point::new(100.0, 0.0), Point::new(100.0, 100.0), true),
            (Point::new(100.0, 100.0), Point::new(0.0, 100.0), true),
            (Point::new(0.0, 100.0), Point::new(0.0, 0.0), true),
            (Point::new(-50.0, 50.0), Point::new(150.0, 50.0), true),
        ])
    }

    #[test]
    fn ends_middles_crossings_and_lines_are_found_in_that_order() {
        let snaps = snaps();
        let at = |x, y| snaps.nearest(Point::new(x, y), 6.0);
        assert_eq!(
            at(2.0, 3.0),
            Some(Snapped {
                at: Point::new(0.0, 0.0),
                kind: Snap::End
            })
        );
        assert_eq!(at(51.0, 2.0).unwrap().kind, Snap::Middle);
        let crossing = at(98.0, 52.0).unwrap();
        assert_eq!(crossing.kind, Snap::Intersection);
        assert!(crossing.at.distance(Point::new(100.0, 50.0)) < 1e-9);
        let on = at(30.0, 2.0).unwrap();
        assert_eq!(on.kind, Snap::OnLine);
        assert_eq!(on.at, Point::new(30.0, 0.0));
        assert_eq!(at(30.0, 30.0), None);
    }

    #[test]
    fn a_long_line_is_still_found() {
        let snaps = Snaps::from_lines(vec![(
            Point::new(0.0, 0.0),
            Point::new(100_000.0, 100_000.0),
            true,
        )]);
        assert_eq!(
            snaps.nearest(Point::new(5000.0, 5003.0), 6.0).unwrap().kind,
            Snap::OnLine
        );
    }
}
