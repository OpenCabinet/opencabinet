// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks a running session server from outside, as Review would use it:
//! `cargo run -p opencabinet-session --example server_smoke -- ocs1:…`.
//! It starts a session on the server with a drawing made here, joins it
//! as someone else, sees a markup cross, and prints `ok`; it exits with
//! an error otherwise.

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use opencabinet_pdf::{Point, sample};
use opencabinet_review::{Kind, Markup};
use opencabinet_session::{Participant, Shared};

fn main() -> ExitCode {
    let Some(server) = std::env::args().nth(1) else {
        eprintln!("usage: server_smoke ocs1:…");
        return ExitCode::from(2);
    };
    match check(&server) {
        Ok(()) => {
            println!("ok");
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

fn check(server: &str) -> Result<(), String> {
    let pdf = sample::sheets(1, 2592.0, 1728.0, 2);
    let host = Participant::start_on_server(server, Shared::start("Smoke"), pdf.clone(), "Smoke")?;
    let invite = host.invite().ok_or("the server gave no invitation")?;
    let (guest, got) = Participant::join(&invite, "Check")?;
    if got != pdf {
        return Err("the drawing arrived changed".into());
    }
    let cloud = Markup {
        id: "smoke".into(),
        kind: Kind::Cloud,
        points: vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 5.0),
        ],
        ..Markup::default()
    };
    host.link()
        .shared
        .lock()
        .map_err(|_| "the session is gone")?
        .publish(&[cloud], &BTreeMap::new());
    let start = Instant::now();
    while guest
        .link()
        .shared
        .lock()
        .map_err(|_| "the session is gone")?
        .markups()
        .is_empty()
    {
        if start.elapsed() > Duration::from_secs(20) {
            return Err("the markup did not cross the server".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    host.leave();
    guest.leave();
    Ok(())
}
