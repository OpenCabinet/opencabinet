// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The evaluator against the sample kitchen in `fixtures/`, with sizes
//! worked out by hand from the formulas in the crate documentation.
//!
//! The sample's B30 is 30" × 34-1/2" × 24" with a 6" drawer over a pair of
//! doors: case of 18 mm melamine, back, drawer box and bottom of 12 mm
//! birch, fronts of 18 mm maple banded with 0.5 mm. Its standard: toe kick
//! K = 4", stretchers S = 4", front gap G = 1/8", back groove 3/8"; drawer
//! side clearance 1/2", vertical clearance 1-1/2", rear clearance 1",
//! slides from 12" to 22" by 2", bottom groove 1/4"; shelves 1/16"
//! narrower than the inside, set back 1/4" in front and 3/4" for the back.
//! So Hb = 34-1/2" − 4" = 774.7 mm and the inside is W − 2T = 726 mm.

use std::path::PathBuf;

use opencabinet_eval::{Part, evaluate};
use opencabinet_model::{FrontNode, FrontStyle, Id, Length, Library, Project};
use serde::de::DeserializeOwned;

fn read<T: DeserializeOwned>(path: &str) -> T {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/kitchen.ocab")
        .join(path);
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

/// The sample kitchen, with the B30's five-piece doors made slabs, so the
/// tests of slab fronts and of the case read simple sizes.
fn kitchen_with_slab_doors() -> (Project, Library) {
    let mut project: Project = read("project.json");
    let front = project.products[0].front.as_mut().unwrap();
    front.children[1].insert.as_mut().unwrap().style = Some(FrontStyle::Slab);
    (project, read("library.json"))
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

/// The B30's parts whose key starts with `prefix`.
fn b30(project: &Project, library: &Library, prefix: &str) -> Vec<String> {
    parts_of(project, library, "B30", prefix)
}

/// The parts of the product labeled `label` whose key starts with `prefix`.
fn parts_of(project: &Project, library: &Library, label: &str, prefix: &str) -> Vec<String> {
    let id = project
        .products
        .iter()
        .find(|p| p.label == label)
        .unwrap()
        .id;
    evaluate(project, library)
        .parts
        .iter()
        .filter(|p| p.product == id && line(p).starts_with(prefix))
        .map(line)
        .collect()
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
fn the_case_matches_the_sizes_worked_out_by_hand() {
    let (project, library) = kitchen_with_slab_doors();
    assert_eq!(
        b30(&project, &library, "case/"),
        [
            "case/side.left: 774.7 × 609.6 × 18, cut 774.7 × 609.6",
            "case/side.right: 774.7 × 609.6 × 18, cut 774.7 × 609.6",
            "case/bottom: 726 × 609.6 × 18, cut 726 × 609.6",
            "case/stretcher.front: 726 × 101.6 × 18, cut 726 × 101.6",
            "case/stretcher.back: 726 × 101.6 × 18, cut 726 × 101.6",
            // Hb − T + g by W − 2T + 2g.
            "case/back: 766.225 × 745.05 × 12, cut 766.225 × 745.05",
        ]
    );
    let all = evaluate(&project, &library);
    let id = project.products[0].id;
    // Placed twice in the kitchen.
    assert!(
        all.parts
            .iter()
            .filter(|p| p.product == id)
            .all(|p| p.quantity == 2)
    );
}

#[test]
fn the_fronts_match_the_sizes_worked_out_by_hand() {
    let (project, library) = kitchen_with_slab_doors();
    let fronts: Vec<String> = b30(&project, &library, "front/")
        .into_iter()
        .filter(|l| !l.contains("/box."))
        .collect();
    assert_eq!(
        fronts,
        [
            // The 6" opening less the gap, grain across: W − G by 6" − G,
            // cut 1 mm smaller for the bands.
            "front/402/drawer/front: 758.825 × 149.225 × 18, cut 757.825 × 148.225",
            // The doors share Hb − 6" = 622.3 mm, in two halves of 381 mm.
            "front/403/door.1: 619.125 × 377.825 × 18, cut 618.125 × 376.825",
            "front/403/door.2: 619.125 × 377.825 × 18, cut 618.125 × 376.825",
        ]
    );
}

#[test]
fn the_drawer_box_matches_the_sizes_worked_out_by_hand() {
    let (project, library) = kitchen_with_slab_doors();
    assert_eq!(
        b30(&project, &library, "front/402/drawer/box."),
        [
            // 24" − 1" leaves 23": the 22" slide, 558.8 mm, sets the depth.
            // The 6" opening less 1-1/2" is 114.3 mm high.
            "front/402/drawer/box.side.left: 558.8 × 114.3 × 12, cut 558.8 × 114.3",
            "front/402/drawer/box.side.right: 558.8 × 114.3 × 12, cut 558.8 × 114.3",
            // 726 − 2 × 12.7 = 700.6 mm wide outside, less two 12 mm sides.
            "front/402/drawer/box.front: 676.6 × 114.3 × 12, cut 676.6 × 114.3",
            "front/402/drawer/box.back: 676.6 × 114.3 × 12, cut 676.6 × 114.3",
            // The inside plus two 1/4" grooves each way.
            "front/402/drawer/box.bottom: 689.3 × 547.5 × 12, cut 689.3 × 547.5",
        ]
    );
}

#[test]
fn five_piece_doors_match_the_sizes_worked_out_by_hand() {
    // The sample's B30 as it is: five-piece doors 619.125 × 377.825 mm, with
    // 2-1/4" stiles and rails, rails reaching 3/8" into the stiles, a 3/8"
    // panel groove and 1/8" of play for the panel; frames of the doors'
    // 18 mm maple, panels of 12 mm birch, none of them banded.
    let project: Project = read("project.json");
    let library: Library = read("library.json");
    assert_eq!(
        b30(&project, &library, "front/403/door.1/"),
        [
            "front/403/door.1/stile.left: 619.125 × 57.15 × 18, cut 619.125 × 57.15",
            "front/403/door.1/stile.right: 619.125 × 57.15 × 18, cut 619.125 × 57.15",
            // 377.825 − 2 × 57.15 + 2 × 9.525.
            "front/403/door.1/rail.top: 282.575 × 57.15 × 18, cut 282.575 × 57.15",
            "front/403/door.1/rail.bottom: 282.575 × 57.15 × 18, cut 282.575 × 57.15",
            // 619.125 − 114.3 + 19.05 − 3.175 by 377.825 − 114.3 + 19.05 − 3.175.
            "front/403/door.1/panel: 520.7 × 279.4 × 12, cut 520.7 × 279.4",
        ]
    );
}

#[test]
fn shelves_span_the_inside_less_their_clearances() {
    let (mut project, library) = kitchen_with_slab_doors();
    let front = project.products[0].front.as_mut().unwrap();
    front.children[1].insert.as_mut().unwrap().shelves = Some(2);
    assert_eq!(
        b30(&project, &library, "front/403/shelf"),
        [
            // 726 − 1/16" by 24" − 1/4" − 3/4".
            "front/403/shelf.1: 724.4125 × 584.2 × 18, cut 724.4125 × 584.2",
            "front/403/shelf.2: 724.4125 × 584.2 × 18, cut 724.4125 × 584.2",
        ]
    );
}

/// The sample's B30 turned into another kind of cabinet, `w` × `h` × `d`
/// inches, with a pair of slab doors filling its front.
fn variant(product_type: &str, w: i64, h: i64, d: i64) -> (Project, Library) {
    let (mut project, library) = kitchen_with_slab_doors();
    let product = &mut project.products[0];
    product.product_type = product_type.into();
    product.width = Length::from_inches(w);
    product.height = Length::from_inches(h);
    product.depth = Length::from_inches(d);
    let front = product.front.as_mut().unwrap();
    let doors = front.children.remove(1);
    *front = FrontNode {
        size: None,
        ..doors
    };
    project.rooms.clear();
    (project, library)
}

#[test]
fn a_wall_cabinet_has_a_top_and_no_toe_kick() {
    // 30" × 30" × 12": the box is the full 762 mm high.
    let (project, library) = variant("wall-cabinet@1", 30, 30, 12);
    assert_eq!(
        b30(&project, &library, ""),
        [
            "case/side.left: 762 × 304.8 × 18, cut 762 × 304.8",
            "case/side.right: 762 × 304.8 × 18, cut 762 × 304.8",
            "case/bottom: 726 × 304.8 × 18, cut 726 × 304.8",
            "case/top: 726 × 304.8 × 18, cut 726 × 304.8",
            // Grooved into the top and the bottom: H − 2T + 2g.
            "case/back: 745.05 × 745.05 × 12, cut 745.05 × 745.05",
            // The face, 762 mm square, in two halves less the gap.
            "front/403/door.1: 758.825 × 377.825 × 18, cut 757.825 × 376.825",
            "front/403/door.2: 758.825 × 377.825 × 18, cut 757.825 × 376.825",
        ]
    );
}

#[test]
fn a_tall_cabinet_has_a_top_and_a_toe_kick() {
    // 24" × 84" × 24": Hb = 2133.6 − 101.6 = 2032 mm.
    let (project, library) = variant("tall-cabinet@1", 24, 84, 24);
    assert_eq!(
        b30(&project, &library, "case/"),
        [
            "case/side.left: 2032 × 609.6 × 18, cut 2032 × 609.6",
            "case/side.right: 2032 × 609.6 × 18, cut 2032 × 609.6",
            "case/bottom: 573.6 × 609.6 × 18, cut 573.6 × 609.6",
            "case/top: 573.6 × 609.6 × 18, cut 573.6 × 609.6",
            "case/back: 2015.05 × 592.65 × 12, cut 2015.05 × 592.65",
        ]
    );
}

#[test]
fn what_cannot_be_evaluated_yet_is_reported_not_guessed() {
    let project: Project = read("project.json");
    let library: Library = read("library.json");
    let evaluation = evaluate(&project, &library);
    let problems: Vec<String> = evaluation
        .problems
        .iter()
        .map(|p| {
            let kind = if p.blocking { "error" } else { "note" };
            format!("{kind}: {}: {}", p.product, p.message)
        })
        .collect();
    assert_eq!(
        problems,
        [
            "note: B30: no edge band chosen for `edgeBandCase`; those edges are left bare",
            "error: SB36: product type `sink-base@1` cannot be evaluated yet",
            "note: BC42: no edge band chosen for `edgeBandCase`; those edges are left bare",
            "note: BC42: no edge band chosen for `edgeBandFronts`; those edges are left bare",
            "error: IS48: no material chosen for `back`",
            "error: F3: product type `filler@1` cannot be evaluated yet",
            "error: V24: product type `vanity-base@1` cannot be evaluated yet",
        ]
    );
}

#[test]
fn bare_edges_are_noted() {
    let (project, library) = kitchen_with_slab_doors();
    let evaluation = evaluate(&project, &library);
    let notes: Vec<&str> = evaluation
        .problems
        .iter()
        .filter(|p| p.product == "B30")
        .map(|p| p.message.as_str())
        .collect();
    assert_eq!(
        notes,
        ["no edge band chosen for `edgeBandCase`; those edges are left bare"]
    );
}

#[test]
fn evaluation_is_deterministic() {
    let project: Project = read("project.json");
    let library: Library = read("library.json");
    assert_eq!(evaluate(&project, &library), evaluate(&project, &library));
}

#[test]
fn a_face_frame_wall_cabinet_matches_the_sizes_worked_out_by_hand() {
    // The sample's W30, 30" × 30" × 12", with a frame of 3/4" maple
    // (19.05 mm), stiles and rails 1-1/2", two five-piece doors side by
    // side overlaying the frame by 1/2", and the case of 18 mm maple.
    let project: Project = read("project.json");
    let library: Library = read("library.json");
    assert_eq!(
        parts_of(&project, &library, "W30", ""),
        [
            // The frame is part of the depth: 304.8 − 19.05.
            "case/side.left: 762 × 285.75 × 18, cut 762 × 285.75",
            "case/side.right: 762 × 285.75 × 18, cut 762 × 285.75",
            "case/bottom: 726 × 285.75 × 18, cut 726 × 285.75",
            "case/top: 726 × 285.75 × 18, cut 726 × 285.75",
            // The bottom is raised 38.1 − 18 = 20.1 to be flush with the
            // bottom rail, so the back is that much shorter than a
            // frameless one: 762 − 2 × (18 − 9.525) − 20.1.
            "case/back: 724.95 × 745.05 × 12, cut 724.95 × 745.05",
            "frame/stile.left: 762 × 38.1 × 19.05, cut 762 × 38.1",
            "frame/stile.right: 762 × 38.1 × 19.05, cut 762 × 38.1",
            // Between the stiles: 762 − 2 × 38.1.
            "frame/rail.top: 685.8 × 38.1 × 19.05, cut 685.8 × 38.1",
            "frame/rail.bottom: 685.8 × 38.1 × 19.05, cut 685.8 × 38.1",
            // Between the rails.
            "frame/431/mullion.1: 685.8 × 38.1 × 19.05, cut 685.8 × 38.1",
            // Openings (685.8 − 38.1) / 2 = 323.85 wide and 685.8 high,
            // plus 1/2" on each edge: doors 711.2 × 349.25.
            "front/432/door.1/stile.left: 711.2 × 57.15 × 18, cut 711.2 × 57.15",
            "front/432/door.1/stile.right: 711.2 × 57.15 × 18, cut 711.2 × 57.15",
            "front/432/door.1/rail.top: 254 × 57.15 × 18, cut 254 × 57.15",
            "front/432/door.1/rail.bottom: 254 × 57.15 × 18, cut 254 × 57.15",
            "front/432/door.1/panel: 612.775 × 250.825 × 18, cut 612.775 × 250.825",
            "front/433/door.1/stile.left: 711.2 × 57.15 × 18, cut 711.2 × 57.15",
            "front/433/door.1/stile.right: 711.2 × 57.15 × 18, cut 711.2 × 57.15",
            "front/433/door.1/rail.top: 254 × 57.15 × 18, cut 254 × 57.15",
            "front/433/door.1/rail.bottom: 254 × 57.15 × 18, cut 254 × 57.15",
            "front/433/door.1/panel: 612.775 × 250.825 × 18, cut 612.775 × 250.825",
        ]
    );
    // The frame covers the case's front edges, so no band is asked for.
    assert_eq!(problems_of(&project, &library, "W30"), Vec::<String>::new());
}

/// The sample's face frame standard.
const FACE_FRAME: &str = "0199f8a0-0000-7000-8000-000000000132";

/// The sample kitchen with the B30 built with a face frame of 3/4" maple.
fn face_frame_base() -> (Project, Library) {
    let (mut project, library) = kitchen_with_slab_doors();
    let b30 = &mut project.products[0];
    b30.standard = FACE_FRAME.parse().unwrap();
    let maple = "0199f8a0-0000-7000-8000-000000000104".parse().unwrap();
    b30.materials.insert("faceFrame".into(), maple);
    (project, library)
}

/// Sets `rule` of the face frame standard's `group` to `value`.
fn set_rule(library: &mut Library, group: &str, rule: &str, value: serde_json::Value) {
    let id: Id = FACE_FRAME.parse().unwrap();
    let standard = library.standards.iter_mut().find(|s| s.id == id).unwrap();
    standard.rules.get_mut(group).unwrap()[rule] = value;
}

#[test]
fn a_face_frame_base_cabinet_matches_the_sizes_worked_out_by_hand() {
    // The B30, 30" × 34-1/2" × 24", on the face frame standard: the frame
    // is Hb = 774.7 mm high above the toe kick and its opening
    // 685.8 × 698.5 mm. The 6" drawer opening sits over the doors' with a
    // 1-1/2" rail between them, which leaves 508 mm for the doors.
    let (project, library) = face_frame_base();
    assert_eq!(
        b30(&project, &library, ""),
        [
            // 609.6 − 19.05 deep; the frame covers the front edges, so
            // they are not banded.
            "case/side.left: 774.7 × 590.55 × 18, cut 774.7 × 590.55",
            "case/side.right: 774.7 × 590.55 × 18, cut 774.7 × 590.55",
            "case/bottom: 726 × 590.55 × 18, cut 726 × 590.55",
            "case/stretcher.front: 726 × 101.6 × 18, cut 726 × 101.6",
            "case/stretcher.back: 726 × 101.6 × 18, cut 726 × 101.6",
            // 774.7 − (18 − 9.525) − 20.1.
            "case/back: 746.125 × 745.05 × 12, cut 746.125 × 745.05",
            "frame/stile.left: 774.7 × 38.1 × 19.05, cut 774.7 × 38.1",
            "frame/stile.right: 774.7 × 38.1 × 19.05, cut 774.7 × 38.1",
            "frame/rail.top: 685.8 × 38.1 × 19.05, cut 685.8 × 38.1",
            "frame/rail.bottom: 685.8 × 38.1 × 19.05, cut 685.8 × 38.1",
            "frame/401/rail.1: 685.8 × 38.1 × 19.05, cut 685.8 × 38.1",
            // 685.8 + 2 × 12.7 by 152.4 + 2 × 12.7, banded 0.5 mm.
            "front/402/drawer/front: 711.2 × 177.8 × 18, cut 710.2 × 176.8",
            // Through the frame opening: 685.8 − 2 × 12.7 = 660.4 mm wide
            // outside, less two 12 mm sides. 590.55 − 25.4 takes the 22"
            // slide.
            "front/402/drawer/box.side.left: 558.8 × 114.3 × 12, cut 558.8 × 114.3",
            "front/402/drawer/box.side.right: 558.8 × 114.3 × 12, cut 558.8 × 114.3",
            "front/402/drawer/box.front: 636.4 × 114.3 × 12, cut 636.4 × 114.3",
            "front/402/drawer/box.back: 636.4 × 114.3 × 12, cut 636.4 × 114.3",
            "front/402/drawer/box.bottom: 649.1 × 547.5 × 12, cut 649.1 × 547.5",
            // 711.2 wide less the 1/8" between them, in halves, by
            // 508 + 2 × 12.7.
            "front/403/door.1: 533.4 × 354.0125 × 18, cut 532.4 × 353.0125",
            "front/403/door.2: 533.4 × 354.0125 × 18, cut 532.4 × 353.0125",
        ]
    );
    assert_eq!(
        problems_of(&project, &library, "B30"),
        [
            "drawer slides in face frame cabinets mount on blocking out to the frame \
             opening, which is not in the cut list yet"
        ]
    );
}

#[test]
fn inset_fronts_stand_back_from_the_frame_by_the_gap() {
    let (project, mut library) = face_frame_base();
    set_rule(&mut library, "fronts", "overlay", "inset".into());
    let fronts: Vec<String> = b30(&project, &library, "front/")
        .into_iter()
        .filter(|l| !l.contains("/box."))
        .collect();
    assert_eq!(
        fronts,
        [
            // 685.8 − 2 × 3.175 by 152.4 − 2 × 3.175.
            "front/402/drawer/front: 679.45 × 146.05 × 18, cut 678.45 × 145.05",
            // (679.45 − 3.175) / 2 by 508 − 2 × 3.175.
            "front/403/door.1: 501.65 × 338.1375 × 18, cut 500.65 × 337.1375",
            "front/403/door.2: 501.65 × 338.1375 × 18, cut 500.65 × 337.1375",
        ]
    );
}

#[test]
fn face_frames_that_cannot_be_built_are_reported() {
    let errors = |group: &str, rule: &str, value: serde_json::Value| {
        let project: Project = read("project.json");
        let mut library: Library = read("library.json");
        set_rule(&mut library, group, rule, value);
        problems_of(&project, &library, "W30")
    };
    // A 1" mullion: two 1/2" overlays leave nothing between the doors.
    assert_eq!(
        errors("faceFrame", "mullionWidth", 25_400_000.into()),
        [
            "a rail or mullion is too narrow for the fronts on each side of it to \
             clear each other by the gap"
        ]
    );
    assert_eq!(
        errors("fronts", "overlayAmount", 50_800_000.into()),
        ["the fronts overlay more than the face frame around them is wide"]
    );
    assert_eq!(
        errors("fronts", "overlay", "full".into()),
        ["face frame cabinets take partial overlay or inset fronts"]
    );
    assert_eq!(
        errors("faceFrame", "joinery", "mortiseAndTenon".into()),
        ["standard Shop face frame: unknown variant `mortiseAndTenon`, expected `butt`"]
    );
    assert_eq!(
        errors("faceFrame", "bottomRailWidth", 12_700_000.into()),
        ["the face frame's bottom rail is narrower than the case bottom is thick"]
    );
}
