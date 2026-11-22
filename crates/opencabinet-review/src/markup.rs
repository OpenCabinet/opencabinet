// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A markup, how it looks, and how it becomes a standard PDF annotation and
//! back.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use opencabinet_pdf::Point;
use opencabinet_pdf::annotations::{
    Annotation, Change, FONT, Measure, ObjectRef, STATE, Subtype, win_ansi_literal,
};

use crate::scale::Scale;

/// The kinds of markup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A cloud around a shape.
    Cloud,
    /// A rectangle, from corner to corner.
    Rectangle,
    /// An ellipse inside a rectangle.
    Ellipse,
    /// A closed shape, point by point.
    Polygon,
    /// A line ending in an arrow head.
    Arrow,
    /// A straight line.
    Line,
    /// Connected lines, point by point.
    Polyline,
    /// A freehand stroke.
    Pen,
    /// A see-through colored rectangle over what matters.
    Highlight,
    /// Text in a box.
    Text,
    /// Text in a box with a leader to what it is about.
    Callout,
    /// A note shown as an icon, its text in the markup list.
    Note,
    /// A stamp: a few words, who stamped and when.
    Stamp,
    /// A measured length between two points.
    Length,
    /// A measured length along a path.
    Path,
    /// A measured area.
    Area,
    /// A count of points.
    Count,
}

/// How a markup is drawn with the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    /// Dragged from one corner or end to the other.
    Drag,
    /// Clicked point by point, then finished.
    Clicks,
    /// One click.
    Click,
}

impl Kind {
    /// Every kind, in the order the window offers them.
    pub const ALL: [Self; 17] = [
        Self::Cloud,
        Self::Rectangle,
        Self::Ellipse,
        Self::Polygon,
        Self::Arrow,
        Self::Line,
        Self::Polyline,
        Self::Pen,
        Self::Highlight,
        Self::Text,
        Self::Callout,
        Self::Note,
        Self::Stamp,
        Self::Length,
        Self::Path,
        Self::Area,
        Self::Count,
    ];

    /// What the window and the markup list call it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Cloud => "Cloud",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Polygon => "Polygon",
            Self::Arrow => "Arrow",
            Self::Line => "Line",
            Self::Polyline => "Polyline",
            Self::Pen => "Pen",
            Self::Highlight => "Highlight",
            Self::Text => "Text",
            Self::Callout => "Callout",
            Self::Note => "Note",
            Self::Stamp => "Stamp",
            Self::Length => "Length",
            Self::Path => "Path length",
            Self::Area => "Area",
            Self::Count => "Count",
        }
    }

    /// The name saved in the PDF, in Review's own dictionary.
    const fn key(self) -> &'static str {
        match self {
            Self::Cloud => "cloud",
            Self::Rectangle => "rectangle",
            Self::Ellipse => "ellipse",
            Self::Polygon => "polygon",
            Self::Arrow => "arrow",
            Self::Line => "line",
            Self::Polyline => "polyline",
            Self::Pen => "pen",
            Self::Highlight => "highlight",
            Self::Text => "text",
            Self::Callout => "callout",
            Self::Note => "note",
            Self::Stamp => "stamp",
            Self::Length => "length",
            Self::Path => "path",
            Self::Area => "area",
            Self::Count => "count",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// How it is drawn with the mouse.
    #[must_use]
    pub const fn gesture(self) -> Gesture {
        match self {
            Self::Polygon | Self::Polyline | Self::Path | Self::Area | Self::Count => {
                Gesture::Clicks
            }
            Self::Note => Gesture::Click,
            _ => Gesture::Drag,
        }
    }

    /// Whether its two points are the corners of its box.
    #[must_use]
    pub const fn boxed(self) -> bool {
        matches!(
            self,
            Self::Rectangle | Self::Ellipse | Self::Highlight | Self::Text | Self::Stamp
        )
    }

    /// Whether its outline closes on itself.
    #[must_use]
    pub const fn closed(self) -> bool {
        matches!(self, Self::Cloud | Self::Polygon | Self::Area)
    }

    /// Whether it measures something.
    #[must_use]
    pub const fn measures(self) -> bool {
        matches!(self, Self::Length | Self::Path | Self::Area | Self::Count)
    }

    /// Whether it may be filled.
    #[must_use]
    pub const fn fillable(self) -> bool {
        matches!(
            self,
            Self::Cloud | Self::Rectangle | Self::Ellipse | Self::Polygon | Self::Area
        )
    }
}

/// Where a markup stands in the review.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Nobody has answered it yet.
    #[default]
    Open,
    /// Agreed.
    Accepted,
    /// Not agreed.
    Rejected,
    /// Done.
    Completed,
}

impl Status {
    /// Every status, in the order the window offers them.
    pub const ALL: [Self; 4] = [Self::Open, Self::Accepted, Self::Rejected, Self::Completed];

    /// What the window calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Accepted => "Accepted",
            Self::Rejected => "Rejected",
            Self::Completed => "Completed",
        }
    }

    fn from_label(label: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|s| s.label() == label)
            .unwrap_or_default()
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
    /// Two corners for boxed kinds; two ends for lines, arrows and
    /// lengths; the tip, then two corners of the box, for a callout; the
    /// top left corner for a note; every point for the others.
    pub points: Vec<Point>,
    /// Stroke color.
    pub color: Color,
    /// Stroke width, in points.
    pub width: f64,
    /// Whether it is filled with its color, see through.
    pub fill: bool,
    /// A comment, or what a text box, callout or stamp shows.
    pub text: String,
    /// What the markup list groups it by; its kind when empty.
    pub subject: String,
    /// Where it stands in the review.
    pub status: Status,
    /// Who made it.
    pub author: String,
    /// When it last changed, as a PDF date.
    pub modified: String,
    /// The scale another program saved with a measurement; Review's own
    /// measurements use the page's.
    pub scale: Option<Scale>,
    /// The annotation it was read from, if any.
    pub origin: Option<ObjectRef>,
}

impl Default for Markup {
    fn default() -> Self {
        Self {
            id: String::new(),
            page: 0,
            kind: Kind::Cloud,
            points: Vec::new(),
            color: [0.85, 0.1, 0.1],
            width: 2.0,
            fill: false,
            text: String::new(),
            subject: String::new(),
            status: Status::Open,
            author: String::new(),
            modified: String::new(),
            scale: None,
            origin: None,
        }
    }
}

/// A text box's and a label's font size, in points.
pub const FONT_SIZE: f64 = 12.0;
/// How see-through fills and highlights are: their opacity.
pub const FILL_OPACITY: f64 = 0.3;
/// A note icon's side, in points.
pub const NOTE_SIZE: f64 = 20.0;
/// A count mark's radius, in points.
pub const COUNT_RADIUS: f64 = 6.0;
/// How far a cloud's scallops reach past its shape, per point of stroke
/// width, and at least.
const SCALLOP: (f64, f64) = (4.0, 8.0);

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

/// Text a markup shows, in PDF space.
#[derive(Clone, Debug, PartialEq)]
pub struct Caption {
    /// Where its first line's baseline starts, or its middle when
    /// `centered`.
    pub at: Point,
    /// Its lines.
    pub text: String,
    /// Font size, in points.
    pub size: f64,
    /// Whether `at` is the middle of the text rather than its start.
    pub centered: bool,
}

impl Markup {
    /// The corners of its box: lowest x and y, then highest.
    #[must_use]
    pub fn bounds(&self) -> (Point, Point) {
        bounds(&self.points)
    }

    /// The box of a callout's text; its bounds for other kinds.
    #[must_use]
    pub fn text_box(&self) -> (Point, Point) {
        match self.kind {
            Kind::Callout if self.points.len() >= 3 => bounds(&self.points[1..3]),
            _ => self.bounds(),
        }
    }

    /// How far a cloud's scallops reach past its shape.
    fn scallop(&self) -> f64 {
        (self.width * SCALLOP.0).max(SCALLOP.1)
    }

    /// The subject shown and saved: the one given, or its kind's name.
    #[must_use]
    pub fn subject(&self) -> &str {
        if self.subject.is_empty() {
            self.kind.label()
        } else {
            &self.subject
        }
    }

    /// Whether `point` is on it, within `tolerance` points.
    #[must_use]
    pub fn hit(&self, point: Point, tolerance: f64) -> bool {
        let reach = tolerance + self.width / 2.0;
        let near_box = |(low, high): (Point, Point)| {
            point.x >= low.x - reach
                && point.x <= high.x + reach
                && point.y >= low.y - reach
                && point.y <= high.y + reach
        };
        let near_path = |points: &[Point], closed: bool| {
            let mut edges: Vec<(Point, Point)> = points.windows(2).map(|w| (w[0], w[1])).collect();
            if closed && points.len() > 2 {
                edges.push((points[points.len() - 1], points[0]));
            }
            edges
                .iter()
                .any(|&(a, b)| segment_distance(point, a, b) <= reach)
        };
        match self.kind {
            k if k.boxed() => near_box(self.bounds()),
            Kind::Note => near_box(self.note_box()),
            Kind::Count => self
                .points
                .iter()
                .any(|p| p.distance(point) <= COUNT_RADIUS + reach),
            Kind::Callout => {
                near_box(self.text_box())
                    || self
                        .leader()
                        .is_some_and(|(a, b)| near_path(&[a, b], false))
            }
            Kind::Cloud => {
                inside(point, &self.points)
                    || self.points.len() > 2 && {
                        let n = self.points.len();
                        (0..n).any(|i| {
                            segment_distance(point, self.points[i], self.points[(i + 1) % n])
                                <= reach + self.scallop()
                        })
                    }
            }
            k if k.closed() => inside(point, &self.points) || near_path(&self.points, true),
            _ => {
                near_path(&self.points, false)
                    || self.points.len() == 1 && self.points[0].distance(point) <= reach
            }
        }
    }

    /// Moves it by `dx`, `dy`.
    pub fn shift(&mut self, dx: f64, dy: f64) {
        for p in &mut self.points {
            p.x += dx;
            p.y += dy;
        }
    }

    /// A note's icon box.
    fn note_box(&self) -> (Point, Point) {
        let a = self.points.first().copied().unwrap_or_default();
        (
            Point::new(a.x, a.y - NOTE_SIZE),
            Point::new(a.x + NOTE_SIZE, a.y),
        )
    }

    /// A callout's leader: from its tip to the nearest middle of a side of
    /// its box.
    #[must_use]
    pub fn leader(&self) -> Option<(Point, Point)> {
        if self.kind != Kind::Callout {
            return None;
        }
        let tip = *self.points.first()?;
        let (low, high) = self.text_box();
        let (mx, my) = (f64::midpoint(low.x, high.x), f64::midpoint(low.y, high.y));
        let ends = [
            Point::new(low.x, my),
            Point::new(high.x, my),
            Point::new(mx, low.y),
            Point::new(mx, high.y),
        ];
        let end = ends
            .into_iter()
            .min_by(|a, b| a.distance(tip).total_cmp(&b.distance(tip)))?;
        Some((tip, end))
    }

    /// Its outline, in PDF space; the window draws the same.
    #[must_use]
    pub fn outline(&self) -> Vec<Segment> {
        let (low, high) = self.bounds();
        match self.kind {
            Kind::Rectangle | Kind::Highlight | Kind::Text => rectangle(low, high),
            Kind::Stamp => {
                let inset = 3.0 + self.width;
                let mut out = rectangle(low, high);
                out.extend(rectangle(
                    Point::new(low.x + inset, low.y + inset),
                    Point::new(high.x - inset, high.y - inset),
                ));
                out
            }
            Kind::Ellipse => ellipse(low, high),
            Kind::Cloud => cloud(&self.points, self.scallop()),
            Kind::Polygon | Kind::Area => {
                let mut out = polyline(&self.points);
                out.push(Segment::Close);
                out
            }
            Kind::Line | Kind::Pen | Kind::Polyline | Kind::Path => polyline(&self.points),
            Kind::Arrow => {
                let mut out = polyline(&self.points);
                if let [.., a, b] = self.points[..] {
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
            Kind::Callout => {
                let (low, high) = self.text_box();
                let mut out = rectangle(low, high);
                if let Some((tip, end)) = self.leader() {
                    out.extend([Segment::Move(end), Segment::Line(tip)]);
                    out.extend(head(end, tip, head_length(self.width) * 0.8));
                }
                out
            }
            Kind::Note => self.note_outline(),
            Kind::Count => self
                .points
                .iter()
                .flat_map(|&p| {
                    ellipse(
                        Point::new(p.x - COUNT_RADIUS, p.y - COUNT_RADIUS),
                        Point::new(p.x + COUNT_RADIUS, p.y + COUNT_RADIUS),
                    )
                })
                .collect(),
        }
    }

    /// A note's icon: a sheet with a folded corner and three lines.
    fn note_outline(&self) -> Vec<Segment> {
        let (low, high) = self.note_box();
        let fold = NOTE_SIZE * 0.3;
        let mut out = vec![
            Segment::Move(low),
            Segment::Line(Point::new(high.x, low.y)),
            Segment::Line(Point::new(high.x, high.y - fold)),
            Segment::Line(Point::new(high.x - fold, high.y)),
            Segment::Line(Point::new(low.x, high.y)),
            Segment::Close,
        ];
        for n in 1..=3 {
            let y = high.y - NOTE_SIZE * 0.25 * f64::from(n);
            let end = high.x - 4.0 - if n == 1 { fold } else { 0.0 };
            out.extend([
                Segment::Move(Point::new(low.x + 4.0, y)),
                Segment::Line(Point::new(end, y)),
            ]);
        }
        out
    }

    /// Whether its outline is filled.
    #[must_use]
    pub fn filled(&self) -> bool {
        self.kind == Kind::Highlight || self.fill && self.kind.fillable()
    }

    /// Whether its outline is stroked.
    #[must_use]
    pub fn stroked(&self) -> bool {
        self.kind != Kind::Highlight && self.width > 0.0
    }

    /// The text it shows on the page, with `value` the measurement a
    /// measuring markup shows.
    #[must_use]
    pub fn captions(&self, value: Option<&str>) -> Vec<Caption> {
        let caption = |at: Point, text: &str, size: f64, centered: bool| Caption {
            at,
            text: text.to_owned(),
            size,
            centered,
        };
        match self.kind {
            Kind::Text | Kind::Callout if self.text.is_empty() => Vec::new(),
            Kind::Text | Kind::Callout => {
                let (low, high) = self.text_box();
                vec![caption(
                    Point::new(low.x + 4.0, high.y - FONT_SIZE - 2.0),
                    &self.text,
                    FONT_SIZE,
                    false,
                )]
            }
            Kind::Stamp => self.stamp_captions(),
            Kind::Length | Kind::Path => {
                let (Some(value), Some(a), Some(b)) =
                    (value, self.points.first(), self.points.get(1))
                else {
                    return Vec::new();
                };
                let middle = Point::new(f64::midpoint(a.x, b.x), f64::midpoint(a.y, b.y));
                vec![caption(
                    Point::new(middle.x + 4.0, middle.y + 4.0),
                    value,
                    FONT_SIZE,
                    false,
                )]
            }
            Kind::Area => value.map_or_else(Vec::new, |value| {
                vec![caption(centroid(&self.points), value, FONT_SIZE, true)]
            }),
            Kind::Count => self
                .points
                .iter()
                .enumerate()
                .map(|(n, p)| {
                    caption(
                        Point::new(p.x + COUNT_RADIUS + 2.0, p.y + COUNT_RADIUS),
                        &(n + 1).to_string(),
                        FONT_SIZE * 0.8,
                        false,
                    )
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// A stamp's words, as large as fit, and who stamped it when.
    fn stamp_captions(&self) -> Vec<Caption> {
        let (low, high) = self.bounds();
        let (w, h) = (high.x - low.x, high.y - low.y);
        let words = if self.text.is_empty() {
            "STAMP"
        } else {
            &self.text
        };
        let first_line = words.lines().next().unwrap_or(words);
        let size = (h * 0.4)
            .min((w - 24.0) / (text_width(first_line, 1.0).max(0.1)))
            .max(4.0);
        let small = (h * 0.16).clamp(4.0, 12.0);
        let middle = f64::midpoint(low.x, high.x);
        let date = crate::pdf_date_text(&self.modified);
        let date = date.split(' ').next().unwrap_or_default().to_owned();
        let by = match (self.author.is_empty(), date.is_empty()) {
            (false, false) => format!("{} · {date}", self.author),
            (false, true) => self.author.clone(),
            (true, _) => date,
        };
        vec![
            Caption {
                at: Point::new(middle, low.y + h * 0.42),
                text: first_line.to_owned(),
                size,
                centered: true,
            },
            Caption {
                at: Point::new(middle, low.y + h * 0.14),
                text: by,
                size: small,
                centered: true,
            },
        ]
    }

    /// The annotation that saves it. A measuring markup passes what it
    /// measures and its scale, as a standard measure.
    #[must_use]
    pub fn to_annotation(&self, measured: Option<(&str, Measure)>) -> Annotation {
        let label = measured.as_ref().map(|(label, _)| *label);
        let (low, high) = bounds(&outline_points(&self.outline()));
        let mut margin = self.width;
        if self.kind.measures() {
            margin += FONT_SIZE * 4.0;
        }
        let rect = match self.kind {
            Kind::Note => {
                let (low, high) = self.note_box();
                [low.x, low.y, high.x, high.y]
            }
            _ => [
                low.x - margin,
                low.y - margin,
                high.x + margin,
                high.y + margin,
            ],
        };
        let (subtype, vertices) = match self.kind {
            Kind::Rectangle | Kind::Highlight => (Subtype::Square, Vec::new()),
            Kind::Ellipse => (Subtype::Circle, Vec::new()),
            Kind::Cloud | Kind::Polygon | Kind::Area => (Subtype::Polygon, self.points.clone()),
            Kind::Line | Kind::Arrow | Kind::Length => (Subtype::Line, self.points.clone()),
            Kind::Polyline | Kind::Path => (Subtype::PolyLine, self.points.clone()),
            Kind::Pen | Kind::Count => (Subtype::Ink, self.points.clone()),
            Kind::Text | Kind::Callout => (Subtype::FreeText, Vec::new()),
            Kind::Note => (Subtype::Text, Vec::new()),
            Kind::Stamp => (Subtype::Stamp, Vec::new()),
        };
        let contents = match (self.kind.measures(), label) {
            (true, Some(label)) if self.text.is_empty() => label.to_owned(),
            (true, Some(label)) => format!("{label}: {}", self.text),
            _ => self.text.clone(),
        };
        let intent = match self.kind {
            Kind::Length => "LineDimension",
            Kind::Path => "PolyLineDimension",
            Kind::Area => "PolygonDimension",
            Kind::Callout => "FreeTextCallout",
            _ => "",
        };
        Annotation {
            page: self.page,
            subtype,
            rect,
            vertices,
            callout: self
                .leader()
                .map(|(tip, end)| vec![tip, end])
                .unwrap_or_default(),
            inset: [0.0; 4],
            color: self.color,
            interior: self.filled().then_some(self.color),
            opacity: 1.0,
            fill_opacity: FILL_OPACITY,
            width: if self.stroked() { self.width } else { 0.0 },
            contents,
            subject: self.subject().to_owned(),
            author: self.author.clone(),
            name: self.id.clone(),
            modified: self.modified.clone(),
            intent: intent.into(),
            icon: match self.kind {
                Kind::Note => "Comment".into(),
                Kind::Stamp => stamp_icon(&self.text),
                _ => String::new(),
            },
            cloudy: self.kind == Kind::Cloud,
            arrow: self.kind == Kind::Arrow,
            font_size: FONT_SIZE,
            measure: measured.map(|(_, measure)| measure),
            own: self.own(label),
            appearance: self.appearance(label),
            origin: self.origin,
            change: Change::New,
        }
    }

    /// Review's own entries, to read it back exactly.
    fn own(&self, label: Option<&str>) -> BTreeMap<String, String> {
        let mut own = BTreeMap::from([
            ("Kind".to_owned(), self.kind.key().to_owned()),
            ("Points".to_owned(), points_text(&self.points)),
            ("Width".to_owned(), self.width.to_string()),
        ]);
        if self.status != Status::Open {
            own.insert("Status".into(), self.status.label().into());
        }
        if self.fill {
            own.insert("Fill".into(), "true".into());
        }
        if let Some(label) = label {
            own.insert("Value".into(), label.to_owned());
        }
        if self.kind.measures() {
            own.insert("Text".into(), self.text.clone());
        }
        own
    }

    /// The markup an annotation saved; `None` for one Review cannot show,
    /// such as another program's stamp.
    #[must_use]
    pub fn from_annotation(annotation: &Annotation) -> Option<Self> {
        let own = annotation.own.get("Kind").and_then(|k| Kind::from_key(k));
        let kind = match own {
            Some(kind) => kind,
            None => foreign_kind(annotation)?,
        };
        let [x0, y0, x1, y1] = annotation.rect;
        // The box itself, inside what its leader or border needs.
        let [left, bottom, right, top] = annotation.inset;
        let (bx0, by0, bx1, by1) = if x1 - x0 > left + right && y1 - y0 > bottom + top {
            (x0 + left, y0 + bottom, x1 - right, y1 - top)
        } else {
            (x0, y0, x1, y1)
        };
        let saved = annotation
            .own
            .get("Points")
            .filter(|_| own.is_some())
            .and_then(|text| parse_points(text));
        let points = saved.unwrap_or_else(|| match kind {
            Kind::Note => vec![Point::new(x0, y1)],
            Kind::Callout => {
                let tip = annotation
                    .callout
                    .first()
                    .copied()
                    .unwrap_or(Point::new(x0, y0));
                vec![tip, Point::new(bx0, by0), Point::new(bx1, by1)]
            }
            k if k.boxed() => vec![Point::new(bx0, by0), Point::new(bx1, by1)],
            _ => annotation.vertices.clone(),
        });
        if points.is_empty() {
            return None;
        }
        let text = if kind.measures() && own.is_some() {
            annotation.own.get("Text").cloned().unwrap_or_default()
        } else {
            annotation.contents.clone()
        };
        let subject = if annotation.subject == kind.label() {
            String::new()
        } else {
            annotation.subject.clone()
        };
        let width = annotation
            .own
            .get("Width")
            .filter(|_| own.is_some())
            .and_then(|w| w.parse().ok())
            .unwrap_or(annotation.width);
        Some(Self {
            id: annotation.name.clone(),
            page: annotation.page,
            kind,
            points,
            color: annotation.color,
            width,
            fill: match own {
                Some(_) => annotation.own.get("Fill").is_some_and(|f| f == "true"),
                None => annotation.interior.is_some() && kind.fillable(),
            },
            text,
            subject,
            status: annotation
                .own
                .get("Status")
                .map_or(Status::Open, |s| Status::from_label(s)),
            author: annotation.author.clone(),
            modified: annotation.modified.clone(),
            scale: if own.is_some() {
                None
            } else {
                annotation.measure.as_ref().and_then(Scale::from_measure)
            },
            origin: annotation.origin,
        })
    }

    /// How it looks, as PDF drawing operators in PDF space.
    #[allow(clippy::many_single_char_names, reason = "colors and coordinates")]
    fn appearance(&self, value: Option<&str>) -> Vec<u8> {
        let [r, g, b] = self.color;
        // The see-through fill is for the shape only: text stays opaque.
        let mut ops = format!(
            "q /{STATE} gs {r:.3} {g:.3} {b:.3} RG {r:.3} {g:.3} {b:.3} rg {:.2} w 1 J 1 j\n",
            self.width
        );
        write_path(&mut ops, &self.outline());
        ops.push_str(match (self.filled(), self.stroked()) {
            (true, true) => "B Q\n",
            (true, false) => "f Q\n",
            _ => "S Q\n",
        });
        let mut bytes = ops.into_bytes();
        let captions = self.captions(value);
        if !captions.is_empty() {
            bytes.extend_from_slice(format!("{r:.3} {g:.3} {b:.3} rg\n").as_bytes());
        }
        for caption in &captions {
            let mut y = caption.at.y;
            for line in caption.text.lines() {
                let x = if caption.centered {
                    caption.at.x - text_width(line, caption.size) / 2.0
                } else {
                    caption.at.x
                };
                bytes.extend_from_slice(
                    format!("BT /{FONT} {:.1} Tf {x:.2} {y:.2} Td ", caption.size).as_bytes(),
                );
                bytes.extend_from_slice(&win_ansi_literal(line));
                bytes.extend_from_slice(b" Tj ET\n");
                y -= caption.size * 1.2;
            }
        }
        bytes
    }
}

/// The kind of another program's annotation; `None` for one Review cannot
/// draw.
fn foreign_kind(annotation: &Annotation) -> Option<Kind> {
    Some(match (annotation.subtype, annotation.intent.as_str()) {
        (Subtype::Square, _) => Kind::Rectangle,
        (Subtype::Circle, _) => Kind::Ellipse,
        (Subtype::Polygon, "PolygonDimension") => Kind::Area,
        (Subtype::Polygon, _) if annotation.cloudy => Kind::Cloud,
        (Subtype::Polygon, _) => Kind::Polygon,
        (Subtype::PolyLine, "PolyLineDimension") => Kind::Path,
        (Subtype::PolyLine, _) => Kind::Polyline,
        (Subtype::Line, "LineDimension") => Kind::Length,
        (Subtype::Line, _) if annotation.measure.is_some() => Kind::Length,
        (Subtype::Line, _) if annotation.arrow => Kind::Arrow,
        (Subtype::Line, _) => Kind::Line,
        (Subtype::Ink, _) => Kind::Pen,
        (Subtype::FreeText, _) if !annotation.callout.is_empty() => Kind::Callout,
        (Subtype::FreeText, _) => Kind::Text,
        (Subtype::Text, _) => Kind::Note,
        (Subtype::Stamp, _) => return None,
    })
}

/// About how wide `text` is in Helvetica at `size`.
#[must_use]
pub fn text_width(text: &str, size: f64) -> f64 {
    let units: u32 = text
        .chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | '.' | ',' | '\'' | '|' | '!' | ':' | ';' => 250,
            ' ' | 'f' | 't' | 'r' | 'I' | '"' | '-' | '(' | ')' | '/' => 320,
            'm' | 'w' | 'M' | 'W' => 850,
            c if c.is_ascii_uppercase() => 700,
            c if c.is_ascii_digit() => 560,
            _ => 540,
        })
        .sum();
    f64::from(units) / 1000.0 * size
}

fn stamp_icon(text: &str) -> String {
    let words: String = text
        .split_whitespace()
        .filter(|w| w.chars().all(char::is_alphanumeric))
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or_else(String::new, |first| {
                first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect()
            })
        })
        .collect();
    if words.is_empty() {
        "Draft".into()
    } else {
        words
    }
}

fn points_text(points: &[Point]) -> String {
    let mut out = String::new();
    for p in points {
        let _ = write!(out, "{} {} ", p.x, p.y);
    }
    out.trim_end().to_owned()
}

fn parse_points(text: &str) -> Option<Vec<Point>> {
    let values: Option<Vec<f64>> = text.split_whitespace().map(|v| v.parse().ok()).collect();
    let values = values?;
    (values.len() % 2 == 0 && !values.is_empty()).then(|| {
        values
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[x, y]| Point::new(x, y))
            .collect()
    })
}

fn write_path(ops: &mut String, segments: &[Segment]) {
    for segment in segments {
        let _ = match *segment {
            Segment::Move(p) => writeln!(ops, "{:.2} {:.2} m", p.x, p.y),
            Segment::Line(p) => writeln!(ops, "{:.2} {:.2} l", p.x, p.y),
            Segment::Curve(a, b, p) => writeln!(
                ops,
                "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c",
                a.x, a.y, b.x, b.y, p.x, p.y
            ),
            Segment::Close => writeln!(ops, "h"),
        };
    }
}

fn outline_points(segments: &[Segment]) -> Vec<Point> {
    segments
        .iter()
        .flat_map(|s| match *s {
            Segment::Move(p) | Segment::Line(p) => vec![p],
            Segment::Curve(a, b, p) => vec![a, b, p],
            Segment::Close => Vec::new(),
        })
        .collect()
}

fn bounds(points: &[Point]) -> (Point, Point) {
    if points.is_empty() {
        return (Point::default(), Point::default());
    }
    let mut low = Point::new(f64::INFINITY, f64::INFINITY);
    let mut high = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in points {
        low = Point::new(low.x.min(p.x), low.y.min(p.y));
        high = Point::new(high.x.max(p.x), high.y.max(p.y));
    }
    (low, high)
}

/// The middle of a polygon's box.
fn centroid(points: &[Point]) -> Point {
    let (low, high) = bounds(points);
    Point::new(f64::midpoint(low.x, high.x), f64::midpoint(low.y, high.y))
}

/// Whether `point` is inside the polygon through `points`.
fn inside(point: Point, points: &[Point]) -> bool {
    let n = points.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (points[i], points[j]);
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
        j = i;
    }
    inside
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

/// Scallops along each side of the polygon through `points`, bulging out
/// by `reach`.
fn cloud(points: &[Point], reach: f64) -> Vec<Segment> {
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    let n = points.len();
    // Outward is to the right of each side for points going
    // counterclockwise (y up), to the left for clockwise.
    let twice: f64 = (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    let outward = if twice >= 0.0 { 1.0 } else { -1.0 };
    let lift = reach * 4.0 / 3.0;
    let mut out = vec![Segment::Move(first)];
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let length = a.distance(b);
        if length < f64::EPSILON {
            continue;
        }
        let count = (length / (reach * 2.0)).round().max(1.0);
        let (ux, uy) = ((b.x - a.x) / length, (b.y - a.y) / length);
        let (nx, ny) = (uy * outward, -ux * outward);
        let step = length / count;
        let mut k = 0.0;
        while k < count {
            let from = Point::new(a.x + ux * step * k, a.y + uy * step * k);
            let to = Point::new(from.x + ux * step, from.y + uy * step);
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
            kind,
            points: points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
            author: "Andrés".into(),
            ..Markup::default()
        }
    }

    fn points_for(kind: Kind) -> Vec<(f64, f64)> {
        match kind.gesture() {
            Gesture::Clicks => vec![(10.0, 10.0), (60.0, 10.0), (60.0, 50.0), (20.0, 70.0)],
            Gesture::Click => vec![(10.0, 10.0)],
            Gesture::Drag if kind == Kind::Callout => {
                vec![(0.0, 0.0), (40.0, 40.0), (140.0, 80.0)]
            }
            Gesture::Drag if kind == Kind::Cloud => {
                vec![(10.0, 10.0), (110.0, 10.0), (110.0, 60.0), (10.0, 60.0)]
            }
            Gesture::Drag if kind == Kind::Pen => vec![(10.0, 10.0), (20.0, 30.0), (40.0, 35.0)],
            Gesture::Drag => vec![(10.0, 10.0), (110.0, 60.0)],
        }
    }

    #[test]
    fn every_kind_comes_back_from_its_annotation() {
        for kind in Kind::ALL {
            let mut original = markup(kind, &points_for(kind));
            original.text = "Check".into();
            original.status = Status::Accepted;
            original.fill = kind.fillable();
            original.subject = "Phase 2".into();
            original.width = 3.0;
            let measured = kind.measures().then(|| {
                (
                    "12\"",
                    Measure {
                        ratio: "1 in = 48 in".into(),
                        unit: "in".into(),
                        factor: 48.0 / 72.0,
                    },
                )
            });
            let annotation = original.to_annotation(measured);
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
        annotation.intent = "LineDimension".into();
        annotation.measure = Some(Measure {
            ratio: String::new(),
            unit: "ft".into(),
            factor: 1.0 / 18.0,
        });
        let length = Markup::from_annotation(&annotation).unwrap();
        assert_eq!(length.kind, Kind::Length);
        assert!(length.scale.is_some());
        // Another program's stamp cannot be drawn by Review: left alone.
        annotation.subtype = Subtype::Stamp;
        annotation.intent.clear();
        assert_eq!(Markup::from_annotation(&annotation), None);
    }

    #[test]
    fn a_click_near_a_markup_hits_it_and_one_away_does_not() {
        let line = markup(Kind::Line, &[(0.0, 0.0), (100.0, 0.0)]);
        assert!(line.hit(Point::new(50.0, 3.0), 3.0));
        assert!(!line.hit(Point::new(50.0, 10.0), 3.0));
        let area = markup(Kind::Area, &[(0.0, 0.0), (100.0, 0.0), (0.0, 100.0)]);
        assert!(area.hit(Point::new(20.0, 20.0), 1.0));
        assert!(!area.hit(Point::new(80.0, 80.0), 1.0));
        let count = markup(Kind::Count, &[(0.0, 0.0), (100.0, 100.0)]);
        assert!(count.hit(Point::new(100.0, 104.0), 1.0));
        assert!(!count.hit(Point::new(50.0, 50.0), 1.0));
    }

    #[test]
    fn a_cloud_has_whole_scallops_bulging_out_on_every_side() {
        for corners in [
            [(0.0, 0.0), (160.0, 0.0), (160.0, 80.0), (0.0, 80.0)],
            [(0.0, 0.0), (0.0, 80.0), (160.0, 80.0), (160.0, 0.0)],
        ] {
            let cloud = markup(Kind::Cloud, &corners);
            let outline = cloud.outline();
            let curves = outline
                .iter()
                .filter(|s| matches!(s, Segment::Curve(..)))
                .count();
            // Scallops reach 8: 16 wide, so 10 + 5 + 10 + 5.
            assert_eq!(curves, 30);
            let inward = outline.iter().any(|s| match *s {
                Segment::Curve(a, _, _) => a.x > 0.5 && a.x < 159.5 && a.y > 0.5 && a.y < 79.5,
                _ => false,
            });
            assert!(!inward, "{corners:?}");
        }
    }

    #[test]
    fn a_callout_leads_to_the_nearest_side_of_its_box() {
        let callout = markup(Kind::Callout, &[(0.0, 50.0), (100.0, 20.0), (200.0, 80.0)]);
        let (tip, end) = callout.leader().unwrap();
        assert_eq!(tip, Point::new(0.0, 50.0));
        assert_eq!(end, Point::new(100.0, 50.0));
    }

    #[test]
    fn another_programs_callout_keeps_its_box_inside_its_leader() {
        let annotation = Annotation {
            subtype: Subtype::FreeText,
            rect: [0.0, 0.0, 300.0, 100.0],
            inset: [100.0, 20.0, 0.0, 30.0],
            callout: vec![Point::new(0.0, 0.0), Point::new(100.0, 50.0)],
            width: 0.0,
            contents: "SIDE OVERHANG".into(),
            ..Annotation::default()
        };
        let callout = Markup::from_annotation(&annotation).unwrap();
        assert_eq!(callout.kind, Kind::Callout);
        assert_eq!(
            callout.text_box(),
            (Point::new(100.0, 20.0), Point::new(300.0, 70.0))
        );
        // No border: only its words and leader would be drawn.
        assert!(!callout.stroked());
    }

    #[test]
    fn stamp_words_become_an_icon_name() {
        assert_eq!(stamp_icon("APPROVED AS NOTED"), "ApprovedAsNoted");
        assert_eq!(stamp_icon(""), "Draft");
    }
}
