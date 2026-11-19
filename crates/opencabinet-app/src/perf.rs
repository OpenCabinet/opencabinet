// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Support for the performance measurements of ADR-0014.
//!
//! With the `OPENCABINET_PERF` environment variable set, the program prints
//! [`FIRST_FRAME`] on its standard output once its first frame is on screen,
//! and quits [`HOLD`] later, which leaves time to sample its memory. Without
//! the variable, nothing changes.

use std::io::Write;
use std::time::Duration;

use slint::{ComponentHandle, RenderingState, Timer};

use crate::AppWindow;

/// The line printed once the first frame is on screen.
pub const FIRST_FRAME: &str = "OPENCABINET_FIRST_FRAME";

/// How long the program stays open after its first frame.
pub const HOLD: Duration = Duration::from_secs(2);

/// Starts watching for the first frame when `OPENCABINET_PERF` is set.
pub fn watch_first_frame(window: &AppWindow) {
    if std::env::var_os("OPENCABINET_PERF").is_none() {
        return;
    }
    let mut reported = false;
    let notifier = window.window().set_rendering_notifier(move |state, _| {
        if matches!(state, RenderingState::AfterRendering) && !reported {
            reported = true;
            presented();
        }
    });
    // The software renderer, used without a GPU, has no rendering notifier:
    // the first timer tick runs once the event loop has painted the window.
    if notifier.is_err() {
        Timer::single_shot(Duration::from_millis(1), presented);
    }
}

fn presented() {
    let mut stdout = std::io::stdout();
    // The measurement waits for this line; nothing else can be done if the
    // output is closed.
    writeln!(stdout, "{FIRST_FRAME}").ok();
    stdout.flush().ok();
    Timer::single_shot(HOLD, || {
        slint::quit_event_loop().ok();
    });
}
