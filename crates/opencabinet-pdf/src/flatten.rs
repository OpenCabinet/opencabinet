// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Comparing two drawings, and burning markups into the page.

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::{Bitmap, Error};

/// How dark a pixel is, from 0 (white) to 255 (black).
fn ink(pixel: [u8; 4]) -> u8 {
    let luma =
        (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114) / 1000;
    255 - u8::try_from(luma).unwrap_or(255)
}

/// Two drawings of the same size laid over each other: what only `old`
/// has in red, what only `new` has in blue, what both have in gray.
///
/// # Errors
/// When the two are not the same size.
pub fn overlay(old: &Bitmap, new: &Bitmap) -> Result<Bitmap, Error> {
    if (old.width, old.height) != (new.width, new.height) {
        return Err(Error::Invalid);
    }
    let mut pixels = Vec::with_capacity(old.pixels.len());
    for (a, b) in old
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(new.pixels.as_chunks::<4>().0)
    {
        let (ia, ib) = (ink(*a), ink(*b));
        let both = ia.min(ib);
        let only_old = ia - both;
        let only_new = ib - both;
        // Start from white; take away what each shows.
        let gray = 255 - both / 2 - both / 4;
        let r = gray.saturating_sub(only_new);
        let g = gray.saturating_sub(only_old).saturating_sub(only_new);
        let b = gray.saturating_sub(only_old);
        pixels.extend_from_slice(&[r, g, b, 255]);
    }
    Ok(Bitmap {
        width: old.width,
        height: old.height,
        pixels,
    })
}

const MARKUPS: [&[u8]; 12] = [
    b"Square",
    b"Circle",
    b"Line",
    b"PolyLine",
    b"Polygon",
    b"Ink",
    b"FreeText",
    b"Text",
    b"Stamp",
    b"Highlight",
    b"Underline",
    b"StrikeOut",
];

/// `bytes` with every markup drawn into its page's content and removed as
/// an annotation, so it prints and shows the same everywhere and can no
/// longer be edited. Links and form fields stay; hidden markups and
/// pop-up notes go.
///
/// # Errors
/// When the PDF cannot be read or written.
pub fn flatten(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let fail = |e: lopdf::Error| Error::Annotations(e.to_string());
    let mut document = lopdf::Document::load_mem(bytes).map_err(fail)?;
    let pages: Vec<ObjectId> = document.get_pages().values().copied().collect();
    for page in pages {
        flatten_page(&mut document, page).map_err(fail)?;
    }
    let mut out = Vec::new();
    document
        .save_to(&mut out)
        .map_err(|e| Error::Annotations(e.to_string()))?;
    Ok(out)
}

fn numbers(document: &lopdf::Document, object: Option<&Object>) -> Vec<f64> {
    let Some(object) = object else {
        return Vec::new();
    };
    match document.dereference(object) {
        Ok((_, Object::Array(items))) => items
            .iter()
            .filter_map(|o| match *o {
                #[expect(clippy::cast_precision_loss, reason = "PDF numbers")]
                Object::Integer(i) => Some(i as f64),
                Object::Real(r) => Some(f64::from(r)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The transform that puts an appearance with `bbox` and `matrix` into
/// `rect`, as ISO 32000-2 §12.5.5 asks.
fn placement(bbox: &[f64], matrix: &[f64], rect: &[f64]) -> Option<[f64; 6]> {
    let [bx0, by0, bx1, by1] = *bbox else {
        return None;
    };
    let [x0, y0, x1, y1] = *rect else {
        return None;
    };
    let m: [f64; 6] = matrix.try_into().unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let apply = |x: f64, y: f64| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
    let corners = [
        apply(bx0, by0),
        apply(bx1, by0),
        apply(bx1, by1),
        apply(bx0, by1),
    ];
    let (tx0, tx1) = corners
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &(x, _)| {
            (lo.min(x), hi.max(x))
        });
    let (ty0, ty1) = corners
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &(_, y)| {
            (lo.min(y), hi.max(y))
        });
    let (w, h) = (tx1 - tx0, ty1 - ty0);
    if w.abs() < f64::EPSILON || h.abs() < f64::EPSILON {
        return None;
    }
    let (sx, sy) = ((x1.max(x0) - x1.min(x0)) / w, (y1.max(y0) - y1.min(y0)) / h);
    Some([
        sx,
        0.0,
        0.0,
        sy,
        x0.min(x1) - sx * tx0,
        y0.min(y1) - sy * ty0,
    ])
}

fn flatten_page(document: &mut lopdf::Document, page: ObjectId) -> Result<(), lopdf::Error> {
    let page_dict = document.get_dictionary(page)?.clone();
    let annots: Vec<Object> = match page_dict.get(b"Annots") {
        Ok(object) => match document.dereference(object)? {
            (_, Object::Array(items)) => items.clone(),
            _ => Vec::new(),
        },
        Err(_) => return Ok(()),
    };
    let mut kept = Vec::new();
    let mut drawn = Vec::new();
    for entry in annots {
        let (_, Object::Dictionary(annotation)) = document.dereference(&entry)? else {
            kept.push(entry);
            continue;
        };
        let subtype = annotation.get(b"Subtype").and_then(Object::as_name).ok();
        if subtype == Some(b"Popup".as_slice()) {
            continue;
        }
        if !subtype.is_some_and(|s| MARKUPS.contains(&s)) {
            kept.push(entry);
            continue;
        }
        // Hidden (2) or not to be viewed (32): gone without a trace.
        let flags = annotation.get(b"F").and_then(Object::as_i64).unwrap_or(0);
        if flags & (2 | 32) != 0 {
            continue;
        }
        let Some(stream_id) = appearance(document, annotation) else {
            continue;
        };
        let Ok(form) = document.get_object(stream_id).and_then(Object::as_stream) else {
            continue;
        };
        let bbox = numbers(document, form.dict.get(b"BBox").ok());
        let matrix = numbers(document, form.dict.get(b"Matrix").ok());
        let rect = numbers(document, annotation.get(b"Rect").ok());
        if let Some(place) = placement(&bbox, &matrix, &rect) {
            drawn.push((stream_id, place));
        }
    }
    if drawn.is_empty() && kept.len() == page_annots_len(document, &page_dict) {
        return Ok(());
    }
    let mut resources = page_resources(document, page)?;
    let mut xobjects = match resources.get(b"XObject") {
        Ok(object) => match document.dereference(object)? {
            (_, Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        },
        Err(_) => Dictionary::new(),
    };
    let mut content = b"q\n".to_vec();
    for (n, (stream_id, [a, b, c, d, e, f])) in drawn.iter().enumerate() {
        let name = format!("OCFlat{n}");
        xobjects.set(name.as_bytes().to_vec(), Object::Reference(*stream_id));
        content
            .extend_from_slice(format!("q {a} {b} {c} {d} {e} {f} cm /{name} Do Q\n").as_bytes());
    }
    content.extend_from_slice(b"Q\n");
    resources.set("XObject", xobjects);
    let added = document.add_object(Stream::new(dictionary! {}, content));
    // The page's own content first, saved and restored so its state does
    // not leak into the markups.
    let before = document.add_object(Stream::new(dictionary! {}, b"q\n".to_vec()));
    let after = document.add_object(Stream::new(dictionary! {}, b"Q\n".to_vec()));
    let mut contents = vec![Object::Reference(before)];
    match page_dict.get(b"Contents") {
        Ok(Object::Array(items)) => contents.extend(items.iter().cloned()),
        Ok(Object::Reference(id)) => match document.get_object(*id) {
            Ok(Object::Array(items)) => contents.extend(items.iter().cloned()),
            _ => contents.push(Object::Reference(*id)),
        },
        _ => {}
    }
    contents.push(Object::Reference(after));
    contents.push(Object::Reference(added));
    let page = document.get_object_mut(page)?.as_dict_mut()?;
    page.set("Resources", resources);
    page.set("Contents", contents);
    if kept.is_empty() {
        page.remove(b"Annots");
    } else {
        page.set("Annots", kept);
    }
    Ok(())
}

fn page_annots_len(document: &lopdf::Document, page: &Dictionary) -> usize {
    match page.get(b"Annots").map(|o| document.dereference(o)) {
        Ok(Ok((_, Object::Array(items)))) => items.len(),
        _ => 0,
    }
}

/// An annotation's normal appearance stream, picking its state when it
/// has several.
fn appearance(document: &lopdf::Document, annotation: &Dictionary) -> Option<ObjectId> {
    let (_, Object::Dictionary(ap)) = document.dereference(annotation.get(b"AP").ok()?).ok()?
    else {
        return None;
    };
    let normal = ap.get(b"N").ok()?;
    match normal {
        Object::Reference(id) => match document.get_object(*id).ok()? {
            Object::Stream(_) => Some(*id),
            Object::Dictionary(states) => {
                let state = annotation.get(b"AS").and_then(Object::as_name).ok()?;
                states.get(state).and_then(Object::as_reference).ok()
            }
            _ => None,
        },
        Object::Dictionary(states) => {
            let state = annotation.get(b"AS").and_then(Object::as_name).ok()?;
            states.get(state).and_then(Object::as_reference).ok()
        }
        _ => None,
    }
}

/// A page's resources as a dictionary of its own, inherited ones
/// included.
fn page_resources(document: &lopdf::Document, page: ObjectId) -> Result<Dictionary, lopdf::Error> {
    let mut node = Some(page);
    while let Some(id) = node {
        let dict = document.get_dictionary(id)?;
        if let Ok(resources) = dict.get(b"Resources") {
            return Ok(match document.dereference(resources)? {
                (_, Object::Dictionary(d)) => d.clone(),
                _ => Dictionary::new(),
            });
        }
        node = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    Ok(Dictionary::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(width: u32, height: u32, dark: &[(u32, u32)]) -> Bitmap {
        let mut pixels = vec![255; (width * height * 4) as usize];
        for &(x, y) in dark {
            let at = ((y * width + x) * 4) as usize;
            pixels[at..at + 3].copy_from_slice(&[0, 0, 0]);
        }
        Bitmap {
            width,
            height,
            pixels,
        }
    }

    #[test]
    fn what_changed_shows_in_red_and_blue() {
        let old = plain(3, 1, &[(0, 0), (1, 0)]);
        let new = plain(3, 1, &[(1, 0), (2, 0)]);
        let both = overlay(&old, &new).unwrap();
        // Only in the old: red.
        assert_eq!(&both.pixels[0..4], &[255, 0, 0, 255]);
        // In both: gray.
        let gray = both.pixel(1, 0);
        assert!(gray[0] == gray[1] && gray[1] == gray[2] && gray[0] < 128);
        // Only in the new: blue.
        assert_eq!(both.pixel(2, 0), [0, 0, 255, 255]);
        assert!(overlay(&old, &plain(2, 1, &[])).is_err());
    }

    #[test]
    fn an_appearance_is_placed_in_its_rectangle() {
        let place = placement(&[0.0, 0.0, 10.0, 20.0], &[], &[100.0, 200.0, 120.0, 260.0]).unwrap();
        assert_eq!(place, [2.0, 0.0, 0.0, 3.0, 100.0, 200.0]);
    }
}
