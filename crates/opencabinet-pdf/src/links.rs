// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Links on a page (ISO 32000-1 §12.5.6.5, §12.6.4): where each one is,
//! and where it goes: another page of this PDF, a page of another PDF, or
//! a web address.
//!
//! Review follows them when clicked. Its own link markups are written as
//! these same standard links, so other viewers follow them too.

use lopdf::{Dictionary, Object, ObjectId, dictionary};

use crate::Error;
use crate::text::decode;

/// How deep a tree of named destinations may go.
const DEPTH: usize = 32;

/// Where a link goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A page of this PDF, from 0.
    Page(usize),
    /// A page, from 0, of another PDF, named as the link names it,
    /// usually relative to this one.
    File(String, usize),
    /// A web or mail address.
    Web(String),
}

impl Target {
    /// What a person types to make a link: `3` or `page 3` for a page,
    /// `A-301.pdf` or `A-301.pdf#2` for another drawing, and an address
    /// starting with `http://`, `https://` or `mailto:`. `None` for
    /// anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let lower = text.to_lowercase();
        if ["http://", "https://", "mailto:"]
            .iter()
            .any(|p| lower.starts_with(p))
        {
            return (!text.contains(char::is_whitespace)).then(|| Self::Web(text.to_owned()));
        }
        let page = |n: &str| {
            n.trim()
                .parse::<usize>()
                .ok()
                .filter(|&n| n >= 1)
                .map(|n| n - 1)
        };
        if let Some(n) = page(lower.strip_prefix("page").unwrap_or(&lower)) {
            return Some(Self::Page(n));
        }
        let (file, at) = match text.rsplit_once('#') {
            Some((file, n)) if page(n).is_some() => (file, page(n).unwrap_or(0)),
            _ => (text, 0),
        };
        file.to_lowercase()
            .ends_with(".pdf")
            .then(|| Self::File(file.to_owned(), at))
    }

    /// As a person would type it (see [`Target::parse`]).
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Page(n) => format!("page {}", n + 1),
            Self::File(file, 0) => file.clone(),
            Self::File(file, n) => format!("{file}#{}", n + 1),
            Self::Web(address) => address.clone(),
        }
    }

    /// The action a link dictionary carries to go here; `pages` are this
    /// PDF's pages, first to last. `None` for a page it lacks.
    #[must_use]
    pub fn action(&self, pages: &[ObjectId]) -> Option<Dictionary> {
        Some(match self {
            Self::Page(n) => dictionary! {
                "S" => "GoTo",
                "D" => vec![Object::Reference(*pages.get(*n)?), "Fit".into()],
            },
            Self::File(file, n) => dictionary! {
                "S" => "GoToR",
                "F" => Object::string_literal(file.as_str()),
                "D" => vec![i64::try_from(*n).ok()?.into(), "Fit".into()],
                "NewWindow" => true,
            },
            Self::Web(address) => dictionary! {
                "S" => "URI",
                "URI" => Object::string_literal(address.as_str()),
            },
        })
    }
}

/// A link on a page.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    /// Where it is, in PDF space: left, bottom, right, top.
    pub rect: [f64; 4],
    /// Where it goes.
    pub target: Target,
}

/// The links on page `page` (from 0) that go somewhere Review can follow.
///
/// # Errors
/// When the PDF cannot be read.
pub fn links(bytes: &[u8], page: usize) -> Result<Vec<Link>, Error> {
    let document = lopdf::Document::load_mem(bytes).map_err(|_| Error::Invalid)?;
    let pages: Vec<ObjectId> = document.get_pages().values().copied().collect();
    let Some(&page_id) = pages.get(page) else {
        return Ok(Vec::new());
    };
    let page = document
        .get_dictionary(page_id)
        .map_err(|_| Error::Invalid)?;
    let annots = page
        .get(b"Annots")
        .ok()
        .map(|o| resolve(&document, o))
        .and_then(|o| o.as_array().ok())
        .cloned()
        .unwrap_or_default();
    Ok(annots
        .iter()
        .filter_map(|a| resolve(&document, a).as_dict().ok())
        .filter(|d| d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Link"))
        .filter_map(|d| {
            let rect: Vec<f64> = resolve(&document, d.get(b"Rect").ok()?)
                .as_array()
                .ok()?
                .iter()
                .filter_map(|n| resolve(&document, n).as_float().ok().map(f64::from))
                .collect();
            let [x0, y0, x1, y1] = rect[..] else {
                return None;
            };
            Some(Link {
                rect: [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)],
                target: target(&document, &pages, d)?,
            })
        })
        .collect())
}

fn resolve<'a>(document: &'a lopdf::Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => document.get_object(*id).unwrap_or(object),
        _ => object,
    }
}

fn text_of(document: &lopdf::Document, object: &Object) -> Option<String> {
    match resolve(document, object) {
        Object::String(bytes, _) => Some(decode(bytes)),
        Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
        _ => None,
    }
}

/// Where the link `link` goes.
fn target(document: &lopdf::Document, pages: &[ObjectId], link: &Dictionary) -> Option<Target> {
    if let Ok(dest) = link.get(b"Dest") {
        return destination(document, pages, dest).map(Target::Page);
    }
    let action = resolve(document, link.get(b"A").ok()?).as_dict().ok()?;
    match action.get(b"S").and_then(Object::as_name).ok()? {
        b"URI" => {
            let address = text_of(document, action.get(b"URI").ok()?)?;
            let lower = address.to_lowercase();
            ["http://", "https://", "mailto:"]
                .iter()
                .any(|p| lower.starts_with(p))
                .then_some(Target::Web(address))
        }
        b"GoTo" => destination(document, pages, action.get(b"D").ok()?).map(Target::Page),
        b"GoToR" => {
            let file = match resolve(document, action.get(b"F").ok()?) {
                Object::Dictionary(spec) => spec
                    .get(b"UF")
                    .or_else(|_| spec.get(b"F"))
                    .ok()
                    .and_then(|f| text_of(document, f))?,
                other => text_of(document, other)?,
            };
            let page = action
                .get(b"D")
                .ok()
                .map(|d| resolve(document, d))
                .and_then(|d| d.as_array().ok())
                .and_then(|d| d.first())
                .and_then(|n| n.as_i64().ok())
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0);
            Some(Target::File(file, page))
        }
        _ => None,
    }
}

/// The page, from 0, a destination goes to: an array starting with the
/// page, or a name looked up in the document's destinations.
fn destination(document: &lopdf::Document, pages: &[ObjectId], dest: &Object) -> Option<usize> {
    match resolve(document, dest) {
        Object::Array(items) => {
            let page = items.first()?.as_reference().ok()?;
            pages.iter().position(|&p| p == page)
        }
        Object::Dictionary(d) => destination(document, pages, d.get(b"D").ok()?),
        named @ (Object::Name(_) | Object::String(..)) => {
            let name = text_of(document, named)?;
            let found = named_destination(document, &name)?;
            destination(document, pages, &found)
        }
        _ => None,
    }
}

/// The destination called `name`, in the catalog's `Dests` or its
/// `Names` tree.
fn named_destination(document: &lopdf::Document, name: &str) -> Option<Object> {
    let catalog = document.catalog().ok()?;
    if let Some(dests) = catalog
        .get(b"Dests")
        .ok()
        .and_then(|d| resolve(document, d).as_dict().ok())
        && let Ok(found) = dests.get(name.as_bytes())
    {
        return Some(found.clone());
    }
    let names = catalog
        .get(b"Names")
        .ok()
        .and_then(|n| resolve(document, n).as_dict().ok())?;
    let tree = names
        .get(b"Dests")
        .ok()
        .and_then(|t| resolve(document, t).as_dict().ok())?;
    in_tree(document, tree, name, 0)
}

fn in_tree(
    document: &lopdf::Document,
    node: &Dictionary,
    name: &str,
    depth: usize,
) -> Option<Object> {
    if depth > DEPTH {
        return None;
    }
    if let Some(pairs) = node
        .get(b"Names")
        .ok()
        .and_then(|n| resolve(document, n).as_array().ok())
    {
        for pair in pairs.chunks(2) {
            if let [key, value] = pair
                && text_of(document, key).as_deref() == Some(name)
            {
                return Some(value.clone());
            }
        }
    }
    let kids = node
        .get(b"Kids")
        .ok()
        .and_then(|k| resolve(document, k).as_array().ok())?;
    kids.iter()
        .filter_map(|k| resolve(document, k).as_dict().ok())
        .find_map(|kid| in_tree(document, kid, name, depth + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_person_types_becomes_a_target_and_back() {
        assert_eq!(Target::parse("3"), Some(Target::Page(2)));
        assert_eq!(Target::parse("Page 12"), Some(Target::Page(11)));
        assert_eq!(
            Target::parse("A-301.pdf#2"),
            Some(Target::File("A-301.pdf".into(), 1))
        );
        assert_eq!(
            Target::parse("elevations/A-301.PDF"),
            Some(Target::File("elevations/A-301.PDF".into(), 0))
        );
        assert_eq!(
            Target::parse(" https://example.org/specs "),
            Some(Target::Web("https://example.org/specs".into()))
        );
        for nothing in [
            "",
            "0",
            "page",
            "notes.txt",
            "javascript:alert(1)",
            "https://a b",
        ] {
            assert_eq!(Target::parse(nothing), None, "{nothing}");
        }
        for target in [
            Target::Page(4),
            Target::File("A-301.pdf".into(), 0),
            Target::File("A-301.pdf".into(), 2),
            Target::Web("mailto:shop@example.org".into()),
        ] {
            assert_eq!(Target::parse(&target.text()), Some(target));
        }
    }
}
