// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Starts OpenCabinet Review.

// No console window behind the application on Windows.
#![windows_subsystem = "windows"]

use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    // The PDFs given on the command line, or opened with Review from the
    // system's file manager, each in a tab.
    let paths: Vec<std::path::PathBuf> = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .collect();
    let window = opencabinet_review_app::create_all(&paths)?;
    window.run()
}
