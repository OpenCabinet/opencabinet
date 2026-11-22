// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Live review sessions for OpenCabinet Review (RFC-0004 §6).

mod net;
mod shared;

pub use net::{Guest, Host, Invite, Link};
pub use shared::{Activity, Said, Shared};
