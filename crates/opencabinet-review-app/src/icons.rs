// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tools' icons, drawn for OpenCabinet Review: SVG path commands on a
//! 24 × 24 grid, stroked, and filled when they say so.

use opencabinet_review::Kind;

use crate::viewer::Tool;

/// A tool's icon, and whether it is filled.
#[must_use]
pub fn icon(tool: Tool) -> (&'static str, bool) {
    match tool {
        Tool::Select => (
            "M 6 3 L 6 19 L 10 15 L 13 21 L 15.5 20 L 12.5 14 L 18 14 Z",
            true,
        ),
        Tool::Calibrate => (
            "M 3 15 L 15 3 L 21 9 L 9 21 Z M 7 11 L 9 13 M 10 8 L 12 10 M 13 5 L 15 7",
            false,
        ),
        Tool::Draw(kind) => match kind {
            Kind::Cloud => (
                "M 5 9 Q 5 5 9 6 Q 11 3 14 5.5 Q 18 4 18.5 8 Q 22 9 20 13 Q 21.5 17 17 17 \
                 Q 15 20 12 18 Q 8 20 7 16.5 Q 2.5 15.5 4.5 12 Q 3 10 5 9 Z",
                false,
            ),
            Kind::Rectangle => ("M 4 6 L 20 6 L 20 18 L 4 18 Z", false),
            Kind::Ellipse => ("M 3 12 A 9 6.5 0 1 0 21 12 A 9 6.5 0 1 0 3 12 Z", false),
            Kind::Polygon => ("M 5 19 L 3.5 9 L 12 4 L 20.5 9.5 L 17 19.5 Z", false),
            Kind::Arrow => ("M 4 20 L 19 5 M 11 5 L 19 5 L 19 13", false),
            Kind::Line => ("M 4 20 L 20 4", false),
            Kind::Polyline => ("M 3 19 L 9 8 L 14 15 L 21 5", false),
            Kind::Pen => ("M 3 17 C 6 6 10 21 14 12 C 16 7 19 6 21 9", false),
            Kind::Highlight => ("M 3 8 L 21 8 L 21 16 L 3 16 Z", true),
            Kind::Text => ("M 5 5 L 19 5 M 12 5 L 12 20 M 9 20 L 15 20", false),
            Kind::Callout => (
                "M 9 3 L 21 3 L 21 12 L 9 12 Z M 11 12 L 3 21 M 3 21 L 4 16 M 3 21 L 8 20",
                false,
            ),
            Kind::Note => (
                "M 5 3 L 15 3 L 19 7 L 19 21 L 5 21 Z M 8 9 L 16 9 M 8 12.5 L 16 12.5 M 8 16 L 13 16",
                false,
            ),
            Kind::Stamp => (
                "M 9.5 3 L 14.5 3 L 13.5 11 L 20 12.5 L 20 16 L 4 16 L 4 12.5 L 10.5 11 Z M 4 20 L 20 20",
                false,
            ),
            Kind::Length => ("M 3 12 L 21 12 M 3 8 L 3 16 M 21 8 L 21 16", false),
            Kind::Path => (
                "M 3 18 L 9 9 L 15 15 L 21 6 M 2 16 L 4 20 M 20 4 L 22 8",
                false,
            ),
            Kind::Area => ("M 4 6 L 18 4 L 20 18 L 6 20 Z", true),
            Kind::Count => (
                "M 4 7 A 2.5 2.5 0 1 0 9 7 A 2.5 2.5 0 1 0 4 7 Z \
                 M 15 7 A 2.5 2.5 0 1 0 20 7 A 2.5 2.5 0 1 0 15 7 Z \
                 M 9.5 17 A 2.5 2.5 0 1 0 14.5 17 A 2.5 2.5 0 1 0 9.5 17 Z",
                true,
            ),
            Kind::Angle => ("M 20 19 L 4 19 L 15 5 M 10 19 A 6 6 0 0 0 7.6 14.4", false),
            Kind::Link => (
                "M 10 14 L 14 10 M 9 7 L 11 5 A 3.5 3.5 0 0 1 19 13 L 17 15 \
                 M 15 17 L 13 19 A 3.5 3.5 0 0 1 5 11 L 7 9",
                false,
            ),
            Kind::Perimeter => (
                "M 4 6 L 18 4 L 20 18 L 6 20 Z M 2 4 L 6 2 M 18 22 L 22 20",
                false,
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::TOOLS;

    #[test]
    fn every_tool_has_an_icon_of_its_own() {
        let icons: Vec<&str> = TOOLS.iter().map(|&t| icon(t).0).collect();
        for (n, icon) in icons.iter().enumerate() {
            assert!(icon.starts_with("M "), "{n}");
            assert_eq!(icons.iter().filter(|i| *i == icon).count(), 1, "{n}");
        }
    }
}
