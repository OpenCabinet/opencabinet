// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A review from opening to saving and opening again.

use opencabinet_model::{Length, Units};
use opencabinet_pdf::{Point, sample};
use opencabinet_review::{Kind, Markup, Review, Scale};

fn markup(page: usize, kind: Kind, points: &[(f64, f64)]) -> Markup {
    Markup {
        id: String::new(),
        page,
        kind,
        points: points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
        color: [0.85, 0.1, 0.1],
        width: 2.0,
        text: String::new(),
        author: String::new(),
        modified: String::new(),
    }
}

fn sheets() -> Vec<u8> {
    sample::sheets(2, 2592.0, 1728.0, 8)
}

#[test]
fn markups_and_scales_survive_saving_and_opening_again() {
    let original = sheets();
    let mut review = Review::open(original.clone()).unwrap();
    assert_eq!(review.markups().len(), 0);

    let mut cloud = markup(0, Kind::Cloud, &[(300.0, 300.0), (700.0, 500.0)]);
    cloud.text = "Confirm 36\" counter height".into();
    let cloud = review.add(cloud, "Andrés");
    let mut note = markup(0, Kind::Text, &[(800.0, 300.0), (1100.0, 400.0)]);
    note.text = "Filler 3\" here\nboth sides".into();
    review.add(note, "María");
    review.add(
        markup(1, Kind::Arrow, &[(100.0, 100.0), (400.0, 250.0)]),
        "Juan",
    );
    review.add(
        markup(1, Kind::Pen, &[(10.0, 10.0), (20.0, 40.0), (60.0, 45.0)]),
        "Juan",
    );
    // 1/4" = 1'-0": 12' is 3" on the sheet, 216 points.
    let scale = Scale::from_line(
        Point::new(0.0, 0.0),
        Point::new(216.0, 0.0),
        Length::from_inches(144),
    )
    .unwrap();
    review.set_scale(1, scale);
    let length = review.add(
        markup(1, Kind::Length, &[(500.0, 500.0), (500.0, 608.0)]),
        "Juan",
    );
    let shown = review.measured(review.markup(&length).unwrap(), Units::Imperial);
    assert_eq!(shown.as_deref(), Some("72\""));
    assert!(review.modified());

    let saved = review.save(Units::Imperial).unwrap();
    assert!(!review.modified());
    assert!(saved.starts_with(&original), "the original bytes stay");

    let reopened = Review::open(saved).unwrap();
    assert_eq!(reopened.markups(), review.markups());
    assert_eq!(reopened.scale(1), Some(scale));
    assert_eq!(reopened.scale(0), None);
    let cloud = reopened.markup(&cloud).unwrap();
    assert_eq!(cloud.author, "Andrés");
    assert!(cloud.modified.starts_with("D:20"));
}

#[test]
fn the_page_is_drawn_without_the_markups_review_edits() {
    let mut review = Review::open(sheets()).unwrap();
    review.add(
        markup(0, Kind::Rectangle, &[(100.0, 100.0), (300.0, 200.0)]),
        "Andrés",
    );
    let saved = review.save(Units::Imperial).unwrap();
    let reopened = Review::open(saved).unwrap();
    let annotations = opencabinet_pdf::annotations::read(reopened.document().bytes()).unwrap();
    assert_eq!(annotations.annotations.len(), 0);
    assert_eq!(reopened.markups().len(), 1);
}

#[test]
fn markups_are_found_moved_changed_and_removed() {
    let mut review = Review::open(sheets()).unwrap();
    let line = review.add(markup(0, Kind::Line, &[(0.0, 0.0), (100.0, 0.0)]), "A");
    let box_ = review.add(
        markup(0, Kind::Rectangle, &[(40.0, -20.0), (60.0, 20.0)]),
        "A",
    );
    // The rectangle is on top.
    assert_eq!(review.hit(0, Point::new(50.0, 0.0), 2.0).unwrap().id, box_);
    assert_eq!(review.hit(0, Point::new(10.0, 1.0), 2.0).unwrap().id, line);
    assert!(review.hit(1, Point::new(10.0, 1.0), 2.0).is_none());

    review.change(&line, |m| m.shift(5.0, 5.0));
    assert_eq!(
        review.markup(&line).unwrap().points[0],
        Point::new(5.0, 5.0)
    );
    review.remove(&box_);
    assert_eq!(review.markups().len(), 1);
}

#[test]
fn the_markup_list_is_written_for_spreadsheets() {
    let mut review = Review::open(sheets()).unwrap();
    let mut cloud = markup(0, Kind::Cloud, &[(0.0, 0.0), (10.0, 10.0)]);
    cloud.text = "Check \"A\", then B".into();
    review.add(cloud, "Andrés");
    review.add(markup(1, Kind::Length, &[(0.0, 0.0), (72.0, 0.0)]), "Juan");
    let csv = review.csv(Units::Metric);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "Page,Type,Author,Modified,Text,Measurement");
    assert!(lines[1].starts_with("1,Cloud,Andrés,20"), "{}", lines[1]);
    assert!(
        lines[1].ends_with(",\"Check \"\"A\"\", then B\","),
        "{}",
        lines[1]
    );
    assert!(lines[2].ends_with(",Not to scale"), "{}", lines[2]);
}

#[test]
fn a_scale_needs_two_points_and_a_length() {
    let a = Point::new(5.0, 5.0);
    assert!(Scale::from_line(a, a, Length::from_inches(1)).is_none());
    assert!(Scale::from_line(a, Point::new(6.0, 5.0), Length::ZERO).is_none());
    // 1:50 in metric: 1 mm on the sheet, 50 mm real.
    let mm = 72.0 / 25.4;
    let scale = Scale::from_line(a, Point::new(5.0 + mm, 5.0), Length::from_mm(50)).unwrap();
    let measured = scale.measure(Point::new(0.0, 0.0), Point::new(0.0, 20.0 * mm));
    assert_eq!(
        opencabinet_review::shown(measured, Units::Metric),
        "1000 mm"
    );
}
