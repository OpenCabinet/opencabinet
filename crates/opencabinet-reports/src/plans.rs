// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plans drawn as shapes: a room's walls at their thickness with their
//! windows, every product's footprint with its label, solid on the floor
//! and dashed above it, and each wall's length outside the room.
//!
//! A plan has `y` down (see `opencabinet_eval::plan`), while shapes have
//! `y` up, so the shapes are the plan turned over: `(x, -y)`. Drawn, the
//! sample kitchen's North wall is at the top.
//!
//! The layers are the elevations' (`WALL`, `OPENINGS`, `CABINETS`,
//! `DIMENSIONS`, `LABEL`, `SELECTED`) and `CABINETS_ABOVE`, for products
//! off the floor, such as wall cabinets.

use opencabinet_eval::Plan;
use opencabinet_model::{Id, Length};

use crate::elevations::{CABINETS, DIMENSIONS, LABEL, OPENINGS, SELECTED, WALL};
use crate::sketch::Shape;

/// Products off the floor, drawn dashed.
pub const CABINETS_ABOVE: &str = "CABINETS_ABOVE";

/// The height of labels and dimension values, in the drawing's
/// millimeters, as on elevations.
const TEXT_HEIGHT: f64 = 50.0;
/// The length of a dimension's end ticks.
const TICK: f64 = 25.0;
/// How far a wall's dimension line stands off its back.
const ROW: f64 = 250.0;
/// The dashes of a product off the floor, and the gaps between them.
const DASH: f64 = 60.0;
const GAP: f64 = 40.0;

fn flip((x, y): (f64, f64)) -> (f64, f64) {
    (x, -y)
}

fn add(a: (f64, f64), b: (f64, f64), times: f64) -> (f64, f64) {
    (a.0 + b.0 * times, a.1 + b.1 * times)
}

fn polygon(layer: &str, points: &[(f64, f64)]) -> Shape {
    Shape::Polygon {
        layer: layer.into(),
        points: points.iter().copied().map(flip).collect(),
    }
}

fn line(layer: &str, from: (f64, f64), to: (f64, f64)) -> Shape {
    Shape::Line {
        layer: layer.into(),
        from: flip(from),
        to: flip(to),
    }
}

/// A text centered on `at`, or standing off it toward `beside`.
fn text(layer: &str, at: (f64, f64), value: String, beside: Option<(f64, f64)>) -> Shape {
    Shape::Text {
        layer: layer.into(),
        at: flip(at),
        height: TEXT_HEIGHT,
        text: value,
        centered: true,
        beside: beside.map(flip),
    }
}

fn unit((x, y): (f64, f64)) -> (f64, f64) {
    let long = x.hypot(y).max(f64::MIN_POSITIVE);
    (x / long, y / long)
}

/// The outline from `points`, in dashes along each side.
fn dashed(shapes: &mut Vec<Shape>, points: &[(f64, f64)]) {
    for (i, &from) in points.iter().enumerate() {
        let to = points[(i + 1) % points.len()];
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let long = dx.hypot(dy);
        if long <= 0.0 {
            continue;
        }
        let along = (dx / long, dy / long);
        let mut at = 0.0;
        while at < long {
            let end = (at + DASH).min(long);
            shapes.push(line(
                CABINETS_ABOVE,
                add(from, along, at),
                add(from, along, end),
            ));
            at = end + GAP;
        }
    }
}

/// A plan as shapes, with the wall `wall` and the footprints of `product`
/// outlined on `SELECTED`; `show` writes a length in the user's units.
#[must_use]
pub fn plan_shapes(
    plan: &Plan,
    wall: Option<Id>,
    product: Option<Id>,
    show: &dyn Fn(Length) -> String,
) -> Vec<Shape> {
    let mut shapes = Vec::new();
    for w in &plan.walls {
        for piece in &w.pieces {
            shapes.push(polygon(WALL, piece));
            if Some(w.wall) == wall {
                shapes.push(polygon(SELECTED, piece));
            }
        }
        for [from, to] in &w.windows {
            shapes.push(line(OPENINGS, *from, *to));
        }

        // The length, on a line off the wall's back, with extension lines
        // from the back and the wall's name beyond the value.
        let [start, end] = w.front;
        let out = w.outward;
        let off = w.thickness + ROW;
        let (a, b) = (add(start, out, off), add(end, out, off));
        shapes.push(line(DIMENSIONS, a, b));
        for (face, on_line) in [(start, a), (end, b)] {
            shapes.push(line(
                DIMENSIONS,
                add(face, out, w.thickness + TICK),
                add(on_line, out, TICK),
            ));
            let along = unit((end.0 - start.0, end.1 - start.1));
            let slant = (
                (along.0 + out.0) * TICK / 2.0,
                (along.1 + out.1) * TICK / 2.0,
            );
            shapes.push(line(
                DIMENSIONS,
                (on_line.0 - slant.0, on_line.1 - slant.1),
                (on_line.0 + slant.0, on_line.1 + slant.1),
            ));
        }
        // The name and the length in one text beyond the line, so that
        // they cannot cover each other however small the plan is drawn.
        let middle = (f64::midpoint(a.0, b.0), f64::midpoint(a.1, b.1));
        shapes.push(text(
            DIMENSIONS,
            add(middle, out, TICK),
            format!("{} · {}", w.name, show(w.length)),
            Some(out),
        ));
    }
    for cabinet in &plan.cabinets {
        if cabinet.above {
            dashed(&mut shapes, &cabinet.corners);
        } else {
            shapes.push(polygon(CABINETS, &cabinet.corners));
        }
        if Some(cabinet.product) == product {
            shapes.push(polygon(SELECTED, &cabinet.corners));
        }
        // The label in the middle; above the floor, inside the back, so
        // that a wall cabinet's stays clear of the base cabinet's below.
        let [left_back, right_back, _, left_front] = cabinet.corners;
        let label = cabinet.label.clone();
        shapes.push(if cabinet.above {
            let back = (
                f64::midpoint(left_back.0, right_back.0),
                f64::midpoint(left_back.1, right_back.1),
            );
            let inward = unit((left_front.0 - left_back.0, left_front.1 - left_back.1));
            text(LABEL, back, label, Some(inward))
        } else {
            let center = cabinet
                .corners
                .iter()
                .fold((0.0, 0.0), |c, p| (c.0 + p.0 / 4.0, c.1 + p.1 / 4.0));
            text(LABEL, center, label, None)
        });
    }
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencabinet_eval::{PlanCabinet, PlanWall};

    #[test]
    fn the_plan_is_turned_over_and_wall_cabinets_are_dashed() {
        let wall = Id::new();
        let product = Id::new();
        let plan = Plan {
            room: "Kitchen".into(),
            walls: vec![PlanWall {
                wall,
                name: "North".into(),
                length: Length::from_mm(1000),
                front: [(0.0, 0.0), (1000.0, 0.0)],
                outward: (0.0, -1.0),
                thickness: 100.0,
                pieces: vec![[(0.0, 0.0), (1000.0, 0.0), (1000.0, -100.0), (0.0, -100.0)]],
                windows: vec![[(200.0, -50.0), (500.0, -50.0)]],
            }],
            cabinets: vec![PlanCabinet {
                placement: Id::new(),
                product,
                label: "W30".into(),
                corners: [(0.0, 0.0), (762.0, 0.0), (762.0, 305.0), (0.0, 305.0)],
                above: true,
            }],
        };
        let shapes = plan_shapes(&plan, Some(wall), Some(product), &|l| format!("{}", l.nm()));
        // The wall's back, away from the room, is up once turned over.
        let Shape::Polygon { points, .. } = &shapes[0] else {
            panic!("the wall first");
        };
        assert_eq!(points[2], (1000.0, 100.0));
        assert_eq!(shapes.iter().filter(|s| s.layer() == SELECTED).count(), 2);
        assert!(shapes.iter().any(|s| s.layer() == OPENINGS));
        assert!(
            shapes
                .iter()
                .filter(|s| s.layer() == CABINETS_ABOVE)
                .count()
                > 4
        );
        assert!(!shapes.iter().any(|s| s.layer() == CABINETS));
        // The name and the length beyond the line, up; the wall cabinet's
        // label inside its back, down toward the room.
        let texts: Vec<_> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text {
                    text, at, beside, ..
                } => Some((text.as_str(), *at, *beside)),
                _ => None,
            })
            .collect();
        assert_eq!(texts[0].0, "North · 1000000000");
        assert_eq!(texts[0].1, (500.0, 375.0));
        assert_eq!(texts[0].2, Some((0.0, 1.0)));
        assert_eq!(texts[1].0, "W30");
        assert_eq!(texts[1].1, (381.0, -0.0));
        assert_eq!(texts[1].2, Some((0.0, -1.0)));
    }
}
