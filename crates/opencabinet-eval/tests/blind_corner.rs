// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Blind corner cabinets, with sizes worked out by hand from the formulas
//! in the crate documentation.
//!
//! The sample kitchen's BC42 is 42" × 34-1/2" × 24", blind 24" on its
//! right, with one door hinged on the left. Its standard is frameless:
//! toe kick 4", so the box face is 1066.8 × 774.7 from 101.6 up; front gap
//! G = 1/8"; fronts of 18 mm maple banded with 0.5 mm. The sample leaves
//! out its back, which these tests add.

use std::path::PathBuf;

use opencabinet_eval::{FrontKind, Part, evaluate, front_view};
use opencabinet_model::{Id, Insert, InsertKind, Library, Product, Project, Side};
use serde::de::DeserializeOwned;
use serde_json::json;

fn read<T: DeserializeOwned>(path: &str) -> T {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/kitchen.ocab")
        .join(path);
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

fn id(last: &str) -> Id {
    format!("0199f8a0-0000-7000-8000-000000000{last}")
        .parse()
        .unwrap()
}

/// The sample kitchen with the BC42 given a back of 12 mm birch and its
/// fronts banded.
fn kitchen() -> (Project, Library) {
    let mut project: Project = read("project.json");
    let bc42 = bc42(&mut project);
    bc42.materials.insert("back".into(), id("103"));
    bc42.materials.insert("edgeBandFronts".into(), id("111"));
    (project, read("library.json"))
}

/// The same with the face frame standard: 3/4" hard maple stiles, rails
/// and mullions 1-1/2" wide; partial overlay fronts, 1/2" over the frame.
fn face_frame_kitchen() -> (Project, Library) {
    let (mut project, library) = kitchen();
    let bc42 = bc42(&mut project);
    bc42.standard = id("132");
    bc42.materials.insert("faceFrame".into(), id("104"));
    (project, library)
}

fn bc42(project: &mut Project) -> &mut Product {
    project
        .products
        .iter_mut()
        .find(|p| p.label == "BC42")
        .unwrap()
}

/// The BC42's door insert.
fn door(project: &mut Project) -> &mut Insert {
    bc42(project)
        .front
        .as_mut()
        .unwrap()
        .insert
        .as_mut()
        .unwrap()
}

/// Millimeters with only the decimals they need.
fn mm(nm: i64) -> String {
    let text = format!("{}.{:06}", nm / 1_000_000, (nm % 1_000_000).abs());
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// `key: length × width × thickness, cut length × cut width`, in mm, with
/// the front node ids shortened to their last three digits.
fn line(part: &Part) -> String {
    let key = part.key.replace("0199f8a0-0000-7000-8000-000000000", "");
    format!(
        "{key}: {} × {} × {}, cut {} × {}",
        mm(part.length.nm()),
        mm(part.width.nm()),
        mm(part.thickness.nm()),
        mm(part.cut_length.nm()),
        mm(part.cut_width.nm()),
    )
}

/// The BC42's parts whose key starts with one of `prefixes`.
fn parts(project: &Project, library: &Library, prefixes: &[&str]) -> Vec<String> {
    evaluate(project, library)
        .parts
        .iter()
        .filter(|p| p.product == id("303"))
        .map(line)
        .filter(|l| prefixes.iter().any(|k| l.starts_with(k)))
        .collect()
}

/// The BC42's door and its operations.
fn door_part(project: &Project, library: &Library) -> Part {
    evaluate(project, library)
        .parts
        .into_iter()
        .find(|p| p.product == id("303") && p.role == "door")
        .unwrap()
}

/// What the evaluation says about the BC42.
fn problems(project: &Project, library: &Library) -> Vec<String> {
    evaluate(project, library)
        .problems
        .into_iter()
        .filter(|p| p.product == "BC42")
        .map(|p| p.message)
        .collect()
}

#[test]
fn a_frameless_blind_corner_matches_the_sizes_worked_out_by_hand() {
    // The doors' zone is the face's first 42" − 24" = 457.2 mm, the blind
    // panel's the last 609.6 mm; each front is its zone less G, 774.7 −
    // 3.175 = 771.525 high, and is cut 1 mm smaller for its bands.
    let (project, library) = kitchen();
    assert_eq!(
        parts(&project, &library, &["front/"]),
        [
            "front/blindPanel: 771.525 × 606.425 × 18, cut 770.525 × 605.425",
            "front/421/door.1: 771.525 × 454.025 × 18, cut 770.525 × 453.025",
        ]
    );
    assert!(
        problems(&project, &library)
            .iter()
            .all(|p| !p.contains("cannot"))
    );
}

#[test]
fn a_face_frame_blind_corner_matches_the_sizes_worked_out_by_hand() {
    // The case is 24" − 19.05 deep behind the frame. The frame's opening
    // runs 38.1 to 1028.7 across and is 774.7 − 2 × 38.1 = 698.5 high. A
    // door overlaying the mullion by 12.7 must end at 609.6 from the right,
    // so the mullion's door side is 622.3 from the right and its other side
    // 584.2: it stands 444.5 to 482.6 from the left. The door's opening is
    // 38.1 to 444.5, 406.4 wide, and the door 406.4 + 25.4 = 431.8 by
    // 698.5 + 25.4 = 723.9; the blind opening is 482.6 to 1028.7, 546.1
    // wide, and its panel 571.5 by 723.9.
    let (project, library) = face_frame_kitchen();
    assert_eq!(
        parts(&project, &library, &["frame/mullion", "front/"]),
        [
            "frame/mullion.blind: 698.5 × 38.1 × 19.05, cut 698.5 × 38.1",
            "front/blindPanel: 723.9 × 571.5 × 18, cut 722.9 × 570.5",
            "front/421/door.1: 723.9 × 431.8 × 18, cut 722.9 × 430.8",
        ]
    );
    // The door ends at 25.4 + 431.8 = 457.2 from the left, 609.6 from the
    // right; the panel starts 12.7 later, over the mullion.
    let view = front_view(&project.products[2], &library).unwrap();
    let rects: Vec<String> = view
        .fronts
        .iter()
        .map(|f| format!("{} + {}", mm(f.rect.x.nm()), mm(f.rect.width.nm())))
        .collect();
    assert_eq!(rects, ["469.9 + 571.5", "25.4 + 431.8"]);
    assert_eq!(view.frame.len(), 5, "stiles, rails and the blind mullion");
}

#[test]
fn shelves_span_the_whole_inside_of_a_blind_corner() {
    // W − 2T − 1/16" = 1066.8 − 36 − 1.5875; D less 1/4" and 3/4".
    let (mut project, library) = kitchen();
    door(&mut project).shelves = Some(1);
    assert_eq!(
        parts(&project, &library, &["front/421/shelf"]),
        ["front/421/shelf.1: 1029.2125 × 584.2 × 18, cut 1029.2125 × 584.2"]
    );
}

#[test]
fn a_blind_corners_door_hinges_away_from_the_blind_part_by_default() {
    let (mut project, library) = kitchen();
    let explicit = door_part(&project, &library);
    door(&mut project).hinge_side = None;
    assert_eq!(door_part(&project, &library), explicit);
    // Cups 4 mm plus half of 35 mm from the left edge.
    let Some(opencabinet_eval::Operation::Hole { at, .. }) = explicit.operations.first() else {
        panic!("the door is bored for its hinges");
    };
    assert_eq!(mm(at.y.nm()), "21.5");

    let view = front_view(&project.products[2], &library).unwrap();
    let kinds: Vec<FrontKind> = view.fronts.iter().map(|f| f.kind).collect();
    assert_eq!(
        kinds,
        [
            FrontKind::BlindPanel,
            FrontKind::Door {
                hinge: Some(Side::Left)
            },
        ]
    );
}

#[test]
fn a_frameless_door_cannot_hinge_on_the_blind_side() {
    let (mut project, library) = kitchen();
    door(&mut project).hinge_side = Some(Side::Right);
    assert_eq!(
        problems(&project, &library),
        ["a door hinged away from the cabinet's sides has nothing to mount its hinges on"]
    );
    // A face frame door hinges on the mullion.
    let (mut project, library) = face_frame_kitchen();
    door(&mut project).hinge_side = Some(Side::Right);
    assert_eq!(problems(&project, &library), Vec::<String>::new());
}

/// A change that spoils the BC42.
type Change = fn(&mut Product);

#[test]
fn blind_corners_that_cannot_be_built_are_reported() {
    let cases: [(&str, Change); 5] = [
        (
            "a blind corner cabinet needs a `blindSide`, left or right",
            |p| {
                p.parameters.insert("blindSide".into(), json!("middle"));
            },
        ),
        (
            "a blind corner cabinet needs a `blindWidth` greater than zero",
            |p| {
                p.parameters.remove("blindWidth");
            },
        ),
        ("the blind width leaves no room for the doors", |p| {
            p.parameters
                .insert("blindWidth".into(), json!(1_066_800_000));
        }),
        (
            "drawers in a blind corner cabinet cannot be built yet: there is no side to \
             mount their slides on next to the blind part",
            |p| {
                let insert = p.front.as_mut().unwrap().insert.as_mut().unwrap();
                insert.kind = InsertKind::Drawer;
            },
        ),
        ("the blind width is too narrow for the face frame", |p| {
            p.standard = id("132");
            p.materials.insert("faceFrame".into(), id("104"));
            // Less than the stile and the mullion.
            p.parameters.insert("blindWidth".into(), json!(50_800_000));
        }),
    ];
    for (message, change) in cases {
        let (mut project, library) = kitchen();
        change(bc42(&mut project));
        assert_eq!(problems(&project, &library), [message]);
    }
}
