// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! OpenCabinet Review's session server (RFC-0004 §7): holds review
//! sessions for people who are not on the same network.
//!
//! ```text
//! opencabinet-review-server --address review.example.org [--port 47100] [--data DIR]
//! ```
//!
//! It prints the server address people paste into Review's Session panel
//! (`ocs1:…`). Its certificate is kept in the data folder, so the address
//! stays the same between runs; keep that folder private.
//!
//! This program is free software under the GNU Affero General Public
//! License, version 3 or later: if you run it for others, you must offer
//! them its source, with your changes.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use opencabinet_session::PORT;
use opencabinet_session::server::{Identity, Server};

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
struct Options {
    address: String,
    port: u16,
    data: PathBuf,
}

const USAGE: &str = "usage: opencabinet-review-server --address NAME [--port PORT] [--data DIR]

  --address NAME  the name or IPv4 address people reach this server at
  --port PORT     the port to listen on (default 47100)
  --data DIR      where the server keeps its certificate (default ./review-server)";

fn options(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut address = None;
    let mut port = PORT;
    let mut data = PathBuf::from("review-server");
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--address" => address = Some(value()?),
            "--port" => port = value()?.parse().map_err(|_| "--port needs a number")?,
            "--data" => data = PathBuf::from(value()?),
            "--help" | "-h" => return Err(String::new()),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(Options {
        address: address.ok_or("--address is needed")?,
        port,
        data,
    })
}

fn main() -> ExitCode {
    let options = match options(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(why) => {
            if !why.is_empty() {
                eprintln!("{why}\n");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let started = Identity::kept_in(&options.data).and_then(|identity| {
        Server::start(
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, options.port)),
            &options.address,
            &identity,
        )
    });
    let server = match started {
        Ok(server) => server,
        Err(why) => {
            eprintln!("Cannot start: {why}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "OpenCabinet Review session server {}",
        env!("CARGO_PKG_VERSION")
    );
    println!("Server address, for Review's Session panel:");
    println!("{}", server.address);
    println!("Source: {}", env!("CARGO_PKG_REPOSITORY"));
    loop {
        std::thread::sleep(Duration::from_secs(60));
        println!("{} rooms", server.rooms());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn the_command_line_is_read() {
        assert_eq!(
            options(args(
                "--address review.example.org --port 9000 --data /srv/r"
            ))
            .unwrap(),
            Options {
                address: "review.example.org".into(),
                port: 9000,
                data: PathBuf::from("/srv/r"),
            }
        );
        assert_eq!(options(args("--address 10.0.0.2")).unwrap().port, PORT);
        assert!(options(args("--port 9000")).is_err());
        assert!(options(args("--address a --port x")).is_err());
        assert!(options(args("--what")).is_err());
    }
}
