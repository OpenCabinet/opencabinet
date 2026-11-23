// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Changing a document's pages: turning, removing, moving, adding pages
//! from another PDF or blank ones, and writing some pages to a PDF of
//! their own.
//!
//! [`arrange`] does all but the last, as an incremental update: the
//! original bytes stay as they were and a new page list is appended. Each
//! page listed is put straight under the page tree's root, with what it
//! inherited from the nodes in between (media box, crop box, resources,
//! rotation) written into it, so moving it cannot change how it looks.
//! A page's markups and Review's own entries live in the page itself, so
//! they go wherever the page goes.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, IncrementalDocument, Object, ObjectId, dictionary};

use crate::Error;

/// What a page inherits from the nodes above it when it lacks its own.
const INHERITED: [&[u8]; 4] = [b"MediaBox", b"CropBox", b"Resources", b"Rotate"];

/// How deep a page tree may go before it is taken as broken.
const DEPTH: usize = 64;

/// Where a page of the new list comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum From {
    /// This document's page, from 0.
    Here(usize),
    /// Page `.1` of the other document `.0`, both from 0.
    Other(usize, usize),
    /// A blank page this wide and high, in points.
    Blank(f64, f64),
}

/// A page of the new list, and how many quarter turns clockwise it is
/// turned beyond how it was.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Where it comes from.
    pub from: From,
    /// Quarter turns clockwise; negative turns the other way.
    pub turn: i32,
}

impl Placed {
    /// This document's page `page`, as it is.
    #[must_use]
    pub const fn here(page: usize) -> Self {
        Self {
            from: From::Here(page),
            turn: 0,
        }
    }
}

fn fail(why: impl std::fmt::Display) -> Error {
    Error::Pages(why.to_string())
}

/// `bytes` with an incremental update whose pages are `pages`, in that
/// order, taken from this document, from `others` or made blank.
///
/// # Errors
/// When a PDF cannot be read, the list is empty, names a page twice or a
/// page a document lacks, or another document is protected.
pub fn arrange(bytes: &[u8], pages: &[Placed], others: &[&[u8]]) -> Result<Vec<u8>, Error> {
    if pages.is_empty() {
        return Err(fail("a PDF must keep at least one page"));
    }
    let prev = lopdf::Document::load_mem(bytes).map_err(fail)?;
    if prev.is_encrypted() {
        return Err(Error::Encrypted);
    }
    let here: Vec<ObjectId> = prev.get_pages().values().copied().collect();
    let root = prev
        .catalog()
        .and_then(|c| c.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(fail)?;
    let mut seen = BTreeSet::new();
    for placed in pages {
        if let From::Here(page) = placed.from {
            if page >= here.len() {
                return Err(fail(format!("the PDF has no page {}", page + 1)));
            }
            if !seen.insert(page) {
                return Err(fail(format!("page {} is listed twice", page + 1)));
            }
        }
    }
    let mut document = IncrementalDocument::create_from(bytes.to_vec(), prev);
    let mut loaded = Loaded::new();
    let mut taken: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut kids = Vec::with_capacity(pages.len());
    for placed in pages {
        let id = match placed.from {
            From::Here(page) => {
                let id = here[page];
                let prev = document.get_prev_documents();
                let mut dict = prev.get_dictionary(id).map_err(fail)?.clone();
                take_inherited(prev, &mut dict)?;
                dict.set("Parent", root);
                document.new_document.set_object(id, dict);
                id
            }
            From::Other(which, page) => {
                if !taken.insert((which, page)) {
                    return Err(fail(format!("page {} is listed twice", page + 1)));
                }
                put_other(
                    &mut document.new_document,
                    &mut loaded,
                    others,
                    (which, page),
                    root,
                )?
            }
            From::Blank(width, height) => {
                if !(width.is_finite() && height.is_finite() && width >= 1.0 && height >= 1.0) {
                    return Err(fail("a blank page needs a size"));
                }
                #[expect(clippy::cast_possible_truncation, reason = "page sizes in points")]
                let (w, h) = (width as f32, height as f32);
                document.new_document.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => root,
                    "MediaBox" => vec![0.into(), 0.into(), w.into(), h.into()],
                    "Resources" => Dictionary::new(),
                })
            }
        };
        if placed.turn.rem_euclid(4) != 0 {
            let page = document
                .new_document
                .get_object_mut(id)
                .and_then(Object::as_dict_mut)
                .map_err(fail)?;
            let was = page.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
            page.set("Rotate", turned(was, placed.turn));
        }
        kids.push(Object::Reference(id));
    }
    document
        .opt_clone_object_to_new_document(root)
        .map_err(fail)?;
    let node = document
        .new_document
        .get_object_mut(root)
        .and_then(Object::as_dict_mut)
        .map_err(fail)?;
    let count = i64::try_from(kids.len()).unwrap_or(i64::MAX);
    node.set("Kids", kids);
    node.set("Count", count);
    let mut out = Vec::with_capacity(bytes.len() + 4096);
    document.save_to(&mut out).map_err(fail)?;
    Ok(out)
}

/// A PDF of its own holding only `pages` of `bytes`, in that order, with
/// their markups. Bookmarks and the document's structure are left out, as
/// they would point at pages it lacks.
///
/// # Errors
/// As [`arrange`].
pub fn extract(bytes: &[u8], pages: &[usize]) -> Result<Vec<u8>, Error> {
    let list: Vec<Placed> = pages.iter().map(|&p| Placed::here(p)).collect();
    let arranged = arrange(bytes, &list, &[])?;
    let mut document = lopdf::Document::load_mem(&arranged).map_err(fail)?;
    let catalog = document.catalog_mut().map_err(fail)?;
    for key in [
        &b"Outlines"[..],
        b"StructTreeRoot",
        b"PageLabels",
        b"Dests",
        b"OpenAction",
        b"MarkInfo",
    ] {
        catalog.remove(key);
    }
    document.trailer.remove(b"Prev");
    document.prune_objects();
    // Numbered 1 to n again, so the file says how many objects it has.
    document.renumber_objects();
    document.trailer.set("Size", i64::from(document.max_id) + 1);
    let mut out = Vec::with_capacity(arranged.len());
    document.save_to(&mut out).map_err(fail)?;
    Ok(out)
}

/// The documents other than this one, as read and renumbered, with their
/// pages.
type Loaded = BTreeMap<usize, (lopdf::Document, Vec<ObjectId>)>;

/// Copies page `page` of other document `which`, and all it uses, into
/// `into`, under `root`; reads that document the first time.
fn put_other(
    into: &mut lopdf::Document,
    loaded: &mut Loaded,
    others: &[&[u8]],
    (which, page): (usize, usize),
    root: ObjectId,
) -> Result<ObjectId, Error> {
    let (other, ids) = match loaded.entry(which) {
        std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
        std::collections::btree_map::Entry::Vacant(e) => {
            let bytes = others
                .get(which)
                .ok_or_else(|| fail("the other PDF is missing"))?;
            let mut other = lopdf::Document::load_mem(bytes).map_err(fail)?;
            if other.is_encrypted() {
                return Err(Error::Encrypted);
            }
            // Numbered after everything this document has, so nothing
            // is overwritten.
            other.renumber_objects_with(into.max_id + 1);
            into.max_id = into.max_id.max(other.max_id);
            let ids = other.get_pages().values().copied().collect();
            e.insert((other, ids))
        }
    };
    let id = *ids
        .get(page)
        .ok_or_else(|| fail(format!("the other PDF has no page {}", page + 1)))?;
    copy_page(other, ids, id, into)?;
    let mut dict = other.get_dictionary(id).map_err(fail)?.clone();
    take_inherited(other, &mut dict)?;
    dict.set("Parent", root);
    into.set_object(id, dict);
    Ok(id)
}

/// A page's rotation after `turn` quarter turns clockwise, as PDF writes
/// it: 0, 90, 180 or 270.
fn turned(was: i64, turn: i32) -> i64 {
    (was + i64::from(turn) * 90).rem_euclid(360) / 90 * 90
}

/// Writes into `page` what it inherits from the nodes above it.
fn take_inherited(document: &lopdf::Document, page: &mut Dictionary) -> Result<(), Error> {
    let mut parent = page.get(b"Parent").and_then(Object::as_reference).ok();
    let mut depth = 0;
    while let Some(id) = parent {
        depth += 1;
        if depth > DEPTH {
            return Err(fail("the page tree loops"));
        }
        let Ok(node) = document.get_dictionary(id) else {
            break;
        };
        for key in INHERITED {
            if !page.has(key)
                && let Ok(value) = node.get(key)
            {
                page.set(key.to_vec(), value.clone());
            }
        }
        parent = node.get(b"Parent").and_then(Object::as_reference).ok();
    }
    Ok(())
}

/// Copies into `into` every object page `page` of `other` uses, but not
/// the page tree nor `other`'s pages other than this one.
fn copy_page(
    other: &lopdf::Document,
    pages: &[ObjectId],
    page: ObjectId,
    into: &mut lopdf::Document,
) -> Result<(), Error> {
    let skip: BTreeSet<ObjectId> = pages.iter().copied().filter(|&p| p != page).collect();
    let mut todo = vec![page];
    let mut done = BTreeSet::new();
    while let Some(id) = todo.pop() {
        if !done.insert(id) || skip.contains(&id) {
            continue;
        }
        let Ok(object) = other.get_object(id) else {
            continue;
        };
        if object
            .as_dict()
            .and_then(|d| d.get(b"Type"))
            .and_then(Object::as_name)
            .is_ok_and(|t| t == b"Pages")
        {
            continue;
        }
        refs(object, &mut todo);
        if id != page {
            into.set_object(id, object.clone());
        }
    }
    if into.objects.len() > 5_000_000 {
        return Err(fail("the other PDF is too large"));
    }
    Ok(())
}

/// Adds to `out` the objects `object` refers to.
fn refs(object: &Object, out: &mut Vec<ObjectId>) {
    match object {
        Object::Reference(id) => out.push(*id),
        Object::Array(items) => items.iter().for_each(|o| refs(o, out)),
        Object::Dictionary(dict) => dict.iter().for_each(|(_, o)| refs(o, out)),
        Object::Stream(stream) => stream.dict.iter().for_each(|(_, o)| refs(o, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_wrap_to_what_pdf_writes() {
        assert_eq!(turned(0, 1), 90);
        assert_eq!(turned(270, 1), 0);
        assert_eq!(turned(0, -1), 270);
        assert_eq!(turned(90, 6), 270);
        assert_eq!(turned(45, 0), 0);
    }

    #[test]
    fn an_empty_list_is_refused() {
        assert!(arrange(b"", &[], &[]).is_err());
    }
}
