// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Starts OpenCabinet Designer.

// No console window behind the application on Windows.
#![windows_subsystem = "windows"]

use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    let window = opencabinet_app::create()?;
    opencabinet_app::perf::watch_first_frame(&window);
    window.run()
}
