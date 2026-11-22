// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the window shows and does, without the window: the open review,
//! the page, zoom and scroll, the tool, and what the mouse is doing.
//!
//! Screen coordinates are the window's logical pixels inside the page
//! view, from its top left corner.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use opencabinet_model::{Length, Units};
use opencabinet_pdf::find::{Found, find};
use opencabinet_pdf::{Document, Page, Point, View};
use opencabinet_review::{
    Color, FILL_OPACITY, Gesture, Kind, Markup, Review, Scale, Segment, Status, presets, stamps,
    text_width,
};
use opencabinet_session::{Guest, Host, Invite, Link, Shared};

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
pub const TOOLS: [Tool; 19] = [
    Tool::Select,
    Tool::Draw(Kind::Cloud),
    Tool::Draw(Kind::Rectangle),
    Tool::Draw(Kind::Ellipse),
    Tool::Draw(Kind::Polygon),
    Tool::Draw(Kind::Arrow),
    Tool::Draw(Kind::Line),
    Tool::Draw(Kind::Polyline),
    Tool::Draw(Kind::Pen),
    Tool::Draw(Kind::Highlight),
    Tool::Draw(Kind::Text),
    Tool::Draw(Kind::Callout),
    Tool::Draw(Kind::Note),
    Tool::Draw(Kind::Stamp),
    Tool::Calibrate,
    Tool::Draw(Kind::Length),
    Tool::Draw(Kind::Path),
    Tool::Draw(Kind::Area),
    Tool::Draw(Kind::Count),
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
pub const COLORS: [(&str, Color); 6] = [
    ("Red", [0.85, 0.1, 0.1]),
    ("Orange", [0.95, 0.5, 0.0]),
    ("Yellow", [1.0, 0.85, 0.0]),
    ("Green", [0.1, 0.55, 0.2]),
    ("Blue", [0.1, 0.35, 0.85]),
    ("Black", [0.0, 0.0, 0.0]),
];

/// The stroke widths markups can take, in points.
pub const WIDTHS: [(&str, f64); 3] = [("Thin", 1.0), ("Medium", 2.0), ("Thick", 4.0)];

/// How close a click must be to pick a markup, in screen pixels.
const PICK: f64 = 6.0;
/// A drag shorter than this, in screen pixels, is a click.
const CLICK: f64 = 4.0;
/// The size of a text box or callout box made with a click, in points.
const TEXT_BOX: (f64, f64) = (180.0, 48.0);
/// The zoom range: pixels per point.
const ZOOM: (f64, f64) = (0.02, 16.0);

/// Something to draw over the page, in screen pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// SVG path commands.
    pub commands: String,
    /// Stroke color; `None` for no stroke.
    pub stroke: Option<Color>,
    /// Fill color and opacity; `None` for no fill.
    pub fill: Option<(Color, f64)>,
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
    width: usize,
    fill: bool,
    stamp: usize,
    /// Who makes the markups.
    pub author: String,
    /// How lengths are typed and shown.
    pub units: Units,
    drag: Option<Drag>,
    /// A markup drawn point by point: its points, and where the mouse is.
    building: Option<(Kind, Vec<Point>, Option<Point>)>,
    selected: Option<String>,
    calibrating: Option<(Point, Point)>,
    /// What was copied, to paste.
    clipboard: Option<Markup>,
    /// The markup list shows only markups with this status, if any, and
    /// whose subject, author or text has `filter_text`.
    filter_status: Option<Status>,
    filter_text: String,
    /// Where the words searched for were found, and which is shown.
    found: Vec<Found>,
    found_index: usize,
    searched: String,
    /// Another drawing laid over this one, and its name.
    compare: Option<(String, Document)>,
    /// The review session this window is in, if any.
    session: Option<Session>,
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
            width: 1,
            fill: false,
            stamp: 0,
            author: std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_default(),
            units: Units::Imperial,
            drag: None,
            building: None,
            selected: None,
            calibrating: None,
            clipboard: None,
            filter_status: None,
            filter_text: String::new(),
            found: Vec::new(),
            found_index: 0,
            searched: String::new(),
            compare: None,
            session: None,
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
        self.open_bytes(bytes, Some(path))
    }

    fn open_bytes(&mut self, bytes: Vec<u8>, path: Option<&Path>) -> Result<(), String> {
        self.leave_session();
        let review = Review::open(bytes).map_err(|e| capitalize(&e.to_string()))?;
        self.status = format!(
            "{} pages, {} markups.",
            review.document().pages().len(),
            review.markups().len()
        );
        self.review = Some(review);
        self.path = path.map(Path::to_owned);
        self.page = 0;
        self.selected = None;
        self.calibrating = None;
        self.drag = None;
        self.building = None;
        self.found.clear();
        self.searched.clear();
        self.compare = None;
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

    /// The open review, to draw its pages.
    pub fn review_mut(&mut self) -> Option<&mut Review> {
        self.review.as_mut()
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
            self.building = None;
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

    /// Fits the page's width in the view, from its top.
    pub fn fit_width(&mut self) {
        let Some(page) = self.page_size() else {
            return;
        };
        let (w, _) = self.size;
        if w < 1.0 {
            return;
        }
        self.zoom = (w / page.width * 0.98).clamp(ZOOM.0, ZOOM.1);
        self.scroll = ((page.width * self.zoom - w) / 2.0, 0.0);
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

    /// Picks a tool; a shape being drawn point by point is finished.
    pub fn choose_tool(&mut self, tool: Tool) {
        self.finish();
        self.tool = tool;
        self.drag = None;
        self.status = match tool {
            Tool::Select => {
                "Click a markup to select it; drag to move it or the page. Ctrl+wheel zooms.".into()
            }
            Tool::Draw(Kind::Pen) => "Drag to draw.".into(),
            Tool::Draw(Kind::Text) => {
                "Drag a box, or click, then type its text on the right.".into()
            }
            Tool::Draw(Kind::Callout) => {
                "Press where it points, and drag to where its text goes.".into()
            }
            Tool::Draw(Kind::Note) => {
                "Click where the note goes, then type it on the right.".into()
            }
            Tool::Draw(Kind::Stamp) => "Click to stamp, or drag the stamp's box.".into(),
            Tool::Draw(Kind::Count) => {
                "Click each item to count; Enter or double click to finish.".into()
            }
            Tool::Draw(kind) if kind.measures() && self.page_scale().is_none() => {
                "This page has no scale yet: calibrate it to read measurements.".into()
            }
            Tool::Draw(kind) if kind.gesture() == Gesture::Clicks => format!(
                "Click each point of the {}; Enter or double click to finish.",
                kind.label().to_lowercase()
            ),
            Tool::Draw(Kind::Length) => "Drag from one end to the other.".into(),
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
        self.change_selected(|m| m.color = rgb);
    }

    /// The stroke width in use, by its place in [`WIDTHS`].
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Picks a stroke width, and gives it to the selected markup too.
    pub fn choose_width(&mut self, width: usize) {
        let Some(&(_, points)) = WIDTHS.get(width) else {
            return;
        };
        self.width = width;
        self.change_selected(|m| m.width = points);
    }

    /// Whether new shapes are filled.
    #[must_use]
    pub fn fill(&self) -> bool {
        self.fill
    }

    /// Fills new shapes, and the selected one, or not.
    pub fn set_fill(&mut self, fill: bool) {
        self.fill = fill;
        self.change_selected(|m| m.fill = fill);
    }

    /// The stamp placed next, by its place in [`stamps::TEMPLATES`].
    #[must_use]
    pub fn stamp(&self) -> usize {
        self.stamp
    }

    /// Picks the stamp placed next.
    pub fn choose_stamp(&mut self, stamp: usize) {
        if stamp < stamps::TEMPLATES.len() {
            self.stamp = stamp;
            self.choose_tool(Tool::Draw(Kind::Stamp));
        }
    }

    fn change_selected(&mut self, change: impl FnOnce(&mut Markup)) {
        if let (Some(id), Some(review)) = (self.selected.clone(), self.review.as_mut()) {
            review.change(&id, change);
        }
    }

    /// A new markup of `kind` through `points`, in the style in use.
    fn new_markup(&self, kind: Kind, points: Vec<Point>) -> Markup {
        let template = stamps::TEMPLATES[self.stamp];
        Markup {
            page: self.page,
            kind,
            points,
            color: if kind == Kind::Stamp {
                template.color
            } else {
                COLORS[self.color].1
            },
            width: WIDTHS[self.width].1,
            fill: self.fill && kind.fillable(),
            text: if kind == Kind::Stamp {
                template.words.into()
            } else {
                String::new()
            },
            ..Markup::default()
        }
    }

    fn add(&mut self, markup: Markup) {
        let author = self.author.clone();
        if let Some(review) = self.review.as_mut() {
            self.selected = Some(review.add(markup, &author));
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
        if button == 1 {
            self.finish();
            return;
        }
        if button != 0 {
            return;
        }
        if let Some((_, points, _)) = &mut self.building {
            points.push(point);
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
                    Some(id) => {
                        if let Some(review) = self.review.as_mut() {
                            review.checkpoint();
                        }
                        Drag::Move { id, last: point }
                    }
                    None => Drag::Pan { last: at },
                }
            }
            Tool::Draw(kind) => match kind.gesture() {
                Gesture::Clicks => {
                    self.building = Some((kind, vec![point], None));
                    return;
                }
                Gesture::Click => {
                    self.add(self.new_markup(kind, vec![point]));
                    self.status = "Type the note on the right.".into();
                    return;
                }
                Gesture::Drag => Drag::Draw {
                    kind,
                    points: vec![point, point],
                    start: at,
                },
            },
            Tool::Calibrate => Drag::Calibrate { a: point, b: point },
        });
    }

    /// The mouse moved, a button held or not.
    pub fn drag(&mut self, at: (f64, f64)) {
        let Some(point) = self.to_pdf(at) else {
            return;
        };
        if let Some((_, _, cursor)) = &mut self.building {
            *cursor = Some(point);
        }
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
                    review.change_quietly(&id, |m| m.shift(dx, dy));
                }
            }
            Some(Drag::Calibrate { b, .. }) => *b = point,
            None => {}
        }
    }

    /// The mouse button came up.
    pub fn release(&mut self, at: (f64, f64)) {
        if self.drag.is_some() {
            self.drag(at);
        }
        match self.drag.take() {
            Some(Drag::Draw {
                kind,
                points,
                start,
            }) => {
                let click = (at.0 - start.0).hypot(at.1 - start.1) < CLICK;
                if let Some(points) = drawn(kind, points, click) {
                    self.add(self.new_markup(kind, points));
                    if matches!(kind, Kind::Text | Kind::Callout) {
                        self.status = "Type the text on the right.".into();
                    }
                }
            }
            Some(Drag::Calibrate { a, b }) if a.distance(b) * self.zoom >= CLICK => {
                self.calibrating = Some((a, b));
                self.status = "Type the real length of the line on the right.".into();
            }
            _ => {}
        }
    }

    /// A double click: finishes a shape drawn point by point.
    pub fn double_click(&mut self) {
        self.finish();
    }

    /// Finishes a shape drawn point by point, if it has enough points.
    pub fn finish(&mut self) {
        let Some((kind, mut points, _)) = self.building.take() else {
            return;
        };
        if kind != Kind::Count {
            // A double click's second press adds its point again.
            points.dedup_by(|a, b| a.distance(*b) < 1e-9);
        }
        let enough = match kind {
            Kind::Count => 1,
            Kind::Polygon | Kind::Area => 3,
            _ => 2,
        };
        if points.len() >= enough {
            self.add(self.new_markup(kind, points));
        }
    }

    /// Whether a shape is being drawn point by point.
    #[must_use]
    pub fn building(&self) -> bool {
        self.building.is_some()
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

    /// The common scales' names, to choose from.
    #[must_use]
    pub fn scale_names() -> Vec<String> {
        presets().into_iter().map(|(name, _)| name).collect()
    }

    /// Calibrates the page, or every page, with a common scale.
    pub fn choose_scale(&mut self, preset: usize, every_page: bool) {
        let Some((name, scale)) = presets().into_iter().nth(preset) else {
            return;
        };
        let page = self.page;
        if let Some(review) = self.review.as_mut() {
            if every_page {
                review.set_scale_everywhere(scale);
            } else {
                review.set_scale(page, scale);
            }
            self.calibrating = None;
            self.status = if every_page {
                format!("Every page is at {name}.")
            } else {
                format!("This page is at {name}.")
            };
        }
    }

    /// Calibrates every page with this page's scale.
    pub fn scale_every_page(&mut self) {
        if let Some(scale) = self.page_scale()
            && let Some(review) = self.review.as_mut()
        {
            review.set_scale_everywhere(scale);
            self.status = "Every page has this page's scale.".into();
        }
    }

    /// Copies the selected markup.
    pub fn copy(&mut self) {
        if let Some(markup) = self.selected().cloned() {
            self.clipboard = Some(markup);
            self.status = "Copied.".into();
        }
    }

    /// Pastes what was copied on the page shown: beside where it was on
    /// the same page, in the same place on another.
    pub fn paste(&mut self) {
        let Some(mut markup) = self.clipboard.clone() else {
            return;
        };
        let offset = if markup.page == self.page { 18.0 } else { 0.0 };
        markup.shift(offset, -offset);
        markup.page = self.page;
        markup.origin = None;
        markup.scale = None;
        // The next paste lands beside this one.
        if let Some(copy) = self.clipboard.as_mut() {
            copy.page = self.page;
            copy.points.clone_from(&markup.points);
        }
        self.add(markup);
    }

    /// Copies and pastes the selected markup.
    pub fn duplicate(&mut self) {
        let kept = self.clipboard.take();
        self.copy();
        self.paste();
        self.clipboard = kept;
    }

    /// Zooms to the selected markup, with room around it.
    pub fn zoom_to_selected(&mut self) {
        let (Some(markup), Some(page)) = (self.selected(), self.page_size()) else {
            return;
        };
        let (low, high) = markup.bounds();
        let corners = [page.to_page.apply(low), page.to_page.apply(high)];
        let (x0, x1) = (
            corners[0].x.min(corners[1].x),
            corners[0].x.max(corners[1].x),
        );
        let (y0, y1) = (
            corners[0].y.min(corners[1].y),
            corners[0].y.max(corners[1].y),
        );
        let margin = 36.0;
        let (w, h) = self.size;
        if w < 1.0 || h < 1.0 {
            return;
        }
        let zoom = (w / (x1 - x0 + 2.0 * margin))
            .min(h / (y1 - y0 + 2.0 * margin))
            .clamp(ZOOM.0, ZOOM.1);
        self.zoom = zoom;
        self.scroll = (
            f64::midpoint(x0, x1) * zoom - w / 2.0,
            f64::midpoint(y0, y1) * zoom - h / 2.0,
        );
    }

    /// Escape: stops what is under way and goes back to the select tool;
    /// with nothing under way, lets go of the selection.
    pub fn escape(&mut self) {
        let busy = self.building.is_some()
            || self.drag.is_some()
            || self.calibrating.is_some()
            || self.tool != Tool::Select;
        self.drag = None;
        self.building = None;
        self.calibrating = None;
        if busy {
            self.choose_tool(Tool::Select);
        } else {
            self.selected = None;
        }
    }

    /// Shows in the markup list only markups with `status` (all when
    /// `None`) whose subject, author or text has `text`.
    pub fn set_filter(&mut self, status: Option<Status>, text: &str) {
        self.filter_status = status;
        text.trim().to_lowercase().clone_into(&mut self.filter_text);
    }

    /// The markups the list shows, in its order.
    fn listed(&self) -> Vec<&Markup> {
        let Some(review) = self.review.as_ref() else {
            return Vec::new();
        };
        review
            .markups()
            .iter()
            .filter(|m| self.filter_status.is_none_or(|s| m.status == s))
            .filter(|m| {
                self.filter_text.is_empty()
                    || [
                        m.subject(),
                        m.author.as_str(),
                        m.text.as_str(),
                        m.kind.label(),
                    ]
                    .iter()
                    .any(|field| field.to_lowercase().contains(&self.filter_text))
            })
            .collect()
    }

    /// How many markups the list shows, of how many.
    #[must_use]
    pub fn list_label(&self) -> String {
        let all = self.review.as_ref().map_or(0, |r| r.markups().len());
        let shown = self.listed().len();
        if shown == all {
            format!("{all} markups")
        } else {
            format!("{shown} of {all} markups")
        }
    }

    /// Finds `query` on every page and shows the first place; an empty
    /// query clears the search.
    pub fn find(&mut self, query: &str) {
        self.found.clear();
        self.found_index = 0;
        query.trim().clone_into(&mut self.searched);
        if self.searched.is_empty() {
            return;
        }
        let Some(document) = self.review.as_ref().map(|r| r.document().clone()) else {
            return;
        };
        for page in 0..document.pages().len() {
            if let Ok(found) = find(&document, page, &self.searched) {
                self.found.extend(found);
            }
        }
        // From the page shown onwards.
        self.found_index = self
            .found
            .iter()
            .position(|f| f.page >= self.page)
            .unwrap_or(0);
        self.show_found();
    }

    /// Shows the next place found, or the one before with `back`.
    pub fn find_next(&mut self, back: bool) {
        let count = self.found.len();
        if count == 0 {
            return;
        }
        self.found_index = if back {
            (self.found_index + count - 1) % count
        } else {
            (self.found_index + 1) % count
        };
        self.show_found();
    }

    fn show_found(&mut self) {
        let Some(found) = self.found.get(self.found_index).cloned() else {
            self.status = format!("\"{}\" is not in this PDF.", self.searched);
            return;
        };
        self.go_to(found.page);
        if let Some(page) = self.page_size() {
            let [x0, y0, x1, y1] = found.rect;
            let a = page.to_page.apply(Point::new(x0, y0));
            let b = page.to_page.apply(Point::new(x1, y1));
            let middle = (f64::midpoint(a.x, b.x), f64::midpoint(a.y, b.y));
            // Close enough to read it, with what is around it.
            let (w, h) = self.size;
            self.zoom = (self.zoom.max(1.0)).clamp(ZOOM.0, ZOOM.1);
            self.scroll = (
                middle.0 * self.zoom - w / 2.0,
                middle.1 * self.zoom - h / 2.0,
            );
            self.clamp_scroll();
        }
        self.status = format!("Found on page {}.", found.page + 1);
    }

    /// `3 of 12`, `No matches`, or nothing before a search.
    #[must_use]
    pub fn found_label(&self) -> String {
        match (self.searched.is_empty(), self.found.len()) {
            (true, _) => String::new(),
            (false, 0) => "No matches".into(),
            (false, count) => format!("{} of {count}", self.found_index + 1),
        }
    }

    /// Lays the PDF at `path` over this one, to see what changed: what
    /// only this one has in red, what only the other has in blue.
    ///
    /// # Errors
    /// A message for the user when it cannot be read.
    pub fn compare_with(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
        let document = Document::open(bytes).map_err(|e| capitalize(&e.to_string()))?;
        let name = file_name(path);
        self.status =
            format!("Comparing with {name}: red is only in this drawing, blue only in {name}.");
        self.compare = Some((name, document));
        Ok(())
    }

    /// Stops comparing.
    pub fn stop_compare(&mut self) {
        if self.compare.take().is_some() {
            self.status = "Comparison closed.".into();
        }
    }

    /// The drawing laid over this one, if any.
    #[must_use]
    pub fn compared(&self) -> Option<&Document> {
        self.compare.as_ref().map(|(_, document)| document)
    }

    /// Writes a copy with every markup drawn into the page to `path`.
    ///
    /// # Errors
    /// A message for the user when it cannot be written.
    pub fn export_flattened(&mut self, path: &Path) -> Result<(), String> {
        let review = self.review.as_ref().ok_or("There is no PDF open")?;
        let bytes = review
            .flattened(self.units)
            .map_err(|e| capitalize(&e.to_string()))?;
        std::fs::write(path, bytes).map_err(|e| format!("Cannot save: {e}"))?;
        self.status = format!("Wrote {}, with the markups drawn in.", file_name(path));
        Ok(())
    }

    /// Starts a review session with the PDF open, hosted from here.
    ///
    /// # Errors
    /// A message for the user when there is no PDF or the computer cannot
    /// host.
    pub fn start_session(&mut self) -> Result<(), String> {
        let review = self.review.as_ref().ok_or("Open a PDF first")?;
        if self.session.is_some() {
            return Err("You are in a session already".into());
        }
        let mut shared = Shared::start(&self.name());
        shared.publish(review.markups(), review.scales());
        let pdf = Arc::new(review.document().bytes().to_vec());
        let host = Host::start(shared, pdf)?;
        self.status = "Hosting a session: send the invitation to who joins.".into();
        self.session = Some(Session {
            link: host.link,
            invite: Some(host.invite),
            seen: 0,
        });
        Ok(())
    }

    /// Joins the review session `invite` names, with its drawing.
    ///
    /// # Errors
    /// A message for the user when the invitation is wrong or the host
    /// cannot be reached.
    pub fn join_session(&mut self, invite: &str) -> Result<(), String> {
        let guest = Guest::join(invite, &self.name())?;
        let Guest { link, pdf } = guest;
        if let Err(message) = self.open_bytes(pdf, None) {
            link.stop();
            return Err(message);
        }
        self.session = Some(Session {
            link,
            invite: None,
            seen: 0,
        });
        self.pull();
        self.status = "Joined the session. Save as to keep a copy.".into();
        Ok(())
    }

    /// Leaves the review session; the markups stay as they are here.
    pub fn leave_session(&mut self) {
        if let Some(session) = self.session.take() {
            session.link.stop();
            self.status = "Left the session.".into();
        }
    }

    fn name(&self) -> String {
        if self.author.trim().is_empty() {
            "Someone".into()
        } else {
            self.author.trim().to_owned()
        }
    }

    /// Shares this window's markups with the session, if any.
    pub fn publish(&mut self) {
        let (Some(session), Some(review)) = (self.session.as_ref(), self.review.as_ref()) else {
            return;
        };
        if let Ok(mut shared) = session.link.shared.lock() {
            shared.publish(review.markups(), review.scales());
        }
    }

    /// Takes in what others changed in the session since last asked;
    /// whether anything did.
    pub fn pull(&mut self) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        let changed = session.link.changed.load(Ordering::SeqCst);
        if changed == session.seen && session.seen != 0 {
            return false;
        }
        session.seen = changed.max(1);
        let Ok(shared) = session.link.shared.lock() else {
            return false;
        };
        let (markups, scales) = (shared.markups(), shared.scales());
        drop(shared);
        let taken = self
            .review
            .as_mut()
            .is_some_and(|review| review.take_shared(markups, scales));
        if taken {
            self.forget_missing();
        }
        taken
    }

    /// Whether this window is in a session.
    #[must_use]
    pub fn in_session(&self) -> bool {
        self.session.is_some()
    }

    /// The invitation to send, when hosting.
    #[must_use]
    pub fn invite(&self) -> Option<&Invite> {
        self.session.as_ref()?.invite.as_ref()
    }

    /// The session panel's first line.
    #[must_use]
    pub fn session_state(&self) -> String {
        match &self.session {
            None => {
                "Not in a session. Start one with the PDF open, or paste an invitation to join one."
                    .into()
            }
            Some(session) if session.link.stopped() => {
                "The session ended: the host left or the connection closed.".into()
            }
            Some(session) => {
                let people = session.link.shared.lock().map_or(0, |s| s.people().len());
                let role = if session.invite.is_some() {
                    "Hosting"
                } else {
                    "In a session"
                };
                format!("{role} · {people} people")
            }
        }
    }

    /// Who is in the session.
    #[must_use]
    pub fn people(&self) -> Vec<String> {
        self.session
            .as_ref()
            .and_then(|s| s.link.shared.lock().ok().map(|s| s.people()))
            .unwrap_or_default()
    }

    /// What everyone did, newest first: `14:32 María: added a cloud on
    /// page 1`.
    #[must_use]
    pub fn activity(&self) -> Vec<String> {
        let Some(session) = self.session.as_ref() else {
            return Vec::new();
        };
        let Ok(mut shared) = session.link.shared.lock() else {
            return Vec::new();
        };
        shared
            .activity()
            .into_iter()
            .rev()
            .map(|a| {
                let time = chrono::DateTime::from_timestamp(a.when, 0)
                    .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                    .unwrap_or_default();
                format!("{time} {}: {}", a.who, a.what)
            })
            .collect()
    }

    /// Stops what is under way: a shape, a drag, a calibration, a
    /// selection.
    pub fn cancel(&mut self) {
        self.drag = None;
        self.building = None;
        self.calibrating = None;
        self.selected = None;
    }

    /// Undoes the last step.
    pub fn undo(&mut self) {
        if let Some(review) = self.review.as_mut() {
            review.undo();
        }
        self.forget_missing();
    }

    /// Redoes the last step undone.
    pub fn redo(&mut self) {
        if let Some(review) = self.review.as_mut() {
            review.redo();
        }
        self.forget_missing();
    }

    /// Whether there is something to undo, and to redo.
    #[must_use]
    pub fn history(&self) -> (bool, bool) {
        self.review
            .as_ref()
            .map_or((false, false), |r| (r.can_undo(), r.can_redo()))
    }

    fn forget_missing(&mut self) {
        if self.selected().is_none() {
            self.selected = None;
        }
    }

    fn page_scale(&self) -> Option<Scale> {
        self.review.as_ref()?.scale(self.page)
    }

    /// The page's scale, such as `Scale 1" = 48"`, or that there is none.
    #[must_use]
    pub fn scale_label(&self) -> String {
        match self.page_scale() {
            Some(scale) => format!("Scale {}", scale.label(self.units)),
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
        let Some(markup) = self.listed().get(row).copied() else {
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
        self.listed().iter().position(|m| m.id == id)
    }

    /// Changes the selected markup's text.
    pub fn set_text(&mut self, text: &str) {
        if let (Some(id), Some(review)) = (self.selected.clone(), self.review.as_mut()) {
            review.set_text(&id, text);
        }
    }

    /// Changes the selected markup's subject.
    pub fn set_subject(&mut self, subject: &str) {
        let subject = subject.trim().to_owned();
        self.change_selected(|m| {
            m.subject = if subject == m.kind.label() {
                String::new()
            } else {
                subject
            };
        });
    }

    /// Changes the selected markup's status.
    pub fn set_status(&mut self, status: usize) {
        if let Some(&status) = Status::ALL.get(status) {
            self.change_selected(|m| m.status = status);
        }
    }

    /// Deletes the selected markup.
    pub fn delete_selected(&mut self) {
        if let (Some(id), Some(review)) = (self.selected.take(), self.review.as_mut()) {
            review.remove(&id);
        }
    }

    /// The markup list: page, subject, status, author, and text or
    /// measurement.
    #[must_use]
    pub fn rows(&self) -> Vec<[String; 5]> {
        let Some(review) = self.review.as_ref() else {
            return Vec::new();
        };
        self.listed()
            .into_iter()
            .map(|m| {
                let text = m.text.lines().next().unwrap_or_default();
                let what = match review.measured(m, self.units) {
                    Some(value) if text.is_empty() => value,
                    Some(value) => format!("{value} · {text}"),
                    None => text.to_owned(),
                };
                [
                    (m.page + 1).to_string(),
                    m.subject().to_owned(),
                    m.status.label().to_owned(),
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
                stroke: markup.stroked().then_some(markup.color),
                fill: markup.filled().then_some((markup.color, FILL_OPACITY)),
                width: (markup.width * self.zoom).max(1.0),
                selected,
            });
            let value = review.measured_on_page(markup, self.units);
            for caption in markup.captions(value.as_deref()) {
                let size = caption.size * self.zoom;
                let (x, y) = self.to_screen(&page, caption.at);
                let first = caption.text.lines().next().unwrap_or_default();
                let left = if caption.centered {
                    x - text_width(first, caption.size) * self.zoom / 2.0
                } else {
                    x
                };
                labels.push(Label {
                    x: left,
                    y: y - size * 0.95,
                    text: caption.text,
                    color: markup.color,
                    size,
                });
            }
        };
        let mut outlines = Vec::new();
        for markup in review.markups().iter().filter(|m| m.page == self.page) {
            let selected = self.selected.as_deref() == Some(markup.id.as_str());
            if review.draws(markup) {
                draw(markup, selected);
            } else if selected {
                // Drawn on the page from its own look: only show it is
                // picked.
                outlines.push(Shape {
                    commands: self.commands(&page, &markup.outline()),
                    stroke: None,
                    fill: None,
                    width: 1.0,
                    selected: true,
                });
            }
        }
        if let Some(Drag::Draw { kind, points, .. }) = &self.drag
            && let Some(points) = drawn(*kind, points.clone(), false)
        {
            draw(&self.new_markup(*kind, points), false);
        }
        if let Some((kind, points, cursor)) = &self.building {
            let mut points = points.clone();
            if *kind != Kind::Count {
                points.extend(*cursor);
            }
            draw(&self.new_markup(*kind, points), false);
        }
        shapes.extend(outlines);
        // Where the search found the words on this page.
        for (index, found) in self.found.iter().enumerate() {
            if found.page != self.page {
                continue;
            }
            let [x0, y0, x1, y1] = found.rect;
            let current = index == self.found_index;
            shapes.push(Shape {
                commands: self.commands(
                    &page,
                    &[
                        Segment::Move(Point::new(x0, y0)),
                        Segment::Line(Point::new(x1, y0)),
                        Segment::Line(Point::new(x1, y1)),
                        Segment::Line(Point::new(x0, y1)),
                        Segment::Close,
                    ],
                ),
                stroke: current.then_some([0.95, 0.5, 0.0]),
                fill: Some(([1.0, 0.85, 0.0], 0.4)),
                width: 2.0,
                selected: false,
            });
        }
        let line = match &self.drag {
            Some(Drag::Calibrate { a, b }) => Some((*a, *b)),
            _ => self.calibrating,
        };
        if let Some((a, b)) = line {
            shapes.push(Shape {
                commands: self.commands(&page, &[Segment::Move(a), Segment::Line(b)]),
                stroke: Some([0.0, 0.45, 0.9]),
                fill: None,
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

/// A review session this window is in.
#[derive(Debug)]
struct Session {
    link: Link,
    /// The invitation, when this window hosts.
    invite: Option<Invite>,
    /// How many changes from others were taken in.
    seen: u64,
}

impl Drop for Viewer {
    fn drop(&mut self) {
        self.leave_session();
    }
}

/// The points of a markup of `kind` dragged through `points` (press, then
/// where the mouse is); `None` when a click makes nothing.
fn drawn(kind: Kind, mut points: Vec<Point>, click: bool) -> Option<Vec<Point>> {
    let a = *points.first()?;
    let b = *points.last()?;
    match kind {
        Kind::Text | Kind::Stamp if click => {
            let size = if kind == Kind::Stamp {
                stamps::SIZE
            } else {
                TEXT_BOX
            };
            Some(vec![
                Point::new(a.x, a.y - size.1),
                Point::new(a.x + size.0, a.y),
            ])
        }
        _ if click => None,
        Kind::Cloud => Some(vec![
            Point::new(a.x, a.y),
            Point::new(b.x, a.y),
            Point::new(b.x, b.y),
            Point::new(a.x, b.y),
        ]),
        Kind::Callout => {
            // The box starts where the mouse let go, away from the tip.
            let right = if b.x >= a.x { 1.0 } else { -1.0 };
            let far = Point::new(b.x + TEXT_BOX.0 * right, b.y - TEXT_BOX.1);
            Some(vec![a, b, far])
        }
        Kind::Pen => {
            points.dedup_by(|p, q| p.distance(*q) < 1e-9);
            Some(points)
        }
        _ => Some(vec![a, b]),
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

    fn click(viewer: &mut Viewer, at: (f64, f64)) {
        viewer.press(at, 0);
        viewer.release(at);
    }

    #[test]
    fn opening_fits_the_page_in_the_view() {
        let (mut viewer, dir) = viewer();
        viewer.fit();
        // 2592 × 1728 in 1000 × 700: the width decides, 0.96 × 1000/2592.
        assert!((viewer.zoom - 0.96 * 1000.0 / 2592.0).abs() < 1e-9);
        assert_eq!(viewer.page_label(), "Page 1 of 2");
        assert_eq!(viewer.title(), "sheets.pdf — OpenCabinet Review");
        viewer.fit_width();
        assert!((viewer.zoom - 0.98 * 1000.0 / 2592.0).abs() < 1e-9);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_drag_draws_a_cloud_where_the_mouse_went() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Cloud));
        drag(&mut viewer, (100.0, 100.0), (300.0, 200.0));
        let markup = viewer.selected().unwrap().clone();
        assert_eq!(markup.kind, Kind::Cloud);
        // Screen y down from the top; PDF y up from the bottom.
        assert_eq!(
            markup.points,
            [
                Point::new(100.0, 1628.0),
                Point::new(300.0, 1628.0),
                Point::new(300.0, 1528.0),
                Point::new(100.0, 1528.0),
            ]
        );
        assert_eq!(markup.author, viewer.author);
        assert!(viewer.title().starts_with("* "));
        // A click draws nothing.
        click(&mut viewer, (500.0, 500.0));
        assert_eq!(viewer.rows().len(), 1);
        let (shapes, _) = viewer.overlay();
        assert_eq!(shapes.len(), 1);
        assert!(shapes[0].commands.starts_with("M 100.0 100.0 C"));
        assert!(shapes[0].selected);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_markup_is_picked_moved_restyled_and_deleted() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        drag(&mut viewer, (100.0, 100.0), (200.0, 200.0));
        viewer.choose_tool(Tool::Select);
        viewer.cancel();
        drag(&mut viewer, (100.0, 150.0), (150.0, 160.0));
        // 50 right and 10 down on screen: 10 lower in PDF space, from the
        // corner first pressed.
        assert_eq!(
            viewer.selected().unwrap().points[0],
            Point::new(150.0, 1618.0)
        );
        viewer.set_text("Check with GC");
        viewer.choose_color(4);
        viewer.choose_width(2);
        viewer.set_fill(true);
        viewer.set_status(1);
        viewer.set_subject("Phase 2");
        let markup = viewer.selected().unwrap();
        assert_eq!(markup.color, COLORS[4].1);
        assert!((markup.width - 4.0).abs() < f64::EPSILON);
        assert!(markup.fill);
        let author = viewer.author.clone();
        assert_eq!(
            viewer.rows()[0],
            [
                "1".to_owned(),
                "Phase 2".into(),
                "Accepted".into(),
                author,
                "Check with GC".into()
            ]
        );
        let (shapes, _) = viewer.overlay();
        assert!(shapes[0].fill.is_some());
        // Six steps back: subject, status, fill, width, color, text; then
        // the move, which was one step however far it went.
        for _ in 0..7 {
            viewer.undo();
        }
        assert_eq!(
            viewer.selected().unwrap().points[0],
            Point::new(100.0, 1628.0)
        );
        viewer.delete_selected();
        assert_eq!(viewer.rows().len(), 0);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn shapes_are_drawn_point_by_point_and_finished() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Polygon));
        click(&mut viewer, (100.0, 100.0));
        click(&mut viewer, (300.0, 100.0));
        viewer.drag((300.0, 300.0));
        // While drawing, the shape follows the mouse.
        let (shapes, _) = viewer.overlay();
        assert!(shapes[0].commands.contains("L 300.0 300.0"));
        click(&mut viewer, (300.0, 300.0));
        // A double click: its second press adds the same point again.
        click(&mut viewer, (300.0, 300.0));
        viewer.double_click();
        assert!(!viewer.building());
        assert_eq!(viewer.selected().unwrap().points.len(), 3);

        // Two points are not a polygon: nothing is made.
        click(&mut viewer, (500.0, 100.0));
        click(&mut viewer, (600.0, 100.0));
        viewer.finish();
        assert_eq!(viewer.rows().len(), 1);

        viewer.choose_tool(Tool::Draw(Kind::Count));
        for x in [100.0, 150.0, 200.0, 250.0] {
            click(&mut viewer, (x, 500.0));
        }
        // Changing tools finishes the count.
        viewer.choose_tool(Tool::Select);
        assert_eq!(viewer.rows()[1][4], "4");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_calibrated_page_measures_lengths_and_areas() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Calibrate);
        // 216 points: 3" on the sheet.
        drag(&mut viewer, (100.0, 100.0), (316.0, 100.0));
        assert!(viewer.calibrating());
        assert_eq!(viewer.calibrate("tall"), Err("Not a length".into()));
        viewer.calibrate("144").unwrap();
        assert_eq!(viewer.scale_label(), "Scale 1/4\" = 1'-0\"");
        viewer.choose_tool(Tool::Draw(Kind::Length));
        drag(&mut viewer, (100.0, 300.0), (100.0, 408.0));
        assert_eq!(viewer.rows()[0][4], "6'-0\"");
        let (_, labels) = viewer.overlay();
        assert_eq!(labels[0].text, "6'-0\"");

        viewer.choose_tool(Tool::Draw(Kind::Area));
        for at in [(0.0, 0.0), (216.0, 0.0), (216.0, 180.0), (0.0, 180.0)] {
            click(&mut viewer, at);
        }
        viewer.finish();
        // 12' × 10' at 1" = 4': 120 square feet.
        assert_eq!(viewer.rows()[1][4], "120.00 sq ft");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn text_notes_callouts_and_stamps_are_placed() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Text));
        click(&mut viewer, (100.0, 100.0));
        viewer.set_text("Filler 3\"");
        let (low, high) = viewer.selected().unwrap().bounds();
        assert_eq!((high.x - low.x, high.y - low.y), TEXT_BOX);

        viewer.choose_tool(Tool::Draw(Kind::Note));
        viewer.press((400.0, 100.0), 0);
        viewer.release((400.0, 100.0));
        viewer.set_text("Ask about the filler");
        assert_eq!(viewer.selected().unwrap().kind, Kind::Note);

        viewer.choose_tool(Tool::Draw(Kind::Callout));
        drag(&mut viewer, (600.0, 300.0), (700.0, 200.0));
        assert_eq!(viewer.selected().unwrap().points.len(), 3);

        viewer.choose_stamp(3);
        click(&mut viewer, (100.0, 400.0));
        assert_eq!(viewer.selected().unwrap().text, "REJECTED");
        let (_, labels) = viewer.overlay();
        assert!(labels.iter().any(|l| l.text == "REJECTED"));
        assert_eq!(viewer.rows().len(), 4);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn markups_are_copied_pasted_and_duplicated() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Arrow));
        drag(&mut viewer, (100.0, 100.0), (300.0, 200.0));
        viewer.set_text("Here");
        viewer.copy();
        viewer.paste();
        viewer.paste();
        let rows = viewer.rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2][4], "Here");
        // Each paste lands beside the one before.
        let third = viewer.selected().unwrap().points[0];
        assert_eq!(third, Point::new(136.0, 1628.0 - 36.0));
        viewer.duplicate();
        assert_eq!(viewer.rows().len(), 4);
        // On another page, in the same place.
        viewer.go_to(1);
        viewer.paste();
        assert_eq!(viewer.selected().unwrap().page, 1);
        assert_eq!(viewer.selected().unwrap().points[0], third);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_common_scale_is_chosen_for_one_page_or_all() {
        let (mut viewer, dir) = viewer();
        let names = Viewer::scale_names();
        let quarter = names.iter().position(|n| n == "1/4\" = 1'-0\"").unwrap();
        viewer.choose_scale(quarter, false);
        assert_eq!(viewer.scale_label(), "Scale 1/4\" = 1'-0\"");
        viewer.go_to(1);
        assert_eq!(viewer.scale_label(), "Not to scale");
        viewer.go_to(0);
        viewer.scale_every_page();
        viewer.go_to(1);
        assert_eq!(viewer.scale_label(), "Scale 1/4\" = 1'-0\"");
        viewer.choose_tool(Tool::Draw(Kind::Length));
        // Back to 1 pixel a point; 18 points is a foot.
        viewer.zoom = 1.0;
        viewer.scroll = (0.0, 0.0);
        drag(&mut viewer, (100.0, 300.0), (100.0, 381.0));
        assert_eq!(viewer.rows()[0][4], "4'-6\"");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn zooming_to_a_markup_frames_it() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        drag(&mut viewer, (100.0, 100.0), (300.0, 200.0));
        viewer.zoom_to_selected();
        // 200 × 100 plus 36 around, in 1000 × 700: the width decides.
        assert!((viewer.zoom - 1000.0 / 272.0).abs() < 1e-9);
        let middle = viewer.to_pdf((500.0, 350.0)).unwrap();
        assert!(middle.distance(Point::new(200.0, 1578.0)) < 1e-6);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn escape_goes_back_to_the_select_tool() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Area));
        click(&mut viewer, (100.0, 100.0));
        viewer.escape();
        assert_eq!(viewer.tool(), Tool::Select);
        assert!(!viewer.building());
        assert_eq!(viewer.rows().len(), 0);
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        drag(&mut viewer, (100.0, 100.0), (200.0, 200.0));
        viewer.escape();
        assert_eq!(viewer.tool(), Tool::Select);
        assert!(viewer.selected().is_some());
        // With nothing under way, it lets go of the selection.
        viewer.escape();
        assert!(viewer.selected().is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_list_is_filtered_by_status_and_words() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        drag(&mut viewer, (100.0, 100.0), (200.0, 200.0));
        viewer.set_text("Filler");
        drag(&mut viewer, (300.0, 100.0), (400.0, 200.0));
        viewer.set_text("Crown");
        viewer.set_status(1);
        viewer.set_filter(Some(Status::Accepted), "");
        assert_eq!(viewer.rows().len(), 1);
        assert_eq!(viewer.list_label(), "1 of 2 markups");
        viewer.set_filter(None, "fill");
        assert_eq!(viewer.rows()[0][4], "Filler");
        viewer.select_row(0);
        assert_eq!(viewer.selected().unwrap().text, "Filler");
        viewer.set_filter(None, "");
        assert_eq!(viewer.list_label(), "2 markups");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn words_are_found_page_by_page() {
        let (mut viewer, dir) = viewer();
        viewer.find("a-20");
        assert_eq!(viewer.found_label(), "1 of 2");
        assert_eq!(viewer.page(), 0);
        let (shapes, _) = viewer.overlay();
        assert_eq!(shapes.len(), 1);
        viewer.find_next(false);
        assert_eq!(viewer.found_label(), "2 of 2");
        assert_eq!(viewer.page(), 1);
        viewer.find_next(false);
        assert_eq!(viewer.page(), 0);
        viewer.find("not there");
        assert_eq!(viewer.found_label(), "No matches");
        viewer.find("");
        assert_eq!(viewer.found_label(), "");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn another_drawing_is_laid_over_and_a_flat_copy_written() {
        let (mut viewer, dir) = viewer();
        let other = dir.join("other.pdf");
        std::fs::write(
            &other,
            opencabinet_pdf::sample::sheets(1, 2592.0, 1728.0, 6),
        )
        .unwrap();
        viewer.compare_with(&other).unwrap();
        assert_eq!(viewer.compared().unwrap().pages().len(), 1);
        viewer.stop_compare();
        assert!(viewer.compared().is_none());

        viewer.choose_tool(Tool::Draw(Kind::Cloud));
        drag(&mut viewer, (100.0, 100.0), (300.0, 200.0));
        let flat = dir.join("flat.pdf");
        viewer.export_flattened(&flat).unwrap();
        let bytes = std::fs::read(&flat).unwrap();
        assert_eq!(
            opencabinet_pdf::annotations::read(&bytes)
                .unwrap()
                .annotations
                .len(),
            0
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_session_is_hosted_joined_and_kept_in_step() {
        let (mut host, dir) = viewer();
        host.author = "Andrés".into();
        host.choose_tool(Tool::Draw(Kind::Cloud));
        drag(&mut host, (100.0, 100.0), (300.0, 200.0));
        host.start_session().unwrap();
        assert!(host.session_state().starts_with("Hosting"));
        // On the network address, which this test does not use: a guest
        // joins on this computer's loopback address instead.
        let mut invite = host.invite().unwrap().clone();
        invite
            .address
            .set_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

        let mut guest = Viewer::default();
        guest.author = "María".into();
        guest.resize(1000.0, 700.0);
        guest.join_session(&invite.to_string()).unwrap();
        assert_eq!(guest.rows().len(), 1);
        assert_eq!(guest.page_label(), "Page 1 of 2");

        guest.zoom = 1.0;
        guest.scroll = (0.0, 0.0);
        guest.choose_tool(Tool::Draw(Kind::Arrow));
        drag(&mut guest, (400.0, 400.0), (500.0, 500.0));
        guest.publish();
        let start = std::time::Instant::now();
        while !host.pull() {
            assert!(start.elapsed().as_secs() < 10, "the arrow did not arrive");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(host.rows().len(), 2);
        assert_eq!(host.rows()[1][3], "María");
        assert_eq!(host.people(), ["Andrés", "María"]);
        assert!(host.activity()[0].contains("María: added an arrow on page 1"));
        guest.leave_session();
        host.leave_session();
        assert!(!host.in_session());
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
        assert!(
            std::fs::read_to_string(&list)
                .unwrap()
                .contains(",Arrow,Arrow,Open,")
        );
        let mut again = Viewer::default();
        again.open(&dir.join("sheets.pdf")).unwrap();
        assert_eq!(again.rows().len(), 1);
        assert_eq!(again.rows()[0][1], "Arrow");
        std::fs::remove_dir_all(dir).ok();
    }
}
