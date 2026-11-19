// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bundled font (ADR-0015). It is installed as separate files next to
//! the program, not compiled into it, and read when a PDF is made.

use std::path::{Path, PathBuf};

use opencabinet_reports::Fonts;

const REGULAR: &str = "NotoSans-Regular.ttf";
const BOLD: &str = "NotoSans-Bold.ttf";

/// Reads the bundled font: from the `fonts` folder next to the program, or,
/// when running from the source tree, from `assets/fonts/noto-sans`.
///
/// # Errors
/// A message for the user when the font files are missing or damaged.
pub fn load() -> Result<Fonts, String> {
    let dir = folders()
        .into_iter()
        .find(|dir| dir.join(REGULAR).is_file())
        .ok_or("The font files are missing; reinstall OpenCabinet Designer")?;
    let read = |name: &str| {
        std::fs::read(dir.join(name)).map_err(|e| format!("Cannot read the font {name}: {e}"))
    };
    Fonts::new(read(REGULAR)?, read(BOLD)?).map_err(|e| format!("The font files are damaged: {e}"))
}

fn folders() -> Vec<PathBuf> {
    let installed = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("fonts")));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/noto-sans");
    installed.into_iter().chain([source]).collect()
}
