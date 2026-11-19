// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes the sample kitchen's elevations as a PDF and as one DXF per wall
//! into a folder, so CI can check them with independent readers.
//!
//! `cargo run -p opencabinet-reports --example elevations -- <out-dir>`

use std::path::{Path, PathBuf};

use opencabinet_model::{Length, Library, Project, Units};
use opencabinet_reports::elevations::elevations_dxf;
use opencabinet_reports::{Fonts, Paper, elevations_pdf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: elevations <out-dir>")?;
    let out = Path::new(&out);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = root.join("fixtures/kitchen.ocab");
    let project: Project =
        serde_json::from_str(&std::fs::read_to_string(fixture.join("project.json"))?)?;
    let library: Library =
        serde_json::from_str(&std::fs::read_to_string(fixture.join("library.json"))?)?;
    let precision = Length::from_inch_fraction(1, 32).ok_or("fraction")?;
    let show = |length: Length| length.show(Units::Imperial, precision).text;
    let fonts = root.join("assets/fonts/noto-sans");
    let fonts = Fonts::new(
        std::fs::read(fonts.join("NotoSans-Regular.ttf"))?,
        std::fs::read(fonts.join("NotoSans-Bold.ttf"))?,
    )?;

    std::fs::create_dir_all(out)?;
    let pdf = elevations_pdf(
        &project,
        &library,
        Units::Imperial,
        &show,
        &project.name,
        "OpenCabinet example",
        Paper::Letter,
        &fonts,
    )?;
    std::fs::write(out.join("elevations.pdf"), pdf)?;
    for file in elevations_dxf(&project, &library, &show)? {
        std::fs::write(out.join(&file.name), file.contents)?;
    }
    Ok(())
}
