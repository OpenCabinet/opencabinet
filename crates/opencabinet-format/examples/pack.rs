// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Packs a project kept as a folder, such as the sample kitchen in
//! `fixtures/`, into a single `.ocab` file people can open.
//!
//! `cargo run -p opencabinet-format --example pack -- <folder> <out.ocab>`

use std::path::Path;

use opencabinet_format::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(folder), Some(out)) = (args.next(), args.next()) else {
        return Err("usage: pack <folder> <out.ocab>".into());
    };
    let document = Document::load(Path::new(&folder))?;
    document.save_zip(Path::new(&out))?;
    Ok(())
}
