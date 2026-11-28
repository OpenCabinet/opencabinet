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

pub mod captures;
mod markup;
mod scale;
pub mod sets;
pub mod stamps;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use opencabinet_model::{Id, Length, Units};
use opencabinet_pdf::annotations::{self, Annotation, Change, Markups, ObjectRef};
use opencabinet_pdf::pages::{self, From, Placed};
use opencabinet_pdf::{Document, Error, Point};

pub use opencabinet_pdf::annotations::Reply;

pub use markup::{
    COUNT_RADIUS, Caption, Color, FILL_OPACITY, FONT_SIZE, Gesture, Kind, Markup, NOTE_SIZE,
    Segment, Status, text_width,
};
pub use scale::{Scale, angle, path_length, perimeter, polygon_area, presets, shown, shown_area};

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
    /// The review as it was before its pages last changed, to go back to,
    /// and as it was after, to go forward to again.
    before: Option<Box<Review>>,
    after: Option<Box<Review>>,
    /// Whether the pages changed since the file was opened or saved.
    pages_changed: bool,
    /// The markups kept out of sight, by id: not drawn, even those drawn
    /// from their own look on the page. The file does not change.
    unseen: BTreeSet<String>,
    /// The captures drawn into `shown`, as they were then.
    shown_captures: Vec<Annotation>,
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
            before: None,
            after: None,
            pages_changed: false,
            unseen: BTreeSet::new(),
            shown_captures: Vec::new(),
        })
    }

    /// Keeps the markups with these ids out of sight; the file does not
    /// change, and they stay in the list.
    pub fn set_unseen(&mut self, ids: BTreeSet<String>) {
        self.unseen = ids;
    }

    /// Whether the markup is kept out of sight.
    #[must_use]
    pub fn unseen(&self, markup: &Markup) -> bool {
        self.unseen.contains(&markup.id)
    }

    /// Changes the pages to `list` (see [`pages::arrange`]), with the
    /// markups as they are now. Undo goes back to the pages as they were.
    ///
    /// # Errors
    /// When the pages cannot be changed; then nothing is.
    pub fn arrange(
        &mut self,
        list: &[Placed],
        others: &[&[u8]],
        units: Units,
    ) -> Result<(), Error> {
        let with_markups = annotations::save(self.original.bytes(), &self.to_save(units))?;
        let arranged = pages::arrange(&with_markups, list, others)?;
        let mut next = Self::open(arranged)?;
        next.pages_changed = true;
        let mut before = std::mem::replace(self, next);
        before.typing = None;
        before.after = None;
        self.before = Some(Box::new(before));
        Ok(())
    }

    /// Turns `chosen` pages by `turn` quarter turns clockwise.
    ///
    /// # Errors
    /// As [`Review::arrange`].
    pub fn turn_pages(&mut self, chosen: &[usize], turn: i32, units: Units) -> Result<(), Error> {
        let list: Vec<Placed> = (0..self.original.pages().len())
            .map(|page| Placed {
                from: From::Here(page),
                turn: if chosen.contains(&page) { turn } else { 0 },
            })
            .collect();
        self.arrange(&list, &[], units)
    }

    /// Removes `chosen` pages, with their markups.
    ///
    /// # Errors
    /// When every page would go, or as [`Review::arrange`].
    pub fn remove_pages(&mut self, chosen: &[usize], units: Units) -> Result<(), Error> {
        let list: Vec<Placed> = (0..self.original.pages().len())
            .filter(|page| !chosen.contains(page))
            .map(Placed::here)
            .collect();
        self.arrange(&list, &[], units)
    }

    /// Moves `chosen` pages, in their order, to before page `to` of the
    /// pages left, or to the end. Returns where the first of them is now.
    ///
    /// # Errors
    /// As [`Review::arrange`].
    pub fn move_pages(
        &mut self,
        chosen: &[usize],
        to: usize,
        units: Units,
    ) -> Result<usize, Error> {
        let count = self.original.pages().len();
        let mut moved: Vec<usize> = chosen.iter().copied().filter(|&p| p < count).collect();
        moved.sort_unstable();
        moved.dedup();
        let mut rest: Vec<usize> = (0..count).filter(|p| !moved.contains(p)).collect();
        let at = to.min(rest.len());
        rest.splice(at..at, moved);
        let list: Vec<Placed> = rest.into_iter().map(Placed::here).collect();
        self.arrange(&list, &[], units)?;
        Ok(at)
    }

    /// Puts `pages` of the PDF `other` before page `at`, or at the end.
    ///
    /// # Errors
    /// When `other` cannot be read, or as [`Review::arrange`].
    pub fn insert_pages(
        &mut self,
        at: usize,
        other: &[u8],
        pages: &[usize],
        units: Units,
    ) -> Result<(), Error> {
        let mut list: Vec<Placed> = (0..self.original.pages().len()).map(Placed::here).collect();
        let at = at.min(list.len());
        let new = pages.iter().map(|&p| Placed {
            from: From::Other(0, p),
            turn: 0,
        });
        list.splice(at..at, new);
        self.arrange(&list, &[other], units)
    }

    /// Puts a blank page as large as page `like` before page `at`.
    ///
    /// # Errors
    /// As [`Review::arrange`].
    pub fn insert_blank(&mut self, at: usize, like: usize, units: Units) -> Result<(), Error> {
        let pages = self.original.pages();
        let size = pages.get(like).or(pages.first()).ok_or(Error::NoPages)?;
        let mut list: Vec<Placed> = (0..pages.len()).map(Placed::here).collect();
        let at = at.min(list.len());
        list.insert(
            at,
            Placed {
                from: From::Blank(size.width, size.height),
                turn: 0,
            },
        );
        self.arrange(&list, &[], units)
    }

    /// A PDF of its own holding `chosen` pages, in their order, with their
    /// markups as they are now. The review does not change.
    ///
    /// # Errors
    /// When the pages cannot be written.
    pub fn extract(&self, chosen: &[usize], units: Units) -> Result<Vec<u8>, Error> {
        let with_markups = annotations::save(self.original.bytes(), &self.to_save(units))?;
        pages::extract(&with_markups, chosen)
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
            .filter(|m| self.draws(m) || self.unseen.contains(&m.id))
            .filter_map(|m| m.origin)
            .chain(self.state.removed.iter().copied())
            .collect();
        // Captures show a part of a page: drawn into the page, from its own
        // content, not by the window.
        let captures: Vec<Annotation> = self
            .state
            .markups
            .iter()
            .filter(|m| m.kind == Kind::Capture && self.draws(m) && !self.unseen.contains(&m.id))
            .map(|m| {
                let mut a = m.to_annotation(None);
                a.origin = None;
                a
            })
            .collect();
        if hidden != self.hidden || captures != self.shown_captures {
            self.shown = if hidden.is_empty() && captures.is_empty() {
                self.original.clone()
            } else {
                let without = Markups {
                    annotations: captures.clone(),
                    pages: self.read_pages.clone(),
                    removed: hidden.iter().copied().collect(),
                };
                Document::open(annotations::save(self.original.bytes(), &without)?)?
            };
            self.hidden = hidden;
            self.shown_captures = captures;
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
        self.pages_changed || self.state != self.saved
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
        self.after = None;
    }

    /// Whether there is something to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.before.is_some() || self.undo.iter().any(|s| *s != self.state)
    }

    /// Whether there is something to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.after.is_some() || !self.redo.is_empty()
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
        // Back past the last change of pages.
        if let Some(before) = self.before.take() {
            let unseen = std::mem::take(&mut self.unseen);
            let mut after = std::mem::replace(self, *before);
            self.unseen = unseen;
            after.typing = None;
            self.after = Some(Box::new(after));
            // The file may hold either; only saving says which.
            self.pages_changed = true;
        }
    }

    /// Goes forward again after undoing.
    pub fn redo(&mut self) {
        self.typing = None;
        if let Some(state) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.state, state));
        } else if let Some(after) = self.after.take() {
            let before = std::mem::replace(self, *after);
            self.before = Some(Box::new(before));
            self.pages_changed = true;
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

    /// Adds `markups` made by `author` as one step; their ids.
    pub fn add_all(&mut self, markups: Vec<Markup>, author: &str) -> Vec<String> {
        if markups.is_empty() {
            return Vec::new();
        }
        self.checkpoint();
        markups
            .into_iter()
            .map(|mut markup| {
                markup.id = Id::new().to_string();
                markup.origin = None;
                author.clone_into(&mut markup.author);
                markup.modified = now();
                let id = markup.id.clone();
                self.state.markups.push(markup);
                id
            })
            .collect()
    }

    /// Removes the markup with `id`, and its replies.
    pub fn remove(&mut self, id: &str) {
        self.remove_all(&[id.to_owned()]);
    }

    /// Removes the markups with `ids`, and their replies, as one step.
    pub fn remove_all(&mut self, ids: &[String]) {
        if !self.state.markups.iter().any(|m| ids.contains(&m.id)) {
            return;
        }
        self.checkpoint();
        let (gone, kept): (Vec<Markup>, Vec<Markup>) = std::mem::take(&mut self.state.markups)
            .into_iter()
            .partition(|m| ids.contains(&m.id));
        self.state.markups = kept;
        for markup in gone {
            let origins = markup
                .origin
                .into_iter()
                .chain(markup.replies.iter().filter_map(|r| r.origin));
            self.state.removed.extend(origins);
        }
    }

    /// Adds a reply by `author` to the markup with `id`; whether there
    /// was such a markup and something to say. The markup itself does
    /// not change, so another program's markup keeps its own look.
    pub fn reply(&mut self, id: &str, text: &str, author: &str) -> bool {
        let text = text.trim();
        if text.is_empty() || !self.state.markups.iter().any(|m| m.id == id) {
            return false;
        }
        self.checkpoint();
        if let Some(markup) = self.state.markups.iter_mut().find(|m| m.id == id) {
            markup.replies.push(Reply {
                name: Id::new().to_string(),
                author: author.to_owned(),
                contents: text.to_owned(),
                modified: now(),
                origin: None,
            });
        }
        true
    }

    /// Removes reply `index` of the markup with `id`.
    pub fn remove_reply(&mut self, id: &str, index: usize) {
        let exists = self
            .state
            .markups
            .iter()
            .any(|m| m.id == id && index < m.replies.len());
        if !exists {
            return;
        }
        self.checkpoint();
        if let Some(markup) = self.state.markups.iter_mut().find(|m| m.id == id) {
            let reply = markup.replies.remove(index);
            if let Some(origin) = reply.origin {
                self.state.removed.push(origin);
            }
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
            .find(|m| m.page == page && !self.unseen.contains(&m.id) && m.hit(point, tolerance))
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

    /// The pages' scales.
    #[must_use]
    pub fn scales(&self) -> &BTreeMap<usize, Scale> {
        &self.state.scales
    }

    /// Takes the markups and scales a review session holds, as others
    /// left them; whether anything changed. A markup read from the PDF
    /// that is no longer there is removed when saving. What could be
    /// undone before is forgotten: it was undone against another state.
    pub fn take_shared(&mut self, markups: Vec<Markup>, scales: BTreeMap<usize, Scale>) -> bool {
        if markups == self.state.markups && scales == self.state.scales {
            return false;
        }
        let replies_of = |markup: &Markup| -> BTreeSet<ObjectRef> {
            markup.replies.iter().filter_map(|r| r.origin).collect()
        };
        let mut gone: Vec<ObjectRef> = Vec::new();
        for old in &self.state.markups {
            match markups.iter().find(|n| n.id == old.id) {
                None => gone.extend(old.origin.into_iter().chain(replies_of(old))),
                Some(new) => gone.extend(replies_of(old).difference(&replies_of(new))),
            }
        }
        for gone in gone {
            if !self.state.removed.contains(&gone) {
                self.state.removed.push(gone);
            }
        }
        self.state.markups = markups;
        self.state.scales = scales;
        self.undo.clear();
        self.redo.clear();
        self.typing = None;
        true
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
        if markup.kind == Kind::Angle {
            // An angle is the same at any scale.
            return angle(&markup.points).map(|degrees| format!("{degrees:.1}°"));
        }
        let Some(scale) = self.scale_of(markup) else {
            return Some("Not to scale".to_owned());
        };
        Some(match markup.kind {
            Kind::Area => shown_area(scale.area_mm2(polygon_area(&markup.points)), units),
            Kind::Perimeter => shown(scale.length(perimeter(&markup.points)), units),
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
        self.pages_changed = false;
        Ok(bytes)
    }

    /// A copy of the PDF with every markup drawn into its page, to send
    /// where it must not be edited. The review itself is not saved.
    ///
    /// # Errors
    /// When the PDF cannot be written.
    pub fn flattened(&self, units: Units) -> Result<Vec<u8>, Error> {
        let bytes = annotations::save(self.original.bytes(), &self.to_save(units))?;
        opencabinet_pdf::flatten(&bytes)
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
        // Replies are written on their own: answering a markup does not
        // change it.
        let markup = &Markup {
            replies: read.replies.clone(),
            ..markup.clone()
        };
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
        let mut out =
            String::from("Page,Type,Subject,Status,Author,Modified,Text,Measurement,Replies\r\n");
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
                markup
                    .replies
                    .iter()
                    .map(|r| format!("{}: {}", r.author, r.contents))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ];
            let row: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
            let _ = write!(out, "{}\r\n", row.join(","));
        }
        out
    }

    /// What the measuring markups add up to, by kind and subject, in the
    /// order they were first made: lengths and areas summed at each page's
    /// scale, counts counted.
    #[must_use]
    pub fn quantities(&self, units: Units) -> Vec<Quantity> {
        let mut groups: Vec<(Kind, String, Vec<&Markup>)> = Vec::new();
        for markup in self.state.markups.iter().filter(|m| m.kind.measures()) {
            let subject = markup.subject().to_owned();
            match groups
                .iter_mut()
                .find(|(k, s, _)| *k == markup.kind && *s == subject)
            {
                Some((_, _, list)) => list.push(markup),
                None => groups.push((markup.kind, subject, vec![markup])),
            }
        }
        groups
            .into_iter()
            .map(|(kind, subject, list)| {
                let pages: BTreeSet<usize> = list.iter().map(|m| m.page + 1).collect();
                let scaled: Vec<(&Markup, Scale)> = list
                    .iter()
                    .filter_map(|m| self.scale_of(m).map(|s| (*m, s)))
                    .collect();
                let unscaled = match kind {
                    Kind::Count | Kind::Angle => 0,
                    _ => list.len() - scaled.len(),
                };
                let total = match kind {
                    Kind::Count => list
                        .iter()
                        .map(|m| m.points.len())
                        .sum::<usize>()
                        .to_string(),
                    Kind::Angle => String::new(),
                    Kind::Area => shown_area(
                        scaled
                            .iter()
                            .map(|(m, s)| s.area_mm2(polygon_area(&m.points)))
                            .sum(),
                        units,
                    ),
                    _ => {
                        let nm: i64 = scaled
                            .iter()
                            .map(|(m, s)| {
                                let points = if kind == Kind::Perimeter {
                                    perimeter(&m.points)
                                } else {
                                    path_length(&m.points)
                                };
                                s.length(points).nm()
                            })
                            .sum();
                        shown(Length::from_nm(nm), units)
                    }
                };
                Quantity {
                    kind,
                    subject,
                    items: list.len(),
                    pages: pages
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    total,
                    unscaled,
                }
            })
            .collect()
    }

    /// The quantities for spreadsheets: one row per kind and subject.
    #[must_use]
    pub fn quantities_csv(&self, units: Units) -> String {
        let mut out = String::from("Type,Subject,Items,Pages,Total,Not to scale\r\n");
        for q in self.quantities(units) {
            let fields = [
                q.kind.label().to_owned(),
                q.subject,
                q.items.to_string(),
                q.pages,
                q.total,
                q.unscaled.to_string(),
            ];
            let row: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
            let _ = write!(out, "{}\r\n", row.join(","));
        }
        out
    }
}

/// What the measuring markups of one kind and subject add up to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quantity {
    /// What they measure.
    pub kind: Kind,
    /// Their subject.
    pub subject: String,
    /// How many markups.
    pub items: usize,
    /// The pages they are on, from 1, as `2, 5`.
    pub pages: String,
    /// What they add up to, in the units asked for; empty for angles.
    pub total: String,
    /// How many could not be measured, their page having no scale.
    pub unscaled: usize,
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
