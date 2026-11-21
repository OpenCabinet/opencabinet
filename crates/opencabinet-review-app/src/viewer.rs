// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the window shows and does, without the window: the open review,
//! the page, zoom and scroll, the tool, and what the mouse is doing.
//!
//! Screen coordinates are the window's logical pixels inside the page
//! view, from its top left corner.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use opencabinet_model::{Length, Units};
use opencabinet_pdf::{Page, Point, View};
use opencabinet_review::{Color, FONT_SIZE, Kind, Markup, Review, Scale, Segment, shown};

/// What the mouse does on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Picks, moves and pans.
    Select,
    /// Draws a markup of this kind.
    Draw(Kind),
    /// Draws a line of known length to set the page's scale.
    Calibrate,
}

/// The tools, in the order the window offers them.
pub const TOOLS: [Tool; 10] = [
    Tool::Select,
    Tool::Draw(Kind::Cloud),
    Tool::Draw(Kind::Rectangle),
    Tool::Draw(Kind::Ellipse),
    Tool::Draw(Kind::Arrow),
    Tool::Draw(Kind::Line),
    Tool::Draw(Kind::Pen),
    Tool::Draw(Kind::Text),
    Tool::Draw(Kind::Length),
    Tool::Calibrate,
];

impl Tool {
    /// What the window calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Draw(kind) => kind.label(),
            Self::Calibrate => "Calibrate",
        }
    }
}

/// The colors markups can take.
pub const COLORS: [(&str, Color); 4] = [
    ("Red", [0.85, 0.1, 0.1]),
    ("Blue", [0.1, 0.35, 0.85]),
    ("Green", [0.1, 0.55, 0.2]),
    ("Black", [0.0, 0.0, 0.0]),
];

/// A markup's stroke width, in points.
const WIDTH: f64 = 2.0;
/// How close a click must be to pick a markup, in screen pixels.
const PICK: f64 = 6.0;
/// A drag shorter than this, in screen pixels, is a click.
const CLICK: f64 = 4.0;
/// The size of a text box made with a click, in points.
const TEXT_BOX: (f64, f64) = (180.0, 48.0);
/// The zoom range: pixels per point.
const ZOOM: (f64, f64) = (0.02, 16.0);

/// Something to draw over the page, in screen pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// SVG path commands.
    pub commands: String,
    /// Stroke color.
    pub color: Color,
    /// Stroke width in pixels.
    pub width: f64,
    /// Whether it is the selected markup.
    pub selected: bool,
}

/// Text to draw over the page, in screen pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    /// Left.
    pub x: f64,
    /// Top.
    pub y: f64,
    /// What it says.
    pub text: String,
    /// Its color.
    pub color: Color,
    /// Font size in pixels.
    pub size: f64,
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    Pan {
        last: (f64, f64),
    },
    Draw {
        kind: Kind,
        points: Vec<Point>,
        start: (f64, f64),
    },
    Move {
        id: String,
        last: Point,
    },
    Calibrate {
        a: Point,
        b: Point,
    },
}

/// The window's state.
#[derive(Debug)]
pub struct Viewer {
    review: Option<Review>,
    path: Option<PathBuf>,
    page: usize,
    zoom: f64,
    scroll: (f64, f64),
    size: (f64, f64),
    needs_fit: bool,
    tool: Tool,
    color: usize,
    /// Who makes the markups.
    pub author: String,
    /// How lengths are typed and shown.
    pub units: Units,
    drag: Option<Drag>,
    selected: Option<String>,
    calibrating: Option<(Point, Point)>,
    status: String,
}

impl Default for Viewer {
    fn default() -> Self {
        Self {
            review: None,
            path: None,
            page: 0,
            zoom: 1.0,
            scroll: (0.0, 0.0),
            size: (0.0, 0.0),
            needs_fit: false,
            tool: Tool::Select,
            color: 0,
            author: std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_default(),
            units: Units::Imperial,
            drag: None,
            selected: None,
            calibrating: None,
            status: "Open a PDF to start.".into(),
        }
    }
}

impl Viewer {
    /// Opens the PDF at `path`.
    ///
    /// # Errors
    /// A message for the user when it cannot be read.
    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
        let review = Review::open(bytes).map_err(|e| capitalize(&e.to_string()))?;
        self.status = format!(
            "{} pages, {} markups.",
            review.document().pages().len(),
            review.markups().len()
        );
        self.review = Some(review);
        self.path = Some(path.to_owned());
        self.page = 0;
        self.selected = None;
        self.calibrating = None;
        self.drag = None;
        self.fit();
        Ok(())
    }

    /// Saves to `path`, or where it was opened from.
    ///
    /// # Errors
    /// A message for the user when it cannot be written.
    pub fn save(&mut self, path: Option<&Path>) -> Result<(), String> {
        let path = path
            .map(Path::to_owned)
            .or_else(|| self.path.clone())
            .ok_or("There is nowhere to save")?;
        let units = self.units;
        let review = self.review.as_mut().ok_or("There is no PDF open")?;
        let bytes = review.save(units).map_err(|e| capitalize(&e.to_string()))?;
        std::fs::write(&path, bytes).map_err(|e| format!("Cannot save: {e}"))?;
        self.status = format!("Saved {}.", file_name(&path));
        self.path = Some(path);
        Ok(())
    }

    /// Writes the markup list for spreadsheets to `path`.
    ///
    /// # Errors
    /// A message for the user when it cannot be written.
    pub fn export_list(&mut self, path: &Path) -> Result<(), String> {
        let review = self.review.as_ref().ok_or("There is no PDF open")?;
        std::fs::write(path, review.csv(self.units)).map_err(|e| format!("Cannot save: {e}"))?;
        self.status = format!("Wrote the markup list to {}.", file_name(path));
        Ok(())
    }

    /// The open review.
    #[must_use]
    pub fn review(&self) -> Option<&Review> {
        self.review.as_ref()
    }

    /// Where the PDF was opened from or saved to.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether there are changes not saved.
    #[must_use]
    pub fn modified(&self) -> bool {
        self.review.as_ref().is_some_and(Review::modified)
    }

    /// The window's title.
    #[must_use]
    pub fn title(&self) -> String {
        let name = self.path.as_deref().map(file_name);
        match name {
            Some(name) => format!(
                "{}{name} — OpenCabinet Review",
                if self.modified() { "* " } else { "" }
            ),
            None => "OpenCabinet Review".into(),
        }
    }

    /// The line at the bottom of the window.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    fn page_size(&self) -> Option<Page> {
        self.review
            .as_ref()?
            .document()
            .pages()
            .get(self.page)
            .copied()
    }

    /// The page shown, from 0.
    #[must_use]
    pub fn page(&self) -> usize {
        self.page
    }

    /// `Page 2 of 14`, or nothing without a PDF.
    #[must_use]
    pub fn page_label(&self) -> String {
        self.review.as_ref().map_or_else(String::new, |r| {
            format!("Page {} of {}", self.page + 1, r.document().pages().len())
        })
    }

    /// Shows page `page`, fit to the window.
    pub fn go_to(&mut self, page: usize) {
        let count = self
            .review
            .as_ref()
            .map_or(0, |r| r.document().pages().len());
        if page < count && page != self.page {
            self.page = page;
            self.drag = None;
            self.calibrating = None;
            self.fit();
        }
    }

    /// The zoom as a percentage of the page's real size.
    #[must_use]
    pub fn zoom_label(&self) -> String {
        format!("{:.0} %", self.zoom * 100.0 * 96.0 / 72.0)
    }

    /// The page view's new size.
    pub fn resize(&mut self, width: f64, height: f64) {
        self.size = (width, height);
        if self.needs_fit {
            self.fit();
        }
    }

    /// Fits the whole page in the view.
    pub fn fit(&mut self) {
        let Some(page) = self.page_size() else {
            return;
        };
        let (w, h) = self.size;
        if w < 1.0 || h < 1.0 {
            self.needs_fit = true;
            return;
        }
        self.needs_fit = false;
        self.zoom = ((w / page.width).min(h / page.height) * 0.96).clamp(ZOOM.0, ZOOM.1);
        self.scroll = (
            (page.width * self.zoom - w) / 2.0,
            (page.height * self.zoom - h) / 2.0,
        );
    }

    /// Zooms by `factor`, keeping what is under `at` where it is.
    pub fn zoom_by(&mut self, factor: f64, at: (f64, f64)) {
        let zoom = (self.zoom * factor).clamp(ZOOM.0, ZOOM.1);
        let factor = zoom / self.zoom;
        self.scroll = (
            (at.0 + self.scroll.0) * factor - at.0,
            (at.1 + self.scroll.1) * factor - at.1,
        );
        self.zoom = zoom;
        self.clamp_scroll();
    }

    /// Zooms around the middle of the view.
    pub fn zoom_center(&mut self, factor: f64) {
        self.zoom_by(factor, (self.size.0 / 2.0, self.size.1 / 2.0));
    }

    /// The mouse wheel: scrolls, or zooms with Ctrl held.
    pub fn wheel(&mut self, delta: (f64, f64), zoom: bool, at: (f64, f64)) {
        if zoom {
            self.zoom_by(1.2_f64.powf(delta.1 / 120.0), at);
        } else {
            self.scroll.0 -= delta.0;
            self.scroll.1 -= delta.1;
            self.clamp_scroll();
        }
    }

    /// Keeps part of the page in view.
    fn clamp_scroll(&mut self) {
        let Some(page) = self.page_size() else {
            return;
        };
        let keep = 48.0;
        let (w, h) = self.size;
        self.scroll.0 = self
            .scroll
            .0
            .clamp(keep - w, (page.width * self.zoom - keep).max(keep - w));
        self.scroll.1 = self
            .scroll
            .1
            .clamp(keep - h, (page.height * self.zoom - keep).max(keep - h));
    }

    /// What to draw, in physical pixels at `scale` per logical pixel.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "view sizes are small positive numbers"
    )]
    pub fn view(&self, scale: f64) -> Option<View> {
        self.review.as_ref()?;
        let (w, h) = self.size;
        if w < 1.0 || h < 1.0 {
            return None;
        }
        Some(View {
            page: self.page,
            zoom: self.zoom * scale,
            origin: Point::new(self.scroll.0 * scale, self.scroll.1 * scale),
            width: (w * scale).ceil() as u32,
            height: (h * scale).ceil() as u32,
        })
    }

    fn to_pdf(&self, at: (f64, f64)) -> Option<Point> {
        let page = self.page_size()?;
        Some(page.to_pdf().apply(Point::new(
            (at.0 + self.scroll.0) / self.zoom,
            (at.1 + self.scroll.1) / self.zoom,
        )))
    }

    fn to_screen(&self, page: &Page, point: Point) -> (f64, f64) {
        let p = page.to_page.apply(point);
        (
            p.x * self.zoom - self.scroll.0,
            p.y * self.zoom - self.scroll.1,
        )
    }

    /// The tool in use.
    #[must_use]
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// Picks a tool.
    pub fn choose_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.drag = None;
        self.status = match tool {
            Tool::Select => "Click a markup to select it; drag to move it or the page.".into(),
            Tool::Draw(Kind::Pen) => "Drag to draw.".into(),
            Tool::Draw(Kind::Text) => {
                "Drag a box, or click, then type its text on the right.".into()
            }
            Tool::Draw(Kind::Length) => match self.page_scale() {
                Some(_) => "Drag from one end to the other.".into(),
                None => "This page has no scale yet: calibrate it to read lengths.".into(),
            },
            Tool::Draw(kind) => format!("Drag to draw a {}.", kind.label().to_lowercase()),
            Tool::Calibrate => "Drag along a dimension you know, then type its length.".into(),
        };
    }

    /// The color in use, by its place in [`COLORS`].
    #[must_use]
    pub fn color(&self) -> usize {
        self.color
    }

    /// Picks a color, and gives it to the selected markup too.
    pub fn choose_color(&mut self, color: usize) {
        let Some(&(_, rgb)) = COLORS.get(color) else {
            return;
        };
        self.color = color;
        if let (Some(id), Some(review)) = (self.selected.clone(), self.review.as_mut()) {
            review.change(&id, |m| m.color = rgb);
        }
    }

    /// A mouse button went down: 0 left, 1 right, 2 middle.
    pub fn press(&mut self, at: (f64, f64), button: u8) {
        let Some(point) = self.to_pdf(at) else {
            return;
        };
        if button == 2 {
            self.drag = Some(Drag::Pan { last: at });
            return;
        }
        if button != 0 {
            return;
        }
        self.drag = Some(match self.tool {
            Tool::Select => {
                let tolerance = PICK / self.zoom;
                let hit = self
                    .review
                    .as_ref()
                    .and_then(|r| r.hit(self.page, point, tolerance))
                    .map(|m| m.id.clone());
                self.selected.clone_from(&hit);
                match hit {
                    Some(id) => Drag::Move { id, last: point },
                    None => Drag::Pan { last: at },
                }
            }
            Tool::Draw(kind) => Drag::Draw {
                kind,
                points: vec![point, point],
                start: at,
            },
            Tool::Calibrate => Drag::Calibrate { a: point, b: point },
        });
    }

    /// The mouse moved, a button held or not.
    pub fn drag(&mut self, at: (f64, f64)) {
        let Some(point) = self.to_pdf(at) else {
            return;
        };
        match &mut self.drag {
            Some(Drag::Pan { last }) => {
                let (dx, dy) = (at.0 - last.0, at.1 - last.1);
                *last = at;
                self.scroll.0 -= dx;
                self.scroll.1 -= dy;
                self.clamp_scroll();
            }
            Some(Drag::Draw { kind, points, .. }) => {
                if *kind == Kind::Pen {
                    let far = points
                        .last()
                        .is_none_or(|p| p.distance(point) * self.zoom >= 2.0);
                    if far {
                        points.push(point);
                    }
                } else if let Some(last) = points.last_mut() {
                    *last = point;
                }
            }
            Some(Drag::Move { id, last }) => {
                let (dx, dy) = (point.x - last.x, point.y - last.y);
                *last = point;
                let id = id.clone();
                if let Some(review) = self.review.as_mut() {
                    review.change(&id, |m| m.shift(dx, dy));
                }
            }
            Some(Drag::Calibrate { b, .. }) => *b = point,
            None => {}
        }
    }

    /// The mouse button came up.
    pub fn release(&mut self, at: (f64, f64)) {
        self.drag(at);
        match self.drag.take() {
            Some(Drag::Draw {
                kind,
                mut points,
                start,
            }) => {
                let click = (at.0 - start.0).hypot(at.1 - start.1) < CLICK;
                if click && kind == Kind::Text {
                    let a = points[0];
                    points = vec![
                        Point::new(a.x, a.y - TEXT_BOX.1),
                        Point::new(a.x + TEXT_BOX.0, a.y),
                    ];
                } else if click {
                    return;
                }
                if kind == Kind::Pen {
                    points.dedup();
                }
                let markup = Markup {
                    id: String::new(),
                    page: self.page,
                    kind,
                    points,
                    color: COLORS[self.color].1,
                    width: WIDTH,
                    text: String::new(),
                    author: String::new(),
                    modified: String::new(),
                };
                let author = self.author.clone();
                if let Some(review) = self.review.as_mut() {
                    self.selected = Some(review.add(markup, &author));
                }
                if kind == Kind::Text {
                    self.status = "Type the text on the right.".into();
                }
            }
            Some(Drag::Calibrate { a, b }) if a.distance(b) * self.zoom >= CLICK => {
                self.calibrating = Some((a, b));
                self.status = "Type the real length of the line on the right.".into();
            }
            _ => {}
        }
    }

    /// Whether a calibration line waits for its length.
    #[must_use]
    pub fn calibrating(&self) -> bool {
        self.calibrating.is_some()
    }

    /// Sets the page's scale from the calibration line and its real
    /// length, typed.
    ///
    /// # Errors
    /// A message for the user when the length cannot be read.
    pub fn calibrate(&mut self, text: &str) -> Result<(), String> {
        let (a, b) = self.calibrating.ok_or("Draw a calibration line first")?;
        let real = Length::parse(text, self.units).map_err(|e| capitalize(&e.to_string()))?;
        let scale = Scale::from_line(a, b, real).ok_or("Must be greater than zero")?;
        let page = self.page;
        let review = self.review.as_mut().ok_or("There is no PDF open")?;
        review.set_scale(page, scale);
        self.calibrating = None;
        self.status = format!("Scale set: {}.", self.scale_label());
        Ok(())
    }

    /// Stops what is under way: a drag, a calibration, a selection.
    pub fn cancel(&mut self) {
        self.drag = None;
        self.calibrating = None;
        self.selected = None;
    }

    fn page_scale(&self) -> Option<Scale> {
        self.review.as_ref()?.scale(self.page)
    }

    /// The page's scale, such as `1" = 48"`, or why there is none.
    #[must_use]
    pub fn scale_label(&self) -> String {
        match self.page_scale() {
            Some(scale) => {
                let inch = Scale::from_line(
                    Point::new(0.0, 0.0),
                    Point::new(scale.points, 0.0),
                    scale.real,
                )
                .map(|s| s.measure(Point::new(0.0, 0.0), Point::new(72.0, 0.0)))
                .unwrap_or_default();
                format!("Scale 1\" = {}", shown(inch, self.units))
            }
            None if self.review.is_some() => "Not to scale".into(),
            None => String::new(),
        }
    }

    /// The selected markup.
    #[must_use]
    pub fn selected(&self) -> Option<&Markup> {
        let id = self.selected.as_deref()?;
        self.review.as_ref()?.markup(id)
    }

    /// Selects the markup listed at `row`, and shows its page.
    pub fn select_row(&mut self, row: usize) {
        let Some(markup) = self.review.as_ref().and_then(|r| r.markups().get(row)) else {
            return;
        };
        let (id, page) = (markup.id.clone(), markup.page);
        self.go_to(page);
        self.selected = Some(id);
    }

    /// The selected markup's place in the list.
    #[must_use]
    pub fn selected_row(&self) -> Option<usize> {
        let id = self.selected.as_deref()?;
        self.review
            .as_ref()?
            .markups()
            .iter()
            .position(|m| m.id == id)
    }

    /// Changes the selected markup's text.
    pub fn set_text(&mut self, text: &str) {
        if let (Some(id), Some(review)) = (self.selected.clone(), self.review.as_mut()) {
            review.change(&id, |m| text.clone_into(&mut m.text));
        }
    }

    /// Deletes the selected markup.
    pub fn delete_selected(&mut self) {
        if let (Some(id), Some(review)) = (self.selected.take(), self.review.as_mut()) {
            review.remove(&id);
        }
    }

    /// The markup list: page, type, author, and text or measurement.
    #[must_use]
    pub fn rows(&self) -> Vec<[String; 4]> {
        let Some(review) = self.review.as_ref() else {
            return Vec::new();
        };
        review
            .markups()
            .iter()
            .map(|m| {
                let what = match review.measured(m, self.units) {
                    Some(value) if m.text.is_empty() => value,
                    Some(value) => format!("{value} · {}", m.text),
                    None => m.text.lines().next().unwrap_or_default().to_owned(),
                };
                [
                    (m.page + 1).to_string(),
                    m.kind.label().to_owned(),
                    m.author.clone(),
                    what,
                ]
            })
            .collect()
    }

    /// What to draw over the page: the markups on it, the one being drawn
    /// and the calibration line.
    #[must_use]
    pub fn overlay(&self) -> (Vec<Shape>, Vec<Label>) {
        let (Some(review), Some(page)) = (self.review.as_ref(), self.page_size()) else {
            return (Vec::new(), Vec::new());
        };
        let mut shapes = Vec::new();
        let mut labels = Vec::new();
        let mut draw = |markup: &Markup, selected: bool| {
            shapes.push(Shape {
                commands: self.commands(&page, &markup.outline()),
                color: markup.color,
                width: (markup.width * self.zoom).max(1.0),
                selected,
            });
            let size = FONT_SIZE * self.zoom;
            match markup.kind {
                Kind::Text => {
                    let (low, high) = markup.bounds();
                    let (x, y) = self.to_screen(&page, Point::new(low.x, high.y));
                    labels.push(Label {
                        x: x + 4.0 * self.zoom,
                        y: y + 2.0 * self.zoom,
                        text: markup.text.clone(),
                        color: markup.color,
                        size,
                    });
                }
                Kind::Length => {
                    if let (Some(value), [a, b]) =
                        (review.measured(markup, self.units), &markup.points[..])
                    {
                        let middle = Point::new(f64::midpoint(a.x, b.x), f64::midpoint(a.y, b.y));
                        let (x, y) = self.to_screen(&page, middle);
                        labels.push(Label {
                            x: x + 4.0 * self.zoom,
                            y: y - size - 4.0 * self.zoom,
                            text: value,
                            color: markup.color,
                            size,
                        });
                    }
                }
                _ => {}
            }
        };
        for markup in review.markups().iter().filter(|m| m.page == self.page) {
            draw(markup, self.selected.as_deref() == Some(markup.id.as_str()));
        }
        if let Some(Drag::Draw { kind, points, .. }) = &self.drag {
            let preview = Markup {
                id: String::new(),
                page: self.page,
                kind: *kind,
                points: points.clone(),
                color: COLORS[self.color].1,
                width: WIDTH,
                text: String::new(),
                author: String::new(),
                modified: String::new(),
            };
            draw(&preview, false);
        }
        let line = match &self.drag {
            Some(Drag::Calibrate { a, b }) => Some((*a, *b)),
            _ => self.calibrating,
        };
        if let Some((a, b)) = line {
            shapes.push(Shape {
                commands: self.commands(&page, &[Segment::Move(a), Segment::Line(b)]),
                color: [0.0, 0.45, 0.9],
                width: 2.0,
                selected: false,
            });
        }
        (shapes, labels)
    }

    /// An outline in PDF space as SVG commands in screen pixels.
    fn commands(&self, page: &Page, segments: &[Segment]) -> String {
        let mut out = String::new();
        let point = |out: &mut String, p: Point| {
            let (x, y) = self.to_screen(page, p);
            let _ = write!(out, " {x:.1} {y:.1}");
        };
        for segment in segments {
            match *segment {
                Segment::Move(p) => {
                    out.push_str(" M");
                    point(&mut out, p);
                }
                Segment::Line(p) => {
                    out.push_str(" L");
                    point(&mut out, p);
                }
                Segment::Curve(a, b, p) => {
                    out.push_str(" C");
                    point(&mut out, a);
                    point(&mut out, b);
                    point(&mut out, p);
                }
                Segment::Close => out.push_str(" Z"),
            }
        }
        out.trim_start().to_owned()
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A viewer with a two-sheet set open, 1000 × 700 pixels, scrolled so
    /// that the page's top left is at the view's and 1 pixel is 1 point.
    fn viewer() -> (Viewer, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "opencabinet-review-viewer-{}-{}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace("::", "-")
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sheets.pdf");
        std::fs::write(&path, opencabinet_pdf::sample::sheets(2, 2592.0, 1728.0, 4)).unwrap();
        let mut viewer = Viewer::default();
        viewer.resize(1000.0, 700.0);
        viewer.open(&path).unwrap();
        viewer.zoom = 1.0;
        viewer.scroll = (0.0, 0.0);
        (viewer, dir)
    }

    fn drag(viewer: &mut Viewer, from: (f64, f64), to: (f64, f64)) {
        viewer.press(from, 0);
        viewer.drag((f64::midpoint(from.0, to.0), f64::midpoint(from.1, to.1)));
        viewer.release(to);
    }

    #[test]
    fn opening_fits_the_page_in_the_view() {
        let (mut viewer, dir) = viewer();
        viewer.fit();
        // 2592 × 1728 in 1000 × 700: the width decides, 0.96 × 1000/2592.
        assert!((viewer.zoom - 0.96 * 1000.0 / 2592.0).abs() < 1e-9);
        assert_eq!(viewer.page_label(), "Page 1 of 2");
        assert_eq!(viewer.title(), "sheets.pdf — OpenCabinet Review");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_drag_draws_a_markup_where_the_mouse_went() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Cloud));
        drag(&mut viewer, (100.0, 100.0), (300.0, 200.0));
        let markup = viewer.selected().unwrap().clone();
        assert_eq!(markup.kind, Kind::Cloud);
        // Screen y down from the top; PDF y up from the bottom.
        assert_eq!(
            markup.points,
            [Point::new(100.0, 1628.0), Point::new(300.0, 1528.0)]
        );
        assert_eq!(markup.author, viewer.author);
        assert!(viewer.title().starts_with("* "));
        // A click draws nothing.
        viewer.press((500.0, 500.0), 0);
        viewer.release((501.0, 500.0));
        assert_eq!(viewer.rows().len(), 1);
        let (shapes, _) = viewer.overlay();
        assert_eq!(shapes.len(), 1);
        // A cloud starts at its lowest corner in PDF space: bottom left.
        assert!(shapes[0].commands.starts_with("M 100.0 200.0 C"));
        assert!(shapes[0].selected);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_markup_is_picked_moved_retyped_and_deleted() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        drag(&mut viewer, (100.0, 100.0), (200.0, 200.0));
        viewer.choose_tool(Tool::Select);
        viewer.cancel();
        drag(&mut viewer, (100.0, 150.0), (150.0, 160.0));
        let moved = viewer.selected().unwrap();
        // 50 right and 10 down on screen: 10 lower in PDF space, from the
        // corner first pressed.
        assert_eq!(moved.points[0], Point::new(150.0, 1618.0));
        viewer.set_text("Check with GC");
        viewer.choose_color(1);
        assert_eq!(viewer.selected().unwrap().color, COLORS[1].1);
        assert_eq!(viewer.rows()[0][3], "Check with GC");
        viewer.delete_selected();
        assert_eq!(viewer.rows().len(), 0);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_calibrated_page_measures_lengths() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Calibrate);
        // 216 points: 3" on the sheet.
        drag(&mut viewer, (100.0, 100.0), (316.0, 100.0));
        assert!(viewer.calibrating());
        assert_eq!(viewer.calibrate("tall"), Err("Not a length".into()));
        viewer.calibrate("144").unwrap();
        assert_eq!(viewer.scale_label(), "Scale 1\" = 48\"");
        viewer.choose_tool(Tool::Draw(Kind::Length));
        drag(&mut viewer, (100.0, 300.0), (100.0, 408.0));
        assert_eq!(viewer.rows()[0][3], "72\"");
        let (_, labels) = viewer.overlay();
        assert_eq!(labels[0].text, "72\"");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_click_with_the_text_tool_makes_a_box_to_type_in() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Text));
        viewer.press((100.0, 100.0), 0);
        viewer.release((100.0, 100.0));
        viewer.set_text("Filler 3\"");
        let (low, high) = viewer.selected().unwrap().bounds();
        assert_eq!((high.x - low.x, high.y - low.y), TEXT_BOX);
        let (_, labels) = viewer.overlay();
        assert_eq!(labels[0].text, "Filler 3\"");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn zooming_keeps_the_point_under_the_mouse() {
        let (mut viewer, dir) = viewer();
        let before = viewer.to_pdf((400.0, 300.0)).unwrap();
        viewer.wheel((0.0, 120.0), true, (400.0, 300.0));
        assert!((viewer.zoom - 1.2).abs() < 1e-9);
        let after = viewer.to_pdf((400.0, 300.0)).unwrap();
        assert!(before.distance(after) < 1e-9);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn saving_keeps_the_markups_for_the_next_opening() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Arrow));
        drag(&mut viewer, (100.0, 100.0), (300.0, 200.0));
        viewer.save(None).unwrap();
        assert!(!viewer.modified());
        let list = dir.join("list.csv");
        viewer.export_list(&list).unwrap();
        assert!(std::fs::read_to_string(&list).unwrap().contains(",Arrow,"));
        let mut again = Viewer::default();
        again.open(&dir.join("sheets.pdf")).unwrap();
        assert_eq!(again.rows().len(), 1);
        assert_eq!(again.rows()[0][1], "Arrow");
        std::fs::remove_dir_all(dir).ok();
    }
}
