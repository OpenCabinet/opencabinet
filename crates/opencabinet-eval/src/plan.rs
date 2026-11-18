// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plans (RFC-0001 §5): a room seen from above, with its walls at their
//! thickness, their openings, and the footprint of every product placed in
//! it.
//!
//! Points are millimeters in the room's plan coordinates. A wall's front
//! face runs from its start to its end, the room on the side of
//! `(-dy, dx)` as elevations have it, and its thickness lies behind it. A
//! plan is meant to be drawn with `y` down, `x` to the right: the sample
//! kitchen's North wall is then at the top and its East wall on the right,
//! and every elevation shows what someone standing in the room sees, the
//! wall's start on their left. RFC-0001 does not settle this yet.

use opencabinet_model::{Id, Length, Project, Wall, WallOpeningKind};

use crate::elevation::{mm, plan_point, wall_length};

/// A room seen from above.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The room's name, such as "Kitchen".
    pub room: String,
    /// Its walls, in the room's order.
    pub walls: Vec<PlanWall>,
    /// The products placed in it, in the room's order.
    pub cabinets: Vec<PlanCabinet>,
}

/// A wall seen from above.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanWall {
    /// The wall.
    pub wall: Id,
    /// Its name, such as "North".
    pub name: String,
    /// From its start to its end.
    pub length: Length,
    /// Its front face, from its start to its end.
    pub front: [(f64, f64); 2],
    /// One millimeter away from the room, square to the front face.
    pub outward: (f64, f64),
    /// Front face to back face, in millimeters.
    pub thickness: f64,
    /// Its outline in pieces, each with four corners: the wall is cut
    /// where a door or a pass-through goes through it, and its back meets
    /// the walls that share its ends.
    pub pieces: Vec<[(f64, f64); 4]>,
    /// Its windows, and openings of kinds from a newer version: each a line
    /// along the middle of the wall, which they do not cut.
    pub windows: Vec<[(f64, f64); 2]>,
}

/// A product's footprint in a room.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanCabinet {
    /// The placement.
    pub placement: Id,
    /// The product placed.
    pub product: Id,
    /// The product's label.
    pub label: String,
    /// Its outline: left back, right back, right front, left front, as
    /// seen from the front.
    pub corners: [(f64, f64); 4],
    /// Whether it is off the floor, as a wall cabinet is, so that a plan
    /// draws it dashed.
    pub above: bool,
}

/// The plan of `room`, if the project has it. Placements of products the
/// project lacks, and those neither on a wall of the room nor given a
/// position, are left out.
#[must_use]
pub fn plan(project: &Project, room: Id) -> Option<Plan> {
    let room = project.rooms.iter().find(|r| r.id == room)?;
    let walls = room
        .walls
        .iter()
        .map(|wall| plan_wall(wall, &room.walls))
        .collect();
    let cabinets = room
        .placements
        .iter()
        .filter_map(|placement| {
            let product = project
                .products
                .iter()
                .find(|p| p.id == placement.product)?;
            let (width, depth) = (mm(product.width), mm(product.depth));
            let (corners, bottom) = if let Some(wall) = placement
                .wall
                .and_then(|id| room.walls.iter().find(|w| w.id == id))
            {
                let left = mm(placement.offset.unwrap_or_default());
                let at = |a: f64, b: f64| plan_point(wall, a, b);
                (
                    [
                        at(left, 0.0),
                        at(left + width, 0.0),
                        at(left + width, depth),
                        at(left, depth),
                    ],
                    placement.elevation.unwrap_or_default(),
                )
            } else {
                let origin = placement.position?;
                // The product's width runs along the turned x axis and its
                // depth along the turned y axis, as on a wall running that
                // way.
                #[allow(clippy::cast_precision_loss, reason = "below 2^53 µdeg")]
                let turn =
                    placement.rotation.unwrap_or_default().microdegrees() as f64 / 1_000_000.0;
                let (sin, cos) = turn.to_radians().sin_cos();
                let (x, y) = (mm(origin.x), mm(origin.y));
                let at = |a: f64, b: f64| (x + a * cos - b * sin, y + a * sin + b * cos);
                (
                    [
                        at(0.0, 0.0),
                        at(width, 0.0),
                        at(width, depth),
                        at(0.0, depth),
                    ],
                    origin.z,
                )
            };
            Some(PlanCabinet {
                placement: placement.id,
                product: product.id,
                label: product.label.clone(),
                corners,
                above: bottom > Length::ZERO,
            })
        })
        .collect();
    Some(Plan {
        room: room.name.clone(),
        walls,
        cabinets,
    })
}

/// A wall of `walls` seen from above.
fn plan_wall(wall: &Wall, walls: &[Wall]) -> PlanWall {
    let length = wall_length(wall);
    let (long, thick) = (mm(length), mm(wall.thickness));
    let front = [plan_point(wall, 0.0, 0.0), plan_point(wall, long, 0.0)];
    let back = |a: f64| plan_point(wall, a, -thick);
    let outward = {
        let (x, y) = back(0.0);
        let (x, y) = (x - front[0].0, y - front[0].1);
        let norm = x.hypot(y).max(f64::MIN_POSITIVE);
        (x / norm, y / norm)
    };
    // The back's ends, where it meets the backs of the walls sharing the
    // wall's start and end.
    let back_ends = [
        meeting(wall, walls, wall.start, front[0]).unwrap_or_else(|| back(0.0)),
        meeting(wall, walls, wall.end, front[1]).unwrap_or_else(|| back(long)),
    ];

    // Where the wall stands, from start to end, without what doors and
    // pass-throughs take.
    let mut gaps: Vec<(f64, f64)> = wall
        .openings
        .iter()
        .filter(|o| matches!(o.kind, WallOpeningKind::Door | WallOpeningKind::PassThrough))
        .map(|o| {
            let from = mm(o.offset).clamp(0.0, long);
            (from, mm(o.offset + o.width).clamp(from, long))
        })
        .filter(|(from, to)| to > from)
        .collect();
    gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut stands = Vec::new();
    let mut from = 0.0;
    for (start, end) in gaps {
        if start > from {
            stands.push((from, start));
        }
        from = from.max(end);
    }
    if from < long {
        stands.push((from, long));
    }
    let pieces = stands
        .into_iter()
        .map(|(a, b)| {
            let back_a = if a <= 0.0 { back_ends[0] } else { back(a) };
            let back_b = if b >= long { back_ends[1] } else { back(b) };
            [
                plan_point(wall, a, 0.0),
                plan_point(wall, b, 0.0),
                back_b,
                back_a,
            ]
        })
        .collect();

    let windows = wall
        .openings
        .iter()
        .filter(|o| !matches!(o.kind, WallOpeningKind::Door | WallOpeningKind::PassThrough))
        .map(|o| {
            let from = mm(o.offset).clamp(0.0, long);
            let to = mm(o.offset + o.width).clamp(from, long);
            [
                plan_point(wall, from, -thick / 2.0),
                plan_point(wall, to, -thick / 2.0),
            ]
        })
        .collect();

    PlanWall {
        wall: wall.id,
        name: wall.name.clone(),
        length,
        front,
        outward,
        thickness: thick,
        pieces,
        windows,
    }
}

/// Where `wall`'s back meets the back of another wall of `walls` that
/// starts or ends at `end`, the point of `wall`'s front face at that end;
/// `None` without one, or when the two run straight on, or meet at so
/// sharp an angle that the corner would reach far beyond the walls.
fn meeting(
    wall: &Wall,
    walls: &[Wall],
    end: opencabinet_model::Point,
    at: (f64, f64),
) -> Option<(f64, f64)> {
    let other = walls
        .iter()
        .find(|w| w.id != wall.id && (w.start == end || w.end == end))?;
    let line = |w: &Wall| {
        let long = mm(wall_length(w));
        let thick = mm(w.thickness);
        let p = plan_point(w, 0.0, -thick);
        let q = plan_point(w, long, -thick);
        (p, (q.0 - p.0, q.1 - p.1))
    };
    let ((from, along), (other_from, other_along)) = (line(wall), line(other));
    let cross = along.0 * other_along.1 - along.1 * other_along.0;
    let scale = along.0.hypot(along.1) * other_along.0.hypot(other_along.1);
    if cross.abs() <= scale * 1e-9 {
        return None;
    }
    let between = (other_from.0 - from.0, other_from.1 - from.1);
    let part = (between.0 * other_along.1 - between.1 * other_along.0) / cross;
    let point = (from.0 + part * along.0, from.1 + part * along.1);
    let reach = mm(wall.thickness).max(mm(other.thickness)) * 10.0;
    ((point.0 - at.0).hypot(point.1 - at.1) <= reach).then_some(point)
}
