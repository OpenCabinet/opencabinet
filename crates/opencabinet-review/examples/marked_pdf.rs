// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes a sheet set with one markup of every kind, for independent PDF
//! readers to check in CI: `cargo run -p opencabinet-review --example
//! marked_pdf -- marked.pdf`.

use opencabinet_model::{Length, Units};
use opencabinet_pdf::{Point, sample};
use opencabinet_review::{Kind, Markup, Review, Scale};

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
        let x = 200.0 + 280.0 * f64::from(u8::try_from(n).unwrap_or(0));
        let points = match kind {
            Kind::Pen => vec![
                Point::new(x, 600.0),
                Point::new(x + 60.0, 700.0),
                Point::new(x + 140.0, 650.0),
                Point::new(x + 200.0, 760.0),
            ],
            _ => vec![Point::new(x, 600.0), Point::new(x + 200.0, 760.0)],
        };
        let markup = Markup {
            id: String::new(),
            page: 0,
            kind,
            points,
            color: [0.85, 0.1, 0.1],
            width: 2.0,
            text: format!("{} — Andrés", kind.label()),
            author: String::new(),
            modified: String::new(),
        };
        review.add(markup, "Andrés");
    }
    let bytes = review.save(Units::Imperial).expect("the review saves");
    std::fs::write(&path, bytes).expect("the file is written");
    println!("{path}");
}
