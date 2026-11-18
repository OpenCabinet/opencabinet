// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes the sample kitchen's cut list as a PDF, so CI can check it with
//! independent readers.
//!
//! `cargo run -p opencabinet-reports --example cut_list_pdf -- <out.pdf>`

use std::path::PathBuf;

use opencabinet_model::{FrontStyle, Length, Library, Project, Units};
use opencabinet_reports::{Fonts, Paper, bill_of_materials, cut_list, cut_list_pdf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: cut_list_pdf <out.pdf>")?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = root.join("fixtures/kitchen.ocab");
    let mut project: Project =
        serde_json::from_str(&std::fs::read_to_string(fixture.join("project.json"))?)?;
    let library: Library =
        serde_json::from_str(&std::fs::read_to_string(fixture.join("library.json"))?)?;
    // Slab doors, so the cut list shows the doors' and drawer front's sizes.
    if let Some(front) = project.products[0].front.as_mut() {
        front.children[1].insert.as_mut().ok_or("no insert")?.style = Some(FrontStyle::Slab);
    }
    let fonts = root.join("assets/fonts/noto-sans");
    let fonts = Fonts::new(
        std::fs::read(fonts.join("NotoSans-Regular.ttf"))?,
        std::fs::read(fonts.join("NotoSans-Bold.ttf"))?,
    )?;
    let precision = Length::from_inch_fraction(1, 32).ok_or("fraction")?;
    let list = cut_list(&project, &library, Units::Imperial, precision);
    let materials = bill_of_materials(&project, &library, Units::Imperial, precision);
    let pdf = cut_list_pdf(
        &list,
        &materials,
        &project.name,
        "OpenCabinet example",
        Paper::Letter,
        &fonts,
    )?;
    std::fs::write(out, pdf)?;
    Ok(())
}
