// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rooms seen from above: walls at their thickness, cut by doors, their
//! backs meeting at the corners, and every product where it stands.

use std::collections::BTreeMap;
use std::path::PathBuf;

use opencabinet_eval::plan;
use opencabinet_model::{
    Angle, Id, Length, Placement, Point, Point3, Project, Wall, WallOpening, WallOpeningKind,
};

fn kitchen() -> Project {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kitchen.ocab/project.json");
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

/// A point in whole inches, rounded, to compare without float noise.
fn inches((x, y): (f64, f64)) -> (i64, i64) {
    #[allow(clippy::cast_possible_truncation, reason = "a room")]
    let round = |v: f64| (v / 25.4).round() as i64;
    (round(x), round(y))
}

fn corners(points: &[(f64, f64)]) -> Vec<(i64, i64)> {
    points.iter().copied().map(inches).collect()
}

fn wall(name: &str, start: (i64, i64), end: (i64, i64)) -> Wall {
    let point = |(x, y): (i64, i64)| Point {
        x: Length::from_inches(x),
        y: Length::from_inches(y),
    };
    Wall {
        id: Id::new(),
        name: name.into(),
        start: point(start),
        end: point(end),
        height: Length::from_inches(96),
        thickness: Length::from_inches(4),
        openings: Vec::new(),
        unknown: BTreeMap::default(),
    }
}

fn on_wall(product: Id, wall: Id, offset: i64) -> Placement {
    Placement {
        id: Id::new(),
        product,
        wall: Some(wall),
        offset: Some(Length::from_inches(offset)),
        elevation: Some(Length::ZERO),
        position: None,
        rotation: None,
        unknown: BTreeMap::default(),
    }
}

#[test]
fn the_kitchen_seen_from_above() {
    let project = kitchen();
    let room = &project.rooms[0];
    let plan = plan(&project, room.id).unwrap();
    assert_eq!(plan.room, room.name);

    let north = &plan.walls[0];
    assert_eq!(north.name, "North");
    assert_eq!(corners(&north.front), [(0, 0), (144, 0)]);
    // Away from the room, which is below it.
    assert_eq!(
        inches((north.outward.0 * 25.4, north.outward.1 * 25.4)),
        (0, -1)
    );
    // The window does not cut the wall; its line runs along the middle.
    assert_eq!(north.pieces.len(), 1);
    assert_eq!(north.windows.len(), 1);
    let window = corners(&north.windows[0]);
    assert_eq!((window[0].0, window[1].0), (60, 96));
    assert!(north.windows[0][0].1 < 0.0 && north.windows[0][0].1 > -north.thickness);
    // Its back meets East's at the corner; its start is free.
    assert_eq!(
        corners(&north.pieces[0]),
        [(0, 0), (144, 0), (149, -5), (0, -5)]
    );

    let labels: Vec<(&str, bool)> = plan
        .cabinets
        .iter()
        .map(|c| (c.label.as_str(), c.above))
        .collect();
    assert!(labels.contains(&("B30", false)));
    assert!(labels.contains(&("W30", true)));
    let first = &plan.cabinets[0];
    assert_eq!(
        corners(&first.corners),
        [(0, 0), (30, 0), (30, 24), (0, 24)]
    );
    let wall_cabinet = plan.cabinets.iter().find(|c| c.label == "W30").unwrap();
    assert_eq!(
        corners(&wall_cabinet.corners),
        [(0, 0), (30, 0), (30, 12), (0, 12)]
    );
}

/// A 120" × 96" room walked with the room on the side of (-dy, dx): North
/// along the top, then East, South and West, a B30 10" from each wall's
/// start.
#[test]
fn walls_facing_every_way_hold_their_cabinets_inside() {
    let mut project = kitchen();
    let b30 = project
        .products
        .iter()
        .find(|p| p.label == "B30")
        .unwrap()
        .id;
    let room = &mut project.rooms[0];
    room.walls = vec![
        wall("North", (0, 0), (120, 0)),
        wall("East", (120, 0), (120, 96)),
        wall("South", (120, 96), (0, 96)),
        wall("West", (0, 96), (0, 0)),
    ];
    room.placements = room.walls.iter().map(|w| on_wall(b30, w.id, 10)).collect();
    let id = room.id;
    let plan = plan(&project, id).unwrap();

    let footprints: Vec<Vec<(i64, i64)>> =
        plan.cabinets.iter().map(|c| corners(&c.corners)).collect();
    assert_eq!(
        footprints,
        [
            vec![(10, 0), (40, 0), (40, 24), (10, 24)],
            vec![(120, 10), (120, 40), (96, 40), (96, 10)],
            vec![(110, 96), (80, 96), (80, 72), (110, 72)],
            vec![(0, 86), (0, 56), (24, 56), (24, 86)],
        ]
    );
    // Each wall's back is outside the room, and the backs meet at every
    // corner.
    let outlines: Vec<Vec<(i64, i64)>> = plan
        .walls
        .iter()
        .map(|w| {
            assert_eq!(w.pieces.len(), 1);
            corners(&w.pieces[0])
        })
        .collect();
    assert_eq!(
        outlines,
        [
            vec![(0, 0), (120, 0), (124, -4), (-4, -4)],
            vec![(120, 0), (120, 96), (124, 100), (124, -4)],
            vec![(120, 96), (0, 96), (-4, 100), (124, 100)],
            vec![(0, 96), (0, 0), (-4, -4), (-4, 100)],
        ]
    );
}

#[test]
fn a_door_cuts_the_wall_and_an_island_stands_turned() {
    let mut project = kitchen();
    let b30 = project
        .products
        .iter()
        .find(|p| p.label == "B30")
        .unwrap()
        .id;
    let room = &mut project.rooms[0];
    let mut north = wall("North", (0, 0), (120, 0));
    north.openings.push(WallOpening {
        id: Id::new(),
        kind: WallOpeningKind::Door,
        offset: Length::from_inches(40),
        width: Length::from_inches(36),
        height: Length::from_inches(80),
        sill_height: Length::ZERO,
        unknown: BTreeMap::default(),
    });
    room.walls = vec![north];
    room.placements = vec![Placement {
        id: Id::new(),
        product: b30,
        wall: None,
        offset: None,
        elevation: None,
        position: Some(Point3 {
            x: Length::from_inches(60),
            y: Length::from_inches(40),
            z: Length::ZERO,
        }),
        rotation: Some(Angle::from_degrees(90)),
        unknown: BTreeMap::default(),
    }];
    let id = room.id;
    let plan = plan(&project, id).unwrap();

    let pieces: Vec<Vec<(i64, i64)>> = plan.walls[0].pieces.iter().map(|p| corners(p)).collect();
    assert_eq!(
        pieces,
        [
            vec![(0, 0), (40, 0), (40, -4), (0, -4)],
            vec![(76, 0), (120, 0), (120, -4), (76, -4)],
        ]
    );
    assert_eq!(plan.walls[0].windows.len(), 0);
    // Turned a quarter, its width runs along y and its depth back along x.
    assert_eq!(
        corners(&plan.cabinets[0].corners),
        [(60, 40), (60, 70), (36, 70), (36, 40)]
    );
    assert!(!plan.cabinets[0].above);
}
