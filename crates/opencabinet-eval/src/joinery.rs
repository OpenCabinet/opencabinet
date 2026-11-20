// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Joinery: the dowels and confirmat screws that join the case's sides to
//! the parts they hold, by the standard's `joinery` rules.
//!
//! Each joint is the end of a bottom, a top or a stretcher against the
//! inside face of a side. Its fasteners stand in one row along the joint,
//! `spacing` apart and centered on it, at least `endDistance` from its
//! ends. A joint too short for `minimum` of them at that spacing takes the
//! minimum, evenly from one end distance to the other, as long as they stay
//! at least two diameters apart; otherwise one, in the middle. Confirmat
//! joints take at least two screws. With `both`, dowels and screws
//! alternate from the front, at the dowels' spacing.
//!
//! Every fastener is placed in the product first, then bored into each
//! part in that part's own coordinates, so the hole in the side and the
//! hole in the edge always meet.
//!
//! - A **dowel** (`diameter`, `length`) takes a hole `faceDepth` deep in
//!   the side's face A and one `edgeDepth` deep in the other part's edge,
//!   centered in its thickness. The two together must hold the dowel, and
//!   the face hole must not go through the side.
//! - A **confirmat** screw (`shankDiameter`, `pilotDiameter`, `length`)
//!   takes a hole for its shank through the side from face B, countersunk
//!   for its head, and a pilot hole in the edge as deep as the screw
//!   reaches past the side, plus `extraDepth`.

use opencabinet_model::{Hardware, Id, Length};

use crate::machining::{Edge, Face, Operation, Point};
use crate::rules::{CaseJoinery, Confirmat, Dowels, Fastening};
use crate::{HardwareKind, Lookup, Part};

/// A dowel or screw that joins two parts, carried by the part it goes into
/// edgewise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fastener {
    /// What it is.
    pub kind: FastenerKind,
    /// It, from the library's hardware.
    pub hardware: Id,
    /// Its end in the side: the tip of a dowel, the head of a screw; in
    /// the carrying part's own `(u, v, w)`.
    pub start: [Length; 3],
    /// Its other end, in the carrying part's own `(u, v, w)`.
    pub end: [Length; 3],
    /// Its diameter.
    pub diameter: Length,
}

/// A kind of fastener.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FastenerKind {
    /// A glued wooden dowel.
    Dowel,
    /// A confirmat screw, its head the diameter of its countersink.
    Confirmat {
        /// The head's diameter.
        head: Length,
    },
}

/// The sizes of a dowel, read from the library.
#[derive(Clone, Copy)]
struct DowelSize<'r> {
    rules: &'r Dowels,
    diameter: Length,
    length: Length,
}

/// The sizes of a confirmat screw, read from the library.
#[derive(Clone, Copy)]
struct ScrewSize<'r> {
    rules: &'r Confirmat,
    shank: Length,
    pilot: Length,
    length: Length,
}

/// Where one fastener of a joint goes.
#[derive(Clone, Copy)]
struct Spot {
    /// The side, 0 left and 1 right.
    side: usize,
    /// The end of the part it enters.
    edge: Edge,
    /// That end, along the part's length.
    end: Length,
    /// Across the part's width.
    along: Length,
    /// On the side's faces.
    at: Point,
}

/// One fastener of a joint.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dowel,
    Screw,
}

/// Where fasteners go along a joint `length` long: `spacing` apart, as
/// many as fit at least `end` from both ends, centered. When fewer than
/// `minimum` fit, `minimum` evenly from `end` to `length − end` if that
/// keeps them `gap` apart, or else one in the middle. `None` when the joint
/// is shorter than twice `end`.
pub(crate) fn row(
    length: Length,
    end: Length,
    spacing: Length,
    (minimum, gap): (u32, Length),
) -> Option<Vec<Length>> {
    let room = length - end * 2;
    if room < Length::ZERO || spacing <= Length::ZERO {
        return None;
    }
    let fit = u32::try_from(room.nm() / spacing.nm()).ok()? + 1;
    if fit >= minimum.max(1) {
        let span = spacing * i64::from(fit - 1);
        let first = (length - span).div_round(2);
        return Some((0..fit).map(|i| first + spacing * i64::from(i)).collect());
    }
    if room < gap * i64::from(minimum - 1) {
        return Some(vec![length.div_round(2)]);
    }
    let mut at = end;
    let mut positions = vec![at];
    for step in room.split(minimum - 1) {
        at += step;
        positions.push(at);
    }
    Some(positions)
}

/// The dimension `name` of `hardware`.
fn dimension(hardware: &Hardware, name: &str) -> Result<Length, String> {
    hardware.dimensions.get(name).copied().ok_or_else(|| {
        format!(
            "the {} `{}` has no `{name}`",
            hardware.category, hardware.name
        )
    })
}

/// Bores the joints between the sides and the parts they hold, and gives
/// those parts their fasteners. Returns the fasteners used, for the bill of
/// materials.
pub(crate) fn join_case(
    parts: &mut [Part],
    rules: &CaseJoinery,
    lookup: &Lookup<'_>,
) -> Result<Vec<(HardwareKind, u32)>, String> {
    let needs = |kind: Kind| {
        [rules.bottom, rules.top, rules.stretchers]
            .iter()
            .any(|f| match kind {
                Kind::Dowel => matches!(f, Fastening::Dowels | Fastening::Both),
                Kind::Screw => matches!(f, Fastening::Confirmat | Fastening::Both),
            })
    };
    let dowel = if needs(Kind::Dowel) {
        let rules = rules
            .dowels
            .as_ref()
            .ok_or("the standard joins the case with dowels but has no `dowels` rules")?;
        let hardware = lookup.hardware(rules.hardware, "dowel")?;
        Some(DowelSize {
            rules,
            diameter: dimension(hardware, "diameter")?,
            length: dimension(hardware, "length")?,
        })
    } else {
        None
    };
    let screw = if needs(Kind::Screw) {
        let rules = rules
            .confirmat
            .as_ref()
            .ok_or("the standard joins the case with confirmat but has no `confirmat` rules")?;
        let hardware = lookup.hardware(rules.hardware, "confirmat screw")?;
        Some(ScrewSize {
            rules,
            shank: dimension(hardware, "shankDiameter")?,
            pilot: dimension(hardware, "pilotDiameter")?,
            length: dimension(hardware, "length")?,
        })
    } else {
        None
    };
    let side_index = |key: &str| parts.iter().position(|p| p.key == key);
    let (Some(left), Some(right)) = (side_index("case/side.left"), side_index("case/side.right"))
    else {
        return Ok(Vec::new());
    };
    let mut joints = Joints {
        dowel,
        screw,
        side_operations: [Vec::new(), Vec::new()],
        counts: (0, 0),
        sides: [parts[left].clone(), parts[right].clone()],
        lookup,
    };
    for part in parts.iter_mut() {
        let fastening = match part.role {
            "bottom" => rules.bottom,
            "top" => rules.top,
            "stretcher" => rules.stretchers,
            _ => continue,
        };
        if fastening == Fastening::None {
            continue;
        }
        joints.join(part, fastening)?;
    }
    let Joints {
        side_operations: [to_left, to_right],
        counts: (dowels, screws),
        ..
    } = joints;
    parts[left].operations.extend(to_left);
    parts[right].operations.extend(to_right);
    let mut hardware = Vec::new();
    if let (Some(size), true) = (dowel, dowels > 0) {
        hardware.push((HardwareKind::Fastener(size.rules.hardware), dowels));
    }
    if let (Some(size), true) = (screw, screws > 0) {
        hardware.push((HardwareKind::Fastener(size.rules.hardware), screws));
    }
    Ok(hardware)
}

/// The joints of one case being bored.
struct Joints<'r, 'l, 'p> {
    dowel: Option<DowelSize<'r>>,
    screw: Option<ScrewSize<'r>>,
    /// The left and right sides, as they were placed.
    sides: [Part; 2],
    /// What to bore into the left and right sides.
    side_operations: [Vec<Operation>; 2],
    /// How many dowels and screws.
    counts: (u32, u32),
    lookup: &'l Lookup<'p>,
}

impl Joints<'_, '_, '_> {
    /// Joins both ends of `part` to the sides with `fastening`.
    fn join(&mut self, part: &mut Part, fastening: Fastening) -> Result<(), String> {
        let Some(kinds) = self.row(part.width, fastening) else {
            self.lookup.note(&format!(
                "a {} is too narrow for its joinery; it is not bored",
                part.role
            ));
            return Ok(());
        };
        // The row runs from the front: the end of the width nearer the
        // front of the cabinet.
        let depth_at = |across: Length| part.placement.point(Length::ZERO, across, Length::ZERO)[2];
        let front_first = depth_at(part.width) > depth_at(Length::ZERO);
        let middle = part.thickness.div_round(2);
        for (side, edge) in [(0, Edge::X0), (1, Edge::X1)] {
            let end = if edge == Edge::X0 {
                Length::ZERO
            } else {
                part.length
            };
            for &(from_front, kind) in &kinds {
                let along = if front_first {
                    part.width - from_front
                } else {
                    from_front
                };
                let crossing = part.placement.point(end, along, middle);
                let [x, y, depth] = self.sides[side].placement.local(crossing);
                if depth != self.sides[side].thickness {
                    return Err(format!(
                        "the {} does not meet the side where its joint is",
                        part.role
                    ));
                }
                let spot = Spot {
                    side,
                    edge,
                    end,
                    along,
                    at: Point { x, y },
                };
                self.bore(part, spot, kind)?;
            }
        }
        Ok(())
    }

    /// Bores one fastener's holes at `spot` and sets it into `part`.
    fn bore(&mut self, part: &mut Part, spot: Spot, kind: Kind) -> Result<(), String> {
        let thickness = self.sides[spot.side].thickness;
        // From the side's face A, into the part along its length.
        let inward: i64 = if spot.edge == Edge::X0 { 1 } else { -1 };
        let middle = part.thickness.div_round(2);
        let point = |depth: Length| [spot.end + depth * inward, spot.along, middle];
        let (face_hole, edge_hole, fastener) = match kind {
            Kind::Dowel => {
                let dowel = self.dowel.expect("dowels were read");
                if dowel.diameter >= part.thickness {
                    return Err("the dowels are thicker than the parts they join".into());
                }
                let start = -dowel.rules.face_depth;
                self.counts.0 += 1;
                (
                    (Face::A, dowel.diameter, dowel.rules.face_depth, None),
                    (dowel.diameter, dowel.rules.edge_depth),
                    Fastener {
                        kind: FastenerKind::Dowel,
                        hardware: dowel.rules.hardware,
                        start: point(start),
                        end: point(start + dowel.length),
                        diameter: dowel.diameter,
                    },
                )
            }
            Kind::Screw => {
                let screw = self.screw.expect("screws were read");
                if screw.length <= thickness || screw.pilot >= part.thickness {
                    return Err("the confirmat screws do not fit the parts they join".into());
                }
                let countersink = screw.rules.countersink;
                self.counts.1 += 1;
                (
                    (Face::B, screw.shank, thickness, Some(countersink)),
                    (
                        screw.pilot,
                        screw.length - thickness + screw.rules.extra_depth,
                    ),
                    Fastener {
                        kind: FastenerKind::Confirmat { head: countersink },
                        hardware: screw.rules.hardware,
                        start: point(-thickness),
                        end: point(screw.length - thickness),
                        diameter: screw.shank,
                    },
                )
            }
        };
        let (face, diameter, depth, countersink) = face_hole;
        self.side_operations[spot.side].push(Operation::Hole {
            face,
            at: spot.at,
            diameter,
            depth,
            countersink,
        });
        part.operations.push(Operation::EdgeHole {
            edge: spot.edge,
            along: spot.along,
            diameter: edge_hole.0,
            depth: edge_hole.1,
        });
        part.fasteners.push(fastener);
        Ok(())
    }

    /// The row of a joint `length` long, from the front, and what goes at
    /// each place.
    fn row(&self, length: Length, fastening: Fastening) -> Option<Vec<(Length, Kind)>> {
        let positions = |end, spacing, minimum, diameter: Length| {
            row(length, end, spacing, (minimum, diameter * 2))
        };
        Some(match fastening {
            Fastening::None => Vec::new(),
            Fastening::Dowels => {
                let size = self.dowel?;
                let r = size.rules;
                positions(r.end_distance, r.spacing, r.minimum, size.diameter)?
                    .into_iter()
                    .map(|v| (v, Kind::Dowel))
                    .collect()
            }
            Fastening::Confirmat => {
                let size = self.screw?;
                let r = size.rules;
                positions(r.end_distance, r.spacing, 2, size.shank)?
                    .into_iter()
                    .map(|v| (v, Kind::Screw))
                    .collect()
            }
            Fastening::Both => {
                let size = self.dowel?;
                let r = size.rules;
                let widest = size.diameter.max(self.screw?.shank);
                positions(r.end_distance, r.spacing, r.minimum.max(2), widest)?
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| (v, if i % 2 == 0 { Kind::Screw } else { Kind::Dowel }))
                    .collect()
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mm(value: i64) -> Length {
        Length::from_mm(value)
    }

    #[test]
    fn a_row_is_centered_at_its_spacing() {
        // 560 less 32 at each end leaves 496: six dowels 96 apart, 480 in
        // all, 40 from each end.
        let dowels = (2, mm(16));
        assert_eq!(
            row(mm(560), mm(32), mm(96), dowels),
            Some(vec![mm(40), mm(136), mm(232), mm(328), mm(424), mm(520)])
        );
        // A stretcher 100 wide: too short for the spacing, so the minimum.
        assert_eq!(
            row(mm(100), mm(32), mm(96), dowels),
            Some(vec![mm(32), mm(68)])
        );
        // Too short for two 16 apart: one in the middle.
        assert_eq!(row(mm(76), mm(32), mm(96), dowels), Some(vec![mm(38)]));
        assert_eq!(row(mm(64), mm(32), mm(96), dowels), Some(vec![mm(32)]));
        // Too short for any.
        assert_eq!(row(mm(60), mm(32), mm(96), dowels), None);
    }
}
