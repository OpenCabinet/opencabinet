// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes a sheet set whose pages were changed, for independent PDF
//! readers to check in CI: `cargo run -p opencabinet-review --example
//! pages_pdf -- arranged.pdf extracted.pdf`. The arranged one has four
//! pages: the marked sheet turned a quarter clockwise first, a blank page,
//! the other sheet and a letter page from another PDF. The extracted one
//! has the marked sheet alone.

use opencabinet_model::Units;
use opencabinet_pdf::{Point, sample};
use opencabinet_review::{Kind, Markup, Review};

fn main() {
    let mut args = std::env::args().skip(1);
    let arranged = args.next().unwrap_or_else(|| "arranged.pdf".into());
    let extracted = args.next().unwrap_or_else(|| "extracted.pdf".into());
    let units = Units::Imperial;
    let mut review = Review::open(sample::sheets(3, 2592.0, 1728.0, 6)).expect("our sheets open");
    review.add(
        Markup {
            page: 1,
            kind: Kind::Cloud,
            points: vec![
                Point::new(300.0, 300.0),
                Point::new(700.0, 300.0),
                Point::new(700.0, 500.0),
                Point::new(300.0, 500.0),
            ],
            text: "Confirm the opening".into(),
            ..Markup::default()
        },
        "Andrés",
    );
    review.move_pages(&[1], 0, units).expect("the pages move");
    review.turn_pages(&[0], 1, units).expect("the page turns");
    review.remove_pages(&[2], units).expect("a page goes");
    review
        .insert_blank(1, 0, units)
        .expect("a blank page goes in");
    let letter = sample::sheets(1, 612.0, 792.0, 0);
    review
        .insert_pages(3, &letter, &[0], units)
        .expect("a page from another PDF goes in");
    let alone = review.extract(&[0], units).expect("a page comes out");
    std::fs::write(&extracted, alone).expect("the extracted copy is written");
    let bytes = review.save(units).expect("the review saves");
    std::fs::write(&arranged, bytes).expect("the file is written");
}
