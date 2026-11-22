// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A review from opening to saving and opening again.

use opencabinet_model::{Length, Units};
use opencabinet_pdf::annotations;
use opencabinet_pdf::{Point, sample};
use opencabinet_review::{Kind, Markup, Review, Scale, Status, stamps};

fn markup(page: usize, kind: Kind, points: &[(f64, f64)]) -> Markup {
    Markup {
        page,
        kind,
        points: points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
        ..Markup::default()
    }
}

fn sheets() -> Vec<u8> {
    sample::sheets(2, 2592.0, 1728.0, 8)
}

/// 1/4" = 1'-0": 18 points to the foot.
fn quarter_inch() -> Scale {
    Scale::from_line(
        Point::new(0.0, 0.0),
        Point::new(18.0, 0.0),
        Length::from_inches(12),
    )
    .unwrap()
}

#[test]
fn markups_and_scales_survive_saving_and_opening_again() {
    let original = sheets();
    let mut review = Review::open(original.clone()).unwrap();
    assert_eq!(review.markups().len(), 0);

    let mut cloud = markup(
        0,
        Kind::Cloud,
        &[
            (300.0, 300.0),
            (700.0, 300.0),
            (700.0, 500.0),
            (300.0, 500.0),
        ],
    );
    cloud.text = "Confirm 36\" counter height".into();
    cloud.fill = true;
    cloud.status = Status::Accepted;
    let cloud = review.add(cloud, "Andrés");
    let mut note = markup(0, Kind::Text, &[(800.0, 300.0), (1100.0, 400.0)]);
    note.text = "Filler 3\" here\nboth sides".into();
    review.add(note, "María");
    review.add(
        markup(1, Kind::Arrow, &[(100.0, 100.0), (400.0, 250.0)]),
        "Juan",
    );
    review.set_scale(1, quarter_inch());
    // 12' along, then 10' up: 22'.
    let path = review.add(
        markup(
            1,
            Kind::Path,
            &[(500.0, 500.0), (716.0, 500.0), (716.0, 680.0)],
        ),
        "Juan",
    );
    // A 10' × 12' room.
    let area = review.add(
        markup(
            1,
            Kind::Area,
            &[(0.0, 0.0), (216.0, 0.0), (216.0, 180.0), (0.0, 180.0)],
        ),
        "Juan",
    );
    let count = review.add(
        markup(1, Kind::Count, &[(10.0, 10.0), (50.0, 10.0), (90.0, 10.0)]),
        "Juan",
    );
    let shown = |review: &Review, id: &str| {
        review
            .measured(review.markup(id).unwrap(), Units::Imperial)
            .unwrap()
    };
    assert_eq!(shown(&review, &path), "22'-0\"");
    assert_eq!(shown(&review, &area), "120.00 sq ft");
    assert_eq!(shown(&review, &count), "3");
    assert!(review.modified());

    let saved = review.save(Units::Imperial).unwrap();
    assert!(!review.modified());
    assert!(saved.starts_with(&original), "the original bytes stay");

    let reopened = Review::open(saved.clone()).unwrap();
    let same = |m: &Markup| Markup {
        origin: None,
        ..m.clone()
    };
    assert_eq!(
        reopened.markups().iter().map(same).collect::<Vec<_>>(),
        review.markups().iter().map(same).collect::<Vec<_>>()
    );
    assert_eq!(reopened.scale(1), Some(quarter_inch()));
    assert_eq!(shown(&reopened, &area), "120.00 sq ft");
    let cloud = reopened.markup(&cloud).unwrap();
    assert_eq!(cloud.author, "Andrés");
    assert!(cloud.modified.starts_with("D:20"));

    // Measurements carry a standard scale for other programs.
    let read = annotations::read(&saved).unwrap();
    let path = read.annotations.iter().find(|a| a.name == path).unwrap();
    assert_eq!(path.intent, "PolyLineDimension");
    assert!(path.measure.is_some());
}

#[test]
fn another_programs_markups_are_shown_and_kept() {
    let original = sample::foreign_markups();
    let mut review = Review::open(original.clone()).unwrap();
    // The cloud and the measured line become markups.
    assert_eq!(review.markups().len(), 2);
    let line = review.markups()[1].clone();
    assert_eq!(line.kind, Kind::Length);
    // Its own scale, 1/4" = 1'-0", although the page has none.
    assert_eq!(review.scale(0), None);
    assert_eq!(
        review.measured(&line, Units::Imperial).as_deref(),
        Some("12'-0\"")
    );
    // Untouched, they are drawn from their own look, with the page.
    assert!(!review.draws(&line));
    let (page, first) = review.page_document().unwrap();
    assert_eq!(
        annotations::read(page.bytes()).unwrap().annotations.len(),
        2
    );

    // Saved untouched, the file is unchanged.
    assert_eq!(review.save(Units::Imperial).unwrap(), original);

    // Moving the cloud keeps the other program's look and keys.
    let cloud = review.markups()[0].id.clone();
    review.checkpoint();
    review.change_quietly(&cloud, |m| m.shift(20.0, 0.0));
    // Moved, Review draws it, and the page leaves it out.
    assert!(review.draws(&review.markups()[0].clone()));
    let (page, second) = review.page_document().unwrap();
    assert_eq!(
        annotations::read(page.bytes()).unwrap().annotations.len(),
        1
    );
    assert_ne!(first, second);
    let saved = review.save(Units::Imperial).unwrap();
    let read = annotations::read(&saved).unwrap();
    assert_eq!(read.annotations[0].rect, [110.0, 90.0, 330.0, 210.0]);
    assert_eq!(read.annotations[0].subject, "Cloud+");
}

#[test]
fn every_step_can_be_undone_and_redone() {
    let mut review = Review::open(sheets()).unwrap();
    assert!(!review.can_undo());
    let line = review.add(markup(0, Kind::Line, &[(0.0, 0.0), (100.0, 0.0)]), "A");
    review.change(&line, |m| m.color = [0.0, 0.0, 1.0]);
    // Typing is one step, however many keys.
    review.set_text(&line, "C");
    review.set_text(&line, "Ch");
    review.set_text(&line, "Check");
    review.remove(&line);
    assert_eq!(review.markups().len(), 0);

    review.undo();
    assert_eq!(review.markup(&line).unwrap().text, "Check");
    review.undo();
    assert_eq!(review.markup(&line).unwrap().text, "");
    review.undo();
    assert_eq!(review.markup(&line).unwrap().color, [0.85, 0.1, 0.1]);
    review.undo();
    assert_eq!(review.markups().len(), 0);
    assert!(!review.can_undo());
    assert!(!review.modified());

    review.redo();
    review.redo();
    assert_eq!(review.markup(&line).unwrap().color, [0.0, 0.0, 1.0]);
    assert!(review.can_redo());
    // A new step forgets what could be redone.
    review.set_scale(0, quarter_inch());
    assert!(!review.can_redo());
}

#[test]
fn markups_are_found_moved_and_removed() {
    let mut review = Review::open(sheets()).unwrap();
    let line = review.add(markup(0, Kind::Line, &[(0.0, 0.0), (100.0, 0.0)]), "A");
    let rectangle = review.add(
        markup(0, Kind::Rectangle, &[(40.0, -20.0), (60.0, 20.0)]),
        "A",
    );
    // The rectangle is on top.
    assert_eq!(
        review.hit(0, Point::new(50.0, 0.0), 2.0).unwrap().id,
        rectangle
    );
    assert_eq!(review.hit(0, Point::new(10.0, 1.0), 2.0).unwrap().id, line);
    assert!(review.hit(1, Point::new(10.0, 1.0), 2.0).is_none());

    review.change(&line, |m| m.shift(5.0, 5.0));
    assert_eq!(
        review.markup(&line).unwrap().points[0],
        Point::new(5.0, 5.0)
    );
    review.remove(&rectangle);
    assert_eq!(review.markups().len(), 1);
}

#[test]
fn a_stamp_says_its_words_and_who_stamped_it() {
    let mut review = Review::open(sheets()).unwrap();
    let template = stamps::TEMPLATES[1];
    let mut stamp = markup(0, Kind::Stamp, &[(100.0, 100.0), (316.0, 164.0)]);
    stamp.text = template.words.into();
    stamp.color = template.color;
    let id = review.add(stamp, "Andrés");
    let stamp = review.markup(&id).unwrap();
    let captions = stamp.captions(None);
    assert_eq!(captions[0].text, "APPROVED AS NOTED");
    assert!(captions[1].text.starts_with("Andrés · 20"));
    let saved = review.save(Units::Imperial).unwrap();
    let read = annotations::read(&saved).unwrap();
    assert_eq!(read.annotations[0].icon, "ApprovedAsNoted");
    assert_eq!(read.annotations[0].contents, "APPROVED AS NOTED");
}

#[test]
fn the_markup_list_is_written_for_spreadsheets() {
    let mut review = Review::open(sheets()).unwrap();
    let mut cloud = markup(
        0,
        Kind::Cloud,
        &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
    );
    cloud.text = "Check \"A\", then B".into();
    cloud.subject = "Phase 2".into();
    review.add(cloud, "Andrés");
    review.add(markup(1, Kind::Length, &[(0.0, 0.0), (72.0, 0.0)]), "Juan");
    let csv = review.csv(Units::Metric);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(
        lines[0],
        "Page,Type,Subject,Status,Author,Modified,Text,Measurement"
    );
    assert!(
        lines[1].starts_with("1,Cloud,Phase 2,Open,Andrés,20"),
        "{}",
        lines[1]
    );
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

#[test]
fn a_flattened_copy_draws_the_markups_into_the_page() {
    let original = sample::foreign_markups();
    let mut review = Review::open(original).unwrap();
    review.add(
        markup(0, Kind::Rectangle, &[(1000.0, 1000.0), (1200.0, 1100.0)]),
        "Andrés",
    );
    let flat = review.flattened(Units::Imperial).unwrap();
    assert!(review.modified(), "the review itself is not saved");
    // No markups left to edit; the link stays.
    assert_eq!(annotations::read(&flat).unwrap().annotations.len(), 0);
    let document = opencabinet_pdf::Document::open(flat).unwrap();
    // The rectangle's top edge, y = 1100 in PDF space, drawn red.
    let view = opencabinet_pdf::View {
        page: 0,
        zoom: 1.0,
        origin: Point::new(0.0, 0.0),
        width: 1400,
        height: 1728,
    };
    let red = opencabinet_pdf::render(&document, view)
        .unwrap()
        .pixel(1100, 1728 - 1100);
    assert!(red[0] > 150 && red[1] < 90, "{red:?}");
}
