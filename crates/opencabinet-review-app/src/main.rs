// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Starts OpenCabinet Review.

// No console window behind the application on Windows.
#![windows_subsystem = "windows"]

use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    // A PDF given on the command line, or opened with Review from the
    // system's file manager.
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let window = opencabinet_review_app::create(path.as_deref())?;
    window.run()
}
