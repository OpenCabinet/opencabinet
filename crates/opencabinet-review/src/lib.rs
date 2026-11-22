// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! OpenCabinet Review's documents (RFC-0004): a PDF, the markups on it,
//! and each page's scale.
//!
//! A [`Review`] keeps the PDF's bytes as they were opened. The markups the
//! PDF already had become editable [`Markup`]s, whoever made them, and the
//! page is drawn from a copy without them, so a markup moves without
//! leaving its old picture behind. Annotations Review cannot draw, such as
//! another program's stamps, stay on the page as they were.
//!
//! Saving appends one incremental update to the original bytes (see
//! [`opencabinet_pdf::annotations`]): a markup that was not touched is
//! left exactly as it was, one only moved keeps its own look, and the
//! others are written as standard annotations, keeping every key Review
//! does not manage.

mod markup;
mod scale;
pub mod stamps;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use opencabinet_model::{Id, Length, Units};
use opencabinet_pdf::annotations::{self, Change, Markups, ObjectRef};
use opencabinet_pdf::{Document, Error, Point};

pub use markup::{
    COUNT_RADIUS, Caption, Color, FILL_OPACITY, FONT_SIZE, Gesture, Kind, Markup, NOTE_SIZE,
    Segment, Status, text_width,
};
pub use scale::{Scale, path_length, polygon_area, presets, shown, shown_area};

/// How many steps can be undone.
const HISTORY: usize = 200;

/// What can be undone: the markups, the scales, and what was removed.
#[derive(Clone, Debug, PartialEq)]
struct State {
    markups: Vec<Markup>,
    scales: BTreeMap<usize, Scale>,
    removed: Vec<ObjectRef>,
}

/// A PDF under review.
#[derive(Clone, Debug)]
pub struct Review {
    original: Document,
    /// The PDF the pages are drawn from: the original without the
    /// annotations in `hidden`, which Review draws itself.
    shown: Document,
    hidden: BTreeSet<ObjectRef>,
    /// Bumped whenever `shown` changes.
    shown_version: u64,
    /// Review's own entries for each page, as read.
    read_pages: BTreeMap<usize, BTreeMap<String, String>>,
    state: State,
    /// The markups read from the PDF, as read, by id.
    read: BTreeMap<String, Markup>,
    /// The page scales as read.
    read_scales: BTreeMap<usize, Scale>,
    undo: Vec<State>,
    redo: Vec<State>,
    /// The markup whose text is being typed, so typing is one step.
    typing: Option<String>,
    /// The state when last opened or saved.
    saved: State,
}

impl Review {
    /// Opens a PDF from its bytes.
    ///
    /// # Errors
    /// When it is not a PDF Review can read.
    pub fn open(bytes: Vec<u8>) -> Result<Self, Error> {
        let original = Document::open(bytes)?;
        let saved = annotations::read(original.bytes())?;
        let mut seen = std::collections::BTreeSet::new();
        let markups: Vec<Markup> = saved
            .annotations
            .iter()
            .filter_map(Markup::from_annotation)
            .map(|mut m| {
                // Ids must be unique; other programs' names may not be.
                if m.id.is_empty() || !seen.insert(m.id.clone()) {
                    m.id = Id::new().to_string();
                }
                m
            })
            .collect();
        let scales: BTreeMap<usize, Scale> = saved
            .pages
            .iter()
            .filter_map(|(&page, own)| {
                let points = own.get("ScalePoints")?.parse().ok()?;
                let real = Length::from_nm(own.get("ScaleNm")?.parse().ok()?);
                (points > 0.0 && real > Length::ZERO).then_some((page, Scale { points, real }))
            })
            .collect();
        let read = markups
            .iter()
            .filter(|m| m.origin.is_some())
            .map(|m| (m.id.clone(), m.clone()))
            .collect();
        let state = State {
            markups,
            scales: scales.clone(),
            removed: Vec::new(),
        };
        Ok(Self {
            // Markups read from the PDF are drawn from their own
            // appearance, as their program made them, until they change.
            shown: original.clone(),
            hidden: BTreeSet::new(),
            shown_version: 0,
            read_pages: saved.pages,
            original,
            saved: state.clone(),
            state,
            read,
            read_scales: scales,
            undo: Vec::new(),
            redo: Vec::new(),
            typing: None,
        })
    }

    /// The PDF as opened: its pages and their sizes.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.original
    }

    /// Whether Review draws `markup` itself: one made here, or one read
    /// from the PDF that has changed since. The others are drawn from
    /// their own appearance, as the program that made them drew them.
    #[must_use]
    pub fn draws(&self, markup: &Markup) -> bool {
        markup.origin.is_none() || self.change_of(markup) != Change::Unchanged
    }

    /// The PDF to draw pages from, without the annotations Review draws
    /// itself, and a number that changes whenever it does.
    ///
    /// # Errors
    /// When the PDF can no longer be written.
    pub fn page_document(&mut self) -> Result<(&Document, u64), Error> {
        let hidden: BTreeSet<ObjectRef> = self
            .state
            .markups
            .iter()
            .filter(|m| self.draws(m))
            .filter_map(|m| m.origin)
            .chain(self.state.removed.iter().copied())
            .collect();
        if hidden != self.hidden {
            self.shown = if hidden.is_empty() {
                self.original.clone()
            } else {
                let without = Markups {
                    annotations: Vec::new(),
                    pages: self.read_pages.clone(),
                    removed: hidden.iter().copied().collect(),
                };
                Document::open(annotations::save(self.original.bytes(), &without)?)?
            };
            self.hidden = hidden;
            self.shown_version += 1;
        }
        Ok((&self.shown, self.shown_version))
    }

    /// The markups, in the order they were made.
    #[must_use]
    pub fn markups(&self) -> &[Markup] {
        &self.state.markups
    }

    /// The markup with `id`.
    #[must_use]
    pub fn markup(&self, id: &str) -> Option<&Markup> {
        self.state.markups.iter().find(|m| m.id == id)
    }

    /// Whether there are changes not saved.
    #[must_use]
    pub fn modified(&self) -> bool {
        self.state != self.saved
    }

    /// Remembers the state before a change, so it can be undone.
    pub fn checkpoint(&mut self) {
        self.typing = None;
        self.remember();
    }

    fn remember(&mut self) {
        if self.undo.last() != Some(&self.state) {
            self.undo.push(self.state.clone());
            if self.undo.len() > HISTORY {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
    }

    /// Whether there is something to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.undo.iter().any(|s| *s != self.state)
    }

    /// Whether there is something to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Goes back one step.
    pub fn undo(&mut self) {
        self.typing = None;
        while let Some(state) = self.undo.pop() {
            if state != self.state {
                self.redo.push(std::mem::replace(&mut self.state, state));
                return;
            }
        }
    }

    /// Goes forward again after undoing.
    pub fn redo(&mut self) {
        self.typing = None;
        if let Some(state) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.state, state));
        }
    }

    /// Adds a markup made by `author`, and returns its id.
    pub fn add(&mut self, mut markup: Markup, author: &str) -> String {
        self.checkpoint();
        markup.id = Id::new().to_string();
        markup.origin = None;
        author.clone_into(&mut markup.author);
        markup.modified = now();
        let id = markup.id.clone();
        self.state.markups.push(markup);
        id
    }

    /// Removes the markup with `id`.
    pub fn remove(&mut self, id: &str) {
        let Some(at) = self.state.markups.iter().position(|m| m.id == id) else {
            return;
        };
        self.checkpoint();
        let markup = self.state.markups.remove(at);
        if let Some(origin) = markup.origin {
            self.state.removed.push(origin);
        }
    }

    /// Changes the markup with `id` through `change`, as one step.
    pub fn change(&mut self, id: &str, change: impl FnOnce(&mut Markup)) {
        self.checkpoint();
        self.change_quietly(id, change);
    }

    /// Changes the markup with `id` without starting a new step: for a
    /// drag, whose step starts with a [`checkpoint`](Self::checkpoint).
    pub fn change_quietly(&mut self, id: &str, change: impl FnOnce(&mut Markup)) {
        if let Some(markup) = self.state.markups.iter_mut().find(|m| m.id == id) {
            let before = markup.clone();
            change(markup);
            if *markup != before {
                markup.modified = now();
            }
        }
    }

    /// Sets the text of the markup with `id`; typing into one markup is
    /// one step.
    pub fn set_text(&mut self, id: &str, text: &str) {
        if self.typing.as_deref() != Some(id) {
            self.checkpoint();
            self.typing = Some(id.to_owned());
        }
        self.change_quietly(id, |m| text.clone_into(&mut m.text));
    }

    /// The topmost markup on `page` at `point`, within `tolerance` points.
    #[must_use]
    pub fn hit(&self, page: usize, point: Point, tolerance: f64) -> Option<&Markup> {
        self.state
            .markups
            .iter()
            .rev()
            .find(|m| m.page == page && m.hit(point, tolerance))
    }

    /// The scale of `page`, if it was calibrated.
    #[must_use]
    pub fn scale(&self, page: usize) -> Option<Scale> {
        self.state.scales.get(&page).copied()
    }

    /// Calibrates `page`.
    pub fn set_scale(&mut self, page: usize, scale: Scale) {
        if self.state.scales.get(&page) != Some(&scale) {
            self.checkpoint();
            self.state.scales.insert(page, scale);
        }
    }

    /// Calibrates every page with `scale`, as one step.
    pub fn set_scale_everywhere(&mut self, scale: Scale) {
        let pages = self.original.pages().len();
        if (0..pages).all(|page| self.state.scales.get(&page) == Some(&scale)) {
            return;
        }
        self.checkpoint();
        for page in 0..pages {
            self.state.scales.insert(page, scale);
        }
    }

    /// What a measuring markup shows on the page: its measurement when it
    /// can be read, nothing when its page has no scale.
    #[must_use]
    pub fn measured_on_page(&self, markup: &Markup, units: Units) -> Option<String> {
        if markup.kind != Kind::Count && self.scale_of(markup).is_none() {
            return None;
        }
        self.measured(markup, units)
    }

    /// The scale a markup measures with: its own, or its page's.
    #[must_use]
    pub fn scale_of(&self, markup: &Markup) -> Option<Scale> {
        markup.scale.or_else(|| self.scale(markup.page))
    }

    /// What a measuring markup measures, in `units`; `None` for other
    /// kinds.
    #[must_use]
    pub fn measured(&self, markup: &Markup, units: Units) -> Option<String> {
        if !markup.kind.measures() {
            return None;
        }
        if markup.kind == Kind::Count {
            return Some(markup.points.len().to_string());
        }
        let Some(scale) = self.scale_of(markup) else {
            return Some("Not to scale".to_owned());
        };
        Some(match markup.kind {
            Kind::Area => shown_area(scale.area_mm2(polygon_area(&markup.points)), units),
            _ => shown(scale.length(path_length(&markup.points)), units),
        })
    }

    /// The PDF as opened, with an incremental update holding what changed.
    /// Marks the review saved.
    ///
    /// # Errors
    /// When the PDF cannot be written.
    pub fn save(&mut self, units: Units) -> Result<Vec<u8>, Error> {
        let bytes = annotations::save(self.original.bytes(), &self.to_save(units))?;
        self.saved = self.state.clone();
        Ok(bytes)
    }

    fn to_save(&self, units: Units) -> Markups {
        let annotations = self
            .state
            .markups
            .iter()
            .map(|m| {
                let label = self.measured(m, units);
                let measure = match (m.kind, &label, self.scale_of(m)) {
                    (Kind::Count, ..) | (_, None, _) | (_, _, None) => None,
                    (_, Some(label), Some(scale)) => {
                        Some((label.as_str(), scale.to_measure(units)))
                    }
                };
                let mut annotation = m.to_annotation(measure);
                if let (Kind::Count, Some(label)) = (m.kind, &label) {
                    annotation.own.insert("Value".into(), label.clone());
                    annotation.contents = if m.text.is_empty() {
                        label.clone()
                    } else {
                        format!("{label}: {}", m.text)
                    };
                }
                annotation.change = self.change_of(m);
                annotation
            })
            .collect();
        let pages = self
            .state
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
        Markups {
            annotations,
            pages,
            removed: self.state.removed.clone(),
        }
    }

    /// What happened to a markup since it was read.
    fn change_of(&self, markup: &Markup) -> Change {
        let Some(read) = self
            .read
            .get(&markup.id)
            .filter(|_| markup.origin.is_some())
        else {
            return Change::New;
        };
        // A measurement whose page scale changed shows another value.
        let rescaled = markup.kind.measures()
            && markup.scale.is_none()
            && self.read_scales.get(&markup.page) != self.state.scales.get(&markup.page);
        if rescaled {
            return Change::Rewritten;
        }
        if markup == read {
            return Change::Unchanged;
        }
        let (Some(a), Some(b)) = (read.points.first(), markup.points.first()) else {
            return Change::Rewritten;
        };
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let mut moved = read.clone();
        moved.shift(dx, dy);
        moved.modified.clone_from(&markup.modified);
        let close = moved.points.len() == markup.points.len()
            && moved
                .points
                .iter()
                .zip(&markup.points)
                .all(|(p, q)| p.distance(*q) < 1e-6);
        moved.points.clone_from(&markup.points);
        if close && moved == *markup {
            Change::Moved { dx, dy }
        } else {
            Change::Rewritten
        }
    }

    /// The markup list for spreadsheets: one row per markup.
    #[must_use]
    pub fn csv(&self, units: Units) -> String {
        let mut out = String::from("Page,Type,Subject,Status,Author,Modified,Text,Measurement\r\n");
        for markup in &self.state.markups {
            let fields = [
                (markup.page + 1).to_string(),
                markup.kind.label().to_owned(),
                markup.subject().to_owned(),
                markup.status.label().to_owned(),
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
