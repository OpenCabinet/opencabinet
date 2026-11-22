// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Live review sessions for OpenCabinet Review (RFC-0004 §6).

pub mod firewall;
mod net;
mod shared;

pub use net::{Guest, Host, Invite, Link, PORT, local_addresses};
pub use shared::{Activity, Said, Shared};
