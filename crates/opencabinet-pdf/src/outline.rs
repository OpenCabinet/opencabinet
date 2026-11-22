// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A PDF's bookmarks (its outline): the sheet index many drawing sets
//! carry, each entry pointing at a page.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, Object, ObjectId};

use crate::Error;
use crate::text::decode;

/// The most bookmarks read, and how deep, so a broken or looping outline
/// cannot hold the program.
const MOST: usize = 10_000;
const DEEPEST: usize = 32;

/// A bookmark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bookmark {
    /// What it says.
    pub title: String,
    /// How deep it is: 0 for the outline's first level.
    pub level: usize,
    /// The page it goes to, from 0; `None` when it goes elsewhere.
    pub page: Option<usize>,
}

/// The bookmarks of the PDF in `bytes`, in order; none when it has none.
///
/// # Errors
/// When the file cannot be read.
pub fn bookmarks(bytes: &[u8]) -> Result<Vec<Bookmark>, Error> {
    let doc = lopdf::Document::load_mem(bytes).map_err(|e| Error::Annotations(e.to_string()))?;
    let pages: BTreeMap<ObjectId, usize> = doc
        .get_pages()
        .values()
        .enumerate()
        .map(|(index, &id)| (id, index))
        .collect();
    let Ok(catalog) = doc.catalog() else {
        return Ok(Vec::new());
    };
    let Some(outlines) = catalog.get(b"Outlines").ok().and_then(|o| dict(&doc, o)) else {
        return Ok(Vec::new());
    };
    let reader = Reader {
        doc: &doc,
        pages,
        catalog,
    };
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    reader.walk(outlines.get(b"First").ok(), 0, &mut found, &mut seen);
    Ok(found)
}

fn dict<'a>(doc: &'a lopdf::Document, object: &'a Object) -> Option<&'a Dictionary> {
    doc.dereference(object).ok()?.1.as_dict().ok()
}

struct Reader<'a> {
    doc: &'a lopdf::Document,
    pages: BTreeMap<ObjectId, usize>,
    catalog: &'a Dictionary,
}

impl<'a> Reader<'a> {
    fn walk(
        &self,
        mut next: Option<&'a Object>,
        level: usize,
        found: &mut Vec<Bookmark>,
        seen: &mut BTreeSet<ObjectId>,
    ) {
        while let Some(object) = next {
            if found.len() >= MOST || level > DEEPEST {
                return;
            }
            let Ok(id) = object.as_reference() else {
                return;
            };
            if !seen.insert(id) {
                return;
            }
            let Ok(item) = self.doc.get_dictionary(id) else {
                return;
            };
            let title = item
                .get(b"Title")
                .ok()
                .and_then(|t| self.doc.dereference(t).ok())
                .and_then(|(_, t)| t.as_str().ok())
                .map(decode)
                .unwrap_or_default();
            found.push(Bookmark {
                title: title.trim().to_owned(),
                level,
                page: self.page_of(item),
            });
            self.walk(item.get(b"First").ok(), level + 1, found, seen);
            next = item.get(b"Next").ok();
        }
    }

    /// The page an outline item goes to.
    fn page_of(&self, item: &Dictionary) -> Option<usize> {
        let target = if let Ok(dest) = item.get(b"Dest") {
            dest
        } else {
            let action = dict(self.doc, item.get(b"A").ok()?)?;
            if action.get(b"S").ok()?.as_name().ok()? != b"GoTo" {
                return None;
            }
            action.get(b"D").ok()?
        };
        self.page_of_destination(target, 0)
    }

    fn page_of_destination(&self, target: &Object, depth: usize) -> Option<usize> {
        if depth > 4 {
            return None;
        }
        let (_, target) = self.doc.dereference(target).ok()?;
        match target {
            Object::Array(parts) => match parts.first()? {
                Object::Reference(page) => self.pages.get(page).copied(),
                // Some writers give the page's number instead.
                Object::Integer(number) => usize::try_from(*number).ok(),
                _ => None,
            },
            Object::Dictionary(d) => self.page_of_destination(d.get(b"D").ok()?, depth + 1),
            Object::Name(name) | Object::String(name, _) => {
                let named = self.named(name)?;
                self.page_of_destination(named, depth + 1)
            }
            _ => None,
        }
    }

    /// A named destination: in the catalog's `/Dests`, or its names tree.
    fn named(&self, name: &[u8]) -> Option<&Object> {
        if let Some(found) = self
            .catalog
            .get(b"Dests")
            .ok()
            .and_then(|d| dict(self.doc, d))
            .and_then(|d| d.get(name).ok())
        {
            return Some(found);
        }
        let tree = dict(self.doc, self.catalog.get(b"Names").ok()?)?
            .get(b"Dests")
            .ok()?;
        self.in_tree(dict(self.doc, tree)?, name, 0)
    }

    fn in_tree<'b>(
        &'b self,
        node: &'b Dictionary,
        name: &[u8],
        depth: usize,
    ) -> Option<&'b Object> {
        if depth > DEEPEST {
            return None;
        }
        if let Ok(names) = node.get(b"Names").and_then(Object::as_array) {
            for pair in names.chunks(2) {
                if let [key, value] = pair
                    && self.doc.dereference(key).ok()?.1.as_str().ok()? == name
                {
                    return Some(value);
                }
            }
        }
        let kids = node.get(b"Kids").and_then(Object::as_array).ok()?;
        kids.iter()
            .filter_map(|kid| dict(self.doc, kid))
            .find_map(|kid| self.in_tree(kid, name, depth + 1))
    }
}
