// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Compiles the `.slint` interface into Rust.

fn main() {
    // The headless tests find elements through debug info; release builds
    // leave it out to stay small (ADR-0014).
    let debug = std::env::var("PROFILE").is_ok_and(|profile| profile == "debug");
    let config = slint_build::CompilerConfiguration::new().with_debug_info(debug);
    slint_build::compile_with_config("ui/app.slint", config).expect("the interface compiles");
}
