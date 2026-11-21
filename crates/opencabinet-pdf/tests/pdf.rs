// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opening, drawing, and saving markups as an incremental update.

use std::collections::BTreeMap;

use opencabinet_pdf::annotations::{self, Annotation, Markups, Subtype};
use opencabinet_pdf::{Document, Point, View, render, sample};

/// A 36" × 24" sheet.
const SHEET: (f64, f64) = (2592.0, 1728.0);

fn square(page: usize, name: &str) -> Annotation {
    Annotation {
        page,
        subtype: Subtype::Square,
        rect: [100.0, 200.0, 400.0, 300.0],
        vertices: Vec::new(),
        color: [1.0, 0.0, 0.0],
        width: 4.0,
        contents: "Confirm 36\" counter height".into(),
        author: "Andrés".into(),
        name: name.into(),
        modified: "D:20261121120000Z".into(),
        cloudy: false,
        arrow: false,
        font_size: 12.0,
        own: BTreeMap::from([("Kind".into(), "rectangle".into())]),
        appearance: b"1 0 0 RG 4 w 102 202 296 96 re S".to_vec(),
    }
}

#[test]
fn a_sheet_opens_with_its_size_and_turns_to_page_space() {
    let document = Document::open(sample::sheets(3, SHEET.0, SHEET.1, 10)).unwrap();
    assert_eq!(document.pages().len(), 3);
    let page = document.pages()[0];
    assert_eq!((page.width, page.height), SHEET);
    // PDF space is y up from the bottom; page space y down from the top.
    let top_left = page.to_page.apply(Point::new(0.0, SHEET.1));
    assert!(top_left.distance(Point::new(0.0, 0.0)) < 1e-6);
    let back = page.to_pdf().apply(Point::new(10.0, 20.0));
    assert!(back.distance(Point::new(10.0, SHEET.1 - 20.0)) < 1e-6);
}

#[test]
fn a_view_is_drawn_white_on_the_page_and_gray_around_it() {
    let document = Document::open(sample::sheets(1, SHEET.0, SHEET.1, 0)).unwrap();
    // Half size, starting 100 px left of and above the page.
    let view = View {
        page: 0,
        zoom: 0.5,
        origin: Point::new(-100.0, -100.0),
        width: 800,
        height: 600,
    };
    let bitmap = render(&document, view).unwrap();
    assert_eq!(bitmap.pixels.len(), 800 * 600 * 4);
    assert_eq!(bitmap.pixel(50, 50), [0x80, 0x80, 0x80, 0xff]);
    assert_eq!(bitmap.pixel(400, 400), [0xff, 0xff, 0xff, 0xff]);
    // The border, 18 pt in and 2 pt wide: at 0.5, 9 px into the page.
    let border = bitmap.pixel(400, 109);
    assert!(border[0] < 0xa0, "{border:?}");
    assert!(render(&document, View { page: 1, ..view }).is_err());
}

#[test]
fn markups_are_appended_and_the_original_stays_untouched() {
    let original = sample::sheets(2, SHEET.0, SHEET.1, 5);
    let mut markups = Markups::default();
    markups.annotations.push(square(1, "a"));
    let mut cloud = square(1, "b");
    cloud.subtype = Subtype::Polygon;
    cloud.cloudy = true;
    cloud.vertices = vec![
        Point::new(100.0, 200.0),
        Point::new(400.0, 200.0),
        Point::new(400.0, 300.0),
        Point::new(100.0, 300.0),
    ];
    markups.annotations.push(cloud.clone());
    markups
        .pages
        .insert(1, BTreeMap::from([("ScaleNm".into(), "3657600000".into())]));
    let saved = annotations::save(&original, &markups).unwrap();
    assert!(saved.starts_with(&original));

    let read = annotations::read(&saved).unwrap();
    assert_eq!(read.pages, markups.pages);
    let mut expected = markups.annotations.clone();
    for annotation in &mut expected {
        annotation.appearance.clear();
    }
    assert_eq!(read.annotations, expected);

    // Saving again replaces them rather than adding: one markup left.
    markups.annotations.remove(0);
    let again = annotations::save(&saved, &markups).unwrap();
    assert!(again.starts_with(&saved));
    let read = annotations::read(&again).unwrap();
    assert_eq!(read.annotations.len(), 1);
    assert_eq!(read.annotations[0].name, "b");
    assert!(read.annotations[0].cloudy);

    // With nothing to keep and none before, nothing is appended.
    let blank = sample::sheets(1, SHEET.0, SHEET.1, 0);
    assert_eq!(
        annotations::save(&blank, &Markups::default()).unwrap(),
        blank
    );
}

#[test]
fn a_saved_markup_is_drawn_from_its_appearance() {
    let original = sample::sheets(1, SHEET.0, SHEET.1, 0);
    let markups = Markups {
        annotations: vec![square(0, "a")],
        pages: BTreeMap::new(),
    };
    let saved = Document::open(annotations::save(&original, &markups).unwrap()).unwrap();
    // The top edge of the rectangle, y = 298 in PDF space, at 1 px per pt.
    let view = View {
        page: 0,
        zoom: 1.0,
        origin: Point::new(0.0, 0.0),
        width: 600,
        height: 1728,
    };
    let red = render(&saved, view).unwrap().pixel(250, 1728 - 298);
    assert!(red[0] > 200 && red[1] < 60 && red[2] < 60, "{red:?}");
}

#[test]
fn a_markup_on_a_missing_page_is_refused() {
    let original = sample::sheets(1, SHEET.0, SHEET.1, 0);
    let markups = Markups {
        annotations: vec![square(3, "a")],
        pages: BTreeMap::new(),
    };
    assert!(annotations::save(&original, &markups).is_err());
}
