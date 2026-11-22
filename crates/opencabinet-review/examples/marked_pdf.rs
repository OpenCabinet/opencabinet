// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes a sheet set with one markup of every kind, for independent PDF
//! readers to check in CI: `cargo run -p opencabinet-review --example
//! marked_pdf -- marked.pdf`. It prints how many markups it wrote.

use opencabinet_model::{Length, Units};
use opencabinet_pdf::{Point, sample};
use opencabinet_review::{Gesture, Kind, Markup, Review, Scale, stamps};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "marked.pdf".into());
    let mut review = Review::open(sample::sheets(2, 2592.0, 1728.0, 12)).expect("our sheets open");
    let scale = Scale::from_line(
        Point::new(0.0, 0.0),
        Point::new(216.0, 0.0),
        Length::from_inches(144),
    )
    .expect("a real scale");
    review.set_scale(0, scale);
    for (n, kind) in Kind::ALL.into_iter().enumerate() {
        let n = f64::from(u8::try_from(n).unwrap_or(0));
        let (x, y) = (
            150.0 + 280.0 * (n % 8.0),
            1300.0 - 300.0 * (n / 8.0).floor(),
        );
        let p = |dx: f64, dy: f64| Point::new(x + dx, y + dy);
        let points = match (kind, kind.gesture()) {
            (Kind::Pen, _) => vec![p(0.0, 0.0), p(60.0, 100.0), p(140.0, 50.0), p(200.0, 160.0)],
            (Kind::Cloud, _) => vec![p(0.0, 0.0), p(200.0, 0.0), p(200.0, 160.0), p(0.0, 160.0)],
            (Kind::Callout, _) => vec![p(0.0, 0.0), p(60.0, 80.0), p(220.0, 160.0)],
            (_, Gesture::Clicks) => {
                vec![p(0.0, 0.0), p(200.0, 0.0), p(160.0, 160.0), p(20.0, 120.0)]
            }
            (_, Gesture::Click) => vec![p(0.0, 160.0)],
            (_, Gesture::Drag) => vec![p(0.0, 0.0), p(200.0, 160.0)],
        };
        let template = stamps::TEMPLATES[1];
        let markup = Markup {
            page: 0,
            kind,
            points,
            fill: kind.fillable(),
            color: if kind == Kind::Stamp {
                template.color
            } else {
                [0.85, 0.1, 0.1]
            },
            text: if kind == Kind::Stamp {
                template.words.into()
            } else {
                format!("{} — Andrés", kind.label())
            },
            ..Markup::default()
        };
        review.add(markup, "Andrés");
    }
    let bytes = review.save(Units::Imperial).expect("the review saves");
    std::fs::write(&path, bytes).expect("the file is written");
    println!("{}", review.markups().len());
}
