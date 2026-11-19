// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The kitchen users get with a release is the first thing they open, so
//! nothing in it may report a problem. Unlike `kitchen.ocab`, which shows what
//! cannot be evaluated yet on purpose.

use std::path::PathBuf;

use opencabinet_eval::{elevation, evaluate, placement_problems};
use opencabinet_model::{Length, Library, Project};
use serde::de::DeserializeOwned;

fn read<T: DeserializeOwned>(path: &str) -> T {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/sample-kitchen.ocab")
        .join(path);
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

fn kitchen() -> (Project, Library) {
    (read("project.json"), read("library.json"))
}

#[test]
fn nothing_reports_a_problem() {
    let (project, library) = kitchen();
    let evaluation = evaluate(&project, &library);
    assert_eq!(evaluation.problems, []);
    assert_eq!(placement_problems(&project), Vec::<String>::new());
}

#[test]
fn it_has_the_usual_kinds_of_cabinet_on_two_walls() {
    let (project, library) = kitchen();
    let room = &project.rooms[0];
    assert_eq!(room.walls.len(), 2);
    assert_eq!(room.placements.len(), 12);
    let labels: Vec<&str> = project.products.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["B30", "B36", "DB18", "W30", "W18", "T30"]);
    for wall in &room.walls {
        assert!(elevation(&project, &library, wall.id).is_some());
    }
}

/// Worked out by hand: a 30" × 34-1/2" × 24" base on a 4" toe kick has sides of
/// 876.3 - 101.6 by 609.6 mm, and a pair of doors with a 1/8" gap at the sides
/// and between them is (762 - 2 × 3.175) / 2 mm wide each.
#[test]
fn the_base_cabinets_sizes_are_right() {
    let (project, library) = kitchen();
    let evaluation = evaluate(&project, &library);
    let b30 = project.products[0].id;
    let of_role = |role: &str| -> Vec<(Length, Length)> {
        evaluation
            .parts
            .iter()
            .filter(|p| p.product == b30 && p.role == role)
            .map(|p| (p.length, p.width))
            .collect()
    };
    let nm = |mm_tenths: i64| Length::from_nm(mm_tenths * 100_000);
    let sides = of_role("side");
    assert_eq!(sides.len(), 2);
    assert!(sides.contains(&(nm(7747), nm(6096))), "{sides:?}");
    let doors = of_role("door");
    assert_eq!(doors.len(), 2);
    let door_width = Length::from_nm(377_825_000);
    assert!(doors.iter().all(|d| d.1 == door_width), "{doors:?}");
}
