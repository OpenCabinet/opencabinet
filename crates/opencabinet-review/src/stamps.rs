// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The stamps Review comes with: the words a reviewer answers a drawing
//! with, each in a color of its own. A stamp's words can be changed after
//! it is placed, so a shop makes its own from these.

use crate::Color;

/// A stamp to place: its words and color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Template {
    /// What it says.
    pub words: &'static str,
    /// Its color.
    pub color: Color,
}

const GREEN: Color = [0.1, 0.5, 0.2];
const BLUE: Color = [0.1, 0.3, 0.75];
const ORANGE: Color = [0.85, 0.45, 0.0];
const RED: Color = [0.8, 0.1, 0.1];
const GRAY: Color = [0.35, 0.35, 0.35];

/// The stamps, in the order the window offers them.
pub const TEMPLATES: [Template; 8] = [
    Template {
        words: "APPROVED",
        color: GREEN,
    },
    Template {
        words: "APPROVED AS NOTED",
        color: BLUE,
    },
    Template {
        words: "REVISE AND RESUBMIT",
        color: ORANGE,
    },
    Template {
        words: "REJECTED",
        color: RED,
    },
    Template {
        words: "REVIEWED",
        color: BLUE,
    },
    Template {
        words: "FOR INFORMATION ONLY",
        color: GRAY,
    },
    Template {
        words: "NOT FOR CONSTRUCTION",
        color: RED,
    },
    Template {
        words: "FIELD VERIFY",
        color: ORANGE,
    },
];

/// A stamp's size when placed with a click, in points.
pub const SIZE: (f64, f64) = (216.0, 64.0);
