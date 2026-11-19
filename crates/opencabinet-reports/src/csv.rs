// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The cut list and the bill of materials as CSV, for spreadsheets and
//! other programs.

use std::fmt::Write;

use crate::bill_of_materials::{Bom, BomRow};
use crate::cut_list::{CutList, CutRow};

/// What separates the fields of a row.
///
/// The CSV standard (RFC 4180) uses commas, but Excel reads a file with
/// the list separator of the computer's language settings: in Spanish and
/// most languages that write decimals with a comma, a semicolon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Separator {
    /// `,`, as the standard and English-language spreadsheets expect.
    Comma,
    /// `;`, as Excel expects in Spanish and many other languages.
    Semicolon,
}

impl Separator {
    fn char(self) -> char {
        match self {
            Self::Comma => ',',
            Self::Semicolon => ';',
        }
    }
}

/// The cut list as CSV text: a byte order mark so that Excel reads it as
/// UTF-8, a header row, one row per part, and CRLF line ends, with fields
/// quoted when they contain the separator, a quote or a line break, as
/// RFC 4180 describes. Inch marks make quoting common.
#[must_use]
pub fn to_csv(list: &CutList, separator: Separator) -> String {
    table(
        &CutRow::TITLES,
        list.rows.iter().map(CutRow::cells),
        separator,
    )
}

/// The bill of materials as CSV text, written as [`to_csv`] writes the cut
/// list.
#[must_use]
pub fn materials_to_csv(bom: &Bom, separator: Separator) -> String {
    table(
        &BomRow::TITLES,
        bom.rows.iter().map(BomRow::cells),
        separator,
    )
}

/// A header row and rows of cells as CSV text.
fn table<'r, const N: usize>(
    titles: &[&str; N],
    rows: impl Iterator<Item = [&'r str; N]>,
    separator: Separator,
) -> String {
    let separator = separator.char();
    let mut csv = String::from('\u{feff}');
    let mut line = |cells: &[&str]| {
        let fields: Vec<String> = cells.iter().map(|c| field(c, separator)).collect();
        let mut row = String::new();
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                row.push(separator);
            }
            row.push_str(field);
        }
        write!(csv, "{row}\r\n").expect("writing to a String");
    };
    line(titles);
    for row in rows {
        line(&row);
    }
    csv
}

fn field(cell: &str, separator: char) -> String {
    if cell.contains([separator, '"', '\r', '\n']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> CutList {
        CutList {
            rows: vec![CutRow {
                product: "B30".into(),
                part: "Bottom".into(),
                material: "Plywood; 3/4\"".into(),
                quantity: "2".into(),
                length: "28-19/32\"".into(),
                width: "24\"".into(),
                thickness: "23/32\"".into(),
                cut_length: "28-19/32\"".into(),
                cut_width: "24\"".into(),
                edges: "1 edge".into(),
            }],
            problems: Vec::new(),
        }
    }

    #[test]
    fn starts_with_a_byte_order_mark_and_a_header() {
        let csv = to_csv(&list(), Separator::Comma);
        assert!(csv.starts_with("\u{feff}Product,Part,Material,Qty,"));
        assert!(csv.ends_with("\r\n"));
        assert_eq!(csv.lines().count(), 2);
    }

    #[test]
    fn quotes_fields_with_inch_marks_or_the_separator() {
        let comma = to_csv(&list(), Separator::Comma);
        assert_eq!(
            comma.lines().nth(1).unwrap(),
            "B30,Bottom,\"Plywood; 3/4\"\"\",2,\"28-19/32\"\"\",\"24\"\"\",\
             \"23/32\"\"\",\"28-19/32\"\"\",\"24\"\"\",1 edge"
        );
        let semicolon = to_csv(&list(), Separator::Semicolon);
        assert!(
            semicolon
                .lines()
                .nth(1)
                .unwrap()
                .starts_with("B30;Bottom;\"Plywood; 3/4\"\"\";2;")
        );
    }
}
