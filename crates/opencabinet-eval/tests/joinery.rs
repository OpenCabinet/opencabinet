// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The case's joinery on the sample kitchen: every dowel and screw meets a
//! hole in both parts it joins, no hole runs into another or into a
//! groove, and confirmat screws are bored as the rules say.

use std::path::PathBuf;

use opencabinet_eval::{Edge, Face, FastenerKind, HardwareKind, Operation, Part, Point, evaluate};
use opencabinet_model::{FrontStyle, Id, Length, Library, Project};
use serde::de::DeserializeOwned;

fn read<T: DeserializeOwned>(path: &str) -> T {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/kitchen.ocab")
        .join(path);
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

/// The sample kitchen, the B30's doors made slabs with two shelves behind
/// them so its sides are line bored too.
fn kitchen() -> (Project, Library) {
    let mut project: Project = read("project.json");
    let front = project.products[0].front.as_mut().unwrap();
    let doors = front.children[1].insert.as_mut().unwrap();
    doors.style = Some(FrontStyle::Slab);
    doors.shelves = Some(2);
    (project, read("library.json"))
}

/// Joins every case of the kitchen with `fastening`.
fn joined_with(library: &mut Library, fastening: &str) {
    for standard in &mut library.standards {
        let joinery = standard.rules.get_mut("joinery").unwrap();
        for joint in ["bottom", "top", "stretchers"] {
            joinery[joint] = fastening.into();
        }
    }
}

fn mm(value: i64) -> Length {
    Length::from_mm(value)
}

fn um(value: i64) -> Length {
    Length::from_nm(value * 1_000)
}

/// The parts of the product labeled `label`.
fn parts_of<'e>(project: &Project, parts: &'e [Part], label: &str) -> Vec<&'e Part> {
    let id = project
        .products
        .iter()
        .find(|p| p.label == label)
        .unwrap()
        .id;
    parts.iter().filter(|p| p.product == id).collect()
}

/// Checks that each fastener of `part` passes through a hole in the face
/// of a side of its product and sits in a hole in `part`'s edge, both deep
/// enough. Returns how many it checked.
fn check_fasteners(part: &Part, parts: &[&Part]) -> usize {
    for fastener in &part.fasteners {
        let [start, across, middle] = fastener.start;
        assert_eq!(
            fastener.end[1..],
            [across, middle],
            "{} runs along its length",
            part.key
        );
        // The end the fastener enters, and how far it reaches either way.
        let (edge, end) = if start < Length::ZERO {
            (Edge::X0, Length::ZERO)
        } else {
            (Edge::X1, part.length)
        };
        let into_side = (end - start).nm().abs();
        let into_part = (fastener.end[0] - end).nm().abs();
        let crossing = part.placement.point(end, across, middle);
        let side = parts
            .iter()
            .find(|s| {
                s.role == "side" && {
                    let [_, _, depth] = s.placement.local(crossing);
                    depth == s.thickness
                }
            })
            .unwrap_or_else(|| panic!("{}: no side where its fastener crosses", part.key));
        let [x, y, _] = side.placement.local(crossing);
        let at = Point { x, y };
        let found = side.operations.iter().any(|operation| match *operation {
            Operation::Hole {
                face: Face::A,
                at: hole,
                diameter,
                depth,
                countersink: None,
            } => {
                fastener.kind == FastenerKind::Dowel
                    && hole == at
                    && diameter == fastener.diameter
                    && depth.nm() >= into_side
            }
            Operation::Hole {
                face: Face::B,
                at: hole,
                diameter,
                depth,
                countersink: Some(_),
            } => {
                matches!(fastener.kind, FastenerKind::Confirmat { .. })
                    && hole == at
                    && diameter == fastener.diameter
                    && depth == side.thickness
            }
            _ => false,
        });
        assert!(found, "{}: no hole in {} at {at:?}", part.key, side.key);
        let pilot = part.operations.iter().any(|operation| {
            matches!(*operation, Operation::EdgeHole { edge: bored, along, depth, .. }
                if bored == edge && along == across && depth.nm() >= into_part)
        });
        assert!(pilot, "{}: no hole in its edge at {across}", part.key);
    }
    part.fasteners.len()
}

#[test]
fn every_fastener_meets_a_hole_in_both_parts() {
    for fastening in ["dowels", "confirmat", "both"] {
        let (project, mut library) = kitchen();
        joined_with(&mut library, fastening);
        let evaluation = evaluate(&project, &library);
        // The kitchen has products the evaluator cannot build yet; none of
        // its problems is about joinery.
        let joinery = ["joinery", "dowel", "confirmat", "joint"];
        assert!(
            !evaluation
                .problems
                .iter()
                .any(|p| joinery.iter().any(|j| p.message.contains(j))),
            "{fastening}: {:?}",
            evaluation.problems
        );
        let mut checked = 0;
        for product in &project.products {
            let parts: Vec<&Part> = evaluation
                .parts
                .iter()
                .filter(|p| p.product == product.id)
                .collect();
            for part in &parts {
                checked += check_fasteners(part, &parts);
            }
        }
        assert!(checked > 20, "{fastening}: {checked}");
    }
}

/// A round hole on a face: its center and radius, in nanometers.
type Circle = ((i64, i64), i64);

/// A groove on a face, from its lowest to its highest corner.
type Area = ((i64, i64), (i64, i64));

/// Whether a point `(dx, dy)` from a center is at least `r` from it.
fn clear((dx, dy): (i64, i64), r: i64) -> bool {
    i128::from(dx).pow(2) + i128::from(dy).pow(2) >= i128::from(r).pow(2)
}

/// The holes and grooves on face `face` of `part`, as circles and as
/// rectangles from their lowest to their highest corner.
fn marks(part: &Part, face: Face) -> (Vec<Circle>, Vec<Area>) {
    let (mut circles, mut grooves) = (Vec::new(), Vec::new());
    for operation in &part.operations {
        match *operation {
            Operation::Hole {
                face: f,
                at,
                diameter,
                depth,
                countersink,
            } if f == face || depth == part.thickness => {
                // A hole through the part shows on both faces; its
                // countersink only on its own.
                let d = if f == face {
                    countersink.unwrap_or(diameter)
                } else {
                    diameter
                };
                circles.push(((at.x.nm(), at.y.nm()), d.nm() / 2));
            }
            Operation::HolePattern {
                face: f,
                start,
                step,
                count,
                diameter,
                ..
            } if f == face => {
                for k in 0..i64::from(count) {
                    let center = (
                        start.x.nm() + step.x.nm() * k,
                        start.y.nm() + step.y.nm() * k,
                    );
                    circles.push((center, diameter.nm() / 2));
                }
            }
            Operation::Groove {
                face: f,
                start,
                end,
                width,
                ..
            } if f == face => {
                let half = width.nm() / 2;
                let (x0, x1) = (start.x.nm().min(end.x.nm()), start.x.nm().max(end.x.nm()));
                let (y0, y1) = (start.y.nm().min(end.y.nm()), start.y.nm().max(end.y.nm()));
                grooves.push(((x0 - half, y0 - half), (x1 + half, y1 + half)));
            }
            _ => {}
        }
    }
    (circles, grooves)
}

#[test]
fn no_hole_runs_into_another_or_into_a_groove() {
    // Only the joinery's holes are checked here: those the evaluation has
    // that it does not when nothing joins the case.
    let (project, mut library) = kitchen();
    joined_with(&mut library, "none");
    let bare = evaluate(&project, &library);
    for fastening in ["dowels", "confirmat", "both"] {
        let (project, mut library) = kitchen();
        joined_with(&mut library, fastening);
        let evaluation = evaluate(&project, &library);
        for part in &evaluation.parts {
            let before = bare
                .parts
                .iter()
                .find(|p| p.product == part.product && p.key == part.key)
                .unwrap();
            for face in [Face::A, Face::B] {
                let (circles, grooves) = marks(part, face);
                let (existing, _) = marks(before, face);
                for &a @ ((ax, ay), ar) in circles.iter().filter(|c| !existing.contains(c)) {
                    for &b @ ((bx, by), br) in &circles {
                        assert!(
                            a == b || clear((ax - bx, ay - by), ar + br),
                            "{fastening}: holes run together in {} at ({ax}, {ay})",
                            part.key
                        );
                    }
                    for &((x0, y0), (x1, y1)) in &grooves {
                        let near = (ax.clamp(x0, x1), ay.clamp(y0, y1));
                        assert!(
                            clear((ax - near.0, ay - near.1), ar),
                            "{fastening}: a hole runs into a groove in {} at ({ax}, {ay})",
                            part.key
                        );
                    }
                }
            }
            // Holes in an edge stay apart, and clear of a groove that
            // reaches that edge.
            let (_, grooves) = marks(part, Face::A);
            let edge_holes: Vec<(Edge, i64, i64)> = part
                .operations
                .iter()
                .filter_map(|operation| match *operation {
                    Operation::EdgeHole {
                        edge,
                        along,
                        diameter,
                        ..
                    } => Some((edge, along.nm(), diameter.nm() / 2)),
                    _ => None,
                })
                .collect();
            for (i, &(edge, along, r)) in edge_holes.iter().enumerate() {
                for &(other, at, s) in &edge_holes[i + 1..] {
                    assert!(
                        edge != other || (along - at).abs() >= r + s,
                        "{fastening}: edge holes run together in {}",
                        part.key
                    );
                }
                for &((x0, y0), (x1, y1)) in &grooves {
                    let reaches = match edge {
                        Edge::X0 => x0 <= 0,
                        Edge::X1 => x1 >= part.length.nm(),
                        Edge::Y0 | Edge::Y1 => false,
                    };
                    assert!(
                        !reaches || along + r <= y0 || along - r >= y1,
                        "{fastening}: an edge hole runs into a groove in {}",
                        part.key
                    );
                }
            }
        }
    }
}

#[test]
fn confirmat_screws_go_through_the_side_and_into_the_edge() {
    // 7 × 50 mm screws, 10 mm countersinks, 3 mm extra depth, at least
    // 50 from the ends and 256 apart.
    let (project, mut library) = kitchen();
    joined_with(&mut library, "confirmat");
    let evaluation = evaluate(&project, &library);
    let parts = parts_of(&project, &evaluation.parts, "B30");
    let part = |key: &str| *parts.iter().find(|p| p.key == key).unwrap();
    // The bottom is 609.6 deep: two screws 256 apart, 176.8 from each
    // edge. The side is 18 thick: the pilot goes 50 − 18 + 3 into the
    // bottom's edge.
    let pilots: Vec<(Edge, Length, Length, Length)> = part("case/bottom")
        .operations
        .iter()
        .filter_map(|operation| match *operation {
            Operation::EdgeHole {
                edge,
                along,
                diameter,
                depth,
            } => Some((edge, along, diameter, depth)),
            _ => None,
        })
        .collect();
    let at = [um(432_800), um(176_800)];
    let expected: Vec<_> = [Edge::X0, Edge::X1]
        .into_iter()
        .flat_map(|edge| at.map(|along| (edge, along, mm(5), mm(35))))
        .collect();
    assert_eq!(pilots, expected);
    let holes: Vec<&Operation> = part("case/side.left")
        .operations
        .iter()
        .filter(|o| matches!(o, Operation::Hole { face: Face::B, .. }))
        .collect();
    assert_eq!(
        holes[0],
        &Operation::Hole {
            face: Face::B,
            at: Point {
                x: mm(9),
                y: um(432_800)
            },
            diameter: mm(7),
            depth: mm(18),
            countersink: Some(mm(10)),
        }
    );
    // A stretcher, 101.6 wide, is too narrow for two screws 50 from its
    // edges and 14 apart: one, in the middle of each end.
    let stretcher = part("case/stretcher.front");
    assert_eq!(stretcher.fasteners.len(), 2);
    assert!(stretcher.fasteners.iter().all(|f| f.start[1] == um(50_800)));
    // Their heads sit in the sides' outside faces.
    assert!(
        stretcher
            .fasteners
            .iter()
            .all(|f| f.start[0] == -mm(18) || f.start[0] == stretcher.length + mm(18))
    );
}

#[test]
fn both_alternate_screws_and_dowels_from_the_front() {
    let (project, mut library) = kitchen();
    joined_with(&mut library, "both");
    let evaluation = evaluate(&project, &library);
    let parts = parts_of(&project, &evaluation.parts, "B30");
    let bottom = parts.iter().find(|p| p.key == "case/bottom").unwrap();
    // The dowels' row of six, from the front, at the left end.
    let left: Vec<(bool, Length)> = bottom
        .fasteners
        .iter()
        .filter(|f| f.start[0] < Length::ZERO)
        .map(|f| (f.kind == FastenerKind::Dowel, f.start[1]))
        .collect();
    let rows = [544_800, 448_800, 352_800, 256_800, 160_800, 64_800].map(um);
    let expected: Vec<(bool, Length)> = rows
        .into_iter()
        .enumerate()
        .map(|(i, v)| (i % 2 == 1, v))
        .collect();
    assert_eq!(left, expected);
}

#[test]
fn the_bill_of_materials_counts_the_dowels() {
    let id = |p: &Project, label: &str| p.products.iter().find(|q| q.label == label).unwrap().id;
    // The B30: six dowels in each end of the bottom and two in each end of
    // each stretcher, 20, for each of its placements.
    let (project, library) = kitchen();
    let placed = project
        .rooms
        .iter()
        .flat_map(|r| &r.placements)
        .filter(|p| p.product == id(&project, "B30"))
        .count();
    let evaluation = evaluate(&project, &library);
    let dowel: Id = "0199f8a0-0000-7000-8000-000000000123".parse().unwrap();
    let b30 = evaluation
        .hardware
        .iter()
        .find(|h| h.product == id(&project, "B30") && h.kind == HardwareKind::Fastener(dowel))
        .unwrap();
    assert_eq!(b30.quantity, 20 * u32::try_from(placed).unwrap());
}

#[test]
fn without_their_rules_joints_are_an_error() {
    let (project, mut library) = kitchen();
    joined_with(&mut library, "confirmat");
    for standard in &mut library.standards {
        let joinery = standard.rules.get_mut("joinery").unwrap();
        joinery.as_object_mut().unwrap().remove("confirmat");
    }
    let problems = evaluate(&project, &library).problems;
    assert!(problems.iter().any(|p| p.blocking
        && p.message == "the standard joins the case with confirmat but has no `confirmat` rules"));
}
