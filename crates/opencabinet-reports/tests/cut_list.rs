// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The names the cut list gives parts.

use std::path::PathBuf;

use opencabinet_model::{Id, Length, Library, Project, Units};
use opencabinet_reports::cut_list;

fn id(last: &str) -> Id {
    format!("0199f8a0-0000-7000-8000-000000000{last}")
        .parse()
        .unwrap()
}

#[test]
fn a_blind_corners_own_parts_have_names() {
    // The sample's BC42, given a back and the face frame standard.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kitchen.ocab");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let mut project: Project = serde_json::from_str(&read("project.json")).unwrap();
    let library: Library = serde_json::from_str(&read("library.json")).unwrap();
    let bc42 = project
        .products
        .iter_mut()
        .find(|p| p.label == "BC42")
        .unwrap();
    bc42.standard = id("132");
    bc42.materials.insert("back".into(), id("103"));
    bc42.materials.insert("faceFrame".into(), id("104"));

    let list = cut_list(
        &project,
        &library,
        Units::Imperial,
        Length::from_nm(793_750),
    );
    let names: Vec<&str> = list
        .rows
        .iter()
        .filter(|r| r.product == "BC42")
        .map(|r| r.part.as_str())
        .collect();
    assert!(names.contains(&"Face frame, blind mullion"), "{names:?}");
    assert!(names.contains(&"Blind panel"), "{names:?}");
    assert!(names.contains(&"Door 1"), "{names:?}");
}
