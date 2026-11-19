// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Machining on the sample kitchen's parts, with positions worked out by
//! hand.
//!
//! Both sample standards put the back in a 12 mm groove 1/4" from the rear
//! edge, 3/8" deep; bore shelf pin holes 5 mm across and 12 mm deep every
//! 32 mm in columns 37 mm from the front edge and 37 mm in front of the
//! back, 2" clear of an opening's ends; bore 35 mm hinge cups 13 mm deep,
//! 4 mm from the door's edge and 3" from its ends, two of them up to 40";
//! and groove drawer boxes for their 12 mm bottoms 1/2" from the bottom
//! edge, 1/4" deep.

use std::path::PathBuf;

use opencabinet_eval::{Face, Operation, Point, evaluate};
use opencabinet_model::{FrontStyle, Id, Library, Project};
use serde::de::DeserializeOwned;

fn read<T: DeserializeOwned>(path: &str) -> T {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/kitchen.ocab")
        .join(path);
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

/// The sample kitchen, with the B30's doors made slabs and two shelves
/// behind them.
fn kitchen() -> (Project, Library) {
    let mut project: Project = read("project.json");
    let front = project.products[0].front.as_mut().unwrap();
    let doors = front.children[1].insert.as_mut().unwrap();
    doors.style = Some(FrontStyle::Slab);
    doors.shelves = Some(2);
    (project, read("library.json"))
}

/// Millimeters with only the decimals they need.
fn mm(length: opencabinet_model::Length) -> String {
    let nm = length.nm();
    let text = format!("{}.{:06}", nm / 1_000_000, (nm % 1_000_000).abs());
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn point(p: Point) -> String {
    format!("({}, {})", mm(p.x), mm(p.y))
}

fn face(face: Face) -> &'static str {
    match face {
        Face::A => "A",
        Face::B => "B",
    }
}

/// An operation in millimeters.
fn describe(operation: &Operation) -> String {
    match operation {
        Operation::Hole {
            face: f,
            at,
            diameter,
            depth,
        } => format!(
            "hole {} {} ⌀{} × {}",
            face(*f),
            point(*at),
            mm(*diameter),
            mm(*depth)
        ),
        Operation::HolePattern {
            face: f,
            start,
            step,
            count,
            diameter,
            depth,
        } => format!(
            "{count} holes {} {} every {} ⌀{} × {}",
            face(*f),
            point(*start),
            point(*step),
            mm(*diameter),
            mm(*depth)
        ),
        Operation::Groove {
            face: f,
            start,
            end,
            width,
            depth,
        } => format!(
            "groove {} {}–{} {} × {}",
            face(*f),
            point(*start),
            point(*end),
            mm(*width),
            mm(*depth)
        ),
    }
}

/// The operations on the part `key` of the product labeled `label`, its
/// front node ids shortened to their last three digits.
fn operations(project: &Project, library: &Library, label: &str, key: &str) -> Vec<String> {
    let id = project
        .products
        .iter()
        .find(|p| p.label == label)
        .unwrap()
        .id;
    let key = key.replace("front/", "front/0199f8a0-0000-7000-8000-000000000");
    let evaluation = evaluate(project, library);
    let part = evaluation
        .parts
        .iter()
        .find(|p| p.product == id && p.key == key)
        .unwrap_or_else(|| panic!("no part {key}"));
    part.operations.iter().map(describe).collect()
}

/// What the evaluation says about the product labeled `label`.
fn problems_of(project: &Project, library: &Library, label: &str) -> Vec<String> {
    evaluate(project, library)
        .problems
        .into_iter()
        .filter(|p| p.product == label)
        .map(|p| p.message)
        .collect()
}

#[test]
fn the_sides_of_a_frameless_base_are_grooved_and_bored() {
    // The B30: sides 774.7 × 609.6 mm, x up from their bottom end, y
    // forward from their rear edge.
    let (project, library) = kitchen();
    let plates = |x: [&str; 4]| x.map(|x| format!("hole A ({x}, 572.6) ⌀5 × 12")).to_vec();
    let mut left = vec![
        // The back's groove: 1/4" plus half of 12 mm from the rear edge.
        "groove A (0, 12.35)–(774.7, 12.35) 12 × 9.525".to_owned(),
    ];
    // Door 1 hangs on the left side. It starts 1/16" above the side's
    // bottom end; its cups are 76.2 and 542.925 up it, so the plates' holes
    // are 16 mm either side of 77.7875 and 544.5125, 609.6 − 37 forward.
    left.extend(plates(["61.7875", "93.7875", "528.5125", "560.5125"]));
    // The shelves' opening runs from the bottom end to 622.3; the inside
    // starts at the top of the bottom, 18. Holes every 32 from 18 that are
    // 2" clear of both: 82 to 562. The rear column is 37 in front of the
    // back, which reaches 6.35 + 12 in.
    let columns = [
        "16 holes A (82, 572.6) every (32, 0) ⌀5 × 12".to_owned(),
        "16 holes A (82, 55.35) every (32, 0) ⌀5 × 12".to_owned(),
    ];
    left.extend(columns.clone());
    assert_eq!(
        operations(&project, &library, "B30", "case/side.left"),
        left
    );
    // Door 2 hangs on the right side, at the same heights.
    let mut right = vec!["groove A (0, 12.35)–(774.7, 12.35) 12 × 9.525".to_owned()];
    right.extend(plates(["61.7875", "93.7875", "528.5125", "560.5125"]));
    right.extend(columns);
    assert_eq!(
        operations(&project, &library, "B30", "case/side.right"),
        right
    );

    assert_eq!(
        operations(&project, &library, "B30", "case/bottom"),
        ["groove A (0, 12.35)–(726, 12.35) 12 × 9.525"]
    );
    for key in ["case/stretcher.front", "case/back", "front/403/shelf.1"] {
        assert_eq!(
            operations(&project, &library, "B30", key),
            Vec::<String>::new()
        );
    }
}

#[test]
fn doors_are_bored_for_hinge_cups_on_their_hinge_edge() {
    // The B30's doors, 619.125 high and 377.825 wide: x up from the
    // bottom, y from the left edge seen from the front. Two cups 3" from
    // the ends, centered 4 + 35 / 2 from the hinge edge.
    let (project, library) = kitchen();
    assert_eq!(
        operations(&project, &library, "B30", "front/403/door.1"),
        [
            "hole A (76.2, 21.5) ⌀35 × 13",
            "hole A (542.925, 21.5) ⌀35 × 13",
        ]
    );
    assert_eq!(
        operations(&project, &library, "B30", "front/403/door.2"),
        [
            "hole A (76.2, 356.325) ⌀35 × 13",
            "hole A (542.925, 356.325) ⌀35 × 13",
        ]
    );
    assert_eq!(
        operations(&project, &library, "B30", "front/402/drawer/front"),
        Vec::<String>::new()
    );
}

#[test]
fn drawer_boxes_are_grooved_for_their_bottom() {
    // 1/2" plus half of 12 mm up from the bottom edge, the full length.
    let (project, library) = kitchen();
    for (key, length) in [
        ("front/402/drawer/box.side.left", "558.8"),
        ("front/402/drawer/box.side.right", "558.8"),
        ("front/402/drawer/box.front", "676.6"),
        ("front/402/drawer/box.back", "676.6"),
    ] {
        assert_eq!(
            operations(&project, &library, "B30", key),
            [format!("groove A (0, 18.7)–({length}, 18.7) 12 × 6.35")]
        );
    }
    assert_eq!(
        operations(&project, &library, "B30", "front/402/drawer/box.bottom"),
        Vec::<String>::new()
    );
}

#[test]
fn face_frame_doors_take_their_cups_in_the_hinge_stile() {
    // The W30's five-piece doors, 711.2 high with 57.15 stiles: the left
    // one hinges on the left, the right one on the right. Face frame hinges
    // screw to the frame, so the sides only have the back's groove.
    let project: Project = read("project.json");
    let library: Library = read("library.json");
    assert_eq!(
        operations(&project, &library, "W30", "front/432/door.1/stile.left"),
        [
            "hole A (76.2, 21.5) ⌀35 × 13",
            "hole A (635, 21.5) ⌀35 × 13",
        ]
    );
    assert_eq!(
        operations(&project, &library, "W30", "front/433/door.1/stile.right"),
        [
            // 57.15 − 21.5 from the stile's left edge.
            "hole A (76.2, 35.65) ⌀35 × 13",
            "hole A (635, 35.65) ⌀35 × 13",
        ]
    );
    for key in [
        "front/432/door.1/stile.right",
        "front/433/door.1/stile.left",
    ] {
        assert_eq!(
            operations(&project, &library, "W30", key),
            Vec::<String>::new()
        );
    }
    assert_eq!(
        operations(&project, &library, "W30", "case/side.left"),
        ["groove A (0, 12.35)–(762, 12.35) 12 × 9.525"]
    );
    assert_eq!(
        operations(&project, &library, "W30", "case/top"),
        ["groove A (0, 12.35)–(726, 12.35) 12 × 9.525"]
    );
}

/// The sample's frameless standard.
const FRAMELESS: &str = "0199f8a0-0000-7000-8000-000000000131";

/// Sets `rule` of the frameless standard's `group` to `value`, or removes
/// the group when `rule` is empty.
fn set_rule(library: &mut Library, group: &str, rule: &str, value: serde_json::Value) {
    let id: Id = FRAMELESS.parse().unwrap();
    let standard = library.standards.iter_mut().find(|s| s.id == id).unwrap();
    if rule.is_empty() {
        standard.rules.remove(group);
    } else {
        standard.rules.get_mut(group).unwrap()[rule] = value;
    }
}

#[test]
fn shelves_that_would_reach_into_the_back_are_an_error() {
    // A 1" setback puts the back's front face 25.4 + 12 in, past the
    // shelves' 3/4" allowance.
    let (project, mut library) = kitchen();
    set_rule(&mut library, "back", "setback", 25_400_000.into());
    assert_eq!(
        problems_of(&project, &library, "B30"),
        [
            "the shelves reach into the back: their back allowance is less than the \
             back's setback and thickness"
        ]
    );
}

#[test]
fn a_single_door_needs_a_hinge_side() {
    let (mut project, library) = kitchen();
    let front = project.products[0].front.as_mut().unwrap();
    let doors = front.children[1].insert.as_mut().unwrap();
    doors.count = Some(1);
    doors.hinge_side = None;
    assert_eq!(
        problems_of(&project, &library, "B30").last().unwrap(),
        "a single door needs a hinge side"
    );
}

#[test]
fn missing_machining_rules_are_noted_and_the_parts_kept() {
    let (project, mut library) = kitchen();
    let before = evaluate(&project, &library).parts.len();
    set_rule(&mut library, "hinges", "", serde_json::Value::Null);
    set_rule(&mut library, "systemHoles", "", serde_json::Value::Null);
    let back = library
        .standards
        .iter_mut()
        .find(|s| s.id == FRAMELESS.parse::<Id>().unwrap())
        .unwrap()
        .rules
        .get_mut("back")
        .unwrap();
    back.as_object_mut().unwrap().remove("setback");
    assert_eq!(evaluate(&project, &library).parts.len(), before);
    assert_eq!(
        problems_of(&project, &library, "B30"),
        [
            "no edge band chosen for `edgeBandCase`; those edges are left bare",
            "the back's groove is not machined: the standard's `back` rules have no \
             `setback`",
            "doors are not bored for hinges: the standard has no `hinges` rules",
            "the sides are not bored for shelf pins: the standard has no `systemHoles` \
             rules",
        ]
    );
    assert_eq!(
        operations(&project, &library, "B30", "case/side.left"),
        Vec::<String>::new()
    );
}
