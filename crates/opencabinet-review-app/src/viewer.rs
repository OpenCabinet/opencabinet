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
use opencabinet_review::{
    Color, FILL_OPACITY, Gesture, Kind, Markup, Review, Scale, Segment, Status, stamps, text_width,
};

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
        self.building = None;
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
        review
            .markups()
            .iter()
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
            let value = review.measured(markup, self.units);
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
        for markup in review.markups().iter().filter(|m| m.page == self.page) {
            draw(markup, self.selected.as_deref() == Some(markup.id.as_str()));
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
        assert_eq!(viewer.scale_label(), "Scale 1\" = 48\"");
        viewer.choose_tool(Tool::Draw(Kind::Length));
        drag(&mut viewer, (100.0, 300.0), (100.0, 408.0));
        assert_eq!(viewer.rows()[0][4], "72\"");
        let (_, labels) = viewer.overlay();
        assert_eq!(labels[0].text, "72\"");

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
