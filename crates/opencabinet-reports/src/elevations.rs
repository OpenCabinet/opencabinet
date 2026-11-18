// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Elevations drawn as shapes (RFC-0001 §5), for the window, for DXF and
//! for PDF: the wall and its openings, every product on it with its toe
//! kick, face frame and fronts, hinge marks pointing to the hinge side,
//! labels and dimensions.
//!
//! Shapes are in millimeters, `x` along the wall from its start and `y` up
//! from the floor, on these layers: `WALL`, `OPENINGS`, `CABINETS`,
//! `FRONTS`, `HINGE_MARKS`, `DIMENSIONS`, `LABEL`, and `SELECTED` for the
//! outline of a product the window is editing.

use std::collections::BTreeSet;

use opencabinet_eval::front::Rect;
use opencabinet_eval::{Dimension, Elevation, FrontKind, FrontView, elevation};
use opencabinet_model::{Id, Length, Library, Project, Side};

use crate::sketch::{DxfFile, Shape, safe_file_name, write_dxf};

/// The wall's outline.
pub const WALL: &str = "WALL";
/// Windows, doors and pass-throughs.
pub const OPENINGS: &str = "OPENINGS";
/// Products' outlines, toe kicks and face frames.
pub const CABINETS: &str = "CABINETS";
/// Doors, drawer fronts and false fronts.
pub const FRONTS: &str = "FRONTS";
/// Lines pointing to a door's hinge side.
pub const HINGE_MARKS: &str = "HINGE_MARKS";
/// Dimension lines and their values.
pub const DIMENSIONS: &str = "DIMENSIONS";
/// Products' labels.
pub const LABEL: &str = "LABEL";
/// The outline of the product being edited, in the window.
pub const SELECTED: &str = "SELECTED";

/// The height of labels and dimension values, in the drawing's
/// millimeters: about 2.5 mm on paper at 1:20.
const TEXT_HEIGHT: f64 = 50.0;
/// The length of a dimension's end ticks.
const TICK: f64 = 25.0;

#[allow(clippy::cast_precision_loss, reason = "a room is far below 2^53 nm")]
fn mm(length: Length) -> f64 {
    length.nm() as f64 / 1_000_000.0
}

/// The outline of `rect`, moved by `at`, on `layer`.
fn rect(layer: &str, rect: Rect, at: (f64, f64)) -> Shape {
    let (left, bottom) = (at.0 + mm(rect.x), at.1 + mm(rect.y));
    let (right, top) = (left + mm(rect.width), bottom + mm(rect.height));
    Shape::Polygon {
        layer: layer.into(),
        points: vec![(left, bottom), (right, bottom), (right, top), (left, top)],
    }
}

fn line(layer: &str, from: (f64, f64), to: (f64, f64)) -> Shape {
    Shape::Line {
        layer: layer.into(),
        from,
        to,
    }
}

fn text(layer: &str, at: (f64, f64), value: String) -> Shape {
    Shape::Text {
        layer: layer.into(),
        at,
        height: TEXT_HEIGHT,
        text: value,
        centered: true,
    }
}

/// A front view at `at`: toe kick, face frame and fronts, with hinge
/// marks from the corners of a door's handle side to the middle of its
/// hinge side.
fn front(shapes: &mut Vec<Shape>, view: &FrontView, at: (f64, f64)) {
    for r in view.toe_kick.iter().chain(&view.frame) {
        shapes.push(rect(CABINETS, *r, at));
    }
    for shape in &view.fronts {
        shapes.push(rect(FRONTS, shape.rect, at));
        let FrontKind::Door { hinge: Some(side) } = shape.kind else {
            continue;
        };
        let r = shape.rect;
        let (x0, y0) = (at.0 + mm(r.x), at.1 + mm(r.y));
        let (x1, y1) = (x0 + mm(r.width), y0 + mm(r.height));
        let (hinge, handle) = match side {
            Side::Left => (x0, x1),
            Side::Right => (x1, x0),
        };
        let middle = (hinge, f64::midpoint(y0, y1));
        shapes.push(line(HINGE_MARKS, (handle, y0), middle));
        shapes.push(line(HINGE_MARKS, (handle, y1), middle));
    }
}

/// An elevation as shapes, with the placements of `selected` outlined on
/// `SELECTED`; `show` writes a length in the user's units.
#[must_use]
pub fn elevation_shapes(
    elevation: &Elevation,
    selected: Option<Id>,
    show: &dyn Fn(Length) -> String,
) -> Vec<Shape> {
    let origin = (0.0, 0.0);
    let whole = Rect {
        x: Length::ZERO,
        y: Length::ZERO,
        width: elevation.length,
        height: elevation.height,
    };
    let mut shapes = vec![rect(WALL, whole, origin)];
    for opening in &elevation.openings {
        shapes.push(rect(OPENINGS, opening.rect, origin));
    }
    for cabinet in &elevation.cabinets {
        let at = (mm(cabinet.rect.x), mm(cabinet.rect.y));
        let outline = Rect {
            x: Length::ZERO,
            y: Length::ZERO,
            ..cabinet.rect
        };
        shapes.push(rect(CABINETS, outline, at));
        if let Some(view) = &cabinet.view {
            front(&mut shapes, view, at);
        }
        if Some(cabinet.product) == selected {
            shapes.push(rect(SELECTED, outline, at));
        }
        let center = (
            at.0 + mm(cabinet.rect.width) / 2.0,
            at.1 + mm(cabinet.rect.height) / 2.0,
        );
        shapes.push(text(LABEL, center, cabinet.label.clone()));
    }
    for dimension in elevation.dimensions() {
        let (from, to, label) = match dimension {
            Dimension::Horizontal { from, to, at } => {
                let y = mm(at);
                let (a, b) = ((mm(from), y), (mm(to), y));
                let value = (f64::midpoint(a.0, b.0), y + TICK + TEXT_HEIGHT / 2.0);
                (a, b, text(DIMENSIONS, value, show(to - from)))
            }
            Dimension::Vertical { from, to, at } => {
                let x = mm(at);
                let (a, b) = ((x, mm(from)), (x, mm(to)));
                let value = (x + TICK + 2.0 * TEXT_HEIGHT, f64::midpoint(a.1, b.1));
                (a, b, text(DIMENSIONS, value, show(to - from)))
            }
        };
        shapes.push(line(DIMENSIONS, from, to));
        for end in [from, to] {
            let tick = (end.0 - TICK / 2.0, end.1 - TICK / 2.0);
            shapes.push(line(DIMENSIONS, tick, (tick.0 + TICK, tick.1 + TICK)));
        }
        shapes.push(label);
    }
    shapes
}

/// A product that is not placed on a wall, seen from the front, outlined
/// on `SELECTED`.
#[must_use]
pub fn cabinet_shapes(
    label: &str,
    width: Length,
    height: Length,
    view: Option<&FrontView>,
) -> Vec<Shape> {
    let outline = Rect {
        x: Length::ZERO,
        y: Length::ZERO,
        width,
        height,
    };
    let origin = (0.0, 0.0);
    let mut shapes = vec![rect(CABINETS, outline, origin)];
    if let Some(view) = view {
        front(&mut shapes, view, origin);
    }
    shapes.push(rect(SELECTED, outline, origin));
    shapes.push(text(
        LABEL,
        (mm(width) / 2.0, mm(height) / 2.0),
        label.to_owned(),
    ));
    shapes
}

/// The lowest and highest corners of what `shapes` cover. A text counts
/// as a box around its point, wide enough for its characters.
#[must_use]
pub fn bounds(shapes: &[Shape]) -> ((f64, f64), (f64, f64)) {
    let mut low = (f64::INFINITY, f64::INFINITY);
    let mut high = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut cover = |(x, y): (f64, f64)| {
        low = (low.0.min(x), low.1.min(y));
        high = (high.0.max(x), high.1.max(y));
    };
    for shape in shapes {
        match shape {
            Shape::Polygon { points, .. } => points.iter().copied().for_each(&mut cover),
            Shape::Circle { center, radius, .. } => {
                cover((center.0 - radius, center.1 - radius));
                cover((center.0 + radius, center.1 + radius));
            }
            Shape::Line { from, to, .. } => {
                cover(*from);
                cover(*to);
            }
            Shape::Text {
                at, height, text, ..
            } => {
                #[allow(clippy::cast_precision_loss, reason = "a short label")]
                let half = text.chars().count() as f64 * height * 0.3;
                cover((at.0 - half, at.1 - height));
                cover((at.0 + half, at.1 + height));
            }
        }
    }
    if low.0 > high.0 {
        return ((0.0, 0.0), (1.0, 1.0));
    }
    (low, high)
}

/// Every wall of the project's rooms with its elevation, in order.
pub(crate) fn every_elevation(project: &Project, library: &Library) -> Vec<Elevation> {
    project
        .rooms
        .iter()
        .flat_map(|room| &room.walls)
        .filter_map(|wall| elevation(project, library, wall.id))
        .collect()
}

/// One DXF per wall, named after it, at full size in millimeters, with
/// lengths shown by `show`.
///
/// # Errors
/// When the project has no walls, or a drawing cannot be written.
pub fn elevations_dxf(
    project: &Project,
    library: &Library,
    show: &dyn Fn(Length) -> String,
) -> Result<Vec<DxfFile>, String> {
    let elevations = every_elevation(project, library);
    if elevations.is_empty() {
        return Err("The project has no walls to draw".into());
    }
    let mut names = BTreeSet::new();
    elevations
        .iter()
        .map(|elevation| {
            let base = safe_file_name(&format!(
                "Elevation - {} - {}",
                elevation.room, elevation.name
            ));
            let mut name = format!("{base}.dxf");
            let mut n = 2;
            while !names.insert(name.clone()) {
                name = format!("{base} ({n}).dxf");
                n += 1;
            }
            Ok(DxfFile {
                name,
                contents: write_dxf(&elevation_shapes(elevation, None, show))?,
            })
        })
        .collect()
}
