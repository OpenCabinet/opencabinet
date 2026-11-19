// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Machining operations on parts (RFC-0001 §9): machine-independent
//! descriptions that a manual shop reads as a drilling guide and a CNC
//! post-processor translates for its machine. They never hold G-code.

use opencabinet_model::Length;

/// A face of a part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// The face toward the inside of the cabinet: the inside of a side,
    /// the top of a bottom, the back of a door.
    A,
    /// The other face.
    B,
}

/// A point on a face of a part: `x` along its length from its `x0` end,
/// `y` across its width from its `y0` edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    /// Along the length.
    pub x: Length,
    /// Across the width.
    pub y: Length,
}

/// One machining operation on a part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    /// A hole bored into a face.
    Hole {
        /// The face it is bored into.
        face: Face,
        /// Its center.
        at: Point,
        /// Its diameter.
        diameter: Length,
        /// How deep it goes.
        depth: Length,
    },
    /// A row of equal holes, `step` apart: line boring.
    HolePattern {
        /// The face they are bored into.
        face: Face,
        /// The center of the first hole.
        start: Point,
        /// From one hole's center to the next.
        step: Point,
        /// How many holes.
        count: u32,
        /// Their diameter.
        diameter: Length,
        /// How deep they go.
        depth: Length,
    },
    /// A groove or dado, cut along a straight line.
    Groove {
        /// The face it is cut into.
        face: Face,
        /// Where its center line starts.
        start: Point,
        /// Where its center line ends.
        end: Point,
        /// Its width.
        width: Length,
        /// How deep it goes.
        depth: Length,
    },
}

/// A groove on face A the whole `length` of a part, its center line `y`
/// across it.
pub(crate) fn groove_along(length: Length, y: Length, width: Length, depth: Length) -> Operation {
    Operation::Groove {
        face: Face::A,
        start: Point { x: Length::ZERO, y },
        end: Point { x: length, y },
        width,
        depth,
    }
}

/// Where `count` hinges go along a door `height` tall: the first and last
/// `end` from its ends, the others evenly between them, from the bottom.
/// `count` is at least two.
pub(crate) fn hinge_positions(height: Length, end: Length, count: u32) -> Vec<Length> {
    let mut at = end;
    let mut positions = vec![at];
    for step in (height - end * 2).split(count - 1) {
        at += step;
        positions.push(at);
    }
    positions
}

/// The holes of a grid that starts at `origin` and repeats every `spacing`
/// which fall between `low` and `high`, both included: the first one and
/// how many there are.
pub(crate) fn holes_between(
    origin: Length,
    spacing: Length,
    low: Length,
    high: Length,
) -> Option<(Length, u32)> {
    let step = spacing.nm();
    if step <= 0 || high < low {
        return None;
    }
    // The first grid line at or above `low`, the last at or below `high`.
    let first =
        (low - origin).nm().div_euclid(step) + i64::from((low - origin).nm().rem_euclid(step) != 0);
    let last = (high - origin).nm().div_euclid(step);
    let count = u32::try_from(last - first + 1).ok().filter(|&c| c > 0)?;
    Some((origin + Length::from_nm(first * step), count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mm(value: i64) -> Length {
        Length::from_mm(value)
    }

    #[test]
    fn hinges_sit_at_the_ends_and_evenly_between() {
        assert_eq!(hinge_positions(mm(700), mm(100), 2), [mm(100), mm(600)]);
        assert_eq!(
            hinge_positions(mm(1300), mm(100), 3),
            [mm(100), mm(650), mm(1200)]
        );
        // 1000 nm in three steps: the remainder goes to the first ones.
        let nm = Length::from_nm;
        assert_eq!(
            hinge_positions(nm(1200), nm(100), 4),
            [nm(100), nm(434), nm(767), nm(1100)]
        );
    }

    #[test]
    fn only_the_grid_holes_inside_the_range_are_bored() {
        // A grid from 18 every 32: 82, 114, ... 562 fall in 68.8 to 571.5.
        let um = |value: i64| Length::from_nm(value * 1_000);
        assert_eq!(
            holes_between(mm(18), mm(32), um(68_800), um(571_500)),
            Some((mm(82), 16))
        );
        // A range on grid lines includes them.
        assert_eq!(
            holes_between(mm(18), mm(32), mm(50), mm(114)),
            Some((mm(50), 3))
        );
        // Too short for any hole.
        assert_eq!(holes_between(mm(18), mm(32), mm(51), mm(81)), None);
        assert_eq!(holes_between(mm(18), mm(32), mm(90), mm(60)), None);
    }
}
