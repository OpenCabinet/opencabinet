// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt;

/// Everything that can make a project unreadable or unwritable.
#[derive(Debug)]
pub enum Error {
    /// The file system failed.
    Io(std::io::Error),
    /// The ZIP container is damaged.
    Zip(zip::result::ZipError),
    /// An entry is not the JSON it should be.
    Json(serde_json::Error),
    /// The file is not a valid project file; the text says why.
    InvalidContainer(String),
    /// The file needs a newer major version of the format.
    UnsupportedVersion(String),
}

/// The result of reading or writing a project file.
pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Zip(e) => write!(f, "ZIP error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::InvalidContainer(msg) => write!(f, "invalid .ocab container: {msg}"),
            Self::UnsupportedVersion(v) => {
                write!(f, "format version {v} needs a newer OpenCabinet")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Zip(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
