// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sample kitchen's parts as DXF, read back. CI also audits the files
//! with ezdxf (see `examples/parts_dxf.rs`).

use std::path::PathBuf;

use dxf::Drawing;
use dxf::entities::EntityType;
use dxf::enums::{AcadVersion, Units};
use opencabinet_model::{FrontStyle, Library, Project};
use opencabinet_reports::{ALL_PARTS, DxfFile, parts_dxf};

/// The sample kitchen, with the B30's doors made slabs and two shelves
/// behind them, as in the evaluator's machining tests.
fn kitchen() -> (Project, Library) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kitchen.ocab");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let mut project: Project = serde_json::from_str(&read("project.json")).unwrap();
    let front = project.products[0].front.as_mut().unwrap();
    let doors = front.children[1].insert.as_mut().unwrap();
    doors.style = Some(FrontStyle::Slab);
    doors.shelves = Some(2);
    (
        project,
        serde_json::from_str(&read("library.json")).unwrap(),
    )
}

fn file<'f>(files: &'f [DxfFile], name: &str) -> &'f DxfFile {
    files
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

fn load(file: &DxfFile) -> Drawing {
    Drawing::load(&mut file.contents.as_slice()).unwrap()
}

/// Rounds to micrometers, so sizes compare exactly.
#[allow(
    clippy::cast_possible_truncation,
    reason = "micrometers of a cabinet part"
)]
fn um(value: f64) -> i64 {
    (value * 1000.0).round() as i64
}

/// The drawing's entities on `layer`: polylines as their corners, circles
/// as their center and diameter, in micrometers.
fn on_layer(drawing: &Drawing, layer: &str) -> Vec<Vec<(i64, i64)>> {
    drawing
        .entities()
        .filter(|e| e.common.layer == layer)
        .map(|e| match &e.specific {
            EntityType::LwPolyline(p) => p.vertices.iter().map(|v| (um(v.x), um(v.y))).collect(),
            EntityType::Circle(c) => {
                vec![(um(c.center.x), um(c.center.y)), (um(c.radius * 2.0), 0)]
            }
            other => panic!("unexpected {other:?} on {layer}"),
        })
        .collect()
}

#[test]
fn every_part_has_a_file_and_one_has_them_all() {
    let (project, library) = kitchen();
    let files = parts_dxf(&project, &library).unwrap();
    let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"B30 - Left side.dxf"));
    assert!(names.contains(&"W30 - Face frame, mullion 1.dxf"));
    // The W30's two doors are both "Door 1" of their opening: the second
    // is numbered instead of overwriting the first.
    assert!(names.contains(&"W30 - Door 1, left stile.dxf"));
    assert!(names.contains(&"W30 - Door 1, left stile (2).dxf"));
    assert_eq!(names.last(), Some(&ALL_PARTS));
    // Every part of the overview is one outline.
    let all = load(file(&files, ALL_PARTS));
    assert_eq!(on_layer(&all, "OUTLINE").len(), files.len() - 1);
}

#[test]
fn files_are_dxf_2000_in_millimeters_and_identical_every_time() {
    let (project, library) = kitchen();
    let files = parts_dxf(&project, &library).unwrap();
    assert_eq!(files, parts_dxf(&project, &library).unwrap());
    let drawing = load(file(&files, "B30 - Left side.dxf"));
    assert_eq!(drawing.header.version, AcadVersion::R2000);
    assert_eq!(drawing.header.default_drawing_units, Units::Millimeters);
    // Every table header has its handle and owner (ADR-0015).
    let text = String::from_utf8_lossy(&file(&files, "B30 - Left side.dxf").contents);
    let tables = text.matches("\r\nTABLE\r\n").count();
    assert!(tables > 0);
    let with_handles = text
        .split("\r\nTABLE\r\n  2\r\n")
        .skip(1)
        .filter(|rest| rest.lines().nth(1) == Some("  5") && rest.lines().nth(3) == Some("330"))
        .count();
    assert_eq!(with_handles, tables);
}

#[test]
fn a_side_is_drawn_with_its_groove_and_holes() {
    // 774.7 × 609.6; the sample chose no band for the case, so nothing
    // shifts.
    let (project, library) = kitchen();
    let files = parts_dxf(&project, &library).unwrap();
    let side = load(file(&files, "B30 - Left side.dxf"));
    assert_eq!(
        on_layer(&side, "OUTLINE"),
        [vec![(0, 0), (774_700, 0), (774_700, 609_600), (0, 609_600)]]
    );
    // The back's groove: 12 wide, 6.35 to 18.35 from the rear edge, the
    // full length.
    assert_eq!(
        on_layer(&side, "GROOVE_A_W12_Z9.525"),
        [vec![
            (0, 6350),
            (774_700, 6350),
            (774_700, 18_350),
            (0, 18_350)
        ]]
    );
    // 4 hinge plate holes and two columns of 16 shelf pin holes.
    let holes = on_layer(&side, "HOLE_A_D5_Z12");
    assert_eq!(holes.len(), 4 + 2 * 16);
    // 61.7875, to the nearest micrometer.
    assert_eq!(holes[0], [(61_788, 572_600), (5000, 0)]);
    assert_eq!(holes[4], [(82_000, 572_600), (5000, 0)]);
    assert_eq!(holes[20], [(82_000, 55_350), (5000, 0)]);
}

#[test]
fn holes_shift_by_the_bands_on_the_left_and_bottom_edges() {
    // Door 1 is banded 0.5 mm on all four edges: cut 618.125 × 376.825,
    // and its cups, 76.2 and 542.925 up and 21.5 in on the banded door,
    // are 0.5 less on the cut one.
    let (project, library) = kitchen();
    let files = parts_dxf(&project, &library).unwrap();
    let door = load(file(&files, "B30 - Door 1.dxf"));
    assert_eq!(
        on_layer(&door, "OUTLINE"),
        [vec![(0, 0), (618_125, 0), (618_125, 376_825), (0, 376_825)]]
    );
    assert_eq!(
        on_layer(&door, "HOLE_A_D35_Z13"),
        [
            vec![(75_700, 21_000), (35_000, 0)],
            vec![(542_425, 21_000), (35_000, 0)],
        ]
    );
}

#[test]
fn the_label_names_the_part_and_its_cut() {
    let (project, library) = kitchen();
    let files = parts_dxf(&project, &library).unwrap();
    let door = load(file(&files, "B30 - Door 1.dxf"));
    let texts: Vec<String> = door
        .entities()
        .filter_map(|e| match &e.specific {
            EntityType::Text(t) if e.common.layer == "LABEL" => Some(t.value.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        [
            "Cut 618.125 x 376.825 mm, edge band: left, right, bottom, top",
            "Maple plywood 3/4\", 18 mm, qty 2",
            "B30 - Door 1",
            "grain",
        ]
    );
}
