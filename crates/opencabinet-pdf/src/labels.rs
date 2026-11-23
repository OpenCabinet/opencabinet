// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The names a PDF gives its pages (ISO 32000-1 §12.4.2), such as `A-201`
//! or `iii`, so Review can show a sheet's own name instead of its place.
//!
//! The PDF keeps them as ranges: from a page on, a prefix and a style of
//! numbering (decimal, roman or letters) starting at some number.

use lopdf::{Dictionary, Object};

use crate::Error;
use crate::text::decode;

/// How deep a tree of labels may go before it is taken as broken.
const DEPTH: usize = 32;

/// Each page's label, first to last; empty when the PDF names none.
///
/// # Errors
/// When the PDF cannot be read.
pub fn page_labels(bytes: &[u8]) -> Result<Vec<String>, Error> {
    let document = lopdf::Document::load_mem(bytes).map_err(|_| Error::Invalid)?;
    let count = document.get_pages().len();
    let Some(tree) = document
        .catalog()
        .ok()
        .and_then(|c| c.get(b"PageLabels").ok())
        .and_then(|o| resolve(&document, o).as_dict().ok())
    else {
        return Ok(Vec::new());
    };
    let mut ranges: Vec<(usize, Dictionary)> = Vec::new();
    collect(&document, tree, &mut ranges, 0);
    ranges.sort_by_key(|(start, _)| *start);
    if ranges.is_empty() {
        return Ok(Vec::new());
    }
    let mut labels = Vec::with_capacity(count);
    for page in 0..count {
        let Some((start, range)) = ranges.iter().rev().find(|(start, _)| *start <= page) else {
            labels.push((page + 1).to_string());
            continue;
        };
        let prefix = range
            .get(b"P")
            .ok()
            .map(|o| resolve(&document, o))
            .and_then(|o| o.as_str().ok())
            .map(decode)
            .unwrap_or_default();
        let first = range
            .get(b"St")
            .and_then(Object::as_i64)
            .ok()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&n| n >= 1)
            .unwrap_or(1);
        let number = first + (page - start);
        let style = range.get(b"S").and_then(Object::as_name).ok();
        let numbered = match style {
            Some(b"D") => number.to_string(),
            Some(b"R") => roman(number),
            Some(b"r") => roman(number).to_lowercase(),
            Some(b"A") => letters(number),
            Some(b"a") => letters(number).to_lowercase(),
            _ => String::new(),
        };
        labels.push(format!("{prefix}{numbered}"));
    }
    Ok(labels)
}

fn resolve<'a>(document: &'a lopdf::Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => document.get_object(*id).unwrap_or(object),
        _ => object,
    }
}

/// Adds the ranges of the number tree `node` to `out`.
fn collect(
    document: &lopdf::Document,
    node: &Dictionary,
    out: &mut Vec<(usize, Dictionary)>,
    depth: usize,
) {
    if depth > DEPTH {
        return;
    }
    if let Ok(nums) = node.get(b"Nums").map(|o| resolve(document, o))
        && let Ok(nums) = nums.as_array()
    {
        for pair in nums.chunks(2) {
            let [start, range] = pair else {
                continue;
            };
            let start = resolve(document, start)
                .as_i64()
                .ok()
                .and_then(|n| usize::try_from(n).ok());
            let range = resolve(document, range).as_dict().ok();
            if let (Some(start), Some(range)) = (start, range) {
                out.push((start, range.clone()));
            }
        }
    }
    if let Ok(kids) = node.get(b"Kids").map(|o| resolve(document, o))
        && let Ok(kids) = kids.as_array()
    {
        for kid in kids {
            if let Ok(kid) = resolve(document, kid).as_dict() {
                collect(document, kid, out, depth + 1);
            }
        }
    }
}

/// `n` in capital roman numerals.
fn roman(mut n: usize) -> String {
    const NUMERALS: [(usize, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, numeral) in NUMERALS {
        while n >= value {
            out.push_str(numeral);
            n -= value;
        }
    }
    out
}

/// `n` as PDF letters: A to Z, then AA to ZZ, and so on.
fn letters(n: usize) -> String {
    let n = n.max(1) - 1;
    let letter = char::from(b'A' + u8::try_from(n % 26).unwrap_or(0));
    std::iter::repeat_n(letter, n / 26 + 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_as_pdf_says() {
        assert_eq!(roman(4), "IV");
        assert_eq!(roman(1994), "MCMXCIV");
        assert_eq!(letters(1), "A");
        assert_eq!(letters(26), "Z");
        assert_eq!(letters(27), "AA");
        assert_eq!(letters(53), "AAA");
    }

    #[test]
    fn a_pdf_without_labels_has_none() {
        assert_eq!(
            page_labels(&crate::sample::sheets(2, 100.0, 100.0, 0)).unwrap(),
            Vec::<String>::new()
        );
    }
}
