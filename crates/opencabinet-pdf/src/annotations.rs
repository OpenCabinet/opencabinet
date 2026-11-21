// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The markups a PDF holds, as standard annotations (ISO 32000-2 §12.5).
//!
//! [`read`] returns the annotations of the kinds Review edits, and what
//! Review keeps on each page. [`save`] writes them back as an incremental
//! update: the original bytes stay as they were, and an update appended
//! after them replaces each page's editable annotations with the given
//! ones. Annotations of other kinds (links, form fields, stamps, notes and
//! the like) are kept untouched.
//!
//! Each annotation also carries a small dictionary of Review's own under
//! the key `OpenCabinet`, which other programs ignore; every standard key
//! stays correct on its own, so other programs show the markup.

use std::collections::BTreeMap;

use lopdf::{Dictionary, IncrementalDocument, Object, ObjectId, Stream, StringFormat, dictionary};

pub use crate::text::win_ansi_literal;
use crate::{Error, Point, text};

/// The key of Review's own dictionary, in annotations and pages.
const OWN: &[u8] = b"OpenCabinet";

/// The name of the font that appearance streams use, Helvetica.
pub const FONT: &str = "Helv";

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
    /// A text box.
    FreeText,
}

impl Subtype {
    const ALL: [Self; 7] = [
        Self::Square,
        Self::Circle,
        Self::Line,
        Self::PolyLine,
        Self::Polygon,
        Self::Ink,
        Self::FreeText,
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
        }
    }

    fn from_name(name: &[u8]) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name().as_bytes() == name)
    }
}

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
    /// of a freehand stroke; empty for rectangles, ellipses and text boxes.
    pub vertices: Vec<Point>,
    /// Stroke color, RGB from 0 to 1.
    pub color: [f64; 3],
    /// Stroke width, in points.
    pub width: f64,
    /// Its text: a comment, or what a text box shows.
    pub contents: String,
    /// Who made it.
    pub author: String,
    /// A name unique on the page, kept across saves.
    pub name: String,
    /// When it last changed, as a PDF date (`D:20261121153000Z`).
    pub modified: String,
    /// Whether a rectangle or polygon has a cloudy border.
    pub cloudy: bool,
    /// Whether a line or polyline ends in an arrow head.
    pub arrow: bool,
    /// A text box's font size, in points.
    pub font_size: f64,
    /// Review's own entries.
    pub own: BTreeMap<String, String>,
    /// How it looks: a content stream drawn in PDF space and clipped to
    /// `rect`, which may use the font [`FONT`]. Empty when read from a file.
    pub appearance: Vec<u8>,
}

/// What a PDF holds for Review.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Markups {
    /// The editable annotations, page by page.
    pub annotations: Vec<Annotation>,
    /// Review's own entries for each page that has some, such as its
    /// scale.
    pub pages: BTreeMap<usize, BTreeMap<String, String>>,
}

/// Reads what `bytes` holds for Review.
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
        for annotation in page_annotations(&document, page) {
            let Ok((_, Object::Dictionary(dict))) = document.dereference(annotation) else {
                continue;
            };
            if let Some(annotation) = read_annotation(&document, dict, index) {
                markups.annotations.push(annotation);
            }
        }
    }
    Ok(markups)
}

/// `bytes` with an incremental update that replaces the editable
/// annotations of every page with `markups`, and Review's entries for each
/// page with those in `markups`.
///
/// # Errors
/// When the PDF cannot be read, or an annotation is on a page it lacks.
pub fn save(bytes: &[u8], markups: &Markups) -> Result<Vec<u8>, Error> {
    let fail = |e: lopdf::Error| Error::Annotations(e.to_string());
    let prev = lopdf::Document::load_mem(bytes).map_err(fail)?;
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
    let font = document.new_document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let mut changed = false;
    for (index, &page_id) in pages.iter().enumerate() {
        let prev = document.get_prev_documents();
        let page = prev.get_dictionary(page_id).map_err(fail)?;
        let old: Vec<Object> = page_annotations(prev, page).into_iter().cloned().collect();
        // Annotations Review edits, and the pop-up notes that belong to them.
        let editable: Vec<ObjectId> = old
            .iter()
            .filter_map(|a| {
                let id = a.as_reference().ok()?;
                let dict = prev.get_dictionary(id).ok()?;
                editable_subtype(dict).map(|_| id)
            })
            .collect();
        let kept: Vec<Object> = old
            .iter()
            .filter(|a| {
                let Ok((id, Object::Dictionary(dict))) = prev.dereference(a) else {
                    return true;
                };
                if id.is_some_and(|id| editable.contains(&id)) || editable_subtype(dict).is_some() {
                    return false;
                }
                let popup_of_editable = dict.get(b"Subtype").and_then(Object::as_name).ok()
                    == Some(b"Popup".as_slice())
                    && dict
                        .get(b"Parent")
                        .and_then(Object::as_reference)
                        .is_ok_and(|parent| editable.contains(&parent));
                !popup_of_editable
            })
            .cloned()
            .collect();
        let new: Vec<&Annotation> = markups
            .annotations
            .iter()
            .filter(|a| a.page == index)
            .collect();
        let own_old = own_entries(prev, page).unwrap_or_default();
        let own_new = markups.pages.get(&index).cloned().unwrap_or_default();
        if kept.len() == old.len() && new.is_empty() && own_old == own_new {
            continue;
        }
        changed = true;
        let mut annots = kept;
        for annotation in new {
            let id = write_annotation(&mut document.new_document, annotation, page_id, font);
            annots.push(Object::Reference(id));
        }
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

fn editable_subtype(dict: &Dictionary) -> Option<Subtype> {
    let name = dict.get(b"Subtype").and_then(Object::as_name).ok()?;
    Subtype::from_name(name)
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

fn points(values: &[f64]) -> Vec<Point> {
    values
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[x, y]| Point::new(x, y))
        .collect()
}

fn own_entries(document: &lopdf::Document, dict: &Dictionary) -> Option<BTreeMap<String, String>> {
    let own = dict.get(OWN).ok()?;
    let Ok((_, Object::Dictionary(own))) = document.dereference(own) else {
        return None;
    };
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

fn read_annotation(
    document: &lopdf::Document,
    dict: &Dictionary,
    page: usize,
) -> Option<Annotation> {
    let subtype = editable_subtype(dict)?;
    let rect = numbers(document, dict.get(b"Rect").ok());
    let [x0, y0, x1, y1] = rect[..] else {
        return None;
    };
    let rect = [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)];
    let vertices = match subtype {
        Subtype::Line => points(&numbers(document, dict.get(b"L").ok())),
        Subtype::PolyLine | Subtype::Polygon => {
            points(&numbers(document, dict.get(b"Vertices").ok()))
        }
        Subtype::Ink => match dict.get(b"InkList").map(|o| document.dereference(o)) {
            Ok(Ok((_, Object::Array(strokes)))) => strokes
                .iter()
                .flat_map(|s| points(&numbers(document, Some(s))))
                .collect(),
            _ => Vec::new(),
        },
        Subtype::Square | Subtype::Circle | Subtype::FreeText => Vec::new(),
    };
    let color = match numbers(document, dict.get(b"C").ok())[..] {
        [r, g, b] => [r, g, b],
        [gray] => [gray; 3],
        _ => [1.0, 0.0, 0.0],
    };
    let width = dict
        .get(b"BS")
        .ok()
        .and_then(|bs| match document.dereference(bs) {
            Ok((_, Object::Dictionary(bs))) => bs.get(b"W").ok().and_then(number),
            _ => None,
        })
        .or_else(|| numbers(document, dict.get(b"Border").ok()).get(2).copied())
        .unwrap_or(1.0);
    let cloudy = dict
        .get(b"BE")
        .is_ok_and(|be| match document.dereference(be) {
            Ok((_, Object::Dictionary(be))) => {
                be.get(b"S").and_then(Object::as_name).ok() == Some(b"C".as_slice())
            }
            _ => false,
        });
    let arrow = match dict.get(b"LE").map(|o| document.dereference(o)) {
        Ok(Ok((_, Object::Array(ends)))) => ends
            .get(1)
            .and_then(|end| end.as_name().ok())
            .is_some_and(|end| end != b"None"),
        _ => false,
    };
    let font_size = string(document, dict, b"DA")
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|pair| pair[1] == "Tf")
        .and_then(|pair| pair[0].parse().ok())
        .unwrap_or(12.0);
    Some(Annotation {
        page,
        subtype,
        rect,
        vertices,
        color,
        width,
        contents: string(document, dict, b"Contents"),
        author: string(document, dict, b"T"),
        name: string(document, dict, b"NM"),
        modified: string(document, dict, b"M"),
        cloudy,
        arrow,
        font_size,
        own: own_entries(document, dict).unwrap_or_default(),
        appearance: Vec::new(),
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

fn write_annotation(
    document: &mut lopdf::Document,
    annotation: &Annotation,
    page: ObjectId,
    font: ObjectId,
) -> ObjectId {
    let [x0, y0, x1, y1] = annotation.rect;
    let appearance = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => reals([x0, y0, x1, y1]),
            "Resources" => dictionary! {
                "Font" => dictionary! { FONT => font },
            },
        },
        annotation.appearance.clone(),
    ));
    let [r, g, b] = annotation.color;
    let mut dict = dictionary! {
        "Type" => "Annot",
        "Subtype" => annotation.subtype.name(),
        "Rect" => reals(annotation.rect),
        "P" => page,
        // Printed with the page.
        "F" => 4,
        "C" => reals([r, g, b]),
        "BS" => dictionary! { "Type" => "Border", "W" => real(annotation.width), "S" => "S" },
        "Contents" => text_string(&annotation.contents),
        "T" => text_string(&annotation.author),
        "NM" => text_string(&annotation.name),
        "M" => text_string(&annotation.modified),
        "AP" => dictionary! { "N" => appearance },
    };
    let flat = |points: &[Point]| reals(points.iter().flat_map(|p| [p.x, p.y]));
    match annotation.subtype {
        Subtype::Line => dict.set("L", flat(&annotation.vertices)),
        Subtype::PolyLine | Subtype::Polygon => dict.set("Vertices", flat(&annotation.vertices)),
        Subtype::Ink => dict.set("InkList", vec![flat(&annotation.vertices)]),
        Subtype::FreeText => dict.set(
            "DA",
            text_string(&format!(
                "{r:.3} {g:.3} {b:.3} rg /{FONT} {:.1} Tf",
                annotation.font_size
            )),
        ),
        Subtype::Square | Subtype::Circle => {}
    }
    if annotation.cloudy {
        dict.set("BE", dictionary! { "S" => "C", "I" => 1 });
    }
    if annotation.arrow {
        dict.set(
            "LE",
            vec![
                Object::Name(b"None".to_vec()),
                Object::Name(b"OpenArrow".to_vec()),
            ],
        );
    }
    if !annotation.own.is_empty() {
        dict.set(OWN, own_dictionary(&annotation.own));
    }
    document.add_object(dict)
}
