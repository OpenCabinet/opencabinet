// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opening, drawing, and saving markups as an incremental update; and the
//! markups other programs made, kept as they were.

use std::collections::BTreeMap;

use lopdf::Object;
use opencabinet_pdf::annotations::{self, Annotation, Change, Markups, Measure, Subtype};
use opencabinet_pdf::{Document, Point, View, render, sample};

/// A 36" × 24" sheet.
const SHEET: (f64, f64) = (2592.0, 1728.0);

fn square(page: usize, name: &str) -> Annotation {
    Annotation {
        page,
        subtype: Subtype::Square,
        rect: [100.0, 200.0, 400.0, 300.0],
        color: [1.0, 0.0, 0.0],
        width: 4.0,
        contents: "Confirm 36\" counter height".into(),
        subject: "Rectangle".into(),
        author: "Andrés".into(),
        name: name.into(),
        modified: "D:20261121120000Z".into(),
        own: BTreeMap::from([("Kind".into(), "rectangle".into())]),
        appearance: b"1 0 0 RG 4 w 102 202 296 96 re S".to_vec(),
        ..Annotation::default()
    }
}

/// As `read` returns `annotation` once saved: without its appearance, and
/// with the object it is in.
fn as_read(annotation: &Annotation, read: &Annotation) -> Annotation {
    Annotation {
        appearance: Vec::new(),
        origin: read.origin,
        change: Change::Unchanged,
        ..annotation.clone()
    }
}

fn object(bytes: &[u8], id: (u32, u16)) -> lopdf::Dictionary {
    lopdf::Document::load_mem(bytes)
        .unwrap()
        .get_dictionary(id)
        .unwrap()
        .clone()
}

fn page_annots(bytes: &[u8]) -> usize {
    let document = lopdf::Document::load_mem(bytes).unwrap();
    let page = *document.get_pages().get(&1).unwrap();
    document
        .get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .and_then(Object::as_array)
        .map_or(0, Vec::len)
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
    cloud.interior = Some([1.0, 1.0, 0.0]);
    cloud.opacity = 0.5;
    cloud.vertices = vec![
        Point::new(100.0, 200.0),
        Point::new(400.0, 200.0),
        Point::new(400.0, 300.0),
        Point::new(100.0, 300.0),
    ];
    markups.annotations.push(cloud);
    markups
        .pages
        .insert(1, BTreeMap::from([("ScaleNm".into(), "3657600000".into())]));
    let saved = annotations::save(&original, &markups).unwrap();
    assert!(saved.starts_with(&original));

    let read = annotations::read(&saved).unwrap();
    assert_eq!(read.pages, markups.pages);
    assert_eq!(read.annotations.len(), 2);
    for (read, written) in read.annotations.iter().zip(&markups.annotations) {
        assert!(read.origin.is_some());
        assert_eq!(*read, as_read(written, read));
    }

    // Removing one, the other stays: the update only drops it from the page.
    let mut next = read.clone();
    let gone = next.annotations.remove(0);
    next.removed.push(gone.origin.unwrap());
    let again = annotations::save(&saved, &next).unwrap();
    assert!(again.starts_with(&saved));
    let read = annotations::read(&again).unwrap();
    assert_eq!(read.annotations.len(), 1);
    assert_eq!(read.annotations[0].name, "b");

    // Nothing changed: nothing appended.
    assert_eq!(annotations::save(&again, &read).unwrap(), again);
}

#[test]
fn another_programs_markups_are_kept_as_they_were() {
    let original = sample::foreign_markups();
    let read = annotations::read(&original).unwrap();
    // The cloud and the measured line; not the pop-up, reply or link.
    assert_eq!(read.annotations.len(), 2);
    let cloud = &read.annotations[0];
    assert!(cloud.cloudy);
    assert_eq!(cloud.subject, "Cloud+");
    let line = &read.annotations[1];
    assert_eq!(line.intent, "LineDimension");
    let measure = line.measure.as_ref().unwrap();
    assert_eq!(measure.unit, "ft");
    assert_eq!(measure.ratio, "1 in = 4 ft");
    // 216 points at 1/18 ft each: 12 ft.
    assert!((216.0 * measure.factor - 12.0).abs() < 1e-5);

    // Saved untouched: nothing appended at all.
    assert_eq!(annotations::save(&original, &read).unwrap(), original);

    // Moved: the same object, its appearance and keys, shifted.
    let id = cloud.origin.unwrap();
    let before = object(&original, id);
    let mut moved = read.clone();
    moved.annotations[0].change = Change::Moved { dx: 10.0, dy: -5.0 };
    let saved = annotations::save(&original, &moved).unwrap();
    assert!(saved.starts_with(&original));
    let after = object(&saved, id);
    assert_eq!(after.get(b"AP").unwrap(), before.get(b"AP").unwrap());
    assert_eq!(
        after.get(b"OtherProgram").unwrap(),
        before.get(b"OtherProgram").unwrap()
    );
    let rect = annotations::read(&saved).unwrap().annotations[0].rect;
    assert_eq!(rect, [100.0, 85.0, 320.0, 205.0]);
    assert_eq!(page_annots(&saved), 5);

    // Changed: Review's keys and look, the other program's keys kept.
    let mut changed = read.clone();
    changed.annotations[0].change = Change::Rewritten;
    changed.annotations[0].contents = "Checked".into();
    changed.annotations[0].appearance = b"0 0 1 RG 100 100 200 100 re S".to_vec();
    let saved = annotations::save(&original, &changed).unwrap();
    let after = object(&saved, id);
    assert_ne!(after.get(b"AP").unwrap(), before.get(b"AP").unwrap());
    assert_eq!(
        after.get(b"OtherProgram").unwrap(),
        before.get(b"OtherProgram").unwrap()
    );
    assert_eq!(
        annotations::read(&saved).unwrap().annotations[0].contents,
        "Checked"
    );

    // Removed: with its pop-up and its reply; the line and link stay.
    let mut removed = read.clone();
    removed.annotations.remove(0);
    removed.removed.push(id);
    let saved = annotations::save(&original, &removed).unwrap();
    assert_eq!(page_annots(&saved), 2);
    assert_eq!(annotations::read(&saved).unwrap().annotations.len(), 1);
}

#[test]
fn replies_are_read_with_their_markup_and_new_ones_appended() {
    let original = sample::foreign_markups();
    let read = annotations::read(&original).unwrap();
    let cloud = &read.annotations[0];
    assert_eq!(cloud.replies.len(), 1);
    assert_eq!(cloud.replies[0].contents, "Done");
    assert!(cloud.replies[0].origin.is_some());
    assert_eq!(read.annotations[1].replies.len(), 0);

    // A reply to the other program's cloud leaves the cloud as it was.
    let id = cloud.origin.unwrap();
    let before = object(&original, id);
    let mut answered = read.clone();
    answered.annotations[0].replies.push(annotations::Reply {
        name: "r-1".into(),
        author: "Andrés".into(),
        contents: "Agreed, 36\" it is".into(),
        modified: "D:20261122120000Z".into(),
        origin: None,
    });
    let saved = annotations::save(&original, &answered).unwrap();
    assert!(saved.starts_with(&original));
    assert_eq!(object(&saved, id), before);
    assert_eq!(page_annots(&saved), 6);
    let again = annotations::read(&saved).unwrap();
    let replies = &again.annotations[0].replies;
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[1].contents, "Agreed, 36\" it is");
    assert_eq!(replies[1].author, "Andrés");
    let written = object(&saved, replies[1].origin.unwrap());
    assert_eq!(written.get(b"IRT").unwrap().as_reference().unwrap(), id);
    assert_eq!(written.get(b"RT").unwrap().as_name().unwrap(), b"R");
    // Saved again unchanged: nothing more appended.
    assert_eq!(annotations::save(&saved, &again).unwrap(), saved);

    // A new markup's replies are written with it.
    let mut new = square(0, "a");
    new.replies.push(annotations::Reply {
        name: "r-2".into(),
        author: "Sam".into(),
        contents: "Checked on site".into(),
        modified: "D:20261122130000Z".into(),
        origin: None,
    });
    let blank = sample::sheets(1, SHEET.0, SHEET.1, 0);
    let markups = Markups {
        annotations: vec![new],
        ..Markups::default()
    };
    let saved = annotations::save(&blank, &markups).unwrap();
    let read = annotations::read(&saved).unwrap();
    assert_eq!(read.annotations.len(), 1);
    assert_eq!(read.annotations[0].replies[0].contents, "Checked on site");

    // Removing the markup takes its replies.
    let mut removed = read.clone();
    removed.removed.push(read.annotations[0].origin.unwrap());
    removed.annotations.clear();
    let saved = annotations::save(&saved, &removed).unwrap();
    assert_eq!(page_annots(&saved), 0);
}

#[test]
fn a_dense_sheet_is_read_for_snapping_and_answers_at_once() {
    use opencabinet_pdf::snaps::{Snap, Snaps};
    // A border and a 10 000 by 10 000 line grid: 20 004 lines.
    let bytes = sample::sheets(1, SHEET.0, SHEET.1, 10_000);
    let document = Document::open(bytes).unwrap();
    let started = std::time::Instant::now();
    let snaps = Snaps::of(&document, 0).unwrap();
    assert_eq!(snaps.len(), 20_004);
    let read = started.elapsed();
    let started = std::time::Instant::now();
    for i in 0..1000 {
        let x = 100.0 + f64::from(i);
        assert!(snaps.nearest(Point::new(x, 900.0), 3.0).is_some());
    }
    let asked = started.elapsed();
    // Generous, for slow test machines: a debug build, two cores.
    assert!(read.as_secs_f64() < 5.0, "{read:?}");
    assert!(asked.as_secs_f64() < 2.0, "{asked:?}");
    // On a plain sheet, the border's corner is an end.
    let plain = Document::open(sample::sheets(1, SHEET.0, SHEET.1, 4)).unwrap();
    let corner = Snaps::of(&plain, 0)
        .unwrap()
        .nearest(Point::new(19.0, 19.0), 3.0)
        .unwrap();
    assert_eq!(corner.kind, Snap::End);
    assert_eq!(corner.at, Point::new(18.0, 18.0));
}

#[test]
fn a_measurement_is_saved_with_a_standard_scale() {
    let original = sample::sheets(1, SHEET.0, SHEET.1, 0);
    let mut line = square(0, "m");
    line.subtype = Subtype::Line;
    line.vertices = vec![Point::new(0.0, 0.0), Point::new(216.0, 0.0)];
    line.intent = "LineDimension".into();
    line.measure = Some(Measure {
        ratio: "3 in = 12 ft".into(),
        unit: "in".into(),
        factor: 144.0 / 216.0,
    });
    let markups = Markups {
        annotations: vec![line],
        ..Markups::default()
    };
    let saved = annotations::save(&original, &markups).unwrap();
    let read = annotations::read(&saved).unwrap();
    let measure = read.annotations[0].measure.as_ref().unwrap();
    assert_eq!(measure.unit, "in");
    assert!((measure.factor - 144.0 / 216.0).abs() < 1e-6);
    assert_eq!(read.annotations[0].intent, "LineDimension");
}

#[test]
fn a_saved_markup_is_drawn_from_its_appearance() {
    let original = sample::sheets(1, SHEET.0, SHEET.1, 0);
    let markups = Markups {
        annotations: vec![square(0, "a")],
        ..Markups::default()
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
        ..Markups::default()
    };
    assert!(annotations::save(&original, &markups).is_err());
}

#[test]
fn bookmarks_point_at_their_pages() {
    let found = opencabinet_pdf::outline::bookmarks(&sample::bookmarked()).unwrap();
    let seen: Vec<(&str, usize, Option<usize>)> = found
        .iter()
        .map(|b| (b.title.as_str(), b.level, b.page))
        .collect();
    assert_eq!(
        seen,
        [
            ("Plans", 0, None),
            ("A-101 Floor plan", 1, Some(0)),
            ("A-102 Elevación norte", 1, Some(1)),
            ("Details", 0, Some(2)),
        ]
    );
    // A PDF without bookmarks has none.
    let none = opencabinet_pdf::outline::bookmarks(&sample::sheets(1, 612.0, 792.0, 2)).unwrap();
    assert_eq!(none, []);
}

#[test]
fn a_summary_table_is_a_pdf_with_its_pages() {
    use opencabinet_pdf::report::{Row, Table, table};
    let rows = (0..120)
        .map(|i| Row {
            color: Some([0.85, 0.1, 0.1]),
            cells: vec![
                "Cloud".into(),
                (i % 3 + 1).to_string(),
                format!("Comment number {i}, which is long enough to wrap onto a second line in its column for sure"),
                "Andrés".into(),
            ],
        })
        .collect();
    let bytes = table(&Table {
        title: "Markup summary".into(),
        subtitle: "A-201.pdf · 120 markups".into(),
        columns: vec![
            ("Subject".into(), 1.0),
            ("Page".into(), 0.5),
            ("Comments".into(), 4.0),
            ("Author".into(), 1.0),
        ],
        rows,
    })
    .unwrap();
    let document = Document::open(bytes).unwrap();
    assert!(document.pages().len() > 2);
    let last = document.pages().len() - 1;
    let words = opencabinet_pdf::find::find(&document, last, "Comment number 119").unwrap();
    assert_eq!(words.len(), 1);
    let first = opencabinet_pdf::find::find(&document, 0, "Markup summary").unwrap();
    assert_eq!(first.len(), 1);
}
