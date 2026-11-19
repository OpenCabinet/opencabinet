// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the sample kitchen's parts sit in their products: positions
//! worked out by hand, and no two parts in the same place except where a
//! joint puts one into the other.

use std::path::PathBuf;

use opencabinet_eval::{Axis, Part, evaluate};
use opencabinet_model::{FrontStyle, Length, Library, Project};
use serde::de::DeserializeOwned;

fn read<T: DeserializeOwned>(path: &str) -> T {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/kitchen.ocab")
        .join(path);
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

/// The sample kitchen, and a copy with the B30's doors made slabs with
/// two shelves behind them.
fn kitchens() -> Vec<(Project, Library)> {
    let project: Project = read("project.json");
    let library: Library = read("library.json");
    let mut shelved = project.clone();
    let front = shelved.products[0].front.as_mut().unwrap();
    let doors = front.children[1].insert.as_mut().unwrap();
    doors.style = Some(FrontStyle::Slab);
    doors.shelves = Some(2);
    vec![(project, library.clone()), (shelved, library)]
}

fn bounds(part: &Part) -> ([Length; 3], [Length; 3]) {
    part.placement
        .bounds(part.length, part.width, part.thickness)
}

fn um(value: i64) -> Length {
    Length::from_nm(value * 1_000)
}

/// Whether a joint puts one of these parts into the other.
fn joined(a: &Part, b: &Part) -> bool {
    let pair = |x: &str, y: &[&str]| {
        (a.role == x && y.contains(&b.role)) || (b.role == x && y.contains(&a.role))
    };
    // The back in its grooves, a five-piece front's rails in its stiles and
    // its panel in both, a drawer bottom in its box.
    pair("back", &["side", "bottom", "top"])
        || pair("rail", &["stile"])
        || pair("panel", &["stile", "rail"])
        || pair(
            "drawerBottom",
            &["drawerSide", "drawerSubFront", "drawerBack"],
        )
}

#[test]
fn no_two_parts_are_in_the_same_place() {
    for (project, library) in kitchens() {
        let parts = evaluate(&project, &library).parts;
        for (i, a) in parts.iter().enumerate() {
            for b in &parts[i + 1..] {
                if a.product != b.product || joined(a, b) {
                    continue;
                }
                let ((a0, a1), (b0, b1)) = (bounds(a), bounds(b));
                let overlap = (0..3).all(|k| a0[k] < b1[k] && b0[k] < a1[k]);
                assert!(!overlap, "{} runs into {}", a.key, b.key);
            }
        }
    }
}

#[test]
fn parts_stay_inside_their_product() {
    // Within the product's width and height; from behind an applied back
    // to in front of the fronts.
    for (project, library) in kitchens() {
        for part in evaluate(&project, &library).parts {
            let product = project
                .products
                .iter()
                .find(|p| p.id == part.product)
                .unwrap();
            let (low, high) = bounds(&part);
            let max = [product.width, product.height, product.depth + um(25_400)];
            for k in 0..3 {
                assert!(
                    low[k] >= -um(25_400) && high[k] <= max[k],
                    "{} of {} is outside it on axis {k}: {low:?} to {high:?}",
                    part.key,
                    product.label
                );
            }
        }
    }
}

#[test]
fn the_b30_is_assembled_as_drawn() {
    // The B30, frameless, 762 wide, 876.3 high, 609.6 deep, on a 101.6 toe
    // kick, its case 18 thick; doors 3/4" overlay the box face.
    let (project, library) = kitchens().remove(1);
    let parts = evaluate(&project, &library).parts;
    let b30 = project.products[0].id;
    let part = |key: &str| {
        let key = key.replace("front/", "front/0199f8a0-0000-7000-8000-000000000");
        parts
            .iter()
            .find(|p| p.product == b30 && p.key == key)
            .unwrap_or_else(|| panic!("no part {key}"))
    };
    let corners = |key: &str| {
        let (low, high) = bounds(part(key));
        [low, high].map(|c| c.map(|v| v.nm() / 1_000))
    };
    assert_eq!(
        corners("case/side.left"),
        [[0, 101_600, 0], [18_000, 876_300, 609_600]]
    );
    assert_eq!(part("case/side.right").placement.axes[2], Axis::NegX);
    assert_eq!(
        corners("case/side.right"),
        [[744_000, 101_600, 0], [762_000, 876_300, 609_600]]
    );
    assert_eq!(
        corners("case/bottom"),
        [[18_000, 101_600, 0], [744_000, 119_600, 609_600]]
    );
    // Stretchers under the top of the sides; the back one in front of the
    // back, 6.35 + 12 from the rear edge, 4" wide.
    assert_eq!(
        corners("case/stretcher.back"),
        [[18_000, 858_300, 18_350], [744_000, 876_300, 119_950]]
    );
    // The back in its 9.525 deep grooves, from the bottom's groove to the
    // top of the box.
    assert_eq!(
        corners("case/back"),
        [[8_475, 110_075, 6_350], [753_525, 876_300, 18_350]]
    );
    // The doors stand in front of the box, 1/16" in from its edges.
    let (low, high) = bounds(part("front/403/door.1"));
    assert_eq!(low[2], um(609_600));
    assert_eq!(high[2] - low[2], part("front/403/door.1").thickness);
    assert_eq!(low[0], Length::from_nm(1_587_500));
    // The two shelves share the opening's height evenly.
    let (s1, s2) = (corners("front/403/shelf.1"), corners("front/403/shelf.2"));
    let below = s1[0][1] - 119_600;
    let between = s2[0][1] - s1[1][1];
    assert!((below - between).abs() <= 1, "{below} and {between}");
}
