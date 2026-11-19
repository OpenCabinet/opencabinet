// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Stable identities of authored entities (RFC-0001, invariant 4).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The immutable identity of an authored entity: a UUID, version 7 for
/// every entity this software creates. References always use it, never
/// names or positions in a list.
///
/// Reading accepts any UUID, so a file with a foreign identifier still
/// opens; validation reports identifiers that are not version 7.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(Uuid);

/// The text is not a UUID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseIdError;

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a UUID")
    }
}

impl std::error::Error for ParseIdError {}

impl Id {
    /// A new identifier: a UUIDv7, which sorts by creation time.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// The identifier for an existing UUID, for example one read from a
    /// file or written in a test.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// The underlying UUID.
    #[must_use]
    pub const fn uuid(self) -> Uuid {
        self.0
    }

    /// Whether this is a version 7 UUID, as authored entities require.
    #[must_use]
    pub fn is_v7(self) -> bool {
        self.0.get_version_num() == 7
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl FromStr for Id {
    type Err = ParseIdError;

    fn from_str(text: &str) -> Result<Self, ParseIdError> {
        Uuid::try_parse(text).map(Self).map_err(|_| ParseIdError)
    }
}

impl fmt::Display for Id {
    /// The hyphenated lowercase form used in files.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ids_are_v7_and_increase() {
        let (first, second) = (Id::new(), Id::new());
        assert!(first.is_v7() && second.is_v7());
        assert!(first < second);
    }

    #[test]
    fn parses_and_shows_the_file_form() {
        let text = "0199f8a0-0000-7000-8000-000000000001";
        let id: Id = text.parse().unwrap();
        assert!(id.is_v7());
        assert_eq!(id.to_string(), text);
        assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{text}\""));
        assert_eq!("not-a-uuid".parse::<Id>(), Err(ParseIdError));
        assert!(!Id::from_uuid(Uuid::nil()).is_v7());
    }
}
