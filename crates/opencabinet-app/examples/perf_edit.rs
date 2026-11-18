// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Measures the response to an edit (ADR-0014): the time from typing a new
//! width to the window's state being updated, without drawing it. Prints
//! the median in nanoseconds over many edits.
//!
//! `cargo run --release -p opencabinet-app --example perf_edit`

use std::time::{Duration, Instant};

const EDITS: usize = 201;

fn main() {
    i_slint_backend_testing::init_no_event_loop();
    let window = opencabinet_app::create().expect("the window is created");
    let mut times: Vec<Duration> = (0..EDITS)
        .map(|i| {
            let text = if i % 2 == 0 { "30 1/2" } else { "2'-7 3/16\"" };
            let start = Instant::now();
            window.invoke_width_edited(text.into());
            start.elapsed()
        })
        .collect();
    // The edits really went through: the last one is the one shown.
    assert_eq!(window.get_width_reading(), "= 30-1/2\"");
    times.sort();
    println!("{}", times[EDITS / 2].as_nanos());
}
