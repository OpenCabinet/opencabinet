// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading and writing `.ocab` project files (RFC-0001 §13).
//!
//! A project file is a ZIP container or an unpacked directory with the same
//! entries:
//!
//! ```text
//! kitchen.ocab
//! ├── mimetype        application/vnd.opencabinet.project+zip, first and uncompressed
//! ├── manifest.json   formatVersion, generator, entries with media types
//! ├── project.json    the project
//! ├── library.json    the library snapshots
//! └── …               attachments and plugin data, kept byte for byte
//! ```
//!
//! Saving is deterministic, so a project saved twice is byte for byte the
//! same, and diffs cleanly in version control.

mod container;
mod error;

pub use container::{Attachment, CURRENT, Document, FormatVersion, MIMETYPE};
pub use error::{Error, Result};
