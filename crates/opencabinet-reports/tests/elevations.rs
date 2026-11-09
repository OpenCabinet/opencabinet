// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sample kitchen's elevations as DXF and PDF. CI also checks them with
//! independent readers (see `examples/elevations.rs`).

use std::path::PathBuf;

use dxf::Drawing;
use dxf::entities::EntityType;
use dxf::enums::{AcadVersion, Units as DxfUnits};
use opencabinet_model::{Length, Library, Project, Units};
use opencabinet_reports::elevations::elevations_dxf;
use opencabinet_reports::{Fonts, Paper, elevations_pdf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn kitchen() -> (Project, Library) {
    let dir = root().join("fixtures/kitchen.ocab");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    (
        serde_json::from_str(&read("project.json")).unwrap(),
        serde_json::from_str(&read("library.json")).unwrap(),
    )
}

fn show(length: Length) -> String {
    length
        .show(Units::Imperial, Length::from_inch_fraction(1, 32).unwrap())
        .text
}

fn fonts() -> Fonts {
    let dir = root().join("assets/fonts/noto-sans");
    let read = |name: &str| std::fs::read(dir.join(name)).unwrap();
    Fonts::new(read("NotoSans-Regular.ttf"), read("NotoSans-Bold.ttf")).unwrap()
}

/// Rounds to micrometers, so sizes compare exactly.
#[allow(clippy::cast_possible_truncation, reason = "micrometers of a room")]
fn um(value: f64) -> i64 {
    (value * 1000.0).round() as i64
}

#[test]
fn each_wall_has_a_dxf_with_its_layers() {
    let (project, library) = kitchen();
    let files = elevations_dxf(&project, &library, &show).unwrap();
    let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Elevation - Kitchen - North.dxf",
            "Elevation - Kitchen - East.dxf"
        ]
    );
    assert_eq!(files, elevations_dxf(&project, &library, &show).unwrap());

    let north = Drawing::load(&mut files[0].contents.as_slice()).unwrap();
    assert_eq!(north.header.version, AcadVersion::R2000);
    assert_eq!(north.header.default_drawing_units, DxfUnits::Millimeters);
    // Read back in name order, with the default layer "0".
    let layers: Vec<String> = north.layers().map(|l| l.name.clone()).collect();
    assert_eq!(
        layers,
        [
            "0",
            "CABINETS",
            "DIMENSIONS",
            "FRONTS",
            "HINGE_MARKS",
            "LABEL",
            "OPENINGS",
            "WALL"
        ]
    );
    // The wall at full size, 144" by 96".
    let wall: Vec<Vec<(i64, i64)>> = north
        .entities()
        .filter(|e| e.common.layer == "WALL")
        .filter_map(|e| match &e.specific {
            EntityType::LwPolyline(p) => {
                Some(p.vertices.iter().map(|v| (um(v.x), um(v.y))).collect())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        wall,
        [vec![
            (0, 0),
            (3_657_600, 0),
            (3_657_600, 2_438_400),
            (0, 2_438_400)
        ]]
    );
    let texts: Vec<String> = north
        .entities()
        .filter_map(|e| match &e.specific {
            EntityType::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(&texts[..6], ["B30", "B30", "SB36", "F3", "BC42", "W30"]);
    for value in ["30\"", "36\"", "144\"", "34-1/2\"", "19-1/2\""] {
        assert!(texts.contains(&value.to_owned()), "{value} in {texts:?}");
    }
}

#[test]
fn the_pdf_has_a_page_per_wall_and_is_the_same_every_time() {
    let (project, library) = kitchen();
    let fonts = fonts();
    let pdf = |paper| {
        elevations_pdf(
            &project,
            &library,
            Units::Imperial,
            &show,
            "Sample kitchen",
            "OpenCabinet test",
            paper,
            &fonts,
        )
        .unwrap()
    };
    let letter = pdf(Paper::Letter);
    assert!(letter.starts_with(b"%PDF-"));
    assert_eq!(letter, pdf(Paper::Letter));
    assert_ne!(letter, pdf(Paper::A4));
}

#[test]
fn a_project_without_walls_has_no_elevations() {
    let (mut project, library) = kitchen();
    project.rooms.clear();
    assert_eq!(
        elevations_dxf(&project, &library, &show),
        Err("The project has no walls to draw".into())
    );
    let fonts = fonts();
    let result = elevations_pdf(
        &project,
        &library,
        Units::Imperial,
        &show,
        "Sample kitchen",
        "OpenCabinet test",
        Paper::Letter,
        &fonts,
    );
    assert_eq!(result, Err("The project has no walls to draw".into()));
}
