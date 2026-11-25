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
//! Colors set inside forms the page draws (`Do`) are found and changed
//! too, the forms copied; colors in images are not.

#![allow(
    clippy::many_single_char_names,
    reason = "the operands' names PDF gives them: a to f, x, y, c, m, y, k"
)]

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Object, ObjectId};

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

/// How deep forms inside forms are followed.
const DEPTH: usize = 6;

/// The page's drawing operators, its resources, and the document.
fn operators(
    bytes: &[u8],
    page: usize,
) -> Result<(lopdf::Document, Dictionary, Vec<Operation>), Error> {
    let document = lopdf::Document::load_mem(bytes).map_err(|_| Error::Invalid)?;
    let id = *document
        .get_pages()
        .values()
        .nth(page)
        .ok_or(Error::Invalid)?;
    let content = document.get_page_content(id);
    let operations = Content::decode(&content).map_err(fail)?.operations;
    let resources = page_resources(&document, id);
    Ok((document, resources, operations))
}

/// A page's resources, as a dictionary whose entries may point elsewhere
/// in `document`.
fn page_resources(document: &lopdf::Document, page: ObjectId) -> Dictionary {
    let Ok((direct, ids)) = document.get_page_resources(page) else {
        return Dictionary::new();
    };
    direct
        .cloned()
        .or_else(|| {
            ids.first()
                .and_then(|&id| document.get_dictionary(id).ok().cloned())
        })
        .unwrap_or_default()
}

/// The dictionary `object` is or points to.
fn dictionary_of<'a>(document: &'a lopdf::Document, object: &'a Object) -> Option<&'a Dictionary> {
    match object {
        Object::Dictionary(d) => Some(d),
        Object::Reference(id) => document.get_dictionary(*id).ok(),
        _ => None,
    }
}

/// The form called `name` in `resources`: its stream, its operators, and
/// its own resources (or those it is drawn with, when it has none).
fn form_named(
    document: &lopdf::Document,
    resources: &Dictionary,
    name: &[u8],
) -> Option<(lopdf::Stream, Vec<Operation>, Dictionary)> {
    let xobjects = dictionary_of(document, resources.get(b"XObject").ok()?)?;
    let id = xobjects.get(name).ok()?.as_reference().ok()?;
    let stream = document.get_object(id).ok()?.as_stream().ok()?;
    if stream.dict.get(b"Subtype").ok()?.as_name().ok()? != b"Form" {
        return None;
    }
    let content = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    let operations = Content::decode(&content).ok()?.operations;
    let inner = stream
        .dict
        .get(b"Resources")
        .ok()
        .and_then(|r| dictionary_of(document, r))
        .cloned()
        .unwrap_or_else(|| resources.clone());
    Some((stream.clone(), operations, inner))
}

/// A form's matrix, or none.
fn matrix_of(stream: &lopdf::Stream) -> Matrix {
    stream
        .dict
        .get(b"Matrix")
        .ok()
        .and_then(|m| m.as_array().ok())
        .and_then(|m| {
            let n: Vec<f64> = m.iter().filter_map(number).collect();
            <[f64; 6]>::try_from(n).ok()
        })
        .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
}

/// Goes through drawing operators, counting the colors that draw inside a
/// part of the page.
struct Walk<'a> {
    document: &'a lopdf::Document,
    region: [f64; 4],
    counts: Vec<(Rgb, usize)>,
}

/// The graphics state a walk keeps: transform, fill and stroke.
type State = (Matrix, Rgb, Rgb);

impl Walk<'_> {
    fn inside(&self, (x, y): (f64, f64)) -> bool {
        let r = self.region;
        x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3]
    }

    fn count(&mut self, color: Rgb) {
        match self.counts.iter_mut().find(|(c, _)| same(*c, color)) {
            Some((_, n)) => *n += 1,
            None => self.counts.push((color, 1)),
        }
    }

    /// Walks `operations` drawn with `resources`, starting in `start`.
    #[allow(clippy::too_many_lines, reason = "one arm per PDF drawing operator")]
    fn operations(
        &mut self,
        operations: &[Operation],
        resources: &Dictionary,
        start: State,
        depth: usize,
    ) {
        let mut state: Vec<State> = Vec::new();
        let (mut ctm, mut fill, mut stroke) = start;
        let mut path_inside = false;
        let mut current = (0.0, 0.0);
        let mut text: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let mut line: Matrix = text;
        let mut leading = 0.0;
        for op in operations {
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
                ("Do", _) if depth < DEPTH => {
                    let name = op.operands.first().and_then(|n| n.as_name().ok());
                    if let Some((stream, inner_ops, inner)) =
                        name.and_then(|n| form_named(self.document, resources, n))
                    {
                        let inner_ctm = multiply(matrix_of(&stream), ctm);
                        self.operations(&inner_ops, &inner, (inner_ctm, fill, stroke), depth + 1);
                    }
                }
                ("m", [x, y]) => {
                    current = apply(ctm, *x, *y);
                    path_inside |= self.inside(current);
                }
                ("l", [x, y]) => {
                    let to = apply(ctm, *x, *y);
                    // A line crossing the part counts, ends outside or not.
                    path_inside |= crosses(&[current, to], self.region);
                    current = to;
                }
                ("c", [x1, y1, x2, y2, x, y]) => {
                    let points = [
                        current,
                        apply(ctm, *x1, *y1),
                        apply(ctm, *x2, *y2),
                        apply(ctm, *x, *y),
                    ];
                    path_inside |= crosses(&points, self.region);
                    current = points[3];
                }
                ("v" | "y", [x1, y1, x, y]) => {
                    let points = [current, apply(ctm, *x1, *y1), apply(ctm, *x, *y)];
                    path_inside |= crosses(&points, self.region);
                    current = points[2];
                }
                ("re", [x, y, w, h]) => {
                    let corners = [
                        apply(ctm, *x, *y),
                        apply(ctm, x + w, *y),
                        apply(ctm, x + w, y + h),
                        apply(ctm, *x, y + h),
                    ];
                    // Its outline, side by side: a rectangle around the
                    // part has no side in it.
                    path_inside |=
                        (0..4).any(|i| crosses(&[corners[i], corners[(i + 1) % 4]], self.region));
                    current = corners[0];
                }
                ("S" | "s", _) => {
                    if path_inside {
                        self.count(stroke);
                    }
                    path_inside = false;
                }
                ("f" | "F" | "f*", _) => {
                    if path_inside {
                        self.count(fill);
                    }
                    path_inside = false;
                }
                ("B" | "B*" | "b" | "b*", _) => {
                    if path_inside {
                        self.count(fill);
                        self.count(stroke);
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
                    if self.inside((at[4], at[5])) {
                        self.count(fill);
                    }
                }
                _ => {}
            }
        }
    }
}

/// The colors that draw inside `region` (left, bottom, right, top, in
/// PDF space) of page `page`, the most used first: lines, fills and words,
/// those inside forms the page draws too.
///
/// # Errors
/// When the PDF or the page's content cannot be read.
pub fn colors_in(bytes: &[u8], page: usize, region: [f64; 4]) -> Result<Vec<Rgb>, Error> {
    let (document, resources, operations) = operators(bytes, page)?;
    let mut walk = Walk {
        document: &document,
        region,
        counts: Vec::new(),
    };
    // Black until set, as PDF says.
    walk.operations(
        &operations,
        &resources,
        ([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [0.0; 3], [0.0; 3]),
        0,
    );
    let mut counts = walk.counts;
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
/// they were.
fn recolor(operations: Vec<Operation>, map: &[(Rgb, Rgb)]) -> Vec<Operation> {
    let replace = |operator: &str, operands: &[Object]| -> Option<Operation> {
        let fill = sets_color(operator)?;
        let from = rgb_of(operands)?;
        let (_, to) = map.iter().find(|(f, _)| same(*f, from))?;
        #[expect(clippy::cast_possible_truncation, reason = "PDF reals")]
        let operands = to.iter().map(|&v| Object::Real(v as f32)).collect();
        Some(Operation::new(if fill { "rg" } else { "RG" }, operands))
    };
    operations
        .into_iter()
        .map(|op| replace(&op.operator, &op.operands).unwrap_or(op))
        .collect()
}

/// Resources to draw with, and the forms in them drawn again with colors
/// changed, to be put in the PDF in place of theirs.
#[derive(Clone, Debug, PartialEq)]
pub struct Recolored {
    /// The resources; when `forms` has some, its `XObject` entry is a
    /// dictionary of its own, for them to go in.
    pub resources: Dictionary,
    /// Each form changed: its name in `resources`, and it.
    pub forms: Vec<(Vec<u8>, RecoloredForm)>,
}

/// A form drawn again with colors changed.
#[derive(Clone, Debug, PartialEq)]
pub struct RecoloredForm {
    /// Its dictionary, without its resources or how it was packed.
    pub dict: Dictionary,
    /// Its operators, colors changed.
    pub content: Vec<u8>,
    /// Its resources, and the forms in them changed.
    pub inner: Recolored,
}

/// `resources` with the forms they hold drawn again with the colors in
/// `map` changed, as deep as [`DEPTH`].
fn recolor_resources(
    document: &lopdf::Document,
    resources: &Dictionary,
    map: &[(Rgb, Rgb)],
    depth: usize,
) -> Recolored {
    let mut out = Recolored {
        resources: resources.clone(),
        forms: Vec::new(),
    };
    if map.is_empty() || depth >= DEPTH {
        return out;
    }
    let Some(xobjects) = resources
        .get(b"XObject")
        .ok()
        .and_then(|x| dictionary_of(document, x))
        .cloned()
    else {
        return out;
    };
    for (name, _) in &xobjects {
        let Some((stream, operations, inner)) = form_named(document, resources, name) else {
            continue;
        };
        let Ok(content) = (Content {
            operations: recolor(operations, map),
        })
        .encode() else {
            continue;
        };
        let mut dict = stream.dict.clone();
        for key in [&b"Resources"[..], b"Filter", b"DecodeParms", b"Length"] {
            dict.remove(key);
        }
        out.forms.push((
            name.clone(),
            RecoloredForm {
                dict,
                content,
                inner: recolor_resources(document, &inner, map, depth + 1),
            },
        ));
    }
    if !out.forms.is_empty() {
        out.resources.set("XObject", Object::Dictionary(xobjects));
    }
    out
}

/// What draws `region` of page `page` elsewhere, with the colors in `map`
/// changed: the page's operators, clipped to `region`, and the page's
/// resources with the forms in them changed too, as found in `document`,
/// the PDF the form will go into.
///
/// # Errors
/// When the page or its content cannot be read.
pub fn form(
    document: &lopdf::Document,
    page: ObjectId,
    region: [f64; 4],
    map: &[(Rgb, Rgb)],
) -> Result<(Vec<u8>, Recolored), Error> {
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
        // The default black is set first, so black that was never set
        // changes too.
        Operation::new("g", vec![Object::Integer(0)]),
        Operation::new("G", vec![Object::Integer(0)]),
    ];
    let black = ops.split_off(4);
    ops.extend(recolor(black.into_iter().chain(operations).collect(), map));
    ops.push(Operation::new("Q", Vec::new()));
    let bytes = Content { operations: ops }.encode().map_err(fail)?;
    let resources = page_resources(document, page);
    Ok((bytes, recolor_resources(document, &resources, map, 0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page with a black line on the left, a gray box on the right and
    /// red words in the middle, the right half moved by a transform.
    fn page() -> Vec<u8> {
        let content = "0 0 0 RG 2 w 10 10 m 90 90 l S \
             q 1 0 0 1 100 0 cm 0.5 g 10 10 50 50 re f Q \
             BT /F1 12 Tf 1 0 0 rg 60 200 Td (HI) Tj ET";
        pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [4 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R >>".to_owned(),
            stream("", content),
        ])
    }

    fn stream(dict: &str, content: &str) -> String {
        format!(
            "<< {dict} /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        )
    }

    /// A PDF of `objects`, numbered from 1.
    #[allow(clippy::format_push_string, reason = "a small PDF made for a test")]
    fn pdf(objects: &[String]) -> Vec<u8> {
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
    fn colors_inside_forms_are_found_and_changed() {
        // The page draws a form, moved 100 to the right, which draws a
        // blue line.
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << /XObject << /Blk 5 0 R >> >> /Contents 4 0 R >>".to_owned(),
            stream("", "q 1 0 0 1 100 0 cm /Blk Do Q"),
            stream(
                "/Type /XObject /Subtype /Form /BBox [0 0 100 100]",
                "0 0 1 RG 10 10 m 90 90 l S",
            ),
        ]);
        let blue = [0.0, 0.0, 1.0];
        assert_eq!(
            colors_in(&bytes, 0, [100.0, 0.0, 200.0, 100.0]).unwrap(),
            [blue]
        );
        assert_eq!(
            colors_in(&bytes, 0, [0.0, 0.0, 95.0, 95.0]).unwrap().len(),
            0
        );
        let document = lopdf::Document::load_mem(&bytes).unwrap();
        let id = *document.get_pages().values().next().unwrap();
        let (_, recolored) = form(
            &document,
            id,
            [100.0, 0.0, 200.0, 100.0],
            &[(blue, [1.0, 0.0, 1.0])],
        )
        .unwrap();
        let [(name, changed)] = &recolored.forms[..] else {
            panic!("the form is changed: {recolored:?}");
        };
        assert_eq!(name, b"Blk");
        let text = String::from_utf8_lossy(&changed.content);
        assert!(text.contains("1.0 0.0 1.0 RG"), "{text}");
        assert!(!changed.dict.has(b"Length"));
        assert!(
            recolored
                .resources
                .get(b"XObject")
                .unwrap()
                .as_dict()
                .is_ok()
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
        assert!(resources.resources.has(b"Font"));
        assert_eq!(resources.forms.len(), 0);
    }
}
