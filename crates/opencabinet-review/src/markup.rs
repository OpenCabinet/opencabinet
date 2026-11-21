// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A markup, and how it becomes a standard PDF annotation and back.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use opencabinet_pdf::Point;
use opencabinet_pdf::annotations::{Annotation, FONT, Subtype, win_ansi_literal};

/// The kinds of markup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A rectangle, from corner to corner.
    Rectangle,
    /// An ellipse inside a rectangle.
    Ellipse,
    /// A cloud around a rectangle.
    Cloud,
    /// A straight line.
    Line,
    /// A line ending in an arrow head.
    Arrow,
    /// A freehand stroke.
    Pen,
    /// Text in a box.
    Text,
    /// A measured length, between two points, to the page's scale.
    Length,
}

impl Kind {
    /// Every kind, in the order the window offers them.
    pub const ALL: [Self; 8] = [
        Self::Cloud,
        Self::Rectangle,
        Self::Ellipse,
        Self::Arrow,
        Self::Line,
        Self::Pen,
        Self::Text,
        Self::Length,
    ];

    /// What the window and the markup list call it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Cloud => "Cloud",
            Self::Line => "Line",
            Self::Arrow => "Arrow",
            Self::Pen => "Pen",
            Self::Text => "Text",
            Self::Length => "Length",
        }
    }

    /// The name saved in the PDF, in Review's own dictionary.
    const fn key(self) -> &'static str {
        match self {
            Self::Rectangle => "rectangle",
            Self::Ellipse => "ellipse",
            Self::Cloud => "cloud",
            Self::Line => "line",
            Self::Arrow => "arrow",
            Self::Pen => "pen",
            Self::Text => "text",
            Self::Length => "length",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// Whether it is drawn from one corner to the other, rather than
    /// through its points.
    #[must_use]
    pub const fn boxed(self) -> bool {
        matches!(
            self,
            Self::Rectangle | Self::Ellipse | Self::Cloud | Self::Text
        )
    }
}

/// A color, RGB from 0 to 1.
pub type Color = [f64; 3];

/// One markup on a page, in PDF space.
#[derive(Clone, Debug, PartialEq)]
pub struct Markup {
    /// Unique in the document, kept across saves.
    pub id: String,
    /// The page, from 0.
    pub page: usize,
    /// What it is.
    pub kind: Kind,
    /// Two corners for boxed kinds, two ends for lines and lengths, every
    /// point for a pen stroke.
    pub points: Vec<Point>,
    /// Stroke color.
    pub color: Color,
    /// Stroke width, in points.
    pub width: f64,
    /// A comment, or what a text box shows.
    pub text: String,
    /// Who made it.
    pub author: String,
    /// When it last changed, as a PDF date.
    pub modified: String,
}

/// A text box's font size, in points.
pub const FONT_SIZE: f64 = 12.0;
/// How far a cloud's scallops reach past its rectangle, per point of
/// stroke width, and at least.
const SCALLOP: (f64, f64) = (4.0, 8.0);

impl Markup {
    /// The corners of its box: lowest x and y, then highest.
    #[must_use]
    pub fn bounds(&self) -> (Point, Point) {
        let mut low = Point::new(f64::INFINITY, f64::INFINITY);
        let mut high = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in &self.points {
            low = Point::new(low.x.min(p.x), low.y.min(p.y));
            high = Point::new(high.x.max(p.x), high.y.max(p.y));
        }
        (low, high)
    }

    /// How far a cloud's scallops reach past its rectangle.
    fn scallop(&self) -> f64 {
        (self.width * SCALLOP.0).max(SCALLOP.1)
    }

    /// Whether `point` is on it, within `tolerance` points.
    #[must_use]
    pub fn hit(&self, point: Point, tolerance: f64) -> bool {
        let (low, high) = self.bounds();
        let reach = tolerance + self.width / 2.0;
        if self.kind.boxed() {
            let reach = reach
                + if self.kind == Kind::Cloud {
                    self.scallop()
                } else {
                    0.0
                };
            return point.x >= low.x - reach
                && point.x <= high.x + reach
                && point.y >= low.y - reach
                && point.y <= high.y + reach;
        }
        self.points
            .windows(2)
            .any(|pair| segment_distance(point, pair[0], pair[1]) <= reach)
            || self.points.len() == 1 && self.points[0].distance(point) <= reach
    }

    /// Moves it by `dx`, `dy`.
    pub fn shift(&mut self, dx: f64, dy: f64) {
        for p in &mut self.points {
            p.x += dx;
            p.y += dy;
        }
    }

    /// The annotation that saves it; `label` is the text a length shows.
    #[must_use]
    pub fn to_annotation(&self, label: Option<&str>) -> Annotation {
        let (low, high) = self.bounds();
        let corners = vec![
            low,
            Point::new(high.x, low.y),
            high,
            Point::new(low.x, high.y),
        ];
        let (subtype, vertices) = match self.kind {
            Kind::Rectangle => (Subtype::Square, Vec::new()),
            Kind::Ellipse => (Subtype::Circle, Vec::new()),
            Kind::Cloud => (Subtype::Polygon, corners),
            Kind::Line | Kind::Arrow | Kind::Length => (Subtype::Line, self.points.clone()),
            Kind::Pen => (Subtype::Ink, self.points.clone()),
            Kind::Text => (Subtype::FreeText, Vec::new()),
        };
        let mut margin = self.width;
        match self.kind {
            Kind::Cloud => margin += self.scallop(),
            Kind::Arrow => margin += head_length(self.width),
            Kind::Length => margin += FONT_SIZE * 2.0,
            _ => {}
        }
        let contents = match (self.kind, label) {
            (Kind::Length, Some(label)) if self.text.is_empty() => label.to_owned(),
            (Kind::Length, Some(label)) => format!("{label}: {}", self.text),
            _ => self.text.clone(),
        };
        let mut own = BTreeMap::from([("Kind".to_owned(), self.kind.key().to_owned())]);
        if let Some(label) = label {
            own.insert("Value".into(), label.to_owned());
        }
        if self.kind == Kind::Length {
            own.insert("Text".into(), self.text.clone());
        }
        Annotation {
            page: self.page,
            subtype,
            rect: [
                low.x - margin,
                low.y - margin,
                high.x + margin,
                high.y + margin,
            ],
            vertices,
            color: self.color,
            width: self.width,
            contents,
            author: self.author.clone(),
            name: self.id.clone(),
            modified: self.modified.clone(),
            cloudy: self.kind == Kind::Cloud,
            arrow: self.kind == Kind::Arrow,
            font_size: FONT_SIZE,
            own,
            appearance: self.appearance(label),
        }
    }

    /// The markup an annotation saved; `None` for one Review cannot show.
    #[must_use]
    pub fn from_annotation(annotation: &Annotation) -> Option<Self> {
        let saved = annotation.own.get("Kind").and_then(|k| Kind::from_key(k));
        let kind = saved.unwrap_or(match annotation.subtype {
            Subtype::Square => Kind::Rectangle,
            Subtype::Circle => Kind::Ellipse,
            Subtype::Polygon if annotation.cloudy => Kind::Cloud,
            Subtype::Line if annotation.arrow => Kind::Arrow,
            Subtype::Line => Kind::Line,
            Subtype::PolyLine | Subtype::Polygon | Subtype::Ink => Kind::Pen,
            Subtype::FreeText => Kind::Text,
        });
        let [x0, y0, x1, y1] = annotation.rect;
        let points = if kind.boxed() {
            if annotation.vertices.len() >= 2 {
                // A cloud's rectangle, inside its scallops.
                let mut low = Point::new(f64::INFINITY, f64::INFINITY);
                let mut high = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
                for p in &annotation.vertices {
                    low = Point::new(low.x.min(p.x), low.y.min(p.y));
                    high = Point::new(high.x.max(p.x), high.y.max(p.y));
                }
                vec![low, high]
            } else if saved.is_some() {
                // Review's own: the box was widened by the stroke.
                let w = annotation.width;
                vec![Point::new(x0 + w, y0 + w), Point::new(x1 - w, y1 - w)]
            } else {
                vec![Point::new(x0, y0), Point::new(x1, y1)]
            }
        } else {
            annotation.vertices.clone()
        };
        if points.is_empty() {
            return None;
        }
        let text = if kind == Kind::Length {
            annotation.own.get("Text").cloned().unwrap_or_default()
        } else {
            annotation.contents.clone()
        };
        Some(Self {
            id: annotation.name.clone(),
            page: annotation.page,
            kind,
            points,
            color: annotation.color,
            width: annotation.width,
            text,
            author: annotation.author.clone(),
            modified: annotation.modified.clone(),
        })
    }

    /// How it looks, as PDF drawing operators in PDF space.
    #[allow(clippy::many_single_char_names, reason = "colors and points")]
    fn appearance(&self, label: Option<&str>) -> Vec<u8> {
        let [r, g, b] = self.color;
        let mut ops = format!(
            "{r:.3} {g:.3} {b:.3} RG {r:.3} {g:.3} {b:.3} rg {:.2} w 1 J 1 j\n",
            self.width
        );
        for segment in self.outline() {
            match segment {
                Segment::Move(p) => {
                    let _ = writeln!(ops, "{:.2} {:.2} m", p.x, p.y);
                }
                Segment::Line(p) => {
                    let _ = writeln!(ops, "{:.2} {:.2} l", p.x, p.y);
                }
                Segment::Curve(a, b, p) => {
                    let _ = writeln!(
                        ops,
                        "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c",
                        a.x, a.y, b.x, b.y, p.x, p.y
                    );
                }
                Segment::Close => ops.push_str("h\n"),
            }
        }
        ops.push_str("S\n");
        let mut bytes = ops.into_bytes();
        let mut text = |at: Point, line: &str| {
            bytes.extend_from_slice(
                format!("BT /{FONT} {FONT_SIZE:.1} Tf {:.2} {:.2} Td ", at.x, at.y).as_bytes(),
            );
            bytes.extend_from_slice(&win_ansi_literal(line));
            bytes.extend_from_slice(b" Tj ET\n");
        };
        match self.kind {
            Kind::Text => {
                let (low, high) = self.bounds();
                let mut y = high.y;
                for line in self.text.lines() {
                    y -= FONT_SIZE * 1.2;
                    if y < low.y {
                        break;
                    }
                    text(Point::new(low.x + 4.0, y + 2.0), line);
                }
            }
            Kind::Length => {
                if let (Some(label), [a, b]) = (label, &self.points[..]) {
                    let middle = Point::new(f64::midpoint(a.x, b.x), f64::midpoint(a.y, b.y));
                    text(Point::new(middle.x + 4.0, middle.y + 4.0), label);
                }
            }
            _ => {}
        }
        bytes
    }

    /// Its outline, in PDF space; the window draws the same.
    #[must_use]
    pub fn outline(&self) -> Vec<Segment> {
        let (low, high) = self.bounds();
        match self.kind {
            Kind::Rectangle | Kind::Text => rectangle(low, high),
            Kind::Ellipse => ellipse(low, high),
            Kind::Cloud => cloud(low, high, self.scallop()),
            Kind::Line | Kind::Pen => polyline(&self.points),
            Kind::Arrow => {
                let mut out = polyline(&self.points);
                if let [a, b] = self.points[..] {
                    out.extend(head(a, b, head_length(self.width)));
                }
                out
            }
            Kind::Length => {
                let mut out = polyline(&self.points);
                if let [a, b] = self.points[..] {
                    out.extend(tick(a, b, 6.0 + self.width));
                    out.extend(tick(b, a, 6.0 + self.width));
                }
                out
            }
        }
    }
}

/// A piece of an outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    /// Starts a new piece at the point.
    Move(Point),
    /// A straight line to the point.
    Line(Point),
    /// A cubic Bézier curve through two control points to the point.
    Curve(Point, Point, Point),
    /// Back to where the piece started.
    Close,
}

fn rectangle(low: Point, high: Point) -> Vec<Segment> {
    vec![
        Segment::Move(low),
        Segment::Line(Point::new(high.x, low.y)),
        Segment::Line(high),
        Segment::Line(Point::new(low.x, high.y)),
        Segment::Close,
    ]
}

fn ellipse(low: Point, high: Point) -> Vec<Segment> {
    // Four quarter arcs; 0.5523 puts the control points where a circle's
    // Bézier approximation needs them.
    const K: f64 = 0.552_284_75;
    let (cx, cy) = (f64::midpoint(low.x, high.x), f64::midpoint(low.y, high.y));
    let (rx, ry) = ((high.x - low.x) / 2.0, (high.y - low.y) / 2.0);
    let p = Point::new;
    vec![
        Segment::Move(p(cx + rx, cy)),
        Segment::Curve(
            p(cx + rx, cy + K * ry),
            p(cx + K * rx, cy + ry),
            p(cx, cy + ry),
        ),
        Segment::Curve(
            p(cx - K * rx, cy + ry),
            p(cx - rx, cy + K * ry),
            p(cx - rx, cy),
        ),
        Segment::Curve(
            p(cx - rx, cy - K * ry),
            p(cx - K * rx, cy - ry),
            p(cx, cy - ry),
        ),
        Segment::Curve(
            p(cx + K * rx, cy - ry),
            p(cx + rx, cy - K * ry),
            p(cx + rx, cy),
        ),
        Segment::Close,
    ]
}

/// Scallops along each side of the rectangle, bulging out by `reach`.
fn cloud(low: Point, high: Point, reach: f64) -> Vec<Segment> {
    let corners = [
        low,
        Point::new(high.x, low.y),
        high,
        Point::new(low.x, high.y),
    ];
    let mut out = vec![Segment::Move(low)];
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        let length = a.distance(b);
        if length < f64::EPSILON {
            continue;
        }
        let count = (length / (reach * 2.0)).round().max(1.0);
        let (ux, uy) = ((b.x - a.x) / length, (b.y - a.y) / length);
        // Outward, for corners listed counterclockwise.
        let (nx, ny) = (uy, -ux);
        let step = length / count;
        let mut k = 0.0;
        while k < count {
            let from = Point::new(a.x + ux * step * k, a.y + uy * step * k);
            let to = Point::new(from.x + ux * step, from.y + uy * step);
            let lift = reach * 4.0 / 3.0;
            out.push(Segment::Curve(
                Point::new(from.x + nx * lift, from.y + ny * lift),
                Point::new(to.x + nx * lift, to.y + ny * lift),
                to,
            ));
            k += 1.0;
        }
    }
    out.push(Segment::Close);
    out
}

fn polyline(points: &[Point]) -> Vec<Segment> {
    let mut out = Vec::with_capacity(points.len());
    if let Some((first, rest)) = points.split_first() {
        out.push(Segment::Move(*first));
        out.extend(rest.iter().map(|p| Segment::Line(*p)));
    }
    out
}

fn head_length(width: f64) -> f64 {
    10.0 + 3.0 * width
}

/// An open arrow head at `to`, coming from `from`.
fn head(from: Point, to: Point, length: f64) -> Vec<Segment> {
    let angle = (to.y - from.y).atan2(to.x - from.x);
    let side = |turn: f64| {
        let a = angle + std::f64::consts::PI - turn;
        Point::new(to.x + length * a.cos(), to.y + length * a.sin())
    };
    let spread = 25_f64.to_radians();
    vec![
        Segment::Move(side(spread)),
        Segment::Line(to),
        Segment::Line(side(-spread)),
    ]
}

/// A short line across the end `at` of the line to `other`.
fn tick(at: Point, other: Point, half: f64) -> Vec<Segment> {
    let length = at.distance(other);
    if length < f64::EPSILON {
        return Vec::new();
    }
    let (nx, ny) = (-(other.y - at.y) / length, (other.x - at.x) / length);
    vec![
        Segment::Move(Point::new(at.x + nx * half, at.y + ny * half)),
        Segment::Line(Point::new(at.x - nx * half, at.y - ny * half)),
    ]
}

/// The distance from `p` to the segment from `a` to `b`.
fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx * dx + dy * dy;
    if length < f64::EPSILON {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length).clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn markup(kind: Kind, points: &[(f64, f64)]) -> Markup {
        Markup {
            id: "m".into(),
            page: 0,
            kind,
            points: points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
            color: [1.0, 0.0, 0.0],
            width: 2.0,
            text: String::new(),
            author: "Andrés".into(),
            modified: String::new(),
        }
    }

    #[test]
    fn every_kind_comes_back_from_its_annotation() {
        for kind in Kind::ALL {
            let points: &[(f64, f64)] = if kind == Kind::Pen {
                &[(10.0, 10.0), (20.0, 30.0), (40.0, 35.0)]
            } else {
                &[(10.0, 10.0), (110.0, 60.0)]
            };
            let mut original = markup(kind, points);
            original.text = "Check".into();
            let label = (kind == Kind::Length).then_some("12\"");
            let annotation = original.to_annotation(label);
            let back = Markup::from_annotation(&annotation).unwrap();
            assert_eq!(back, original, "{kind:?}");
        }
    }

    #[test]
    fn markups_from_other_programs_are_read_by_their_type() {
        let mut annotation = markup(Kind::Arrow, &[(0.0, 0.0), (50.0, 0.0)]).to_annotation(None);
        annotation.own.clear();
        assert_eq!(
            Markup::from_annotation(&annotation).unwrap().kind,
            Kind::Arrow
        );
        annotation.arrow = false;
        assert_eq!(
            Markup::from_annotation(&annotation).unwrap().kind,
            Kind::Line
        );
    }

    #[test]
    fn a_click_near_a_line_hits_it_and_one_away_does_not() {
        let line = markup(Kind::Line, &[(0.0, 0.0), (100.0, 0.0)]);
        assert!(line.hit(Point::new(50.0, 3.0), 3.0));
        assert!(!line.hit(Point::new(50.0, 10.0), 3.0));
        let cloud = markup(Kind::Cloud, &[(0.0, 0.0), (100.0, 50.0)]);
        // Inside the scallops, 8 points past the rectangle.
        assert!(cloud.hit(Point::new(-9.0, 20.0), 1.0));
    }

    #[test]
    fn a_cloud_has_whole_scallops_on_every_side() {
        let cloud = markup(Kind::Cloud, &[(0.0, 0.0), (160.0, 80.0)]);
        let curves = cloud
            .outline()
            .iter()
            .filter(|s| matches!(s, Segment::Curve(..)))
            .count();
        // Scallops reach 8: 16 wide, so 10 + 5 + 10 + 5.
        assert_eq!(curves, 30);
        let annotation = cloud.to_annotation(None);
        assert_eq!(annotation.rect, [-10.0, -10.0, 170.0, 90.0]);
    }
}
