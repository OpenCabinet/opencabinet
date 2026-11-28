// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The markups a PDF holds, as standard annotations (ISO 32000-2 §12.5).
//!
//! [`read`] returns the annotations of the kinds Review can edit, each with
//! the object it was read from. [`save`] appends an incremental update:
//! the original bytes stay as they were, and only what changed is written.
//!
//! - An annotation read and not changed is left exactly as it was: the
//!   same object, its own appearance, and every key Review does not know,
//!   whichever program made it.
//! - One that was only moved keeps its object, its appearance and every
//!   key; its coordinates shift.
//! - One that was changed otherwise keeps its object and every key Review
//!   does not manage; Review's keys and its appearance are written anew.
//! - One that was removed goes, with its pop-up note and the replies to
//!   it.
//! - Replies to a markup (text annotations "in reply to" it, §12.5.6.10)
//!   are read with it. A new reply is written as one more such annotation,
//!   with nothing to draw; the replies that were there stay as they were.
//! - Annotations Review does not edit (links, form fields and the like)
//!   are never touched.
//!
//! Each annotation Review writes also carries a small dictionary of its own
//! under the key `OpenCabinet`, which other programs ignore; every standard
//! key stays correct on its own, so other programs show the markup.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, IncrementalDocument, Object, ObjectId, Stream, StringFormat, dictionary};

pub use crate::text::win_ansi_literal;
use crate::{Error, Point, text};

/// The key of Review's own dictionary, in annotations and pages.
const OWN: &[u8] = b"OpenCabinet";

/// The name of the font appearance streams may use, Helvetica.
pub const FONT: &str = "Helv";

/// The name of the graphics state appearance streams may set with `gs`
/// for the annotation's opacity.
pub const STATE: &str = "GS";

/// An object of the PDF: its number and generation.
pub type ObjectRef = (u32, u16);

/// The standard annotation types Review edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subtype {
    /// A rectangle.
    Square,
    /// An ellipse.
    Circle,
    /// A straight line, with or without arrow heads.
    Line,
    /// Connected lines.
    PolyLine,
    /// A closed shape, such as a cloud.
    Polygon,
    /// Freehand strokes.
    Ink,
    /// A text box, with or without a callout line.
    FreeText,
    /// A note shown as an icon.
    Text,
    /// A stamp.
    Stamp,
    /// A link Review made; others' links are left alone and not read.
    Link,
}

impl Subtype {
    const ALL: [Self; 10] = [
        Self::Square,
        Self::Circle,
        Self::Line,
        Self::PolyLine,
        Self::Polygon,
        Self::Ink,
        Self::FreeText,
        Self::Text,
        Self::Stamp,
        Self::Link,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Line => "Line",
            Self::PolyLine => "PolyLine",
            Self::Polygon => "Polygon",
            Self::Ink => "Ink",
            Self::FreeText => "FreeText",
            Self::Text => "Text",
            Self::Stamp => "Stamp",
            Self::Link => "Link",
        }
    }

    fn from_name(name: &[u8]) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name().as_bytes() == name)
    }
}

/// What happened to an annotation since it was read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Change {
    /// Made in Review: written as a new object.
    #[default]
    New,
    /// Read and left alone: nothing is written.
    Unchanged,
    /// Moved by `dx`, `dy` and nothing else: its coordinates shift; its
    /// appearance and every key are kept.
    Moved {
        /// Across, in PDF space.
        dx: f64,
        /// Up, in PDF space.
        dy: f64,
    },
    /// Changed otherwise: Review's keys and appearance are written over it.
    Rewritten,
}

/// A real-world scale a measurement carries (ISO 32000-2 §12.9): one point
/// on the sheet is `factor` `unit`s.
#[derive(Clone, Debug, PartialEq)]
pub struct Measure {
    /// The scale as people read it, such as `1 in = 4 ft`.
    pub ratio: String,
    /// The unit, such as `in`, `ft`, `mm` or `m`.
    pub unit: String,
    /// Units per point.
    pub factor: f64,
}

/// A reply to an annotation: a text annotation "in reply to" it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    /// A name unique in the document, kept across saves.
    pub name: String,
    /// Who wrote it.
    pub author: String,
    /// What it says.
    pub contents: String,
    /// When it was written, as a PDF date.
    pub modified: String,
    /// The object it was read from; `None` for one written in Review.
    pub origin: Option<ObjectRef>,
}

/// How deep a chain of replies to replies is followed.
const REPLY_DEPTH: usize = 16;

/// One annotation, in PDF space.
#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    /// The page, from 0.
    pub page: usize,
    /// What it is.
    pub subtype: Subtype,
    /// Its bounds: left, bottom, right, top.
    pub rect: [f64; 4],
    /// A line's two ends, a polyline's or polygon's vertices, or the points
    /// of a freehand stroke; empty for the others.
    pub vertices: Vec<Point>,
    /// A text box's callout line, from its tip to the box.
    pub callout: Vec<Point>,
    /// How far inside `rect` a text box or shape is drawn: left, bottom,
    /// right, top (`RD`); a callout's box is `rect` less these.
    pub inset: [f64; 4],
    /// Stroke color, RGB from 0 to 1.
    pub color: [f64; 3],
    /// Fill color, if filled.
    pub interior: Option<[f64; 3]>,
    /// Opacity, from 0 to 1.
    pub opacity: f64,
    /// The opacity of its fill inside its appearance, from 0 to 1; other
    /// programs do not read it back.
    pub fill_opacity: f64,
    /// Stroke width, in points.
    pub width: f64,
    /// Its text: a comment, or what a text box or stamp shows.
    pub contents: String,
    /// Its subject, such as `Cloud` or `Approved`.
    pub subject: String,
    /// Who made it.
    pub author: String,
    /// A name unique on the page, kept across saves.
    pub name: String,
    /// When it last changed, as a PDF date (`D:20261121153000Z`).
    pub modified: String,
    /// What it is for, such as `LineDimension` for a measured line.
    pub intent: String,
    /// A note's or stamp's icon name.
    pub icon: String,
    /// Whether a rectangle or polygon has a cloudy border.
    pub cloudy: bool,
    /// Whether a line or polyline ends in an arrow head.
    pub arrow: bool,
    /// A text box's font size, in points.
    pub font_size: f64,
    /// The scale of a measurement.
    pub measure: Option<Measure>,
    /// Review's own entries.
    pub own: BTreeMap<String, String>,
    /// How it looks: a content stream drawn in PDF space and clipped to
    /// `rect`, which may use the font [`FONT`] and the graphics state
    /// [`STATE`]. Empty when read from a file.
    pub appearance: Vec<u8>,
    /// The object it was read from; `None` for one made in Review.
    pub origin: Option<ObjectRef>,
    /// What happened to it since it was read.
    pub change: Change,
    /// The replies to it, in the order the PDF lists them; replies to
    /// replies come with the markup they answer.
    pub replies: Vec<Reply>,
    /// Where a link goes, when it is one, to write its action.
    pub link: Option<crate::links::Target>,
    /// The part of a page it shows, when it is a capture: drawn from the
    /// page's own content instead of `appearance`.
    pub capture: Option<Capture>,
    /// The bounds and resources of its appearance when they are not the
    /// usual ones, as for a capture.
    pub form: Option<([f64; 4], crate::capture::Recolored)>,
}

/// The part of a page a capture shows, and its colors changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    /// The page, from 0.
    pub page: usize,
    /// The part shown, in that page's PDF space: left, bottom, right, top.
    pub region: [f64; 4],
    /// Colors changed, from and to.
    pub colors: Vec<(crate::capture::Rgb, crate::capture::Rgb)>,
}

impl Default for Annotation {
    fn default() -> Self {
        Self {
            page: 0,
            subtype: Subtype::Square,
            rect: [0.0; 4],
            vertices: Vec::new(),
            callout: Vec::new(),
            inset: [0.0; 4],
            color: [1.0, 0.0, 0.0],
            interior: None,
            opacity: 1.0,
            fill_opacity: 1.0,
            width: 1.0,
            contents: String::new(),
            subject: String::new(),
            author: String::new(),
            name: String::new(),
            modified: String::new(),
            intent: String::new(),
            icon: String::new(),
            cloudy: false,
            arrow: false,
            font_size: 12.0,
            measure: None,
            own: BTreeMap::new(),
            appearance: Vec::new(),
            origin: None,
            change: Change::New,
            replies: Vec::new(),
            link: None,
            capture: None,
            form: None,
        }
    }
}

/// What a PDF holds for Review, and what changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Markups {
    /// The editable annotations, page by page.
    pub annotations: Vec<Annotation>,
    /// Review's own entries for each page that has some, such as its
    /// scale.
    pub pages: BTreeMap<usize, BTreeMap<String, String>>,
    /// Annotations read from the PDF and since removed.
    pub removed: Vec<ObjectRef>,
}

/// Reads what `bytes` holds for Review. Every annotation comes back
/// [`Change::Unchanged`], with its `origin`.
///
/// # Errors
/// When the PDF cannot be read.
pub fn read(bytes: &[u8]) -> Result<Markups, Error> {
    let document =
        lopdf::Document::load_mem(bytes).map_err(|e| Error::Annotations(e.to_string()))?;
    let mut markups = Markups::default();
    for (index, page_id) in document.get_pages().values().enumerate() {
        let Ok(page) = document.get_dictionary(*page_id) else {
            continue;
        };
        if let Some(own) = own_entries(&document, page) {
            markups.pages.insert(index, own);
        }
        for entry in page_annotations(&document, page) {
            // Only annotations that are objects of their own can be kept
            // in place.
            let Ok(id) = entry.as_reference() else {
                continue;
            };
            let Ok(dict) = document.get_dictionary(id) else {
                continue;
            };
            if let Some(mut annotation) = read_annotation(&document, dict, index) {
                annotation.origin = Some(id);
                annotation.change = Change::Unchanged;
                markups.annotations.push(annotation);
            }
        }
    }
    read_replies(&document, &mut markups.annotations);
    Ok(markups)
}

/// Gives each annotation in `annotations` the replies the PDF holds for
/// it.
fn read_replies(document: &lopdf::Document, annotations: &mut [Annotation]) {
    let index: BTreeMap<ObjectId, usize> = annotations
        .iter()
        .enumerate()
        .filter_map(|(i, a)| Some((a.origin?, i)))
        .collect();
    if index.is_empty() {
        return;
    }
    for page_id in document.get_pages().values() {
        let Ok(page) = document.get_dictionary(*page_id) else {
            continue;
        };
        for entry in page_annotations(document, page) {
            let Ok(id) = entry.as_reference() else {
                continue;
            };
            let Ok(dict) = document.get_dictionary(id) else {
                continue;
            };
            if dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Text".as_slice()) {
                continue;
            }
            // A group (RT /Group) is one markup in several pieces, not a
            // reply.
            if dict
                .get(b"RT")
                .and_then(Object::as_name)
                .is_ok_and(|rt| rt != b"R")
            {
                continue;
            }
            // The markup the chain of replies starts from.
            let mut parent = dict.get(b"IRT").and_then(Object::as_reference).ok();
            let mut depth = 0;
            while let Some(at) = parent {
                if index.contains_key(&at) || depth >= REPLY_DEPTH {
                    break;
                }
                parent = document
                    .get_dictionary(at)
                    .ok()
                    .and_then(|d| d.get(b"IRT").and_then(Object::as_reference).ok());
                depth += 1;
            }
            let Some(&owner) = parent.and_then(|p| index.get(&p)) else {
                continue;
            };
            annotations[owner].replies.push(Reply {
                name: string(document, dict, b"NM"),
                author: string(document, dict, b"T"),
                contents: string(document, dict, b"Contents"),
                modified: string(document, dict, b"M"),
                origin: Some(id),
            });
        }
    }
}

/// A new reply to the annotation `parent`, whose bounds are `rect`: a
/// closed note with nothing to draw, so other programs list it under the
/// markup instead of showing an icon on the page.
fn reply_dictionary(
    document: &mut lopdf::Document,
    page: ObjectId,
    parent: ObjectId,
    rect: [f64; 4],
    reply: &Reply,
) -> Dictionary {
    let [x0, _, _, y1] = rect;
    let rect = [x0, y1 - 18.0, x0 + 18.0, y1];
    let appearance = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => reals(rect),
        },
        Vec::new(),
    ));
    dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "P" => page,
        "F" => 4,
        "Rect" => reals(rect),
        "IRT" => parent,
        "RT" => "R",
        "Name" => "Comment",
        "Open" => false,
        "Contents" => text_string(&reply.contents),
        "T" => text_string(&reply.author),
        "NM" => text_string(&reply.name),
        "M" => text_string(&reply.modified),
        "AP" => dictionary! { "N" => appearance },
    }
}

/// `bytes` with an incremental update for what changed in `markups`, and
/// Review's entries for each page as in `markups`. Nothing is appended
/// when nothing changed.
///
/// # Errors
/// When the PDF cannot be read, or an annotation is on a page it lacks.
pub fn save(bytes: &[u8], markups: &Markups) -> Result<Vec<u8>, Error> {
    let fail = |e: lopdf::Error| Error::Annotations(e.to_string());
    let prev = lopdf::Document::load_mem(bytes).map_err(fail)?;
    let prepared = with_captures(&prev, markups)?;
    let markups = &prepared;
    let mut document = IncrementalDocument::create_from(bytes.to_vec(), prev);
    let pages: Vec<ObjectId> = document
        .get_prev_documents()
        .get_pages()
        .values()
        .copied()
        .collect();
    if let Some(bad) = markups.annotations.iter().find(|a| a.page >= pages.len()) {
        return Err(Error::Annotations(format!(
            "a markup is on page {}, which the PDF does not have",
            bad.page + 1
        )));
    }
    let removed: BTreeSet<ObjectId> = markups.removed.iter().copied().collect();
    let mut font = None;

    let mut changed = rewrite_in_place(&mut document, markups, &mut font, &pages);

    for (index, &page_id) in pages.iter().enumerate() {
        let prev = document.get_prev_documents();
        let page = prev.get_dictionary(page_id).map_err(fail)?;
        let old: Vec<Object> = page_annotations(prev, page).into_iter().cloned().collect();
        let kept: Vec<Object> = old
            .iter()
            .filter(|entry| !goes_with_removed(prev, entry, &removed))
            .cloned()
            .collect();
        let new: Vec<&Annotation> = markups
            .annotations
            .iter()
            .filter(|a| a.page == index && a.origin.is_none())
            .collect();
        // New replies to annotations the PDF already holds.
        let answered: Vec<&Annotation> = markups
            .annotations
            .iter()
            .filter(|a| {
                a.page == index
                    && a.origin.is_some()
                    && a.replies.iter().any(|r| r.origin.is_none())
            })
            .collect();
        let own_old = own_entries(prev, page).unwrap_or_default();
        let own_new = markups.pages.get(&index).cloned().unwrap_or_default();
        if kept.len() == old.len() && new.is_empty() && answered.is_empty() && own_old == own_new {
            continue;
        }
        changed = true;
        let mut annots = kept;
        append_new(
            &mut document.new_document,
            (page_id, &pages),
            (&new, &answered),
            &mut font,
            &mut annots,
        );
        document
            .opt_clone_object_to_new_document(page_id)
            .map_err(fail)?;
        let page = document
            .new_document
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .map_err(fail)?;
        if annots.is_empty() {
            page.remove(b"Annots");
        } else {
            page.set("Annots", annots);
        }
        if own_new.is_empty() {
            page.remove(OWN);
        } else {
            page.set(OWN, own_dictionary(&own_new));
        }
    }
    if !changed {
        return Ok(bytes.to_vec());
    }
    let mut out = Vec::with_capacity(bytes.len() + 4096);
    document
        .save_to(&mut out)
        .map_err(|e| Error::Annotations(e.to_string()))?;
    Ok(out)
}

/// `markups` with each capture's appearance drawn from its page's content.
fn with_captures(document: &lopdf::Document, markups: &Markups) -> Result<Markups, Error> {
    let mut prepared = markups.clone();
    if prepared.annotations.iter().all(|a| a.capture.is_none()) {
        return Ok(prepared);
    }
    let pages: Vec<ObjectId> = document.get_pages().values().copied().collect();
    for annotation in &mut prepared.annotations {
        let Some(capture) = annotation.capture.as_ref() else {
            continue;
        };
        let page = *pages.get(capture.page).ok_or_else(|| {
            Error::Annotations(format!(
                "a capture shows page {}, which the PDF does not have",
                capture.page + 1
            ))
        })?;
        let (content, resources) =
            crate::capture::form(document, page, capture.region, &capture.colors)?;
        annotation.appearance = content;
        annotation.form = Some((capture.region, resources));
    }
    Ok(prepared)
}

/// Puts the forms `recolored` changed in `document`; the resources that
/// draw with them.
fn add_recolored(
    document: &mut lopdf::Document,
    recolored: &crate::capture::Recolored,
) -> Dictionary {
    let mut resources = recolored.resources.clone();
    if recolored.forms.is_empty() {
        return resources;
    }
    let mut xobjects = resources
        .get(b"XObject")
        .and_then(Object::as_dict)
        .cloned()
        .unwrap_or_default();
    for (name, form) in &recolored.forms {
        let mut dict = form.dict.clone();
        dict.set("Resources", add_recolored(document, &form.inner));
        let id = document.add_object(Stream::new(dict, form.content.clone()));
        xobjects.set(name.clone(), Object::Reference(id));
    }
    resources.set("XObject", xobjects);
    resources
}

/// Adds to `annots`, the entries of page `page`, the annotations in `new`
/// with their replies, and the new replies to the annotations in
/// `answered`.
fn append_new(
    document: &mut lopdf::Document,
    (page, pages): (ObjectId, &[ObjectId]),
    (new, answered): (&[&Annotation], &[&Annotation]),
    font: &mut Option<ObjectId>,
    annots: &mut Vec<Object>,
) {
    for annotation in new {
        let font = *font.get_or_insert_with(|| add_font(document));
        let mut dict = dictionary! {
            "Type" => "Annot",
            "P" => page,
            // Printed with the page.
            "F" => 4,
        };
        fill(document, &mut dict, annotation, (font, pages));
        let id = document.add_object(dict);
        annots.push(Object::Reference(id));
        for reply in &annotation.replies {
            let dict = reply_dictionary(document, page, id, annotation.rect, reply);
            annots.push(Object::Reference(document.add_object(dict)));
        }
    }
    for annotation in answered {
        let Some(parent) = annotation.origin else {
            continue;
        };
        for reply in annotation.replies.iter().filter(|r| r.origin.is_none()) {
            let dict = reply_dictionary(document, page, parent, annotation.rect, reply);
            annots.push(Object::Reference(document.add_object(dict)));
        }
    }
}

/// Writes the annotations read from the PDF that changed over their own
/// objects; whether any did.
fn rewrite_in_place(
    document: &mut IncrementalDocument,
    markups: &Markups,
    font: &mut Option<ObjectId>,
    pages: &[ObjectId],
) -> bool {
    let mut changed = false;
    for annotation in &markups.annotations {
        let Some(id) = annotation.origin else {
            continue;
        };
        let original = match document.get_prev_documents().get_dictionary(id) {
            Ok(dict) => dict.clone(),
            Err(_) => continue,
        };
        let dict = match annotation.change {
            Change::New | Change::Unchanged => continue,
            Change::Moved { dx, dy } => {
                let mut dict = original;
                shift(&mut dict, dx, dy);
                dict.set("M", text_string(&annotation.modified));
                dict
            }
            Change::Rewritten => {
                let font = *font.get_or_insert_with(|| add_font(&mut document.new_document));
                let mut dict = original;
                fill(
                    &mut document.new_document,
                    &mut dict,
                    annotation,
                    (font, pages),
                );
                dict
            }
        };
        document.new_document.set_object(id, dict);
        changed = true;
    }

    changed
}

/// Whether the page's `entry` is a removed annotation, the pop-up note of
/// one, or a reply to one.
fn goes_with_removed(
    document: &lopdf::Document,
    entry: &Object,
    removed: &BTreeSet<ObjectId>,
) -> bool {
    let Ok(id) = entry.as_reference() else {
        return false;
    };
    if removed.contains(&id) {
        return true;
    }
    let Ok(dict) = document.get_dictionary(id) else {
        return false;
    };
    [b"Parent".as_slice(), b"IRT"].iter().any(|key| {
        dict.get(key)
            .and_then(Object::as_reference)
            .is_ok_and(|other| removed.contains(&other))
    })
}

fn add_font(document: &mut lopdf::Document) -> ObjectId {
    document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    })
}

/// The entries of a page's `Annots`, which may be a reference to an array.
fn page_annotations<'a>(document: &'a lopdf::Document, page: &'a Dictionary) -> Vec<&'a Object> {
    let Ok(annots) = page.get(b"Annots") else {
        return Vec::new();
    };
    match document.dereference(annots) {
        Ok((_, Object::Array(items))) => items.iter().collect(),
        _ => Vec::new(),
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "coordinates in a PDF are far below 2^52"
)]
fn number(object: &Object) -> Option<f64> {
    match *object {
        Object::Integer(i) => Some(i as f64),
        // The shortest decimal that reads back as the same single precision
        // number: 0.85 comes back as 0.85, not 0.8500000238.
        Object::Real(r) => r.to_string().parse().ok(),
        _ => None,
    }
}

fn numbers(document: &lopdf::Document, object: Option<&Object>) -> Vec<f64> {
    match object.map(|o| document.dereference(o)) {
        Some(Ok((_, Object::Array(items)))) => items.iter().filter_map(number).collect(),
        _ => Vec::new(),
    }
}

fn string(document: &lopdf::Document, dict: &Dictionary, key: &[u8]) -> String {
    match dict.get(key).map(|o| document.dereference(o)) {
        Ok(Ok((_, Object::String(bytes, _)))) => text::decode(bytes),
        Ok(Ok((_, Object::Name(name)))) => String::from_utf8_lossy(name).into_owned(),
        _ => String::new(),
    }
}

fn dict_in<'a>(
    document: &'a lopdf::Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Dictionary> {
    match document.dereference(dict.get(key).ok()?) {
        Ok((_, Object::Dictionary(inner))) => Some(inner),
        _ => None,
    }
}

fn points(values: &[f64]) -> Vec<Point> {
    values
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[x, y]| Point::new(x, y))
        .collect()
}

fn own_entries(document: &lopdf::Document, dict: &Dictionary) -> Option<BTreeMap<String, String>> {
    let own = dict_in(document, dict, OWN)?;
    let entries: BTreeMap<String, String> = own
        .iter()
        .map(|(key, _)| {
            (
                String::from_utf8_lossy(key).into_owned(),
                string(document, own, key),
            )
        })
        .collect();
    (!entries.is_empty()).then_some(entries)
}

fn own_dictionary(entries: &BTreeMap<String, String>) -> Dictionary {
    let mut dict = Dictionary::new();
    for (key, value) in entries {
        dict.set(key.as_bytes().to_vec(), text_string(value));
    }
    dict
}

fn text_string(text: &str) -> Object {
    let bytes = text::encode(text);
    let format = if text.is_ascii() {
        StringFormat::Literal
    } else {
        StringFormat::Hexadecimal
    };
    Object::String(bytes, format)
}

fn rgb(values: &[f64]) -> Option<[f64; 3]> {
    match *values {
        [r, g, b] => Some([r, g, b]),
        [gray] => Some([gray; 3]),
        [c, m, y, k] => Some([
            (1.0 - c) * (1.0 - k),
            (1.0 - m) * (1.0 - k),
            (1.0 - y) * (1.0 - k),
        ]),
        _ => None,
    }
}

/// A measurement's scale, from its `Measure` dictionary.
fn read_measure(document: &lopdf::Document, dict: &Dictionary) -> Option<Measure> {
    let measure = dict_in(document, dict, b"Measure")?;
    let formats = match measure.get(b"D").or_else(|_| measure.get(b"X")) {
        Ok(formats) => match document.dereference(formats) {
            Ok((_, Object::Array(items))) => items,
            _ => return None,
        },
        Err(_) => return None,
    };
    let Ok((_, Object::Dictionary(first))) = document.dereference(formats.first()?) else {
        return None;
    };
    let factor = number(first.get(b"C").ok()?)?;
    (factor > 0.0).then(|| Measure {
        ratio: string(document, measure, b"R"),
        unit: string(document, first, b"U"),
        factor,
    })
}

fn read_annotation(
    document: &lopdf::Document,
    dict: &Dictionary,
    page: usize,
) -> Option<Annotation> {
    let subtype = Subtype::from_name(dict.get(b"Subtype").and_then(Object::as_name).ok()?)?;
    // Replies to another annotation belong to it, not to the page.
    if dict.has(b"IRT") {
        return None;
    }
    // Other programs' links are followed, not edited (see `links`).
    if subtype == Subtype::Link && own_entries(document, dict).is_none() {
        return None;
    }
    let rect = numbers(document, dict.get(b"Rect").ok());
    let [x0, y0, x1, y1] = rect[..] else {
        return None;
    };
    let rect = [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)];
    let list = |key: &[u8]| points(&numbers(document, dict.get(key).ok()));
    let vertices = match subtype {
        Subtype::Line => list(b"L"),
        Subtype::PolyLine | Subtype::Polygon => list(b"Vertices"),
        Subtype::Ink => match dict.get(b"InkList").map(|o| document.dereference(o)) {
            Ok(Ok((_, Object::Array(strokes)))) => strokes
                .iter()
                .flat_map(|s| points(&numbers(document, Some(s))))
                .collect(),
            _ => Vec::new(),
        },
        Subtype::Square
        | Subtype::Circle
        | Subtype::FreeText
        | Subtype::Text
        | Subtype::Stamp
        | Subtype::Link => Vec::new(),
    };
    let width = dict_in(document, dict, b"BS")
        .and_then(|bs| bs.get(b"W").ok().and_then(number))
        .or_else(|| numbers(document, dict.get(b"Border").ok()).get(2).copied())
        .unwrap_or(1.0);
    let cloudy = dict_in(document, dict, b"BE")
        .is_some_and(|be| be.get(b"S").and_then(Object::as_name).ok() == Some(b"C".as_slice()));
    let arrow = match dict.get(b"LE").map(|o| document.dereference(o)) {
        Ok(Ok((_, Object::Array(ends)))) => ends
            .last()
            .and_then(|end| end.as_name().ok())
            .is_some_and(|end| end != b"None"),
        _ => false,
    };
    let da = string(document, dict, b"DA");
    let words: Vec<&str> = da.split_whitespace().collect();
    let font_size = words
        .windows(2)
        .find(|pair| pair[1] == "Tf")
        .and_then(|pair| pair[0].parse().ok())
        .unwrap_or(12.0);
    // A text box's border color may only be in its default appearance.
    let text_color = words
        .windows(4)
        .find(|w| w[3] == "rg")
        .and_then(|w| Some([w[0].parse().ok()?, w[1].parse().ok()?, w[2].parse().ok()?]));
    let color = rgb(&numbers(document, dict.get(b"C").ok()))
        .or(text_color)
        .unwrap_or([1.0, 0.0, 0.0]);
    Some(Annotation {
        page,
        subtype,
        rect,
        vertices,
        callout: list(b"CL"),
        inset: match numbers(document, dict.get(b"RD").ok())[..] {
            [l, b, r, t] => [l, b, r, t],
            _ => [0.0; 4],
        },
        color,
        interior: rgb(&numbers(document, dict.get(b"IC").ok())),
        opacity: dict.get(b"CA").ok().and_then(number).unwrap_or(1.0),
        fill_opacity: 1.0,
        width,
        contents: string(document, dict, b"Contents"),
        subject: string(document, dict, b"Subj"),
        author: string(document, dict, b"T"),
        name: string(document, dict, b"NM"),
        modified: string(document, dict, b"M"),
        intent: string(document, dict, b"IT"),
        icon: string(document, dict, b"Name"),
        cloudy,
        arrow,
        font_size,
        measure: read_measure(document, dict),
        own: own_entries(document, dict).unwrap_or_default(),
        appearance: Vec::new(),
        origin: None,
        change: Change::Unchanged,
        replies: Vec::new(),
        link: None,
        capture: None,
        form: None,
    })
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "PDF reals are single precision; points need no more"
)]
fn real(value: f64) -> Object {
    Object::Real(value as f32)
}

fn reals(values: impl IntoIterator<Item = f64>) -> Object {
    Object::Array(values.into_iter().map(real).collect())
}

fn flat(points: &[Point]) -> Object {
    reals(points.iter().flat_map(|p| [p.x, p.y]))
}

/// Moves every coordinate of an annotation by `dx`, `dy`.
fn shift(dict: &mut Dictionary, dx: f64, dy: f64) {
    let moved = |object: &Object| -> Option<Object> {
        let Object::Array(items) = object else {
            return None;
        };
        let values: Option<Vec<f64>> = items.iter().map(number).collect();
        let values = values?;
        Some(reals(
            values
                .iter()
                .enumerate()
                .map(|(i, v)| v + if i % 2 == 0 { dx } else { dy }),
        ))
    };
    for key in [b"Rect".as_slice(), b"L", b"Vertices", b"CL", b"QuadPoints"] {
        if let Some(object) = dict.get(key).ok().and_then(moved) {
            dict.set(key.to_vec(), object);
        }
    }
    if let Ok(Object::Array(strokes)) = dict.get(b"InkList") {
        let strokes: Option<Vec<Object>> = strokes.iter().map(moved).collect();
        if let Some(strokes) = strokes {
            dict.set("InkList", strokes);
        }
    }
}

fn number_format(unit: &str, factor: f64) -> Object {
    Object::Array(vec![Object::Dictionary(dictionary! {
        "Type" => "NumberFormat",
        "U" => text_string(unit),
        "C" => real(factor),
        "D" => 100,
    })])
}

/// The keys an annotation has only when they apply.
fn optional_keys(dict: &mut Dictionary, annotation: &Annotation, opacity: f64) {
    let mut set_or_remove = |key: &str, value: Option<Object>| match value {
        Some(value) => dict.set(key, value),
        None => {
            dict.remove(key.as_bytes());
        }
    };
    set_or_remove(
        "Subj",
        (!annotation.subject.is_empty()).then(|| text_string(&annotation.subject)),
    );
    set_or_remove("IC", annotation.interior.map(reals));
    set_or_remove("CA", (opacity < 1.0).then(|| real(opacity)));
    set_or_remove(
        "BE",
        annotation
            .cloudy
            .then(|| Object::Dictionary(dictionary! { "S" => "C", "I" => 1 })),
    );
    set_or_remove(
        "LE",
        annotation.arrow.then(|| {
            Object::Array(vec![
                Object::Name(b"None".to_vec()),
                Object::Name(b"OpenArrow".to_vec()),
            ])
        }),
    );
    set_or_remove(
        "IT",
        (!annotation.intent.is_empty())
            .then(|| Object::Name(annotation.intent.as_bytes().to_vec())),
    );
    set_or_remove(
        "Name",
        (!annotation.icon.is_empty()).then(|| Object::Name(annotation.icon.as_bytes().to_vec())),
    );
    set_or_remove(
        "Measure",
        annotation.measure.as_ref().map(|m| {
            Object::Dictionary(dictionary! {
                "Type" => "Measure",
                "Subtype" => "RL",
                "R" => text_string(&m.ratio),
                "X" => number_format(&m.unit, m.factor),
                "D" => number_format(&m.unit, m.factor),
                "A" => number_format(&format!("sq {}", m.unit), m.factor * m.factor),
            })
        }),
    );
    set_or_remove(
        "OpenCabinet",
        (!annotation.own.is_empty()).then(|| Object::Dictionary(own_dictionary(&annotation.own))),
    );
}

/// Writes Review's keys and appearance into `dict`, keeping the others.
fn fill(
    document: &mut lopdf::Document,
    dict: &mut Dictionary,
    annotation: &Annotation,
    (font, pages): (ObjectId, &[ObjectId]),
) {
    let [x0, y0, x1, y1] = annotation.rect;
    let opacity = annotation.opacity.clamp(0.0, 1.0);
    // A capture: the part of the page, fitted to the annotation's box.
    let form = if let Some((bbox, recolored)) = &annotation.form {
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => reals(*bbox),
            "Resources" => add_recolored(document, recolored),
        }
    } else {
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => reals([x0, y0, x1, y1]),
            "Resources" => dictionary! {
                "Font" => dictionary! { FONT => font },
                "ExtGState" => dictionary! {
                    STATE => dictionary! {
                        "ca" => real(annotation.fill_opacity.clamp(0.0, 1.0) * opacity),
                        "CA" => real(opacity),
                    },
                },
            },
        }
    };
    let appearance = document.add_object(Stream::new(form, annotation.appearance.clone()));
    let [r, g, b] = annotation.color;
    dict.set("Subtype", Object::Name(annotation.subtype.name().into()));
    dict.set("Rect", reals(annotation.rect));
    dict.set("C", reals([r, g, b]));
    dict.set(
        "BS",
        dictionary! { "Type" => "Border", "W" => real(annotation.width), "S" => "S" },
    );
    dict.remove(b"Border");
    dict.set("Contents", text_string(&annotation.contents));
    dict.set("T", text_string(&annotation.author));
    dict.set("NM", text_string(&annotation.name));
    dict.set("M", text_string(&annotation.modified));
    dict.set("AP", dictionary! { "N" => appearance });
    // A state picked from a set of appearances no longer applies.
    dict.remove(b"AS");
    // Rich text would show other words than the new contents.
    dict.remove(b"RC");
    // Its insets belonged to the old rectangle.
    dict.remove(b"RD");
    dict.remove(b"DS");
    optional_keys(dict, annotation, opacity);
    for key in [b"L".as_slice(), b"Vertices", b"InkList", b"CL", b"DA"] {
        dict.remove(key);
    }
    match annotation.subtype {
        Subtype::Line => dict.set("L", flat(&annotation.vertices)),
        Subtype::PolyLine | Subtype::Polygon => dict.set("Vertices", flat(&annotation.vertices)),
        Subtype::Ink => dict.set("InkList", vec![flat(&annotation.vertices)]),
        Subtype::FreeText => {
            dict.set(
                "DA",
                text_string(&format!(
                    "{r:.3} {g:.3} {b:.3} rg /{FONT} {:.1} Tf",
                    annotation.font_size
                )),
            );
            if !annotation.callout.is_empty() {
                dict.set("CL", flat(&annotation.callout));
            }
        }
        Subtype::Link => {
            dict.remove(b"Dest");
            match annotation.link.as_ref().and_then(|t| t.action(pages)) {
                Some(action) => dict.set("A", action),
                None => {
                    dict.remove(b"A");
                }
            }
        }
        Subtype::Square | Subtype::Circle | Subtype::Text | Subtype::Stamp => {}
    }
}
