// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sample kitchen's walls seen from the room, and the checks of where
//! its products are placed.
//!
//! Its North wall is 144" (3657.6 mm) long and 96" high, with a 36" window
//! 60" from its start, its sill at 42". Along it stand two B30s, the SB36,
//! a 3" filler and the BC42; the W30 hangs over the first B30 at 54". The
//! East wall turns at North's end.

use std::path::PathBuf;

use opencabinet_eval::front::Rect;
use opencabinet_eval::{Dimension, FrontKind, elevation, placement_problems};
use opencabinet_model::{Id, Length, Library, Placement, Project, Side};
use serde::de::DeserializeOwned;

fn read<T: DeserializeOwned>(path: &str) -> T {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/kitchen.ocab")
        .join(path);
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

fn kitchen() -> (Project, Library) {
    (read("project.json"), read("library.json"))
}

/// Millimeters with only the decimals they need.
fn mm(length: Length) -> String {
    let nm = length.nm();
    let text = format!("{}.{:06}", nm / 1_000_000, (nm % 1_000_000).abs());
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn rect(r: Rect) -> String {
    format!("{} {} {} × {}", mm(r.x), mm(r.y), mm(r.width), mm(r.height))
}

fn wall(project: &Project, name: &str) -> Id {
    project.rooms[0]
        .walls
        .iter()
        .find(|w| w.name == name)
        .unwrap()
        .id
}

#[test]
fn a_wall_shows_its_openings_and_the_products_on_it() {
    let (project, library) = kitchen();
    let north = elevation(&project, &library, wall(&project, "North")).unwrap();
    assert_eq!(
        (mm(north.length), mm(north.height)),
        ("3657.6".into(), "2438.4".into())
    );
    let openings: Vec<String> = north.openings.iter().map(|o| rect(o.rect)).collect();
    assert_eq!(openings, ["1524 1066.8 914.4 × 1219.2"]);
    let cabinets: Vec<String> = north
        .cabinets
        .iter()
        .map(|c| format!("{} at {}", c.label, rect(c.rect)))
        .collect();
    assert_eq!(
        cabinets,
        [
            "B30 at 0 0 762 × 876.3",
            "B30 at 762 0 762 × 876.3",
            "SB36 at 1524 0 914.4 × 876.3",
            "F3 at 2438.4 0 76.2 × 876.3",
            "BC42 at 2514.6 0 1066.8 × 876.3",
            "W30 at 0 1371.6 762 × 762",
        ]
    );
    // Products of types that cannot be evaluated yet show their outline
    // only.
    let views: Vec<bool> = north.cabinets.iter().map(|c| c.view.is_some()).collect();
    assert_eq!(views, [true, true, false, false, true, true]);
}

#[test]
fn a_front_view_lays_out_fronts_as_the_parts_do() {
    // The B30: toe kick 4", then the box face 774.7 high; the 6" drawer
    // opening on top, the doors below, each front 1/16" in from its
    // opening's edges. The doors of a pair hinge on their outer edges.
    let (project, library) = kitchen();
    let north = elevation(&project, &library, wall(&project, "North")).unwrap();
    let view = north.cabinets[0].view.as_ref().unwrap();
    assert_eq!(rect(view.outline), "0 0 762 × 876.3");
    assert_eq!(rect(view.toe_kick.unwrap()), "0 0 762 × 101.6");
    assert_eq!(view.frame, Vec::<Rect>::new());
    let fronts: Vec<String> = view
        .fronts
        .iter()
        .map(|f| format!("{:?} {}", f.kind, rect(f.rect)))
        .collect();
    assert_eq!(
        fronts,
        [
            "Drawer 1.5875 725.4875 758.825 × 149.225",
            "Door { hinge: Some(Left) } 1.5875 103.1875 377.825 × 619.125",
            "Door { hinge: Some(Right) } 382.5875 103.1875 377.825 × 619.125",
        ]
    );

    // The W30 has a face frame: two stiles, two rails and a mullion, and a
    // single door in each opening, hinged on the side its front says.
    let w30 = north.cabinets[5].view.as_ref().unwrap();
    assert_eq!(w30.toe_kick, None);
    let frame: Vec<String> = w30.frame.iter().map(|r| rect(*r)).collect();
    assert_eq!(
        frame,
        [
            "0 0 38.1 × 762",
            "723.9 0 38.1 × 762",
            "38.1 723.9 685.8 × 38.1",
            "38.1 0 685.8 × 38.1",
            "361.95 38.1 38.1 × 685.8",
        ]
    );
    let hinges: Vec<FrontKind> = w30.fronts.iter().map(|f| f.kind).collect();
    assert_eq!(
        hinges,
        [
            FrontKind::Door {
                hinge: Some(Side::Left)
            },
            FrontKind::Door {
                hinge: Some(Side::Right)
            },
        ]
    );
}

#[test]
fn dimensions_chain_the_widths_and_the_heights() {
    let (project, library) = kitchen();
    let north = elevation(&project, &library, wall(&project, "North")).unwrap();
    let lines: Vec<String> = north
        .dimensions()
        .into_iter()
        .map(|d| match d {
            Dimension::Horizontal { from, to, at } => {
                format!("{}–{} at {}", mm(from), mm(to), mm(at))
            }
            Dimension::Vertical { from, to, at } => {
                format!("{}–{} up at {}", mm(from), mm(to), mm(at))
            }
        })
        .collect();
    assert_eq!(
        lines,
        [
            // The standing products and the gap they leave at the end.
            "0–762 at -100",
            "762–1524 at -100",
            "1524–2438.4 at -100",
            "2438.4–2514.6 at -100",
            "2514.6–3581.4 at -100",
            "3581.4–3657.6 at -100",
            // The hung W30, above the wall.
            "0–762 at 2538.4",
            "762–3657.6 at 2538.4",
            "0–3657.6 at -200",
            // Floor, counter height, the W30's bottom and top, ceiling.
            "0–876.3 up at 3757.6",
            "876.3–1371.6 up at 3757.6",
            "1371.6–2133.6 up at 3757.6",
            "2133.6–2438.4 up at 3757.6",
        ]
    );
}

#[test]
fn the_sample_kitchen_shows_its_blind_panel() {
    // The BC42 stops 3" short of the corner, and nothing stands on the
    // East wall in front of its blind 24".
    let (project, _) = kitchen();
    assert_eq!(placement_problems(&project), [BLIND]);
}

/// The warning for the sample's BC42.
const BLIND: &str = "BC42's blind panel shows: no cabinet stands in front of all of it";

/// The kitchen with the placement of `label` on its wall moved to
/// `offset` and `elevation` inches.
fn moved(label: &str, offset: i64, elevation: i64) -> Project {
    let (mut project, _) = kitchen();
    let id = project
        .products
        .iter()
        .find(|p| p.label == label)
        .unwrap()
        .id;
    let placement = project.rooms[0]
        .placements
        .iter_mut()
        .find(|p| p.product == id)
        .unwrap();
    placement.offset = Some(Length::from_inches(offset));
    placement.elevation = Some(Length::from_inches(elevation));
    project
}

#[test]
fn products_past_the_wall_over_openings_or_into_others_are_reported() {
    // The W30 over the window, 60" along at 54".
    assert_eq!(
        placement_problems(&moved("W30", 60, 54)),
        ["W30 covers the window on wall North", BLIND]
    );
    // Hung at 72", its top is 102", above the 96" wall.
    assert_eq!(
        placement_problems(&moved("W30", 0, 72)),
        ["W30 is taller than wall North", BLIND]
    );
    // The first B30 at 126" runs past the wall's end and into the BC42.
    assert_eq!(
        placement_problems(&moved("B30", 126, 0)),
        [
            "B30 runs past the end of wall North",
            BLIND,
            "B30 and BC42 overlap",
        ]
    );
}

#[test]
fn products_collide_around_a_corner() {
    // A B30 at the start of the East wall stands in the BC42's corner:
    // both are 24" deep.
    let (mut project, _) = kitchen();
    let room = &mut project.rooms[0];
    let mut corner = room.placements[0].clone();
    corner.id = Id::new();
    corner.wall = Some(room.walls[1].id);
    corner.offset = Some(Length::ZERO);
    room.placements.push(corner.clone());
    assert_eq!(
        placement_problems(&project),
        [BLIND, "BC42 and B30 overlap"]
    );

    // Moved 24" along East, it clears the BC42's depth; but the BC42 stops
    // 3" short of the corner, so the B30 hides only 21" of its blind 24".
    let room = &mut project.rooms[0];
    let last: &mut Placement = room.placements.last_mut().unwrap();
    last.offset = Some(Length::from_inches(24));
    assert_eq!(placement_problems(&project), [BLIND]);
}

/// The sample kitchen with the BC42 moved into the corner, at 102" on the
/// North wall, and a B30 on the East wall in front of it, 24" along.
fn blind_corner_in_the_corner() -> Project {
    let mut project = moved("BC42", 102, 0);
    let room = &mut project.rooms[0];
    let mut adjoining = room.placements[0].clone();
    adjoining.id = Id::new();
    adjoining.wall = Some(room.walls[1].id);
    adjoining.offset = Some(Length::from_inches(24));
    room.placements.push(adjoining);
    project
}

#[test]
fn a_cabinet_on_the_adjoining_wall_hides_the_blind_panel() {
    // The B30's side runs from 120" to 144" along the North wall, the
    // BC42's blind 24" exactly.
    assert_eq!(
        placement_problems(&blind_corner_in_the_corner()),
        Vec::<String>::new()
    );
}

#[test]
fn a_cabinet_in_front_of_a_blind_corners_doors_is_reported() {
    // With a blind 20", the doors reach to 124" and the B30 stands in
    // front of their last 4".
    let mut project = blind_corner_in_the_corner();
    let bc42 = project
        .products
        .iter_mut()
        .find(|p| p.label == "BC42")
        .unwrap();
    bc42.parameters
        .insert("blindWidth".into(), Length::from_inches(20).nm().into());
    assert_eq!(
        placement_problems(&project),
        ["B30 stands in front of BC42's doors"]
    );
}

#[test]
fn with_several_rooms_each_problem_says_which() {
    let mut project = moved("W30", 60, 54);
    let mut laundry = project.rooms[0].clone();
    laundry.id = Id::new();
    laundry.name = "Laundry".into();
    laundry.placements.clear();
    for wall in &mut laundry.walls {
        wall.id = Id::new();
    }
    project.rooms.push(laundry);
    assert_eq!(
        placement_problems(&project),
        [
            "Kitchen: W30 covers the window on wall North".to_owned(),
            format!("Kitchen: {BLIND}"),
        ]
    );
    let (_, library) = kitchen();
    let laundry = &project.rooms[1];
    let north = elevation(&project, &library, laundry.walls[0].id).unwrap();
    assert_eq!(
        (north.room.as_str(), north.name.as_str()),
        ("Laundry", "North")
    );
}
