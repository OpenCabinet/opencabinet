// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The project types against the hand-written fixtures in `fixtures/`.

use std::path::PathBuf;

use opencabinet_model::validate::Severity;
use opencabinet_model::{Id, InsertKind, Length, Library, Project, Size, validate};
use serde::Serialize;
use serde::de::DeserializeOwned;

fn fixture(path: &str) -> String {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(path);
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

fn read<T: DeserializeOwned>(path: &str) -> T {
    serde_json::from_str(&fixture(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Pretty JSON with a final newline, as the fixtures are written.
fn written<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap() + "\n"
}

fn kitchen() -> (Project, Library) {
    (
        read("kitchen.ocab/project.json"),
        read("kitchen.ocab/library.json"),
    )
}

fn errors(project: &Project, library: &Library) -> Vec<String> {
    validate(project, library)
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.to_string())
        .collect()
}

#[test]
fn the_sample_kitchen_is_valid() {
    let (project, library) = kitchen();
    assert_eq!(validate(&project, &library), []);
    assert_eq!(project.products.len(), 7);
    assert_eq!(project.products[0].width, Length::from_inches(30));
}

#[test]
fn writing_reproduces_the_fixtures_exactly() {
    for name in ["kitchen", "future-minor"] {
        let project_json = fixture(&format!("{name}.ocab/project.json"));
        let library_json = fixture(&format!("{name}.ocab/library.json"));
        let project: Project = serde_json::from_str(&project_json).unwrap();
        let library: Library = serde_json::from_str(&library_json).unwrap();
        assert_eq!(written(&project), project_json, "{name} project");
        assert_eq!(written(&library), library_json, "{name} library");
    }
}

#[test]
fn data_from_a_newer_version_is_kept_and_flagged() {
    let project: Project = read("future-minor.ocab/project.json");
    let library: Library = read("future-minor.ocab/library.json");
    assert_eq!(project.unknown["jobPhase"], "install");
    assert!(library.unknown.contains_key("countertops"));
    let insert = project.products[0]
        .front
        .as_ref()
        .and_then(|f| f.insert.as_ref())
        .unwrap();
    assert_eq!(insert.kind, InsertKind::Other("tambour".into()));
    assert_eq!(
        errors(&project, &library),
        ["error: product T24: unknown insert `tambour`; open with a newer OpenCabinet"]
    );
}

#[test]
fn validation_reports_broken_references_and_shapes() {
    let (mut project, library) = kitchen();
    let room = &mut project.rooms[0];
    room.walls[0].openings[0].width = Length::from_inches(200);
    room.placements[6].wall = room.placements[0].wall;
    room.placements[1].id = room.placements[0].id;
    project.products[0].quantity = Some(2);
    project.products[1].standard = Id::from_uuid(uuid::Uuid::nil());
    project.products[2].front.as_mut().unwrap().size = Some(Size::Share(1));

    assert_eq!(
        errors(&project, &library),
        [
            "error: wall North: opening 0199f8a0-0000-7000-8000-000000000221 is outside the wall",
            "error: placement 0199f8a0-0000-7000-8000-000000000231: \
             0199f8a0-0000-7000-8000-000000000231 is used more than once",
            "error: placement 0199f8a0-0000-7000-8000-000000000237: \
             use either wall, offset and elevation, or position and rotation",
            "error: product B30: a placed product counts its placements; remove the quantity",
            "error: product SB36: standard is not in the library",
            "error: product BC42: the root of the front has no size",
        ]
    );
}
