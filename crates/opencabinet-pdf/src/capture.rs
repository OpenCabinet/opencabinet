// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A part of a page taken as it is drawn: its lines and words, not a
//! picture of them, so it stays sharp at any size; and its colors changed,
//! such as the black of a detail made magenta to show it stands apart.
//!
//! [`colors_in`] reads which colors draw inside a part of a page, going
//! through the page's drawing operators with their transforms. [`form`]
//! gives the page's operators with chosen colors changed and its
//! resources, to draw that part elsewhere as a form, clipped to it.
//!
//! Colors set inside forms the page uses (`Do`) and in images are not
//! changed; most drawings set them on the page itself.

#![allow(
    clippy::many_single_char_names,
    reason = "the operands' names PDF gives them: a to f, x, y, c, m, y, k"
)]

use lopdf::content::{Content, Operation};
use lopdf::{Object, ObjectId};

use crate::Error;

/// A color, red, green and blue from 0 to 1.
pub type Rgb = [f64; 3];

/// How close two colors must be to be the same.
const NEAR: f64 = 0.02;

fn fail(why: impl std::fmt::Display) -> Error {
    Error::Annotations(why.to_string())
}

#[expect(clippy::cast_precision_loss, reason = "PDF numbers are small")]
fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(f64::from(*r)),
        _ => None,
    }
}

/// The color the operands of a color operator set, when they are numbers
/// a device color can have.
fn rgb_of(operands: &[Object]) -> Option<Rgb> {
    let n: Vec<f64> = operands.iter().map(number).collect::<Option<_>>()?;
    Some(match n[..] {
        [g] => [g, g, g],
        [r, g, b] => [r, g, b],
        [c, m, y, k] => [
            (1.0 - c) * (1.0 - k),
            (1.0 - m) * (1.0 - k),
            (1.0 - y) * (1.0 - k),
        ],
        _ => return None,
    })
    .map(|c: Rgb| c.map(|v| v.clamp(0.0, 1.0)))
}

/// Whether `a` and `b` are the same color.
#[must_use]
pub fn same(a: Rgb, b: Rgb) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= NEAR)
}

/// Whether the operator sets the fill color (`Some(true)`), the stroke
/// color (`Some(false)`), or neither.
fn sets_color(operator: &str) -> Option<bool> {
    match operator {
        "g" | "rg" | "k" | "sc" | "scn" => Some(true),
        "G" | "RG" | "K" | "SC" | "SCN" => Some(false),
        _ => None,
    }
}

type Matrix = [f64; 6];

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn apply(m: Matrix, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// The page's drawing operators, its id and the document.
fn operators(
    bytes: &[u8],
    page: usize,
) -> Result<(lopdf::Document, ObjectId, Vec<Operation>), Error> {
    let document = lopdf::Document::load_mem(bytes).map_err(|_| Error::Invalid)?;
    let id = *document
        .get_pages()
        .values()
        .nth(page)
        .ok_or(Error::Invalid)?;
    let content = document.get_page_content(id);
    let operations = Content::decode(&content).map_err(fail)?.operations;
    Ok((document, id, operations))
}

/// The colors that draw inside `region` (left, bottom, right, top, in
/// PDF space) of page `page`, the most used first: lines, fills and words.
///
/// # Errors
/// When the PDF or the page's content cannot be read.
#[allow(clippy::too_many_lines, reason = "one arm per PDF drawing operator")]
pub fn colors_in(bytes: &[u8], page: usize, region: [f64; 4]) -> Result<Vec<Rgb>, Error> {
    let (_, _, operations) = operators(bytes, page)?;
    let inside =
        |(x, y): (f64, f64)| x >= region[0] && x <= region[2] && y >= region[1] && y <= region[3];
    let mut counts: Vec<(Rgb, usize)> = Vec::new();
    let mut count = |color: Rgb| match counts.iter_mut().find(|(c, _)| same(*c, color)) {
        Some((_, n)) => *n += 1,
        None => counts.push((color, 1)),
    };
    // Black until set, as PDF says.
    let mut state: Vec<(Matrix, Rgb, Rgb)> = Vec::new();
    let (mut ctm, mut fill, mut stroke): (Matrix, Rgb, Rgb) =
        ([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [0.0; 3], [0.0; 3]);
    let mut path_inside = false;
    let mut current = (0.0, 0.0);
    let mut text: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut line: Matrix = text;
    let mut leading = 0.0;
    for op in &operations {
        let n: Vec<f64> = op.operands.iter().filter_map(number).collect();
        match (op.operator.as_str(), n.as_slice()) {
            ("q", _) => state.push((ctm, fill, stroke)),
            ("Q", _) => {
                if let Some((m, f, s)) = state.pop() {
                    (ctm, fill, stroke) = (m, f, s);
                }
            }
            ("cm", [a, b, c, d, e, f]) => ctm = multiply([*a, *b, *c, *d, *e, *f], ctm),
            (o, _) if sets_color(o).is_some() => {
                if let Some(color) = rgb_of(&op.operands) {
                    if sets_color(o) == Some(true) {
                        fill = color;
                    } else {
                        stroke = color;
                    }
                }
            }
            ("m", [x, y]) => {
                current = apply(ctm, *x, *y);
                path_inside |= inside(current);
            }
            ("l", [x, y]) => {
                let to = apply(ctm, *x, *y);
                // A line crossing the part counts, ends outside or not.
                path_inside |= crosses(&[current, to], region);
                current = to;
            }
            ("c", [x1, y1, x2, y2, x, y]) => {
                let points = [
                    current,
                    apply(ctm, *x1, *y1),
                    apply(ctm, *x2, *y2),
                    apply(ctm, *x, *y),
                ];
                path_inside |= crosses(&points, region);
                current = points[3];
            }
            ("v" | "y", [x1, y1, x, y]) => {
                let points = [current, apply(ctm, *x1, *y1), apply(ctm, *x, *y)];
                path_inside |= crosses(&points, region);
                current = points[2];
            }
            ("re", [x, y, w, h]) => {
                let corners = [
                    apply(ctm, *x, *y),
                    apply(ctm, x + w, *y),
                    apply(ctm, x + w, y + h),
                    apply(ctm, *x, y + h),
                ];
                // Its outline, side by side: a rectangle around the part
                // has no side in it.
                path_inside |= (0..4).any(|i| crosses(&[corners[i], corners[(i + 1) % 4]], region));
                current = corners[0];
            }
            ("S" | "s", _) => {
                if path_inside {
                    count(stroke);
                }
                path_inside = false;
            }
            ("f" | "F" | "f*", _) => {
                if path_inside {
                    count(fill);
                }
                path_inside = false;
            }
            ("B" | "B*" | "b" | "b*", _) => {
                if path_inside {
                    count(fill);
                    count(stroke);
                }
                path_inside = false;
            }
            ("n", _) => path_inside = false,
            ("BT", _) => {
                text = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                line = text;
            }
            ("Tm", [a, b, c, d, e, f]) => {
                text = [*a, *b, *c, *d, *e, *f];
                line = text;
            }
            ("Td" | "TD", [x, y]) => {
                if op.operator == "TD" {
                    leading = -y;
                }
                line = multiply([1.0, 0.0, 0.0, 1.0, *x, *y], line);
                text = line;
            }
            ("TL", [l]) => leading = *l,
            ("T*", _) => {
                line = multiply([1.0, 0.0, 0.0, 1.0, 0.0, -leading], line);
                text = line;
            }
            ("Tj" | "TJ" | "'" | "\"", _) => {
                let at = multiply(text, ctm);
                if inside((at[4], at[5])) {
                    count(fill);
                }
            }
            _ => {}
        }
    }
    counts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    Ok(counts.into_iter().map(|(c, _)| c).collect())
}

/// Whether the box around `points` meets `region`, and, for two points,
/// whether the line between them passes through it.
fn crosses(points: &[(f64, f64)], region: [f64; 4]) -> bool {
    let [x0, y0, x1, y1] = region;
    let (lo_x, hi_x) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            (a.min(p.0), b.max(p.0))
        });
    let (lo_y, hi_y) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            (a.min(p.1), b.max(p.1))
        });
    if hi_x < x0 || lo_x > x1 || hi_y < y0 || lo_y > y1 {
        return false;
    }
    let [(ax, ay), (bx, by)] = points[..] else {
        // A curve: its box meeting the part is close enough.
        return true;
    };
    // A line: clipped to the part's box, something is left.
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    let (dx, dy) = (bx - ax, by - ay);
    for (p, q) in [(-dx, ax - x0), (dx, x1 - ax), (-dy, ay - y0), (dy, y1 - ay)] {
        if p.abs() < f64::EPSILON {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    t0 <= t1
}

/// `operations` with the colors in `map` (from, to) changed, the rest as
/// they were. The default black is set first, so black that was never set
/// changes too.
fn recolor(operations: Vec<Operation>, map: &[(Rgb, Rgb)]) -> Vec<Operation> {
    let replace = |operator: &str, operands: &[Object]| -> Option<Operation> {
        let fill = sets_color(operator)?;
        let from = rgb_of(operands)?;
        let (_, to) = map.iter().find(|(f, _)| same(*f, from))?;
        #[expect(clippy::cast_possible_truncation, reason = "PDF reals")]
        let operands = to.iter().map(|&v| Object::Real(v as f32)).collect();
        Some(Operation::new(if fill { "rg" } else { "RG" }, operands))
    };
    let start = [
        Operation::new("g", vec![Object::Integer(0)]),
        Operation::new("G", vec![Object::Integer(0)]),
    ];
    start
        .into_iter()
        .chain(operations)
        .map(|op| replace(&op.operator, &op.operands).unwrap_or(op))
        .collect()
}

/// What draws `region` of page `page` elsewhere, with the colors in `map`
/// changed: the page's operators, clipped to `region`, and the page's
/// resources, as found in `document`, the PDF the form will go into.
///
/// # Errors
/// When the page or its content cannot be read.
pub fn form(
    document: &lopdf::Document,
    page: ObjectId,
    region: [f64; 4],
    map: &[(Rgb, Rgb)],
) -> Result<(Vec<u8>, Object), Error> {
    let content = document.get_page_content(page);
    let operations = Content::decode(&content).map_err(fail)?.operations;
    let [x0, y0, x1, y1] = region;
    let mut ops = vec![
        Operation::new("q", Vec::new()),
        Operation::new(
            "re",
            [x0, y0, x1 - x0, y1 - y0]
                .iter()
                .map(|&v| {
                    #[expect(clippy::cast_possible_truncation, reason = "PDF reals")]
                    let v = v as f32;
                    Object::Real(v)
                })
                .collect(),
        ),
        Operation::new("W", Vec::new()),
        Operation::new("n", Vec::new()),
    ];
    ops.extend(recolor(operations, map));
    ops.push(Operation::new("Q", Vec::new()));
    let bytes = Content { operations: ops }.encode().map_err(fail)?;
    let (direct, ids) = document.get_page_resources(page).map_err(fail)?;
    let resources = match (direct, ids.first()) {
        (Some(dict), _) => Object::Dictionary(dict.clone()),
        (None, Some(&id)) => Object::Reference(id),
        (None, None) => Object::Dictionary(lopdf::Dictionary::new()),
    };
    Ok((bytes, resources))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page with a black line on the left, a gray box on the right and
    /// red words in the middle, the right half moved by a transform.
    #[allow(clippy::format_push_string, reason = "a small PDF made for a test")]
    fn page() -> Vec<u8> {
        let content = "0 0 0 RG 2 w 10 10 m 90 90 l S \
             q 1 0 0 1 100 0 cm 0.5 g 10 10 50 50 re f Q \
             BT /F1 12 Tf 1 0 0 rg 60 200 Td (HI) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [4 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        ];
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
        }
        let xref = pdf.len();
        let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
        for offset in offsets {
            table.push_str(&format!("{offset:010} 00000 n \n"));
        }
        table.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        ));
        pdf.extend_from_slice(table.as_bytes());
        pdf
    }

    #[test]
    fn the_colors_of_a_part_of_the_page_are_found() {
        let bytes = page();
        let black = [0.0; 3];
        let gray = [0.5; 3];
        let red = [1.0, 0.0, 0.0];
        assert_eq!(
            colors_in(&bytes, 0, [0.0, 0.0, 95.0, 95.0]).unwrap(),
            [black]
        );
        // The box is drawn at x 110 to 160 once moved.
        assert_eq!(
            colors_in(&bytes, 0, [100.0, 0.0, 170.0, 70.0]).unwrap(),
            [gray]
        );
        assert_eq!(
            colors_in(&bytes, 0, [50.0, 190.0, 90.0, 220.0]).unwrap(),
            [red]
        );
        let all = colors_in(&bytes, 0, [0.0, 0.0, 300.0, 300.0]).unwrap();
        assert_eq!(all.len(), 3);
        // The black line crosses this part, its ends outside it.
        assert_eq!(
            colors_in(&bytes, 0, [40.0, 40.0, 60.0, 60.0]).unwrap(),
            [black]
        );
        // Nothing passes here.
        assert_eq!(
            colors_in(&bytes, 0, [200.0, 100.0, 290.0, 180.0])
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn a_part_of_the_page_is_drawn_again_with_colors_changed() {
        let bytes = page();
        let document = lopdf::Document::load_mem(&bytes).unwrap();
        let id = *document.get_pages().values().next().unwrap();
        let magenta = [1.0, 0.0, 1.0];
        let (content, resources) = form(
            &document,
            id,
            [0.0, 0.0, 95.0, 95.0],
            &[([0.0; 3], magenta)],
        )
        .unwrap();
        let text = String::from_utf8_lossy(&content);
        assert!(
            text.starts_with("q\n0.0 0.0 95.0 95.0 re\nW\nn\n1.0 0.0 1.0 rg\n1.0 0.0 1.0 RG"),
            "{text}"
        );
        assert!(!text.contains("0 0 0 RG"), "the black line is magenta now");
        assert!(text.contains("0.5 g"), "gray stays");
        assert!(text.trim_end().ends_with('Q'));
        assert!(resources.as_dict().unwrap().has(b"Font"));
    }
}
