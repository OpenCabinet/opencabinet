// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes the sample kitchen's parts as DXF files into a folder, so CI can
//! check them with an independent reader.
//!
//! `cargo run -p opencabinet-reports --example parts_dxf -- <out-dir>`

use std::path::{Path, PathBuf};

use opencabinet_model::{Library, Project};
use opencabinet_reports::parts_dxf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: parts_dxf <out-dir>")?;
    let out = Path::new(&out);
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kitchen.ocab");
    let project: Project =
        serde_json::from_str(&std::fs::read_to_string(fixture.join("project.json"))?)?;
    let library: Library =
        serde_json::from_str(&std::fs::read_to_string(fixture.join("library.json"))?)?;
    std::fs::create_dir_all(out)?;
    for file in parts_dxf(&project, &library)? {
        std::fs::write(out.join(&file.name), file.contents)?;
    }
    Ok(())
}
