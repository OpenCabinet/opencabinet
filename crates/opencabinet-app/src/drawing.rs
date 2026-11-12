// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The drawing area's content: a room's plan, the elevation of a wall, or
//! one cabinet alone, drawn as the PDF and DXF draw them, turned into path
//! commands the window scales to fit and text labels.
//!
//! Coordinates are millimeters from the drawing's top left corner, `y`
//! down, as the window's paths expect; the shapes have `y` up, so they are
//! flipped here.

use std::fmt::Write as _;

use opencabinet_eval::{Elevation, FrontView, Plan};
use opencabinet_model::{Id, Length};
use opencabinet_reports::Shape;
use opencabinet_reports::elevations::{
    DIMENSIONS, HINGE_MARKS, SELECTED, bounds, cabinet_shapes, elevation_shapes,
};
use opencabinet_reports::plans::{CABINETS_ABOVE, plan_shapes};

/// What the drawing area shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    /// What it is, for screen readers: "Elevation of wall North".
    pub title: String,
    /// Its size in millimeters.
    pub width: f32,
    /// Its height in millimeters.
    pub height: f32,
    /// Walls, openings, cabinets and fronts.
    pub lines: String,
    /// Hinge marks on doors, drawn lighter.
    pub marks: String,
    /// The outline of the cabinet being edited.
    pub highlight: String,
    /// Dimension lines.
    pub dimensions: String,
    /// Labels and dimension values.
    pub labels: Vec<Label>,
}

/// A text centered on a point.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    /// From the drawing's left, in millimeters.
    pub x: f32,
    /// From the drawing's top, in millimeters.
    pub y: f32,
    /// What it says.
    pub text: String,
}

/// Room around a drawing, in millimeters.
const MARGIN: f64 = 100.0;

#[allow(clippy::cast_possible_truncation, reason = "screen coordinates")]
fn f32_of(value: f64) -> f32 {
    value as f32
}

/// The elevation of a wall, with the placements of `selected` outlined;
/// `show` writes a length in the user's units.
#[must_use]
pub fn wall(elevation: &Elevation, selected: Id, show: &dyn Fn(Length) -> String) -> Drawing {
    let shapes = elevation_shapes(elevation, Some(selected), show);
    from_shapes(format!("Elevation of wall {}", elevation.name), &shapes)
}

/// A room seen from above, with the wall `wall` and the footprints of
/// `selected` outlined; `show` writes a length in the user's units.
#[must_use]
pub fn plan(
    plan: &Plan,
    wall: Option<Id>,
    selected: Id,
    show: &dyn Fn(Length) -> String,
) -> Drawing {
    let shapes = plan_shapes(plan, wall, Some(selected), show);
    from_shapes(format!("Plan of {}", plan.room), &shapes)
}

/// One cabinet that is not placed on a wall, seen from the front.
#[must_use]
pub fn cabinet(label: &str, width: Length, height: Length, view: Option<&FrontView>) -> Drawing {
    let shapes = cabinet_shapes(label, width, height, view);
    from_shapes(format!("{label}, not placed on a wall"), &shapes)
}

/// Shapes as paths by layer and labels, flipped and moved so the drawing
/// starts at its top left corner.
fn from_shapes(title: String, shapes: &[Shape]) -> Drawing {
    let ((low_x, low_y), (high_x, high_y)) = bounds(shapes);
    let (left, top) = (low_x - MARGIN, high_y + MARGIN);
    let point = |(x, y): (f64, f64)| (x - left, top - y);
    let mut drawing = Drawing {
        title,
        width: f32_of(high_x - low_x + 2.0 * MARGIN),
        height: f32_of(high_y - low_y + 2.0 * MARGIN),
        ..Drawing::default()
    };
    for shape in shapes {
        let path = match shape.layer() {
            // Dashes, drawn lighter as hinge marks are.
            HINGE_MARKS | CABINETS_ABOVE => &mut drawing.marks,
            DIMENSIONS => &mut drawing.dimensions,
            SELECTED => &mut drawing.highlight,
            _ => &mut drawing.lines,
        };
        match shape {
            Shape::Polygon { points, .. } => {
                for (i, p) in points.iter().enumerate() {
                    let (x, y) = point(*p);
                    let command = if i == 0 { 'M' } else { 'L' };
                    write!(path, "{command} {x:.2} {y:.2} ").expect("to a string");
                }
                path.push_str("Z ");
            }
            Shape::Line { from, to, .. } => {
                let (a, b) = (point(*from), point(*to));
                write!(path, "M {:.2} {:.2} L {:.2} {:.2} ", a.0, a.1, b.0, b.1)
                    .expect("to a string");
            }
            Shape::Circle { .. } => {}
            Shape::Text { at, text, .. } => {
                let (x, y) = point(*at);
                drawing.labels.push(Label {
                    x: f32_of(x),
                    y: f32_of(y),
                    text: text.clone(),
                });
            }
        }
    }
    drawing
}
