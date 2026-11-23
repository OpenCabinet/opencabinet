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
use opencabinet_pdf::labels::page_labels;
use opencabinet_pdf::links::{Link, Target, links};
use opencabinet_pdf::outline::{Bookmark, bookmarks};
use opencabinet_pdf::report::{self, Row, Table};
use opencabinet_pdf::snaps::{Snap, Snapped, Snaps};
use opencabinet_pdf::{Document, Page, Point, View};
use opencabinet_review::{
    Color, FILL_OPACITY, Gesture, Kind, Markup, Review, Scale, Segment, Status, pdf_date_text,
    presets, stamps, text_width,
};
use opencabinet_session::firewall::{self, Firewall};
use opencabinet_session::{Participant, Shared};

use crate::chest::{self, SavedTool, Settings};

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
pub const TOOLS: [Tool; 22] = [
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
    Tool::Draw(Kind::Angle),
    Tool::Draw(Kind::Perimeter),
    Tool::Draw(Kind::Link),
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

/// The markup list's columns, in order.
pub const COLUMNS: [&str; 7] = [
    "Subject", "Page", "Comments", "Author", "Date", "Status", "Color",
];

/// The stroke widths markups can take, in points.
pub const WIDTHS: [(&str, f64); 3] = [("Thin", 1.0), ("Medium", 2.0), ("Thick", 4.0)];

/// How close a click must be to pick a markup, in screen pixels.
const PICK: f64 = 6.0;
/// A grip's half side, in pixels.
const GRIP: f64 = 4.0;
/// How near the mouse a point snaps, in pixels.
const SNAP: f64 = 10.0;
/// A snap marker's half side, in pixels.
const MARK: f64 = 6.0;

/// The keys held when a mouse button goes down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    /// Control: adds a markup to the selection, or takes it out.
    pub control: bool,
    /// Shift: drags a box that selects the markups inside it.
    pub shift: bool,
}
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
        ids: Vec<String>,
        last: Point,
    },
    Grip {
        id: String,
        index: usize,
    },
    /// A box that selects what is inside it.
    Area {
        start: Point,
        end: Point,
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
    /// Selected with it, by Control or a box.
    also: Vec<String>,
    calibrating: Option<(Point, Point)>,
    /// The first end of a calibration line placed with a click.
    calibrate_from: Option<Point>,
    /// Snapping, and whether the wheel zooms.
    settings: Settings,
    /// The snap points of a page, read when first needed.
    snaps: Option<(usize, Snaps)>,
    /// What the mouse snaps to now.
    snapped: Option<Snapped>,
    /// Where the mouse is, after snapping, while drawing.
    cursor: Option<Point>,
    /// The keys held at the last mouse event.
    keys: Keys,

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
    /// The list's order: a column of [`COLUMNS`], and whether it goes
    /// down.
    sort: Option<(usize, bool)>,
    /// My tools, where they are kept, and the one in use.
    saved: Vec<SavedTool>,
    saved_path: Option<PathBuf>,
    style: Option<SavedTool>,
    /// The open PDF's bookmarks.
    bookmarks: Vec<Bookmark>,
    /// The pages chosen in the Pages panel, besides the one shown; empty
    /// when only the page shown is.
    chosen_pages: Vec<usize>,
    /// Quarter turns clockwise the pages are seen turned; the file does
    /// not change.
    turns: u8,
    /// The names the PDF gives its pages, if it gives any.
    labels: Vec<String>,
    /// The person's own stamps: their words and color.
    own_stamps: Vec<(String, Color)>,
    /// The links on a page, read when first needed.
    links: Option<(usize, Vec<Link>)>,
    /// A link followed that the window must open: another drawing or a
    /// web address.
    follow: Option<Follow>,
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
            also: Vec::new(),
            calibrating: None,
            calibrate_from: None,
            settings: Settings::default(),
            snaps: None,
            snapped: None,
            cursor: None,
            keys: Keys::default(),

            clipboard: None,
            filter_status: None,
            filter_text: String::new(),
            found: Vec::new(),
            found_index: 0,
            searched: String::new(),
            compare: None,
            session: None,
            sort: None,
            saved: Vec::new(),
            saved_path: None,
            style: None,
            bookmarks: Vec::new(),
            chosen_pages: Vec::new(),
            turns: 0,
            labels: Vec::new(),
            own_stamps: Vec::new(),
            links: None,
            follow: None,
            status: "Open a PDF to start.".into(),
        }
    }
}

impl Viewer {
    /// A viewer with nothing open, drawing as this one does: same author,
    /// units, tool, color, line, My tools and settings, for a new tab.
    #[must_use]
    pub fn new_tab(&self) -> Self {
        let mut tab = Self::default();
        self.share_preferences(&mut tab);
        tab.size = self.size;
        tab
    }

    /// Gives `other` this viewer's author, units, tool, color, line, My
    /// tools and settings, so they follow from tab to tab.
    pub fn share_preferences(&self, other: &mut Self) {
        other.author.clone_from(&self.author);
        other.units = self.units;
        other.color = self.color;
        other.width = self.width;
        other.fill = self.fill;
        other.stamp = self.stamp;
        other.own_stamps.clone_from(&self.own_stamps);
        other.settings = self.settings;
        other.saved.clone_from(&self.saved);
        other.saved_path.clone_from(&self.saved_path);
        other.style.clone_from(&self.style);
        other.clipboard.clone_from(&self.clipboard);
        if other.size != self.size && self.size != (0.0, 0.0) {
            other.size = self.size;
            other.fit();
        }
        if other.drag.is_none() && other.building.is_none() && other.calibrating.is_none() {
            other.tool = self.tool;
        }
    }

    /// The name of the tab: the file's name, or "New".
    #[must_use]
    pub fn tab_title(&self) -> String {
        self.path
            .as_deref()
            .map_or_else(|| "New".to_owned(), file_name)
    }

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
        // A drawing whose bookmarks cannot be read still opens.
        self.bookmarks = bookmarks(review.document().bytes()).unwrap_or_default();
        self.labels = page_labels(review.document().bytes()).unwrap_or_default();
        self.status = format!(
            "{} pages, {} markups.",
            review.document().pages().len(),
            review.markups().len()
        );
        self.review = Some(review);
        self.snaps = None;
        self.path = path.map(Path::to_owned);
        self.page = 0;
        self.selected = None;
        self.calibrating = None;
        self.drag = None;
        self.building = None;
        self.found.clear();
        self.searched.clear();
        self.compare = None;
        self.chosen_pages.clear();
        self.turns = 0;
        self.links = None;
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

    /// What the measuring markups add up to: for each kind and subject,
    /// what it is, its total, and how many markups on which pages.
    #[must_use]
    pub fn quantities(&self) -> Vec<[String; 3]> {
        self.review
            .as_ref()
            .map(|r| r.quantities(self.units))
            .unwrap_or_default()
            .into_iter()
            .map(|q| {
                let what = if q.subject == q.kind.label() {
                    q.subject
                } else {
                    format!("{} · {}", q.subject, q.kind.label())
                };
                let total = if q.total.is_empty() {
                    String::new()
                } else {
                    q.total
                };
                let mut detail = format!(
                    "{} {} on page{} {}",
                    q.items,
                    if q.items == 1 { "markup" } else { "markups" },
                    if q.pages.contains(',') { "s" } else { "" },
                    q.pages
                );
                if q.unscaled > 0 {
                    let _ = write!(detail, "; {} not to scale, left out", q.unscaled);
                }
                [what, total, detail]
            })
            .collect()
    }

    /// Writes the quantities for spreadsheets to `path`.
    ///
    /// # Errors
    /// A message for the user when it cannot be written.
    pub fn export_quantities(&mut self, path: &Path) -> Result<(), String> {
        let review = self.review.as_ref().ok_or("There is no PDF open")?;
        std::fs::write(path, review.quantities_csv(self.units))
            .map_err(|e| format!("Cannot save: {e}"))?;
        self.status = format!("Wrote the quantities to {}.", file_name(path));
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

    /// Says `text` in the line at the bottom of the window.
    pub fn say_status(&mut self, text: &str) {
        text.clone_into(&mut self.status);
    }

    /// The line at the bottom of the window.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The page shown, as seen: turned when the view is.
    fn page_size(&self) -> Option<Page> {
        self.review
            .as_ref()?
            .document()
            .pages()
            .get(self.page)
            .map(|page| page.turned(self.turns))
    }

    /// Turns the view a quarter turn, clockwise when `turn` is positive;
    /// the file does not change. Markups, snapping and measuring turn
    /// with it.
    pub fn turn_view(&mut self, turn: i32) {
        if self.review.is_none() {
            return;
        }
        let turns = (i32::from(self.turns) + turn).rem_euclid(4);
        self.turns = u8::try_from(turns).unwrap_or(0);
        self.drag = None;
        self.building = None;
        self.snapped = None;
        self.fit();
        self.status = match self.turns {
            0 => "The view is upright again.".into(),
            n => format!(
                "The view is turned {}° clockwise. The file does not change; Turn page in the Pages menu does that.",
                u32::from(n) * 90
            ),
        };
    }

    /// How far the view is turned, as `90°`, or nothing when upright.
    #[must_use]
    pub fn turn_label(&self) -> String {
        match self.turns {
            0 => String::new(),
            n => format!("View {}°", u32::from(n) * 90),
        }
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
            let place = format!("Page {} of {}", self.page + 1, r.document().pages().len());
            match self.labels.get(self.page) {
                Some(label) if *label != (self.page + 1).to_string() && !label.is_empty() => {
                    format!("{label} · {place}")
                }
                _ => place,
            }
        })
    }

    /// Each page's name for the thumbnails: the PDF's label, or its
    /// number.
    #[must_use]
    pub fn page_names(&self) -> Vec<String> {
        let count = self
            .review
            .as_ref()
            .map_or(0, |r| r.document().pages().len());
        (0..count)
            .map(|p| {
                self.labels
                    .get(p)
                    .filter(|l| !l.is_empty())
                    .cloned()
                    .unwrap_or_else(|| (p + 1).to_string())
            })
            .collect()
    }

    /// Shows page `page`, fit to the window.
    pub fn go_to(&mut self, page: usize) {
        let count = self
            .review
            .as_ref()
            .map_or(0, |r| r.document().pages().len());
        if page < count && page != self.page {
            if !self.chosen_pages.contains(&page) {
                self.chosen_pages.clear();
            }
            self.page = page;
            self.snapped = None;
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
        // In one full page, the wheel zooms and Ctrl+wheel moves.
        if zoom == self.settings.wheel_zooms {
            self.scroll.0 -= delta.0;
            self.scroll.1 -= delta.1;
            self.clamp_scroll();
        } else {
            self.zoom_by(1.2_f64.powf(delta.1 / 120.0), at);
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
            turn: self.turns,
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
        self.style = None;
        self.tool = tool;
        self.drag = None;
        self.calibrate_from = None;
        self.snapped = None;
        self.status = match tool {
            Tool::Select => {
                "Click a markup to select it, Ctrl+click for several, Shift+drag for a box; drag to move. Ctrl+wheel zooms.".into()
            }
            Tool::Draw(Kind::Pen) => "Drag to draw.".into(),
            Tool::Draw(Kind::Text) => {
                "Drag a box, or click, then type its text on the right.".into()
            }
            Tool::Draw(Kind::Callout) => {
                "Click where it points, then where its text goes (or drag).".into()
            }
            Tool::Draw(Kind::Note) => {
                "Click where the note goes, then type it on the right.".into()
            }
            Tool::Draw(Kind::Stamp) => "Click to stamp, or drag the stamp's box.".into(),
            Tool::Draw(Kind::Count) => {
                "Click each item to count; Enter or double click to finish.".into()
            }
            Tool::Draw(Kind::Link) => "Drag the link's box, then type where it goes on the right: a page number, a drawing such as A-301.pdf, or a web address. Double-click a link to follow it.".into(),
            Tool::Draw(Kind::Angle) => {
                "Click a point on one side, the corner, then a point on the other side.".into()
            }
            Tool::Draw(kind) if kind.measures() && self.page_scale().is_none() => {
                "This page has no scale yet: calibrate it to read measurements.".into()
            }
            Tool::Draw(kind) if kind.gesture() == Gesture::Clicks => format!(
                "Click each point of the {}; Enter or double click to finish.",
                kind.label().to_lowercase()
            ),
            Tool::Draw(Kind::Length) => {
                "Click one end, then the other (or drag); Shift keeps it straight.".into()
            }
            Tool::Draw(kind) => format!(
                "Click one corner of the {}, then the other (or drag); Shift keeps it straight or square.",
                kind.label().to_lowercase()
            ),
            Tool::Calibrate => {
                "Click both ends of a dimension you know (or drag), then type its length.".into()
            }
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
        if let Some(style) = self.style.as_mut() {
            style.color = rgb;
        }
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
        if let Some(style) = self.style.as_mut() {
            style.width = points;
        }
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
        if let Some(style) = self.style.as_mut() {
            style.fill = fill;
        }
        self.change_selected(|m| m.fill = fill);
    }

    /// The stamp placed next, by its place in [`stamps::TEMPLATES`].
    #[must_use]
    pub fn stamp(&self) -> usize {
        self.stamp
    }

    /// Every stamp offered: Review's, then the person's own.
    #[must_use]
    pub fn stamp_names(&self) -> Vec<String> {
        stamps::TEMPLATES
            .iter()
            .map(|t| t.words.to_owned())
            .chain(self.own_stamps.iter().map(|(words, _)| words.clone()))
            .collect()
    }

    /// How many of the stamps offered are Review's own.
    #[must_use]
    pub fn built_in_stamps() -> usize {
        stamps::TEMPLATES.len()
    }

    /// The stamp placed next, its `{name}` and `{date}` filled in.
    fn stamp_template(&self) -> OwnStamp {
        let built_in = |i: usize| {
            let t = stamps::TEMPLATES[i.min(stamps::TEMPLATES.len() - 1)];
            (t.words.to_owned(), t.color)
        };
        let (words, color) = if let Some(own) = self.stamp.checked_sub(stamps::TEMPLATES.len()) {
            self.own_stamps
                .get(own)
                .cloned()
                .unwrap_or_else(|| built_in(0))
        } else {
            built_in(self.stamp)
        };
        let date = chrono::Local::now().date_naive();
        let date = match self.units {
            Units::Imperial => date.format("%m/%d/%Y").to_string(),
            Units::Metric => date.format("%Y-%m-%d").to_string(),
        };
        OwnStamp {
            words: stamps::fill_in(&words, &self.name(), &date),
            color,
        }
    }

    /// Adds a stamp of the person's own with `words`, in the color in use,
    /// and picks it.
    ///
    /// # Errors
    /// When the words are empty, or the stamps cannot be kept.
    pub fn add_stamp(&mut self, words: &str) -> Result<(), String> {
        let words: String = words.trim().chars().take(stamps::MAX_WORDS).collect();
        if words.is_empty() {
            return Err("Write the stamp's words first".into());
        }
        let color = COLORS.get(self.color).map_or([0.8, 0.1, 0.1], |&(_, c)| c);
        self.own_stamps.push((words.clone(), color));
        self.stamp = stamps::TEMPLATES.len() + self.own_stamps.len() - 1;
        self.keep_stamps()?;
        self.status = format!("Added the stamp {words}.");
        Ok(())
    }

    /// Removes stamp `index` of those offered, when it is the person's own.
    ///
    /// # Errors
    /// When the stamps cannot be kept.
    pub fn remove_stamp(&mut self, index: usize) -> Result<(), String> {
        let Some(own) = index.checked_sub(stamps::TEMPLATES.len()) else {
            return Ok(());
        };
        if own < self.own_stamps.len() {
            self.own_stamps.remove(own);
            if self.stamp >= index {
                self.stamp = self.stamp.saturating_sub(1);
            }
            self.keep_stamps()?;
        }
        Ok(())
    }

    fn keep_stamps(&self) -> Result<(), String> {
        match &self.saved_path {
            Some(path) => chest::store_stamps(path, &self.own_stamps)
                .map_err(|e| format!("Cannot keep the stamps: {e}")),
            None => Ok(()),
        }
    }

    /// Picks the stamp placed next.
    pub fn choose_stamp(&mut self, stamp: usize) {
        if stamp < stamps::TEMPLATES.len() + self.own_stamps.len() {
            self.stamp = stamp;
            self.choose_tool(Tool::Draw(Kind::Stamp));
        }
    }

    /// Changes every selected markup through `change`, as one step.
    fn change_selected(&mut self, change: impl Fn(&mut Markup)) {
        let ids = self.chosen();
        let Some(review) = self.review.as_mut() else {
            return;
        };
        if ids.is_empty() {
            return;
        }
        review.checkpoint();
        for id in &ids {
            review.change_quietly(id, &change);
        }
    }

    /// Every selected markup's id: the one shown on the right first.
    #[must_use]
    pub fn chosen(&self) -> Vec<String> {
        let Some(first) = self.selected.clone() else {
            return Vec::new();
        };
        let exists = |id: &String| self.review.as_ref().is_some_and(|r| r.markup(id).is_some());
        let mut ids = vec![first];
        for id in &self.also {
            if !ids.contains(id) && exists(id) {
                ids.push(id.clone());
            }
        }
        ids
    }

    /// Selects only the markup `id`, or nothing.
    fn select_only(&mut self, id: Option<String>) {
        self.selected = id;
        self.also.clear();
    }

    /// Adds `id` to the selection, or takes it out when it is in.
    fn toggle_selected(&mut self, id: String) {
        let mut ids = self.chosen();
        if let Some(at) = ids.iter().position(|i| *i == id) {
            ids.remove(at);
        } else {
            ids.push(id);
        }
        let mut ids = ids.into_iter();
        self.selected = ids.next();
        self.also = ids.collect();
    }

    /// The grip of the selected markup at `point`, if it is the only one
    /// selected.
    fn grip_at(&self, point: Point) -> Option<(String, usize)> {
        if !self.also.is_empty() {
            return None;
        }
        let markup = self.selected()?;
        if markup.page != self.page {
            return None;
        }
        let reach = (GRIP + 2.0) / self.zoom;
        markup
            .grips()
            .iter()
            .position(|g| (g.x - point.x).abs() <= reach && (g.y - point.y).abs() <= reach)
            .map(|index| (markup.id.clone(), index))
    }

    /// A new markup of `kind` through `points`, in the style in use.
    fn new_markup(&self, kind: Kind, points: Vec<Point>) -> Markup {
        let template = self.stamp_template();
        if let Some(style) = self.style.as_ref().filter(|s| s.kind == kind) {
            return Markup {
                page: self.page,
                kind,
                points,
                color: style.color,
                width: style.width,
                fill: style.fill && kind.fillable(),
                subject: style.subject.clone(),
                text: if kind == Kind::Stamp {
                    template.words.clone()
                } else {
                    String::new()
                },
                ..Markup::default()
            };
        }
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
                template.words
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
        self.press_with(at, button, Keys::default());
    }

    /// A mouse button went down with `keys` held.
    pub fn press_with(&mut self, at: (f64, f64), button: u8, keys: Keys) {
        self.keys = keys;
        let Some(raw) = self.to_pdf(at) else {
            return;
        };
        let point = if button == 0 {
            self.aim(raw, keys)
        } else {
            raw
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
        if let Some((kind, points, _)) = &mut self.building {
            points.push(point);
            let (kind, full) = (
                *kind,
                kind.most_points().is_some_and(|most| points.len() >= most),
            );
            // A shape placed by two clicks is done at the second.
            if kind.gesture() == Gesture::Drag
                && let Some((kind, points, _)) = self.building.take()
            {
                self.place_dragged(kind, &points);
            } else if full {
                // An angle is done at its third point.
                self.finish();
            }
            return;
        }
        if self.tool == Tool::Calibrate
            && let Some(a) = self.calibrate_from.take()
        {
            if a.distance(point) * self.zoom >= CLICK {
                self.calibrating = Some((a, point));
                self.status = "Type the real length of the line on the right.".into();
            }
            return;
        }
        let point = if self.tool == Tool::Select {
            raw
        } else {
            point
        };
        self.drag = Some(match self.tool {
            Tool::Select => match self.press_select(at, point, keys) {
                Some(drag) => drag,
                None => return,
            },
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

    /// A press with the select tool: a grip, a markup, or empty paper.
    fn press_select(&mut self, at: (f64, f64), point: Point, keys: Keys) -> Option<Drag> {
        if !keys.control
            && !keys.shift
            && let Some((id, index)) = self.grip_at(point)
        {
            if let Some(review) = self.review.as_mut() {
                review.checkpoint();
            }
            return Some(Drag::Grip { id, index });
        }
        let tolerance = PICK / self.zoom;
        let hit = self
            .review
            .as_ref()
            .and_then(|r| r.hit(self.page, point, tolerance))
            .map(|m| m.id.clone());
        match hit {
            Some(id) if keys.control => {
                self.toggle_selected(id);
                None
            }
            Some(id) => {
                if !self.chosen().contains(&id) {
                    self.select_only(Some(id));
                }
                let ids: Vec<String> = self
                    .chosen()
                    .into_iter()
                    .filter(|i| {
                        self.review
                            .as_ref()
                            .and_then(|r| r.markup(i))
                            .is_some_and(|m| m.page == self.page)
                    })
                    .collect();
                if let Some(review) = self.review.as_mut() {
                    review.checkpoint();
                }
                Some(Drag::Move { ids, last: point })
            }
            None if keys.shift || keys.control => {
                if !keys.control {
                    self.select_only(None);
                }
                Some(Drag::Area {
                    start: point,
                    end: point,
                })
            }
            None => {
                self.select_only(None);
                // A link on the page goes where it says.
                if let Some(target) = self.link_at(point) {
                    self.follow_link(target);
                    return None;
                }
                Some(Drag::Pan { last: at })
            }
        }
    }

    /// Where the link on the page at `point` goes, if one is there.
    fn link_at(&mut self, point: Point) -> Option<Target> {
        let page = self.page;
        if self.links.as_ref().is_none_or(|(p, _)| *p != page) {
            let bytes = self.review.as_ref()?.document().bytes().to_vec();
            self.links = Some((page, links(&bytes, page).unwrap_or_default()));
        }
        let (_, on_page) = self.links.as_ref()?;
        on_page
            .iter()
            .find(|l| {
                let [x0, y0, x1, y1] = l.rect;
                (x0..=x1).contains(&point.x) && (y0..=y1).contains(&point.y)
            })
            .map(|l| l.target.clone())
    }

    /// Goes where `target` says: a page here, or, through the window,
    /// another drawing or a web address.
    pub fn follow_link(&mut self, target: Target) {
        match target {
            Target::Page(page) => {
                let count = self
                    .review
                    .as_ref()
                    .map_or(0, |r| r.document().pages().len());
                if page < count {
                    self.go_to(page);
                    self.status = format!("Went to page {}.", page + 1);
                } else {
                    self.status = format!("This PDF has no page {}.", page + 1);
                }
            }
            Target::File(name, page) => {
                let path = Path::new(&name);
                let path = match self.path.as_deref().and_then(Path::parent) {
                    Some(folder) if path.is_relative() => folder.join(path),
                    _ => path.to_owned(),
                };
                self.status = format!("Opening {}…", file_name(&path));
                self.follow = Some(Follow::File(path, page));
            }
            Target::Web(address) => {
                self.status = format!("Opening {address} in the web browser…");
                self.follow = Some(Follow::Web(address));
            }
        }
    }

    /// The drawing or web address a link asks the window to open, once.
    pub fn take_follow(&mut self) -> Option<Follow> {
        self.follow.take()
    }

    /// The mouse moved, a button held or not.
    pub fn drag(&mut self, at: (f64, f64)) {
        self.pointer_moved(at, self.keys);
    }

    /// The mouse moved with `keys` held: Shift keeps a line at a multiple
    /// of 45° or a box square.
    pub fn pointer_moved(&mut self, at: (f64, f64), keys: Keys) {
        self.keys = keys;
        let Some(raw) = self.to_pdf(at) else {
            return;
        };
        let point = self.aim(raw, keys);
        self.cursor = Some(point);
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
            Some(Drag::Move { ids, last }) => {
                let (dx, dy) = (raw.x - last.x, raw.y - last.y);
                *last = raw;
                let ids = ids.clone();
                if let Some(review) = self.review.as_mut() {
                    for id in &ids {
                        review.change_quietly(id, |m| m.shift(dx, dy));
                    }
                }
            }
            Some(Drag::Grip { id, index }) => {
                let (id, index) = (id.clone(), *index);
                if let Some(review) = self.review.as_mut() {
                    review.change_quietly(&id, |m| m.move_grip(index, point));
                }
            }
            Some(Drag::Area { end, .. }) => *end = raw,
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
                if click && !matches!(kind, Kind::Text | Kind::Stamp | Kind::Pen) {
                    // A click places the first point; the next click the
                    // other.
                    self.building = Some((kind, vec![points[0]], self.cursor));
                    self.status =
                        "Click where it ends. Shift keeps it straight or square; Esc stops.".into();
                } else if let Some(points) = drawn(kind, points, click) {
                    self.add(self.new_markup(kind, points));
                    if matches!(kind, Kind::Text | Kind::Callout) {
                        self.status = "Type the text on the right.".into();
                    }
                }
            }
            Some(Drag::Area { start, end }) => {
                let (low, high) = (
                    Point::new(start.x.min(end.x), start.y.min(end.y)),
                    Point::new(start.x.max(end.x), start.y.max(end.y)),
                );
                let inside: Vec<String> = self
                    .review
                    .as_ref()
                    .map(|r| {
                        r.markups()
                            .iter()
                            .filter(|m| m.page == self.page)
                            .filter(|m| {
                                let (a, b) = m.bounds();
                                a.x >= low.x && a.y >= low.y && b.x <= high.x && b.y <= high.y
                            })
                            .map(|m| m.id.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                let mut ids = self.chosen();
                for id in inside {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                let mut ids = ids.into_iter();
                self.selected = ids.next();
                self.also = ids.collect();
                let count = self.chosen().len();
                if count > 1 {
                    self.status = format!("{count} markups selected.");
                }
            }
            Some(Drag::Calibrate { a, b }) if a.distance(b) * self.zoom >= CLICK => {
                self.calibrating = Some((a, b));
                self.status = "Type the real length of the line on the right.".into();
            }
            Some(Drag::Calibrate { a, .. }) => {
                self.calibrate_from = Some(a);
                self.status = "Click the other end of the known dimension.".into();
            }
            _ => {}
        }
    }

    /// A double click: finishes a shape drawn point by point.
    pub fn double_click(&mut self) {
        if self.building.is_none()
            && let Some(target) = self.selected().and_then(Markup::link)
        {
            self.follow_link(target);
            return;
        }
        self.finish();
    }

    /// Finishes a shape drawn point by point, if it has enough points.
    pub fn finish(&mut self) {
        let Some((kind, mut points, _)) = self.building.take() else {
            return;
        };
        if kind.gesture() == Gesture::Drag {
            // Placed by clicks: done only by its second click.
            if points.len() >= 2 {
                self.place_dragged(kind, &points);
            }
            return;
        }
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

    /// Adds a markup of a dragged kind from its first and last points.
    fn place_dragged(&mut self, kind: Kind, points: &[Point]) {
        let (Some(&a), Some(&b)) = (points.first(), points.last()) else {
            return;
        };
        if a.distance(b) * self.zoom < CLICK {
            return;
        }
        if let Some(points) = drawn(kind, vec![a, b], false) {
            self.add(self.new_markup(kind, points));
            if kind == Kind::Callout {
                self.status = "Type the text on the right.".into();
            }
        }
    }

    /// Whether the point under the mouse snaps: while drawing, measuring,
    /// calibrating or dragging a grip; never for a pen stroke.
    fn aims(&self) -> bool {
        matches!(self.drag, Some(Drag::Grip { .. }))
            || matches!(self.tool, Tool::Draw(kind) if kind != Kind::Pen)
            || self.tool == Tool::Calibrate
    }

    /// The point a line or box being drawn starts from, and whether it
    /// is a box.
    fn anchor(&self) -> Option<(Point, bool)> {
        if let Some((kind, points, _)) = &self.building {
            return Some((*points.last()?, kind.boxed()));
        }
        match &self.drag {
            Some(Drag::Draw { kind, points, .. }) if *kind != Kind::Pen => {
                Some((*points.first()?, kind.boxed()))
            }
            Some(Drag::Calibrate { a, .. }) => Some((*a, false)),
            _ => self.calibrate_from.map(|a| (a, false)),
        }
    }

    /// Where `raw` lands: kept straight or square with Shift, else
    /// snapped to the drawing or a markup when one is near.
    fn aim(&mut self, raw: Point, keys: Keys) -> Point {
        self.snapped = None;
        if !self.aims() {
            return raw;
        }
        if keys.shift
            && let Some((anchor, square)) = self.anchor()
        {
            return straighten(anchor, raw, square);
        }
        if !self.settings.snap {
            return raw;
        }
        let reach = SNAP / self.zoom;
        let rank = |kind: Snap| match kind {
            Snap::End | Snap::Intersection => 0,
            Snap::Middle => 1,
            Snap::OnLine => 2,
        };
        let mut best = self.page_snaps().and_then(|s| s.nearest(raw, reach));
        // The points of the markups on the page, except the one whose grip
        // is dragged.
        let dragged = match &self.drag {
            Some(Drag::Grip { id, .. }) => Some(id.clone()),
            _ => None,
        };
        if let Some(review) = self.review.as_ref() {
            for markup in review.markups() {
                if markup.page != self.page || Some(&markup.id) == dragged.as_ref() {
                    continue;
                }
                for p in markup.grips() {
                    let distance = p.distance(raw);
                    let better =
                        best.is_none_or(|b| (0, distance) < (rank(b.kind), b.at.distance(raw)));
                    if distance <= reach && better {
                        best = Some(Snapped {
                            at: p,
                            kind: Snap::End,
                        });
                    }
                }
            }
        }
        self.snapped = best;
        best.map_or(raw, |s| s.at)
    }

    /// The snap points of the page shown, read once.
    fn page_snaps(&mut self) -> Option<&Snaps> {
        let fresh = self
            .snaps
            .as_ref()
            .is_none_or(|(page, _)| *page != self.page);
        if fresh {
            let snaps = Snaps::of(self.review.as_ref()?.document(), self.page).unwrap_or_default();
            self.snaps = Some((self.page, snaps));
        }
        self.snaps.as_ref().map(|(_, s)| s)
    }

    /// Whether a mouse button is held down on the drawing.
    #[must_use]
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Whether moving the mouse without a button changes what is shown:
    /// a snap mark, or a shape being placed point by point.
    #[must_use]
    pub fn tracks_hover(&self) -> bool {
        self.aims() || self.building.is_some() || self.calibrate_from.is_some()
    }

    /// Whether points snap.
    #[must_use]
    pub fn snapping(&self) -> bool {
        self.settings.snap
    }

    /// Snaps points or not, and keeps the choice.
    pub fn set_snap(&mut self, on: bool) {
        self.settings.snap = on;
        self.snapped = None;
        self.keep_settings();
        self.status = if on {
            "Points snap to the drawing's ends, middles, crossings and lines.".into()
        } else {
            "Points no longer snap.".into()
        };
    }

    /// What the mouse snaps to, for the status bar.
    #[must_use]
    pub fn snap_label(&self) -> String {
        self.snapped
            .map(|s| s.kind.label().to_owned())
            .unwrap_or_default()
    }

    /// Whether the wheel zooms without Ctrl.
    #[must_use]
    pub fn wheel_zooms(&self) -> bool {
        self.settings.wheel_zooms
    }

    /// One full page: the page fits, and the wheel zooms without Ctrl;
    /// or back to the wheel moving the page. The choice is kept.
    pub fn set_wheel_zooms(&mut self, on: bool) {
        self.settings.wheel_zooms = on;
        if on {
            self.fit();
            self.status = "One full page: the wheel zooms, Ctrl+wheel moves the page.".into();
        } else {
            self.status = "The wheel moves the page; Ctrl+wheel zooms.".into();
        }
        self.keep_settings();
    }

    fn keep_settings(&self) {
        if let Some(path) = self.saved_path.as_ref() {
            // Not being able to keep a preference is no reason to stop.
            let _ = chest::store_settings(path, self.settings);
        }
    }

    /// Whether a shape of many points is being drawn, which Enter or a
    /// double click finishes; not a line or box placed by two clicks.
    #[must_use]
    pub fn building_by_points(&self) -> bool {
        self.building
            .as_ref()
            .is_some_and(|(kind, _, _)| kind.gesture() == Gesture::Clicks)
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
        // A copy starts its own conversation.
        markup.replies.clear();
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
        let busy = busy || self.calibrate_from.is_some();
        self.drag = None;
        self.building = None;
        self.calibrating = None;
        self.calibrate_from = None;
        self.snapped = None;
        if busy {
            self.choose_tool(Tool::Select);
        } else {
            self.select_only(None);
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
        let mut listed: Vec<&Markup> = review
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
            .collect();
        if let Some((column, down)) = self.sort {
            listed.sort_by_cached_key(|m| match column {
                1 => (m.page, String::new()),
                4 => (0, m.modified.clone()),
                _ => (0, self.cell(review, m, column).to_lowercase()),
            });
            if down {
                listed.reverse();
            }
        }
        listed
    }

    /// Orders the list by `column` of [`COLUMNS`], down or up; `None`
    /// keeps the order markups were made in.
    pub fn sort_by(&mut self, sort: Option<(usize, bool)>) {
        self.sort = sort.filter(|(column, _)| *column < COLUMNS.len());
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

    /// Which document is open: it changes when the pages do, so the
    /// thumbnails are drawn again.
    #[must_use]
    pub fn document_key(&self) -> usize {
        self.review
            .as_ref()
            .map_or(0, |r| r.document().bytes().as_ptr() as usize)
    }

    /// The pages the page tools act on, first to last: those chosen in
    /// the Pages panel, or the one shown.
    #[must_use]
    pub fn pages_chosen(&self) -> Vec<usize> {
        if self.chosen_pages.is_empty() {
            self.review
                .as_ref()
                .map_or_else(Vec::new, |_| vec![self.page])
        } else {
            self.chosen_pages.clone()
        }
    }

    /// `Page 3` or `3 pages chosen`, for the Pages panel.
    #[must_use]
    pub fn pages_label(&self) -> String {
        match self.pages_chosen().as_slice() {
            [] => String::new(),
            [one] => format!("Page {} chosen", one + 1),
            many => format!("{} pages chosen", many.len()),
        }
    }

    /// A thumbnail clicked: alone it shows its page; with Control it is
    /// added to or taken from the pages chosen; with Shift every page from
    /// the one shown to it is chosen.
    pub fn click_page(&mut self, page: usize, keys: Keys) {
        let count = self
            .review
            .as_ref()
            .map_or(0, |r| r.document().pages().len());
        if page >= count {
            return;
        }
        if keys.control {
            let mut chosen = self.pages_chosen();
            if let Some(at) = chosen.iter().position(|&p| p == page) {
                if chosen.len() > 1 {
                    chosen.remove(at);
                }
            } else {
                chosen.push(page);
                chosen.sort_unstable();
            }
            self.chosen_pages = if chosen.len() > 1 { chosen } else { Vec::new() };
        } else if keys.shift {
            let (from, to) = (self.page.min(page), self.page.max(page));
            self.chosen_pages = if from == to {
                Vec::new()
            } else {
                (from..=to).collect()
            };
        } else {
            self.chosen_pages.clear();
            self.go_to(page);
        }
    }

    /// Whether the page tools can be used: not in a session, where the
    /// others' copies would no longer match.
    #[must_use]
    pub fn can_change_pages(&self) -> bool {
        self.review.is_some() && self.session.is_none()
    }

    fn change_pages(
        &mut self,
        change: impl FnOnce(
            &mut Review,
            &[usize],
            Units,
        ) -> Result<(usize, Vec<usize>), opencabinet_pdf::Error>,
    ) -> Result<(), String> {
        if self.session.is_some() {
            return Err("Leave the session to change pages: the others would no longer see the same drawing.".into());
        }
        let chosen = self.pages_chosen();
        let units = self.units;
        let review = self.review.as_mut().ok_or("There is no PDF open")?;
        let (page, chosen) =
            change(review, &chosen, units).map_err(|e| capitalize(&e.to_string()))?;
        self.pages_changed(page, chosen);
        Ok(())
    }

    /// After the pages changed: shows `page`, with `chosen` chosen, and
    /// forgets what belonged to the pages as they were.
    fn pages_changed(&mut self, page: usize, chosen: Vec<usize>) {
        let count = self
            .review
            .as_ref()
            .map_or(0, |r| r.document().pages().len());
        self.page = page.min(count.saturating_sub(1));
        self.chosen_pages = if chosen.len() > 1 {
            chosen.into_iter().filter(|&p| p < count).collect()
        } else {
            Vec::new()
        };
        self.bookmarks = self
            .review
            .as_ref()
            .and_then(|r| bookmarks(r.document().bytes()).ok())
            .unwrap_or_default();
        self.labels = self
            .review
            .as_ref()
            .and_then(|r| page_labels(r.document().bytes()).ok())
            .unwrap_or_default();
        self.snaps = None;
        self.links = None;
        self.snapped = None;
        self.drag = None;
        self.building = None;
        self.calibrating = None;
        self.calibrate_from = None;
        self.found.clear();
        self.searched.clear();
        self.forget_missing();
        self.fit();
    }

    /// Turns the chosen pages a quarter turn, clockwise when `turn` is
    /// positive.
    ///
    /// # Errors
    /// A message for the user when the pages cannot be changed.
    pub fn turn_pages(&mut self, turn: i32) -> Result<(), String> {
        let page = self.page;
        self.change_pages(|review, chosen, units| {
            review.turn_pages(chosen, turn, units)?;
            Ok((page, chosen.to_vec()))
        })?;
        self.status = format!(
            "Turned {} {}. Undo turns {} back.",
            pages_named(&self.pages_chosen()),
            if turn > 0 {
                "clockwise"
            } else {
                "counterclockwise"
            },
            if self.pages_chosen().len() > 1 {
                "them"
            } else {
                "it"
            }
        );
        Ok(())
    }

    /// Moves the chosen pages one place toward the start (`by` negative)
    /// or the end.
    ///
    /// # Errors
    /// A message for the user when the pages cannot be changed.
    pub fn move_pages(&mut self, by: i32) -> Result<(), String> {
        let chosen = self.pages_chosen();
        let Some(&first) = chosen.first() else {
            return Err("There is no PDF open".into());
        };
        let count = self
            .review
            .as_ref()
            .map_or(0, |r| r.document().pages().len());
        let to = if by < 0 {
            first.saturating_sub(1)
        } else {
            first + 1
        };
        if (by < 0 && first == 0) || (by > 0 && first + chosen.len() >= count) {
            return Ok(());
        }
        let current = self.page;
        self.change_pages(|review, chosen, units| {
            let at = review.move_pages(chosen, to, units)?;
            let moved: Vec<usize> = (at..at + chosen.len()).collect();
            let shown = chosen
                .iter()
                .position(|&p| p == current)
                .map_or(at, |i| at + i);
            Ok((shown, moved))
        })?;
        self.status = format!("Moved {}.", pages_named(&self.pages_chosen()));
        Ok(())
    }

    /// Removes the chosen pages, with their markups.
    ///
    /// # Errors
    /// A message for the user when every page would go, or the pages
    /// cannot be changed.
    pub fn delete_pages(&mut self) -> Result<(), String> {
        let chosen = self.pages_chosen();
        let count = self
            .review
            .as_ref()
            .map_or(0, |r| r.document().pages().len());
        if chosen.len() >= count {
            return Err("A PDF must keep at least one page.".into());
        }
        let named = pages_named(&chosen);
        self.change_pages(|review, chosen, units| {
            review.remove_pages(chosen, units)?;
            Ok((chosen.first().copied().unwrap_or(0), Vec::new()))
        })?;
        let them = if chosen.len() > 1 { "them" } else { "it" };
        self.status = format!("Removed {named}. Undo brings {them} back.");
        Ok(())
    }

    /// Puts a blank page, as large as the one shown, after it.
    ///
    /// # Errors
    /// A message for the user when the pages cannot be changed.
    pub fn insert_blank(&mut self) -> Result<(), String> {
        let page = self.page;
        self.change_pages(|review, _, units| {
            review.insert_blank(page + 1, page, units)?;
            Ok((page + 1, Vec::new()))
        })?;
        self.status = format!("Put a blank page in as page {}.", self.page + 1);
        Ok(())
    }

    /// Puts every page of the PDF at `path` after the page shown.
    ///
    /// # Errors
    /// A message for the user when it cannot be read or the pages cannot
    /// be changed.
    pub fn insert_file(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
        let other = Document::open(bytes).map_err(|e| capitalize(&e.to_string()))?;
        let count = other.pages().len();
        let page = self.page;
        self.change_pages(|review, _, units| {
            let all: Vec<usize> = (0..count).collect();
            review.insert_pages(page + 1, other.bytes(), &all, units)?;
            Ok((page + 1, (page + 1..page + 1 + count).collect()))
        })?;
        self.status = format!(
            "Put in {} from {}, after page {}.",
            if count == 1 {
                "1 page".to_owned()
            } else {
                format!("{count} pages")
            },
            file_name(path),
            page + 1
        );
        Ok(())
    }

    /// Writes the chosen pages, with their markups, to a PDF of their own
    /// at `path`. This PDF does not change.
    ///
    /// # Errors
    /// A message for the user when it cannot be written.
    pub fn extract_pages(&mut self, path: &Path) -> Result<(), String> {
        let chosen = self.pages_chosen();
        let review = self.review.as_ref().ok_or("There is no PDF open")?;
        let bytes = review
            .extract(&chosen, self.units)
            .map_err(|e| capitalize(&e.to_string()))?;
        std::fs::write(path, bytes).map_err(|e| format!("Cannot save: {e}"))?;
        self.status = format!("Wrote {} to {}.", pages_named(&chosen), file_name(path));
        Ok(())
    }

    /// The open PDF's bookmarks.
    #[must_use]
    pub fn bookmarks(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    /// Shows the page bookmark `index` goes to.
    pub fn go_to_bookmark(&mut self, index: usize) {
        let Some(bookmark) = self.bookmarks.get(index) else {
            return;
        };
        match bookmark.page {
            Some(page) => {
                let title = bookmark.title.clone();
                self.go_to(page);
                self.status = title;
            }
            None => self.status = "This bookmark does not go to a page.".into(),
        }
    }

    /// The markup summary: the list as it is shown (filtered and sorted)
    /// as a PDF report.
    ///
    /// # Errors
    /// A message for the user when there is no PDF or it cannot be
    /// written.
    pub fn summary(&self) -> Result<Vec<u8>, String> {
        let review = self.review.as_ref().ok_or("There is no PDF open")?;
        let listed = self.listed();
        let file = self
            .path
            .as_deref()
            .map_or_else(|| "Drawing".to_owned(), file_name);
        let date = chrono::Local::now().format("%Y-%m-%d %H:%M");
        let shown = [0, 1, 2, 3, 4, 5];
        let table = Table {
            title: "Markup summary".into(),
            subtitle: format!("{file} · {date} · {}", self.list_label()),
            columns: shown
                .iter()
                .zip([1.4, 0.5, 4.0, 1.1, 1.3, 0.9])
                .map(|(&c, share)| (COLUMNS[c].to_owned(), share))
                .collect(),
            rows: listed
                .iter()
                .map(|m| Row {
                    color: Some(m.color),
                    cells: shown.iter().map(|&c| self.cell(review, m, c)).collect(),
                })
                .collect(),
        };
        report::table(&table).map_err(|e| capitalize(&e.to_string()))
    }

    /// Writes the markup summary to `path`.
    ///
    /// # Errors
    /// A message for the user when it cannot be written.
    pub fn export_summary(&mut self, path: &Path) -> Result<(), String> {
        let bytes = self.summary()?;
        std::fs::write(path, bytes).map_err(|e| format!("Cannot save: {e}"))?;
        self.status = format!("Wrote the markup summary, {}.", file_name(path));
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
        let pdf = review.document().bytes().to_vec();
        let member = Participant::host(shared, pdf, &self.name())?;
        self.status = "Hosting a session: send the invitation to who joins. If Windows asks, allow private networks.".into();
        let firewall = Arc::new(std::sync::Mutex::new((None, String::new())));
        look_at_firewall(&firewall, None);
        self.session = Some(Session {
            member,
            firewall,
            seen: 0,
        });
        Ok(())
    }

    /// Starts a review session on the session server at `server`
    /// (`ocs1:…`), for people on other networks.
    ///
    /// # Errors
    /// A message for the user when the server cannot be reached or
    /// refuses.
    pub fn start_session_on_server(&mut self, server: &str) -> Result<(), String> {
        let review = self.review.as_ref().ok_or("Open a PDF first")?;
        if self.session.is_some() {
            return Err("You are in a session already".into());
        }
        let mut shared = Shared::start(&self.name());
        shared.publish(review.markups(), review.scales());
        let pdf = review.document().bytes().to_vec();
        let member = Participant::start_on_server(server, shared, pdf, &self.name())?;
        self.status = "The session is on the server: send the invitation to who joins.".into();
        self.session = Some(Session {
            member,
            firewall: Arc::default(),
            seen: 0,
        });
        Ok(())
    }

    /// Hands the session over to `name`, who will host it from their
    /// computer.
    ///
    /// # Errors
    /// When this window does not host the session or nobody is called
    /// `name`.
    pub fn hand_over(&mut self, name: &str) -> Result<(), String> {
        let session = self.session.as_mut().ok_or("You are not in a session")?;
        session.member.hand_over(name)?;
        self.status = format!("Handing the session over to {name}…");
        Ok(())
    }

    /// The others in the session, by name.
    #[must_use]
    pub fn others(&self) -> Vec<String> {
        self.session
            .as_ref()
            .map_or_else(Vec::new, |s| s.member.others())
    }

    /// Whether this window hosts the session.
    #[must_use]
    pub fn hosting(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.member.hosting())
    }

    /// Joins the review session `invite` names, with its drawing.
    ///
    /// # Errors
    /// A message for the user when the invitation is wrong or the host
    /// cannot be reached.
    pub fn join_session(&mut self, invite: &str) -> Result<(), String> {
        let (member, pdf) = Participant::join(invite, &self.name())?;
        if let Err(message) = self.open_bytes(pdf, None) {
            member.leave();
            return Err(message);
        }
        self.session = Some(Session {
            member,
            firewall: Arc::default(),
            seen: 0,
        });
        self.pull();
        self.status = "Joined the session. Save as to keep a copy.".into();
        Ok(())
    }

    /// Leaves the review session; the markups stay as they are here.
    pub fn leave_session(&mut self) {
        if let Some(session) = self.session.take() {
            let hosting = session.member.hosting() && !session.member.others().is_empty();
            session.member.leave();
            self.status = if hosting {
                "Left the session; someone else hosts it now.".into()
            } else {
                "Left the session.".into()
            };
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
        if let Ok(mut shared) = session.member.link().shared.lock() {
            shared.publish(review.markups(), review.scales());
        }
    }

    /// Takes in what others changed in the session since last asked;
    /// whether anything did.
    pub fn pull(&mut self) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        if let Some(news) = session.member.follow() {
            self.status = news;
        }
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        let changed = session.member.link().changed.load(Ordering::SeqCst);
        if changed == session.seen && session.seen != 0 {
            return false;
        }
        session.seen = changed.max(1);
        let Ok(shared) = session.member.link().shared.lock() else {
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
    pub fn invite(&self) -> Option<String> {
        self.session.as_ref()?.member.invite()
    }

    /// The session panel's first line.
    #[must_use]
    pub fn session_state(&self) -> String {
        match &self.session {
            None => {
                "Not in a session. Start one with the PDF open, or paste an invitation to join one."
                    .into()
            }
            Some(session) if session.member.link().stopped() => {
                "The session ended: the host left or the connection closed.".into()
            }
            Some(session) => {
                let people = session
                    .member
                    .link()
                    .shared
                    .lock()
                    .map_or(0, |s| s.people().len());
                let role = if session.member.hosting() {
                    "Hosting"
                } else if session.member.on_server() {
                    "On a server"
                } else {
                    "In a session"
                };
                format!("{role} · {people} people")
            }
        }
    }

    /// What may keep people from joining, when hosting; empty when
    /// nothing is known to.
    #[must_use]
    pub fn firewall_advice(&self) -> String {
        let Some(session) = self.session.as_ref().filter(|s| s.member.hosting()) else {
            return String::new();
        };
        let Ok(state) = session.firewall.lock() else {
            return String::new();
        };
        let advice = state.0.and_then(Firewall::advice).unwrap_or_default();
        match (advice, state.1.as_str()) {
            ("", answer) => answer.to_owned(),
            (advice, "") => advice.to_owned(),
            (advice, answer) => format!("{answer} {advice}"),
        }
    }

    /// Whether the host should be offered to let Review through the
    /// firewall.
    #[must_use]
    pub fn firewall_blocks(&self) -> bool {
        self.session
            .as_ref()
            .filter(|s| s.member.hosting())
            .and_then(|s| s.firewall.lock().ok().and_then(|f| f.0))
            .is_some_and(|f| f.allowed == Some(false))
    }

    /// Asks Windows to let Review through the firewall; Windows asks the
    /// user for permission first.
    pub fn allow_firewall(&mut self) {
        if let Some(session) = self.session.as_ref() {
            look_at_firewall(&session.firewall, Some(()));
            self.status = "Windows asks for permission to let Review in.".into();
        }
    }

    /// Who is in the session.
    #[must_use]
    pub fn people(&self) -> Vec<String> {
        self.session
            .as_ref()
            .and_then(|s| s.member.link().shared.lock().ok().map(|s| s.people()))
            .unwrap_or_default()
    }

    /// What everyone did, newest first: `14:32 María: added a cloud on
    /// page 1`.
    #[must_use]
    pub fn activity(&self) -> Vec<String> {
        let Some(session) = self.session.as_ref() else {
            return Vec::new();
        };
        let Ok(mut shared) = session.member.link().shared.lock() else {
            return Vec::new();
        };
        shared
            .activity()
            .into_iter()
            .rev()
            .map(|a| format!("{} {}: {}", clock(a.when), a.who, a.what))
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
        let before = self.document_key();
        if let Some(review) = self.review.as_mut() {
            review.undo();
        }
        self.forget_missing();
        if self.document_key() != before {
            self.status = "The pages are back as they were.".into();
            self.pages_changed(self.page, Vec::new());
        }
    }

    /// Redoes the last step undone.
    pub fn redo(&mut self) {
        let before = self.document_key();
        if let Some(review) = self.review.as_mut() {
            review.redo();
        }
        self.forget_missing();
        if self.document_key() != before {
            self.status = "The pages are changed again.".into();
            self.pages_changed(self.page, Vec::new());
        }
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
        self.select_only(Some(id));
    }

    /// Selects every markup on the page shown.
    pub fn select_all(&mut self) {
        let ids: Vec<String> = self
            .review
            .as_ref()
            .map(|r| {
                r.markups()
                    .iter()
                    .filter(|m| m.page == self.page)
                    .map(|m| m.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let mut ids = ids.into_iter();
        self.selected = ids.next();
        self.also = ids.collect();
        let count = self.chosen().len();
        self.status = format!(
            "{count} markup{} selected.",
            if count == 1 { "" } else { "s" }
        );
    }

    /// How many markups are selected.
    #[must_use]
    pub fn selected_count(&self) -> usize {
        self.chosen().len()
    }

    /// The selected markup's replies: who and when, then what.
    #[must_use]
    pub fn replies(&self) -> Vec<(String, String, bool)> {
        self.selected()
            .map(|m| {
                m.replies
                    .iter()
                    .map(|r| {
                        let who = if r.author.is_empty() {
                            "Someone"
                        } else {
                            &r.author
                        };
                        let when = pdf_date_text(&r.modified);
                        let header = if when.is_empty() {
                            who.to_owned()
                        } else {
                            format!("{who} · {when}")
                        };
                        (header, r.contents.clone(), r.author == self.author)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Answers the selected markup; whether there was something to say.
    pub fn reply(&mut self, text: &str) -> bool {
        let (Some(id), Some(review)) = (self.selected.clone(), self.review.as_mut()) else {
            return false;
        };
        let author = self.author.clone();
        review.reply(&id, text, &author)
    }

    /// Takes back reply `index` of the selected markup, when it is one of
    /// this person's.
    pub fn remove_reply(&mut self, index: usize) {
        let mine = self
            .selected()
            .and_then(|m| m.replies.get(index))
            .is_some_and(|r| r.author == self.author);
        if !mine {
            self.status = "Only who wrote a reply can take it back.".into();
            return;
        }
        if let (Some(id), Some(review)) = (self.selected.clone(), self.review.as_mut()) {
            review.remove_reply(&id, index);
        }
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
                subject.clone()
            };
        });
    }

    /// Changes the selected markup's status.
    pub fn set_status(&mut self, status: usize) {
        if let Some(&status) = Status::ALL.get(status) {
            self.change_selected(|m| m.status = status);
        }
    }

    /// Deletes the selected markups, as one step.
    pub fn delete_selected(&mut self) {
        let ids = self.chosen();
        self.select_only(None);
        if let Some(review) = self.review.as_mut() {
            review.remove_all(&ids);
        }
    }

    /// The markup list, one cell per column of [`COLUMNS`].
    #[must_use]
    pub fn rows(&self) -> Vec<[String; 7]> {
        let Some(review) = self.review.as_ref() else {
            return Vec::new();
        };
        self.listed()
            .into_iter()
            .map(|m| std::array::from_fn(|column| self.cell(review, m, column)))
            .collect()
    }

    /// What the list shows of `markup` in `column` of [`COLUMNS`].
    fn cell(&self, review: &Review, markup: &Markup, column: usize) -> String {
        match column {
            0 => markup.subject().to_owned(),
            1 => (markup.page + 1).to_string(),
            2 => {
                let text = markup.text.lines().collect::<Vec<_>>().join(" ");
                let text = match review.measured(markup, self.units) {
                    Some(value) if text.is_empty() => value,
                    Some(value) => format!("{value} · {text}"),
                    None => text,
                };
                match markup.replies.len() {
                    0 => text,
                    n => {
                        let replies = if n == 1 {
                            "1 reply".to_owned()
                        } else {
                            format!("{n} replies")
                        };
                        if text.is_empty() {
                            replies
                        } else {
                            format!("{text} · {replies}")
                        }
                    }
                }
            }
            3 => markup.author.clone(),
            4 => pdf_date_text(&markup.modified),
            5 => markup.status.label().to_owned(),
            6 => color_name(markup.color),
            _ => String::new(),
        }
    }

    /// Writes `text` in the session's chat; whether it was written.
    pub fn say(&mut self, text: &str) -> bool {
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        session
            .member
            .link()
            .shared
            .lock()
            .is_ok_and(|mut shared| shared.say(text))
    }

    /// The session's chat, oldest first: `14:32 María: Is it centered?`.
    #[must_use]
    pub fn chat(&self) -> Vec<String> {
        let Some(session) = self.session.as_ref() else {
            return Vec::new();
        };
        let Ok(shared) = session.member.link().shared.lock() else {
            return Vec::new();
        };
        shared
            .chat()
            .into_iter()
            .map(|said| format!("{} {}: {}", clock(said.when), said.who, said.text))
            .collect()
    }

    /// Reads My tools from `path` and keeps them there from now on.
    pub fn keep_saved_tools_at(&mut self, path: PathBuf) {
        self.saved = chest::load(&path);
        self.settings = chest::load_settings(&path);
        self.own_stamps = chest::load_stamps(&path);
        self.saved_path = Some(path);
    }

    /// My tools.
    #[must_use]
    pub fn saved_tools(&self) -> &[SavedTool] {
        &self.saved
    }

    /// Saves the selected markup's style in My tools.
    ///
    /// # Errors
    /// A message for the user when nothing is selected or it cannot be
    /// kept.
    pub fn save_selected_tool(&mut self) -> Result<(), String> {
        let markup = self.selected().ok_or("Select a markup to save its style")?;
        let tool = SavedTool {
            name: markup.subject().to_owned(),
            kind: markup.kind,
            color: markup.color,
            width: markup.width,
            fill: markup.fill,
            subject: markup.subject.clone(),
        };
        let name = tool.name.clone();
        match self
            .saved
            .iter_mut()
            .find(|t| t.name == tool.name && t.kind == tool.kind)
        {
            Some(same) => *same = tool,
            None => self.saved.push(tool),
        }
        self.keep_saved()?;
        self.status = format!("Saved \"{name}\" in My tools.");
        Ok(())
    }

    /// Draws with My tools' `index`.
    pub fn use_saved_tool(&mut self, index: usize) {
        let Some(tool) = self.saved.get(index).cloned() else {
            return;
        };
        self.choose_tool(Tool::Draw(tool.kind));
        if let Some(color) = COLORS
            .iter()
            .position(|(_, c)| c.iter().zip(tool.color).all(|(a, b)| (a - b).abs() < 1e-9))
        {
            self.color = color;
        }
        if let Some(width) = WIDTHS
            .iter()
            .position(|(_, w)| (*w - tool.width).abs() < 1e-9)
        {
            self.width = width;
        }
        self.fill = tool.fill;
        self.status = format!("Drawing with \"{}\" from My tools.", tool.name);
        self.style = Some(tool);
    }

    /// Takes My tools' `index` out.
    ///
    /// # Errors
    /// A message for the user when the change cannot be kept.
    pub fn remove_saved_tool(&mut self, index: usize) -> Result<(), String> {
        if index < self.saved.len() {
            let gone = self.saved.remove(index);
            if self.style.as_ref() == Some(&gone) {
                self.style = None;
            }
            self.keep_saved()?;
        }
        Ok(())
    }

    /// My tools' tool in use, if any.
    #[must_use]
    pub fn saved_in_use(&self) -> Option<usize> {
        let style = self.style.as_ref()?;
        self.saved.iter().position(|t| t == style)
    }

    fn keep_saved(&self) -> Result<(), String> {
        match &self.saved_path {
            Some(path) => {
                chest::store(path, &self.saved).map_err(|e| format!("Cannot keep My tools: {e}"))
            }
            None => Ok(()),
        }
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
        let chosen = self.chosen();
        for markup in review.markups().iter().filter(|m| m.page == self.page) {
            let selected = chosen.contains(&markup.id);
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
            if kind.gesture() == Gesture::Drag {
                if let Some(points) = drawn(*kind, points, false).filter(|p| p.len() >= 2) {
                    draw(&self.new_markup(*kind, points), false);
                }
            } else {
                draw(&self.new_markup(*kind, points), false);
            }
        }
        shapes.extend(outlines);
        self.handles(&page, &chosen, &mut shapes);
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
        (shapes, labels)
    }

    /// What helps place and pick markups: the calibration line, what the
    /// mouse snaps to, the grips of the one markup selected, and the box
    /// being dragged to select.
    fn handles(&self, page: &Page, chosen: &[String], shapes: &mut Vec<Shape>) {
        let line = match &self.drag {
            Some(Drag::Calibrate { a, b }) => Some((*a, *b)),
            _ => self
                .calibrating
                .or_else(|| Some((self.calibrate_from?, self.cursor?))),
        };
        if let Some((a, b)) = line {
            shapes.push(Shape {
                commands: self.commands(page, &[Segment::Move(a), Segment::Line(b)]),
                stroke: Some([0.0, 0.45, 0.9]),
                fill: None,
                width: 2.0,
                selected: false,
            });
        }
        if let Some(snapped) = self.snapped {
            shapes.push(Shape {
                commands: marker(self.to_screen(page, snapped.at), snapped.kind),
                stroke: Some([1.0, 0.45, 0.0]),
                fill: None,
                width: 2.0,
                selected: false,
            });
        }
        // The grips of the one markup selected.
        if let Some(markup) = self
            .selected()
            .filter(|m| m.page == self.page && chosen.len() == 1)
        {
            for grip in markup.grips() {
                let (x, y) = self.to_screen(page, grip);
                shapes.push(Shape {
                    commands: format!(
                        "M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} Z",
                        x - GRIP,
                        y - GRIP,
                        x + GRIP,
                        y - GRIP,
                        x + GRIP,
                        y + GRIP,
                        x - GRIP,
                        y + GRIP
                    ),
                    stroke: Some([0.0, 0.45, 0.9]),
                    fill: Some(([1.0, 1.0, 1.0], 1.0)),
                    width: 1.5,
                    selected: false,
                });
            }
        }
        if let Some(Drag::Area { start, end }) = &self.drag {
            shapes.push(Shape {
                commands: self.commands(
                    page,
                    &[
                        Segment::Move(*start),
                        Segment::Line(Point::new(end.x, start.y)),
                        Segment::Line(*end),
                        Segment::Line(Point::new(start.x, end.y)),
                        Segment::Close,
                    ],
                ),
                stroke: Some([0.0, 0.45, 0.9]),
                fill: Some(([0.0, 0.45, 0.9], 0.08)),
                width: 1.0,
                selected: false,
            });
        }
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
    /// This window's place in the session.
    member: Participant,
    /// What the firewall lets in, when hosting, once looked at; and the
    /// answer to allowing Review through it.
    firewall: Arc<std::sync::Mutex<(Option<Firewall>, String)>>,
    /// How many changes from others were taken in.
    seen: u64,
}

/// Looks at the firewall away from the window, after letting this
/// program through it when `allow` is given.
fn look_at_firewall(state: &Arc<std::sync::Mutex<(Option<Firewall>, String)>>, allow: Option<()>) {
    let Ok(program) = std::env::current_exe() else {
        return;
    };
    let state = Arc::clone(state);
    std::thread::spawn(move || {
        let answer = match allow.map(|()| firewall::allow(&program)) {
            Some(Ok(())) => "Review is allowed through the Windows firewall.".to_owned(),
            Some(Err(message)) => message,
            None => String::new(),
        };
        let found = firewall::check(&program);
        if let Ok(mut state) = state.lock() {
            *state = (Some(found), answer);
        }
    });
}

/// `14:32`, in this computer's time.
fn clock(when: i64) -> String {
    chrono::DateTime::from_timestamp(when, 0)
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// The name of one of [`COLORS`], or `#rrggbb`.
fn color_name(color: Color) -> String {
    if let Some((name, _)) = COLORS
        .iter()
        .find(|(_, c)| c.iter().zip(color).all(|(a, b)| (a - b).abs() < 0.02))
    {
        return (*name).to_owned();
    }
    color.iter().fold("#".to_owned(), |mut out, c| {
        // Clamped to 0–255 first, so nothing is lost.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        let _ = write!(out, "{byte:02x}");
        out
    })
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

/// `p` kept at a multiple of 45° from `anchor`, or, for a box, square.
fn straighten(anchor: Point, p: Point, square: bool) -> Point {
    let (dx, dy) = (p.x - anchor.x, p.y - anchor.y);
    if square {
        let side = dx.abs().max(dy.abs());
        return Point::new(anchor.x + side.copysign(dx), anchor.y + side.copysign(dy));
    }
    let step = std::f64::consts::FRAC_PI_4;
    let angle = (dy.atan2(dx) / step).round() * step;
    let (sin, cos) = angle.sin_cos();
    // The mouse's point projected on that direction, like a square
    // against a straightedge.
    let length = dx * cos + dy * sin;
    // Exactly level or plumb, not a hair off.
    let tidy = |v: f64| if v.abs() < 1e-12 { 0.0 } else { v };
    Point::new(anchor.x + tidy(length * cos), anchor.y + tidy(length * sin))
}

/// The mark of a snap at `(x, y)` on the screen: a square for an end, a
/// triangle for a middle, a cross for a crossing, a diamond on a line.
fn marker((x, y): (f64, f64), kind: Snap) -> String {
    let m = MARK;
    match kind {
        Snap::End => format!(
            "M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} Z",
            x - m,
            y - m,
            x + m,
            y - m,
            x + m,
            y + m,
            x - m,
            y + m
        ),
        Snap::Middle => format!(
            "M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} Z",
            x,
            y - m,
            x + m,
            y + m,
            x - m,
            y + m
        ),
        Snap::Intersection => format!(
            "M {:.1} {:.1} L {:.1} {:.1} M {:.1} {:.1} L {:.1} {:.1}",
            x - m,
            y - m,
            x + m,
            y + m,
            x + m,
            y - m,
            x - m,
            y + m
        ),
        Snap::OnLine => format!(
            "M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} Z",
            x,
            y - m,
            x + m,
            y,
            x,
            y + m,
            x - m,
            y
        ),
    }
}

/// What a followed link asks the window to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Follow {
    /// A drawing, at a page from 0.
    File(PathBuf, usize),
    /// A web or mail address.
    Web(String),
}

/// A stamp to place: its words, filled in, and its color.
struct OwnStamp {
    words: String,
    color: Color,
}

/// `page 3` or `pages 2, 3 and 7`.
fn pages_named(pages: &[usize]) -> String {
    match pages {
        [] => "no page".into(),
        [one] => format!("page {}", one + 1),
        [rest @ .., last] => format!(
            "pages {} and {}",
            rest.iter()
                .map(|p| (p + 1).to_string())
                .collect::<Vec<_>>()
                .join(", "),
            last + 1
        ),
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
    fn pages_are_chosen_turned_moved_put_in_taken_out_and_undone() {
        let (mut viewer, dir) = viewer();
        let control = Keys {
            control: true,
            shift: false,
        };
        // A rectangle on page 2 goes wherever page 2 goes.
        viewer.go_to(1);
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        drag(&mut viewer, (100.0, 100.0), (300.0, 200.0));
        assert_eq!(viewer.pages_chosen(), [1]);
        viewer.click_page(0, control);
        assert_eq!(viewer.pages_chosen(), [0, 1]);
        assert_eq!(viewer.pages_label(), "2 pages chosen");
        viewer.click_page(0, control);
        assert_eq!(viewer.pages_chosen(), [1]);

        viewer.move_pages(-1).unwrap();
        assert_eq!(viewer.page(), 0);
        assert_eq!(viewer.review().unwrap().markups()[0].page, 0);
        viewer.turn_pages(1).unwrap();
        let page = viewer.review().unwrap().document().pages()[0];
        assert_eq!((page.width, page.height), (1728.0, 2592.0));
        assert!(viewer.status().starts_with("Turned page 1 clockwise"));

        viewer.insert_blank().unwrap();
        assert_eq!(viewer.page(), 1);
        let other = dir.join("letter.pdf");
        std::fs::write(&other, opencabinet_pdf::sample::sheets(2, 612.0, 792.0, 0)).unwrap();
        viewer.insert_file(&other).unwrap();
        assert_eq!(viewer.review().unwrap().document().pages().len(), 5);
        assert_eq!(viewer.pages_chosen(), [2, 3]);
        viewer.delete_pages().unwrap();
        assert_eq!(viewer.review().unwrap().document().pages().len(), 3);
        assert!(viewer.modified());

        // Page 1, marked, alone in a PDF of its own.
        viewer.go_to(0);
        let alone = dir.join("alone.pdf");
        viewer.extract_pages(&alone).unwrap();
        let review = Review::open(std::fs::read(&alone).unwrap()).unwrap();
        assert_eq!(review.document().pages().len(), 1);
        assert_eq!(review.markups().len(), 1);

        // Every page cannot go; undo goes back step by step.
        viewer.click_page(
            2,
            Keys {
                control: false,
                shift: true,
            },
        );
        assert_eq!(viewer.pages_chosen(), [0, 1, 2]);
        assert!(viewer.delete_pages().is_err());
        for _ in 0..5 {
            viewer.undo();
        }
        let review = viewer.review().unwrap();
        assert_eq!(review.document().pages().len(), 2);
        assert_eq!(review.markups()[0].page, 1);
        viewer.redo();
        assert_eq!(viewer.review().unwrap().markups()[0].page, 0);
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
    fn a_line_is_placed_by_two_clicks_snapping_to_the_drawing() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Line));
        // Near where two grid lines cross: x 529.2, y 1371.6 in the PDF,
        // 356.4 down on the screen.
        viewer.drag((526.0, 353.0));
        assert_eq!(viewer.snap_label(), "Intersection");
        click(&mut viewer, (526.0, 353.0));
        assert!(viewer.building());
        assert_eq!(viewer.rows().len(), 0);
        // Along the horizontal grid line.
        viewer.drag((700.0, 360.0));
        assert_eq!(viewer.snap_label(), "On line");
        click(&mut viewer, (700.0, 360.0));
        assert!(!viewer.building());
        let line = viewer.selected().unwrap().clone();
        assert_eq!(line.kind, Kind::Line);
        let close = |p: Point, x: f64, y: f64| (p.x - x).abs() < 0.01 && (p.y - y).abs() < 0.01;
        assert!(close(line.points[0], 529.2, 1371.6), "{:?}", line.points);
        assert!(close(line.points[1], 700.0, 1371.6), "{:?}", line.points);
        // Snapping off, the point is where the mouse is.
        viewer.set_snap(false);
        viewer.drag((526.0, 353.0));
        assert_eq!(viewer.snap_label(), "");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn shift_keeps_a_line_level_and_a_box_square() {
        let (mut viewer, dir) = viewer();
        viewer.set_snap(false);
        let shift = Keys {
            shift: true,
            ..Keys::default()
        };
        viewer.choose_tool(Tool::Draw(Kind::Line));
        click(&mut viewer, (100.0, 100.0));
        viewer.pointer_moved((300.0, 110.0), shift);
        viewer.press_with((300.0, 110.0), 0, shift);
        viewer.release((300.0, 110.0));
        let line = viewer.selected().unwrap().clone();
        assert_eq!(line.points[1], Point::new(300.0, 1628.0));
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        viewer.press_with((400.0, 100.0), 0, shift);
        viewer.pointer_moved((500.0, 160.0), shift);
        viewer.release((500.0, 160.0));
        let (low, high) = viewer.selected().unwrap().bounds();
        assert_eq!((high.x - low.x, high.y - low.y), (100.0, 100.0));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn links_go_to_pages_drawings_and_web_addresses() {
        let (mut viewer, dir) = viewer();
        viewer.settings.snap = false;
        viewer.choose_tool(Tool::Draw(Kind::Link));
        drag(&mut viewer, (100.0, 100.0), (260.0, 160.0));
        viewer.set_text("2");
        viewer.choose_tool(Tool::Select);
        viewer.escape();
        // A click picks the link to edit it; a double click follows it.
        click(&mut viewer, (180.0, 130.0));
        assert_eq!(viewer.selected().unwrap().kind, Kind::Link);
        viewer.double_click();
        assert_eq!(viewer.page(), 1);
        assert_eq!(viewer.take_follow(), None);

        viewer.go_to(0);
        // Going to a page fits it; the view the link was drawn in again.
        viewer.zoom = 1.0;
        viewer.scroll = (0.0, 0.0);
        click(&mut viewer, (180.0, 130.0));
        viewer.set_text("A-301.pdf#3");
        viewer.double_click();
        assert_eq!(
            viewer.take_follow(),
            Some(Follow::File(dir.join("A-301.pdf"), 2))
        );
        viewer.set_text("https://example.org/specs");
        viewer.double_click();
        assert_eq!(
            viewer.take_follow(),
            Some(Follow::Web("https://example.org/specs".into()))
        );
        assert_eq!(viewer.take_follow(), None, "once");

        // Another program's link, not a markup here, is followed with a
        // click.
        let mut markups = opencabinet_pdf::annotations::Markups::default();
        markups
            .annotations
            .push(opencabinet_pdf::annotations::Annotation {
                subtype: opencabinet_pdf::annotations::Subtype::Link,
                rect: [100.0, 1500.0, 300.0, 1600.0],
                link: Some(Target::Page(1)),
                ..opencabinet_pdf::annotations::Annotation::default()
            });
        let path = dir.join("linked.pdf");
        let bytes = opencabinet_pdf::sample::sheets(2, 2592.0, 1728.0, 0);
        std::fs::write(
            &path,
            opencabinet_pdf::annotations::save(&bytes, &markups).unwrap(),
        )
        .unwrap();
        viewer.open(&path).unwrap();
        assert_eq!(viewer.review().unwrap().markups().len(), 0);
        let page = viewer.page_size().unwrap();
        let at = viewer.to_screen(&page, Point::new(200.0, 1550.0));
        click(&mut viewer, at);
        assert_eq!(viewer.page(), 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn an_angle_takes_three_clicks_and_quantities_add_up() {
        let (mut viewer, dir) = viewer();
        viewer.settings.snap = false;
        viewer.choose_tool(Tool::Draw(Kind::Angle));
        for at in [(400.0, 300.0), (300.0, 300.0), (300.0, 200.0)] {
            click(&mut viewer, at);
        }
        // Done at the third click, without Enter.
        assert!(!viewer.building());
        let review = viewer.review().unwrap();
        let angle = review.markups().last().unwrap();
        assert_eq!(angle.kind, Kind::Angle);
        assert_eq!(review.measured(angle, viewer.units).unwrap(), "90.0°");

        // A perimeter, on a page at 1/4" = 1'-0".
        let quarter = Viewer::scale_names()
            .iter()
            .position(|n| n == "1/4\" = 1'-0\"")
            .unwrap();
        viewer.choose_scale(quarter, false);
        viewer.choose_tool(Tool::Draw(Kind::Perimeter));
        for at in [
            (100.0, 100.0),
            (280.0, 100.0),
            (280.0, 190.0),
            (100.0, 190.0),
        ] {
            click(&mut viewer, at);
        }
        viewer.finish();
        let rows = viewer.quantities();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1][0], "Perimeter");
        assert_eq!(rows[1][1], "30'-0\"");
        assert_eq!(rows[1][2], "1 markup on page 1");
        let csv = dir.join("totals.csv");
        viewer.export_quantities(&csv).unwrap();
        assert!(
            std::fs::read_to_string(&csv)
                .unwrap()
                .contains("Perimeter,Perimeter,1,1,")
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_view_turns_with_the_markups_and_the_file_stays() {
        let (mut viewer, dir) = viewer();
        let bytes = viewer.review().unwrap().document().bytes().to_vec();
        viewer.turn_view(1);
        assert_eq!(viewer.turn_label(), "View 90°");
        assert_eq!(viewer.view(1.0).unwrap().turn, 1);
        let page = viewer.page_size().unwrap();
        assert_eq!((page.width, page.height), (1728.0, 2592.0));
        // A point goes to the page and back to the same place on screen.
        let at = (321.0, 234.0);
        let back = viewer.to_screen(&page, viewer.to_pdf(at).unwrap());
        assert!((back.0 - at.0).abs() < 1e-6 && (back.1 - at.1).abs() < 1e-6);

        // A rectangle drawn on the turned view is found where it was drawn.
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        drag(&mut viewer, (200.0, 200.0), (320.0, 280.0));
        viewer.choose_tool(Tool::Select);
        viewer.escape();
        click(&mut viewer, (200.0, 240.0));
        assert!(viewer.selected().is_some());
        // Upright again, the same markup is somewhere else on screen.
        for _ in 0..3 {
            viewer.turn_view(1);
        }
        assert_eq!(viewer.turn_label(), "");
        assert_eq!(
            viewer.review().unwrap().document().bytes(),
            bytes.as_slice()
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn stamps_say_who_and_when_and_a_shop_adds_its_own() {
        let (mut viewer, dir) = viewer();
        viewer.author = "Andrés".into();
        viewer.choose_tool(Tool::Draw(Kind::Stamp));
        let reviewed = viewer
            .stamp_names()
            .iter()
            .position(|w| w.contains("{name}"))
            .unwrap();
        viewer.choose_stamp(reviewed);
        click(&mut viewer, (300.0, 300.0));
        let today = chrono::Local::now()
            .date_naive()
            .format("%m/%d/%Y")
            .to_string();
        let placed = viewer
            .review()
            .unwrap()
            .markups()
            .last()
            .unwrap()
            .text
            .clone();
        assert_eq!(placed, format!("REVIEWED BY Andrés {today}"));

        // A stamp of the shop's own, in the color in use, then taken out.
        viewer.choose_color(2);
        assert!(viewer.add_stamp("   ").is_err());
        viewer.add_stamp("SHOP CHECKED {date}").unwrap();
        let own = viewer.stamp_names().len() - 1;
        assert_eq!(viewer.stamp(), own);
        click(&mut viewer, (300.0, 500.0));
        let placed = viewer.review().unwrap().markups().last().unwrap().clone();
        assert_eq!(placed.text, format!("SHOP CHECKED {today}"));
        assert_eq!(placed.color, COLORS[2].1);
        viewer.remove_stamp(0).unwrap();
        assert_eq!(viewer.stamp_names().len(), own + 1, "Review's stay");
        viewer.remove_stamp(own).unwrap();
        assert_eq!(viewer.stamp_names().len(), own);
        assert!(viewer.stamp() < own);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_page_is_calibrated_by_two_clicks() {
        let (mut viewer, dir) = viewer();
        viewer.set_snap(false);
        viewer.choose_tool(Tool::Calibrate);
        click(&mut viewer, (100.0, 100.0));
        assert!(!viewer.calibrating());
        viewer.drag((172.0, 100.0));
        // The line follows the mouse.
        let (shapes, _) = viewer.overlay();
        assert_eq!(shapes.len(), 1);
        click(&mut viewer, (172.0, 100.0));
        assert!(viewer.calibrating());
        viewer.calibrate("1'").unwrap();
        assert_eq!(viewer.scale_label(), "Scale 1\" = 1'-0\"");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn one_full_page_lets_the_wheel_zoom() {
        let (mut viewer, dir) = viewer();
        let before = viewer.zoom;
        viewer.wheel((0.0, 120.0), false, (500.0, 350.0));
        assert!((viewer.zoom - before).abs() < 1e-12);
        viewer.set_wheel_zooms(true);
        assert!(viewer.wheel_zooms());
        let fitted = viewer.zoom;
        viewer.wheel((0.0, 120.0), false, (500.0, 350.0));
        assert!(viewer.zoom > fitted);
        // Ctrl+wheel then moves the page instead.
        let zoomed = viewer.zoom;
        viewer.wheel((0.0, -120.0), true, (500.0, 350.0));
        assert!((viewer.zoom - zoomed).abs() < 1e-12);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_grip_reshapes_the_selected_markup() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        drag(&mut viewer, (100.0, 100.0), (300.0, 200.0));
        viewer.choose_tool(Tool::Select);
        // The bottom right corner, dragged out.
        drag(&mut viewer, (300.0, 200.0), (400.0, 260.0));
        let markup = viewer.selected().unwrap().clone();
        assert_eq!(
            markup.bounds(),
            (Point::new(100.0, 1468.0), Point::new(400.0, 1628.0))
        );
        // One step: undo puts it back.
        viewer.undo();
        assert_eq!(
            viewer.selected().unwrap().bounds(),
            (Point::new(100.0, 1528.0), Point::new(300.0, 1628.0))
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn several_markups_are_selected_moved_changed_and_deleted_together() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        drag(&mut viewer, (100.0, 100.0), (150.0, 150.0));
        drag(&mut viewer, (300.0, 100.0), (350.0, 150.0));
        drag(&mut viewer, (600.0, 100.0), (650.0, 150.0));
        viewer.choose_tool(Tool::Select);
        // A box around the first two, with Shift.
        let shift = Keys {
            shift: true,
            ..Keys::default()
        };
        viewer.press_with((50.0, 50.0), 0, shift);
        viewer.drag((200.0, 120.0));
        viewer.release((400.0, 200.0));
        assert_eq!(viewer.selected_count(), 2);
        // Control adds the third.
        let control = Keys {
            control: true,
            ..Keys::default()
        };
        viewer.press_with((625.0, 100.0), 0, control);
        viewer.release((625.0, 100.0));
        assert_eq!(viewer.selected_count(), 3);
        // No grips with several selected.
        let (shapes, _) = viewer.overlay();
        assert_eq!(shapes.len(), 3);
        // Dragging one moves them all.
        drag(&mut viewer, (125.0, 100.0), (125.0, 140.0));
        let tops: Vec<f64> = viewer
            .review()
            .unwrap()
            .markups()
            .iter()
            .map(|m| m.bounds().1.y)
            .collect();
        assert_eq!(tops, [1588.0, 1588.0, 1588.0]);
        // A status for all, as one step.
        viewer.set_status(1);
        assert!(
            viewer
                .review()
                .unwrap()
                .markups()
                .iter()
                .all(|m| m.status == Status::ALL[1])
        );
        viewer.undo();
        assert!(
            viewer
                .review()
                .unwrap()
                .markups()
                .iter()
                .all(|m| m.status == Status::Open)
        );
        // Control on one selected takes it out.
        viewer.press_with((625.0, 140.0), 0, control);
        viewer.release((625.0, 140.0));
        assert_eq!(viewer.selected_count(), 2);
        viewer.delete_selected();
        assert_eq!(viewer.rows().len(), 1);
        viewer.undo();
        assert_eq!(viewer.rows().len(), 3);
        viewer.select_all();
        assert_eq!(viewer.selected_count(), 3);
        // A plain click on empty paper lets go of them.
        click(&mut viewer, (900.0, 600.0));
        assert_eq!(viewer.selected_count(), 0);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_selected_markup_is_answered_and_its_replies_listed() {
        let (mut viewer, dir) = viewer();
        viewer.author = "Andrés".into();
        viewer.choose_tool(Tool::Draw(Kind::Cloud));
        drag(&mut viewer, (100.0, 100.0), (300.0, 200.0));
        assert!(!viewer.reply("  "));
        assert!(viewer.reply("Check with the fabricator"));
        let replies = viewer.replies();
        assert_eq!(replies.len(), 1);
        assert!(replies[0].0.starts_with("Andrés · 20"));
        assert_eq!(replies[0].1, "Check with the fabricator");
        assert!(replies[0].2);
        assert_eq!(viewer.rows()[0][2], "1 reply");
        // Someone else's reply stays.
        viewer.author = "Sam".into();
        viewer.remove_reply(0);
        assert_eq!(viewer.replies().len(), 1);
        viewer.author = "Andrés".into();
        viewer.remove_reply(0);
        assert_eq!(viewer.replies().len(), 0);
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
        // A click only places the first point; Escape lets it go.
        click(&mut viewer, (500.0, 500.0));
        assert!(viewer.building());
        assert_eq!(viewer.rows().len(), 1);
        viewer.escape();
        viewer.choose_tool(Tool::Draw(Kind::Cloud));
        let (shapes, _) = viewer.overlay();
        // The cloud, and a grip on each of its four corners.
        assert_eq!(shapes.len(), 5);
        assert!(shapes[0].commands.starts_with("M 100.0 100.0 C"));
        assert!(shapes[0].selected);
        assert!(shapes[1].commands.starts_with("M 96.0 96.0 L 104.0 96.0"));
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
        let row = &viewer.rows()[0];
        assert_eq!(
            [&row[0], &row[1], &row[2], &row[3], &row[5], &row[6]],
            ["Phase 2", "1", "Check with GC", &author, "Accepted", "Blue"]
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
        assert_eq!(viewer.rows()[1][2], "4");
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
        assert_eq!(viewer.rows()[0][2], "6'-0\"");
        let (_, labels) = viewer.overlay();
        assert_eq!(labels[0].text, "6'-0\"");

        viewer.choose_tool(Tool::Draw(Kind::Area));
        for at in [(0.0, 0.0), (216.0, 0.0), (216.0, 180.0), (0.0, 180.0)] {
            click(&mut viewer, at);
        }
        viewer.finish();
        // 12' × 10' at 1" = 4': 120 square feet.
        assert_eq!(viewer.rows()[1][2], "120.00 sq ft");
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
        assert_eq!(rows[2][2], "Here");
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
        assert_eq!(viewer.rows()[0][2], "4'-6\"");
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
        assert_eq!(viewer.rows()[0][2], "Filler");
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
        let mut invite: opencabinet_session::Invite = host.invite().unwrap().parse().unwrap();
        invite.addresses = vec!["127.0.0.1".into()];
        assert!(host.hosting());

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
        assert_eq!(host.others(), ["María"]);
        assert!(!guest.hosting());
        assert!(
            guest.hand_over("Andrés").is_err(),
            "only the host hands over"
        );
        assert!(host.activity()[0].contains("María: added an arrow on page 1"));

        // A word in the chat.
        assert!(guest.say("Is the sink centered?"));
        let start = std::time::Instant::now();
        while host.chat().is_empty() {
            assert!(start.elapsed().as_secs() < 10, "the chat did not arrive");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(host.chat()[0].ends_with("María: Is the sink centered?"));
        assert!(!Viewer::default().say("Nobody hears this"));
        guest.leave_session();
        host.leave_session();
        assert!(!host.in_session());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn bookmarks_go_to_their_pages() {
        let dir = std::env::temp_dir().join(format!("oc-bookmarks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("set.pdf");
        std::fs::write(&path, opencabinet_pdf::sample::bookmarked()).unwrap();
        let mut viewer = Viewer::default();
        viewer.resize(1000.0, 700.0);
        viewer.open(&path).unwrap();
        assert_eq!(viewer.bookmarks().len(), 4);
        viewer.go_to_bookmark(2);
        assert_eq!(viewer.page(), 1);
        assert_eq!(viewer.status(), "A-102 Elevación norte");
        viewer.go_to_bookmark(0);
        assert_eq!(viewer.page(), 1);
        viewer.go_to_bookmark(3);
        assert_eq!(viewer.page(), 2);
        viewer.go_to_bookmark(99);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_summary_lists_what_the_list_shows() {
        let (mut viewer, dir) = viewer();
        assert!(Viewer::default().summary().is_err());
        viewer.choose_tool(Tool::Draw(Kind::Cloud));
        drag(&mut viewer, (100.0, 100.0), (200.0, 200.0));
        viewer.set_text("Confirm the sink centerline");
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        drag(&mut viewer, (300.0, 100.0), (400.0, 200.0));
        viewer.set_status(1);
        viewer.set_filter(Some(Status::Open), "");
        let path = dir.join("summary.pdf");
        viewer.export_summary(&path).unwrap();
        let summary = Document::open(std::fs::read(&path).unwrap()).unwrap();
        let found = |words: &str| find(&summary, 0, words).unwrap().len();
        assert_eq!(found("Confirm the sink centerline"), 1);
        assert_eq!(found("1 of 2 markups"), 1);
        // Filtered out, so not in the summary.
        assert_eq!(found("Rectangle"), 0);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_list_sorts_by_any_column() {
        let (mut viewer, dir) = viewer();
        viewer.choose_tool(Tool::Draw(Kind::Rectangle));
        drag(&mut viewer, (100.0, 100.0), (200.0, 200.0));
        viewer.set_subject("Zeta");
        viewer.choose_tool(Tool::Draw(Kind::Cloud));
        drag(&mut viewer, (300.0, 100.0), (400.0, 200.0));
        viewer.set_subject("alpha");
        viewer.choose_color(4);
        let subjects =
            |v: &Viewer| -> Vec<String> { v.rows().into_iter().map(|r| r[0].clone()).collect() };
        assert_eq!(subjects(&viewer), ["Zeta", "alpha"]);
        viewer.sort_by(Some((0, false)));
        assert_eq!(subjects(&viewer), ["alpha", "Zeta"]);
        viewer.sort_by(Some((0, true)));
        assert_eq!(subjects(&viewer), ["Zeta", "alpha"]);
        viewer.sort_by(Some((6, false)));
        assert_eq!(viewer.rows()[0][6], "Blue");
        assert_eq!(viewer.rows()[1][6], "Red");
        // The selection follows its markup wherever it is listed.
        assert_eq!(viewer.selected_row(), Some(0));
        viewer.sort_by(None);
        assert_eq!(viewer.selected_row(), Some(1));
        assert!(viewer.rows()[0][4].starts_with("20"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn my_tools_draw_with_a_saved_style_and_are_kept() {
        let (mut viewer, dir) = viewer();
        let kept = dir.join("settings").join("tools.json");
        viewer.keep_saved_tools_at(kept.clone());
        assert!(viewer.save_selected_tool().is_err());
        viewer.choose_tool(Tool::Draw(Kind::Cloud));
        drag(&mut viewer, (100.0, 100.0), (200.0, 200.0));
        viewer.choose_color(3);
        viewer.choose_width(2);
        viewer.set_subject("RFI");
        viewer.save_selected_tool().unwrap();
        // Saved again under the same name, it is replaced, not repeated.
        viewer.save_selected_tool().unwrap();
        assert_eq!(viewer.saved_tools().len(), 1);

        viewer.choose_tool(Tool::Select);
        viewer.choose_color(0);
        viewer.use_saved_tool(0);
        assert_eq!(viewer.tool(), Tool::Draw(Kind::Cloud));
        assert_eq!(viewer.saved_in_use(), Some(0));
        drag(&mut viewer, (300.0, 300.0), (400.0, 400.0));
        let made = viewer.selected().unwrap();
        assert_eq!((made.subject.as_str(), made.color), ("RFI", COLORS[3].1));
        assert!((made.width - WIDTHS[2].1).abs() < f64::EPSILON);
        // Another tool leaves it.
        viewer.choose_tool(Tool::Draw(Kind::Cloud));
        assert_eq!(viewer.saved_in_use(), None);

        // Another window finds it, and removing it is kept too.
        let mut other = Viewer::default();
        other.keep_saved_tools_at(kept.clone());
        assert_eq!(other.saved_tools()[0].name, "RFI");
        other.remove_saved_tool(0).unwrap();
        let mut third = Viewer::default();
        third.keep_saved_tools_at(kept);
        assert_eq!(third.saved_tools(), []);
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
        assert_eq!(again.rows()[0][0], "Arrow");
        std::fs::remove_dir_all(dir).ok();
    }
}
