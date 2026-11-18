// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exact angles in integer microdegrees (RFC-0001 §2).

use std::fmt;

use serde::{Deserialize, Serialize};

/// An angle: a signed count of microdegrees, counterclockwise seen from
/// above. 90° is `90_000_000`. Every angle shops use, such as 45°, 22.5°
/// or 15°, is exact, and rotations are as deterministic as lengths.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Angle(i64);

impl Angle {
    /// No rotation.
    pub const ZERO: Self = Self(0);
    /// Microdegrees in a degree.
    pub const MICRODEGREES_PER_DEGREE: i64 = 1_000_000;
    /// A full turn, 360°.
    pub const FULL_TURN: Self = Self(360 * Self::MICRODEGREES_PER_DEGREE);

    /// An angle of `microdegrees` millionths of a degree.
    #[must_use]
    pub const fn from_microdegrees(microdegrees: i64) -> Self {
        Self(microdegrees)
    }

    /// A whole number of degrees.
    #[must_use]
    pub const fn from_degrees(degrees: i64) -> Self {
        Self(degrees * Self::MICRODEGREES_PER_DEGREE)
    }

    /// The angle in microdegrees, as stored in files.
    #[must_use]
    pub const fn microdegrees(self) -> i64 {
        self.0
    }

    /// The same direction within [0°, 360°), the range placements use.
    #[must_use]
    pub const fn normalized(self) -> Self {
        Self(self.0.rem_euclid(Self::FULL_TURN.0))
    }
}

impl fmt::Display for Angle {
    /// Degrees with only the decimals they need: `90°`, `22.5°`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let micro = self.0.unsigned_abs();
        let per_degree = Self::MICRODEGREES_PER_DEGREE.unsigned_abs();
        let (whole, rest) = (micro / per_degree, micro % per_degree);
        if rest == 0 {
            write!(f, "{sign}{whole}°")
        } else {
            let decimals = format!("{rest:06}");
            write!(f, "{sign}{whole}.{}°", decimals.trim_end_matches('0'))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_angles_are_exact() {
        assert_eq!(Angle::from_degrees(90).microdegrees(), 90_000_000);
        assert_eq!(Angle::from_microdegrees(22_500_000).to_string(), "22.5°");
        assert_eq!(Angle::from_degrees(-45).to_string(), "-45°");
    }

    #[test]
    fn normalizes_into_one_turn() {
        assert_eq!(
            Angle::from_degrees(-90).normalized(),
            Angle::from_degrees(270)
        );
        assert_eq!(
            Angle::from_degrees(450).normalized(),
            Angle::from_degrees(90)
        );
        assert_eq!(Angle::FULL_TURN.normalized(), Angle::ZERO);
    }

    #[test]
    fn serializes_as_a_plain_integer() {
        let json = serde_json::to_string(&Angle::from_degrees(90)).unwrap();
        assert_eq!(json, "90000000");
    }
}
