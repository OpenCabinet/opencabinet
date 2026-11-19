// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sample kitchen's bill of materials.

use std::path::PathBuf;

use opencabinet_model::{FrontStyle, Length, Library, Project, Units};
use opencabinet_reports::{BomRow, bill_of_materials};

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

fn inches() -> Length {
    Length::from_inch_fraction(1, 32).unwrap()
}

#[test]
fn the_sample_kitchen_matches_the_counts_worked_out_by_hand() {
    // Two B30s with slab doors and two shelves, the BC42 and the W30, the
    // products the kitchen can evaluate. Both standards add 15% waste and
    // 1" of edge band per edge. The BC42 has no edge bands chosen, so its
    // fronts are cut at their finished size.
    let (project, library) = kitchen();
    let bom = bill_of_materials(&project, &library, Units::Imperial, inches());
    let rows: Vec<[&str; 4]> = bom.rows.iter().map(BomRow::cells).collect();
    assert_eq!(
        rows,
        [
            // Per B30: sides 2 × 774.7 × 609.6, bottom 726 × 609.6,
            // stretchers 2 × 726 × 101.6, shelves 2 × 724.4125 × 584.2:
            // 2,381,010.6 mm², twice. The BC42: sides 2 × 774.7 × 609.6,
            // bottom 1030.8 × 609.6, stretchers 2 × 1030.8 × 101.6:
            // 1,782,348.5 mm². In all 6,544,369.7 mm², 70.4 ft²; with waste
            // 81.0 ft², or 2.5 sheets of 97" × 49".
            [
                "Sheets",
                "White melamine 3/4\"",
                "3 sheets",
                "70.4 ft² of parts, 81.0 ft² with waste, on 97\" × 49\" sheets"
            ],
            // The BC42's back adds (774.7 − 8.475) × (1030.8 + 19.05), or
            // 8.7 ft².
            [
                "Sheets",
                "Birch plywood 1/2\"",
                "2 sheets",
                "41.0 ft² of parts, 47.1 ft² with waste, on 97\" × 49\" sheets"
            ],
            // The BC42's door and blind panel add 771.525 × (454.025 +
            // 606.425), or 8.8 ft².
            [
                "Sheets",
                "Maple plywood 3/4\"",
                "2 sheets",
                "36.1 ft² of parts, 41.5 ft² with waste, on 97\" × 49\" sheets"
            ],
            // The W30's frame: 2 × 762 + 3 × 685.8 = 3581.4 mm, 11.8 ft;
            // with waste 4118.6 mm, 1.7 boards of 96".
            [
                "Solid wood",
                "Hard maple 3/4\" S4S",
                "2 boards",
                "11.8 ft of parts, 13.6 ft with waste, in 96\" boards"
            ],
            // The B30's drawer front and doors are banded all round: 12
            // edges, 5803.9 mm, plus 12", twice: 12,217.4 mm, 40.1 ft.
            [
                "Edge band",
                "Maple veneer 13/16\"",
                "40.1 ft",
                "24 edges, with the allowance for trimming"
            ],
            ["Hardware", "Drawer slides 22\"", "2 pairs", ""],
            // Two hinges on each of the B30s' four doors, the BC42's door
            // and the W30's two.
            [
                "Hardware",
                "110° clip hinge, full overlay",
                "14",
                "with mounting plates"
            ],
            [
                "Hardware",
                "Shelf pins",
                "16",
                "4 for each adjustable shelf"
            ],
        ]
    );
    assert_eq!(
        bom.notes,
        [
            "Sheet counts are estimates from the parts' area; pulls, legs, screws and glue are \
          not counted yet"
        ]
    );
}

#[test]
fn metric_projects_count_in_square_meters_and_meters() {
    let (project, library) = kitchen();
    let bom = bill_of_materials(&project, &library, Units::Metric, Length::from_nm(100_000));
    let quantities: Vec<&str> = bom.rows.iter().map(|r| r.quantity.as_str()).collect();
    assert_eq!(quantities[4], "12.22 m");
    assert!(
        bom.rows[0].detail.starts_with("6.54 m² of parts"),
        "{}",
        bom.rows[0].detail
    );
}

#[test]
fn a_standard_without_its_rules_adds_nothing_and_says_so() {
    let (project, mut library) = kitchen();
    for standard in &mut library.standards {
        standard.rules.remove("billOfMaterials");
    }
    let bom = bill_of_materials(&project, &library, Units::Imperial, inches());
    // 5803.9 mm, twice, without the allowance: 38.1 ft.
    assert_eq!(bom.rows[4].quantity, "38.1 ft");
    assert_eq!(
        bom.notes[0],
        "B30: its standard has no `billOfMaterials` rules, so no waste or edge band \
         allowance is added"
    );
}
