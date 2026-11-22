// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A table written as a PDF of its own: a title, a line under it, and rows
//! that wrap their text and carry on to the next page under the same
//! header. Review writes its markup summary with it.

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::Error;
use crate::text::win_ansi_literal;

/// A letter page on its side, in points.
const PAGE: (f64, f64) = (792.0, 612.0);
const MARGIN: f64 = 36.0;
const TITLE_SIZE: f64 = 16.0;
const SIZE: f64 = 8.5;
const LEADING: f64 = 10.5;
const PAD: f64 = 3.0;
/// The square of a row's color, before its first column.
const SWATCH: f64 = 7.0;

/// A table to write.
#[derive(Clone, Debug, Default)]
pub struct Table {
    /// What the report is, such as `Markup summary`.
    pub title: String,
    /// The line under the title: the file, the date, how many.
    pub subtitle: String,
    /// Each column's name and its share of the width.
    pub columns: Vec<(String, f64)>,
    /// The rows, a cell per column.
    pub rows: Vec<Row>,
}

/// A row of a [`Table`].
#[derive(Clone, Debug, Default)]
pub struct Row {
    /// A square of this color before the first cell, if any.
    pub color: Option<[f64; 3]>,
    /// The cells, one per column.
    pub cells: Vec<String>,
}

/// About how wide `text` is in Helvetica at `size`.
fn width(text: &str, size: f64) -> f64 {
    let units: u32 = text
        .chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | '.' | ',' | '\'' | '|' | '!' | ':' | ';' => 250,
            ' ' | 'f' | 't' | 'r' | 'I' | '"' | '-' | '(' | ')' | '/' => 320,
            'm' | 'w' | 'M' | 'W' => 850,
            c if c.is_ascii_uppercase() => 700,
            c if c.is_ascii_digit() => 560,
            _ => 560,
        })
        .sum();
    f64::from(units) / 1000.0 * size
}

/// `text` broken into lines no wider than `room`; a word longer than
/// that is cut.
fn wrap(text: &str, room: f64) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if width(&candidate, SIZE) <= room || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                word.clone_into(&mut line);
            }
            while width(&line, SIZE) > room && line.chars().count() > 1 {
                let mut cut = String::new();
                for c in line.chars() {
                    if width(&format!("{cut}{c}"), SIZE) > room && !cut.is_empty() {
                        break;
                    }
                    cut.push(c);
                }
                let rest: String = line.chars().skip(cut.chars().count()).collect();
                lines.push(cut);
                line = rest;
            }
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn show(out: &mut Vec<u8>, font: &str, size: f64, x: f64, y: f64, text: &str) {
    out.extend_from_slice(format!("BT /{font} {size} Tf {x:.2} {y:.2} Td ").as_bytes());
    out.extend_from_slice(&win_ansi_literal(text));
    out.extend_from_slice(b" Tj ET\n");
}

/// The header row's height.
const HEADER: f64 = LEADING + 2.0 * PAD;
/// Where the header starts, under the title and subtitle.
const TOP: f64 = PAGE.1 - MARGIN - TITLE_SIZE - 20.0;
/// How low rows may go, above the footer.
const BOTTOM: f64 = MARGIN + 14.0;
/// The table's width.
const INNER: f64 = PAGE.0 - 2.0 * MARGIN;

/// A row laid out: its place in the table, each cell's lines, and its
/// height.
type Laid = (usize, Vec<Vec<String>>, f64);

/// How a table is laid out: its columns' widths, and the room before the
/// first column's text for the color squares.
struct Layout {
    widths: Vec<f64>,
    swatch: f64,
}

impl Layout {
    fn of(table: &Table) -> Self {
        let share: f64 = table
            .columns
            .iter()
            .map(|(_, s)| s.max(0.0))
            .sum::<f64>()
            .max(1e-9);
        Self {
            widths: table
                .columns
                .iter()
                .map(|(_, s)| INNER * s.max(0.0) / share)
                .collect(),
            swatch: if table.rows.iter().any(|r| r.color.is_some()) {
                SWATCH + PAD
            } else {
                0.0
            },
        }
    }

    /// The rows, page by page.
    fn pages(&self, table: &Table) -> Vec<Vec<Laid>> {
        let mut pages: Vec<Vec<Laid>> = vec![Vec::new()];
        let mut y = TOP - HEADER;
        for (index, row) in table.rows.iter().enumerate() {
            let cells: Vec<Vec<String>> = self
                .widths
                .iter()
                .enumerate()
                .map(|(column, &w)| {
                    let room = w - 2.0 * PAD - if column == 0 { self.swatch } else { 0.0 };
                    wrap(
                        row.cells.get(column).map_or("", String::as_str),
                        room.max(10.0),
                    )
                })
                .collect();
            let lines = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
            #[allow(clippy::cast_precision_loss, reason = "a row has few lines")]
            let height = lines as f64 * LEADING + 2.0 * PAD;
            if y - height < BOTTOM && !pages.last().is_some_and(Vec::is_empty) {
                pages.push(Vec::new());
                y = TOP - HEADER;
            }
            if let Some(page) = pages.last_mut() {
                page.push((index, cells, height));
            }
            y -= height;
        }
        pages
    }

    /// What page `number` of `count` shows.
    fn draw(&self, table: &Table, rows: &[Laid], number: usize, count: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let left = MARGIN;
        let title_y = PAGE.1 - MARGIN - TITLE_SIZE;
        show(&mut out, "F2", TITLE_SIZE, left, title_y, &table.title);
        show(
            &mut out,
            "F1",
            SIZE + 0.5,
            left,
            title_y - 14.0,
            &table.subtitle,
        );
        let mut y = TOP;
        out.extend_from_slice(
            format!(
                "0.88 g {left:.2} {:.2} {INNER:.2} {HEADER:.2} re f 0 g\n",
                y - HEADER
            )
            .as_bytes(),
        );
        let mut x = left;
        for ((name, _), w) in table.columns.iter().zip(&self.widths) {
            show(&mut out, "F2", SIZE, x + PAD, y - PAD - SIZE, name);
            x += w;
        }
        y -= HEADER;
        for (index, cells, height) in rows {
            if index % 2 == 1 {
                out.extend_from_slice(
                    format!(
                        "0.96 g {left:.2} {:.2} {INNER:.2} {height:.2} re f 0 g\n",
                        y - height
                    )
                    .as_bytes(),
                );
            }
            if let Some([r, g, b]) = table.rows[*index].color {
                let (sx, sy) = (left + PAD, y - PAD - SWATCH - 1.0);
                out.extend_from_slice(
                    format!("{r:.3} {g:.3} {b:.3} rg {sx:.2} {sy:.2} {SWATCH} {SWATCH} re f 0 g\n")
                        .as_bytes(),
                );
            }
            let mut x = left;
            for (column, (lines, w)) in cells.iter().zip(&self.widths).enumerate() {
                let indent = if column == 0 { self.swatch } else { 0.0 };
                let mut line_y = y - PAD - SIZE;
                for line in lines {
                    show(&mut out, "F1", SIZE, x + PAD + indent, line_y, line);
                    line_y -= LEADING;
                }
                x += w;
            }
            y -= height;
            let right = left + INNER;
            out.extend_from_slice(
                format!("0.8 G 0.4 w {left:.2} {y:.2} m {right:.2} {y:.2} l S 0 G\n").as_bytes(),
            );
        }
        let footer = format!("Page {} of {count}", number + 1);
        let footer_x = PAGE.0 - MARGIN - width(&footer, SIZE);
        show(&mut out, "F1", SIZE, footer_x, MARGIN - 4.0, &footer);
        out
    }
}

/// Writes `table` as a PDF.
///
/// # Errors
/// When the PDF cannot be written.
pub fn table(table: &Table) -> Result<Vec<u8>, Error> {
    let layout = Layout::of(table);
    let pages = layout.pages(table);
    let mut doc = lopdf::Document::with_version("1.7");
    let tree = doc.new_object_id();
    let fonts = dictionary! {
        "F1" => doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1",
            "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        }),
        "F2" => doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1",
            "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding",
        }),
    };
    let resources = doc.add_object(dictionary! { "Font" => fonts });
    let mut kids: Vec<Object> = Vec::new();
    for (number, rows) in pages.iter().enumerate() {
        let content = layout.draw(table, rows, number, pages.len());
        let content = doc.add_object(Stream::new(Dictionary::new(), content));
        let leaf = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => tree,
            "MediaBox" => vec![0.into(), 0.into(), PAGE.0.into(), PAGE.1.into()],
            "Resources" => resources,
            "Contents" => content,
        });
        kids.push(leaf.into());
    }
    let count = i64::try_from(kids.len()).unwrap_or(0);
    doc.objects.insert(
        tree,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog: ObjectId = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree });
    doc.trailer.set("Root", catalog);
    let info = doc.add_object(dictionary! {
        "Title" => Object::string_literal(crate::text::encode(&table.title)),
        "Producer" => Object::string_literal("OpenCabinet Review"),
    });
    doc.trailer.set("Info", info);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|e| Error::Annotations(e.to_string()))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_wraps_and_long_words_are_cut() {
        let lines = wrap("Confirm the countertop thickness with the fabricator", 80.0);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| width(l, SIZE) <= 80.0));
        let cut = wrap(&"W".repeat(40), 50.0);
        assert!(cut.len() > 1);
        assert_eq!(cut.concat(), "W".repeat(40));
        assert_eq!(wrap("", 50.0), [""]);
    }
}
