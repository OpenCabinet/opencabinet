// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Scales and what they measure: lengths, lengths along a path, and areas.

use opencabinet_model::{Length, Units};
use opencabinet_pdf::Point;
use opencabinet_pdf::annotations::Measure;

/// A scale: how long a real length is on the sheet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    /// Points on the sheet.
    pub points: f64,
    /// The real length they stand for.
    pub real: Length,
}

/// Nanometers in the units measurements are commonly saved in.
const UNITS: [(&str, f64); 8] = [
    ("in", 25_400_000.0),
    ("ft", 304_800_000.0),
    ("yd", 914_400_000.0),
    ("mi", 1_609_344_000_000.0),
    ("mm", 1_000_000.0),
    ("cm", 10_000_000.0),
    ("m", 1_000_000_000.0),
    ("km", 1_000_000_000_000.0),
];

impl Scale {
    /// The scale that makes the line from `a` to `b` measure `real`;
    /// `None` when the points are the same or the length is not positive.
    #[must_use]
    pub fn from_line(a: Point, b: Point, real: Length) -> Option<Self> {
        let points = a.distance(b);
        (points > f64::EPSILON && real > Length::ZERO).then_some(Self { points, real })
    }

    /// Real nanometers per point.
    #[expect(clippy::cast_precision_loss, reason = "lengths far below 2^52 nm")]
    fn per_point(&self) -> f64 {
        self.real.nm() as f64 / self.points
    }

    /// The real length of `points` on the sheet.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "measured lengths are far inside the range"
    )]
    pub fn length(&self, points: f64) -> Length {
        Length::from_nm((points * self.per_point()).round() as i64)
    }

    /// The real length of the line from `a` to `b`.
    #[must_use]
    pub fn measure(&self, a: Point, b: Point) -> Length {
        self.length(a.distance(b))
    }

    /// The real area of `square_points` on the sheet, in square
    /// millimeters.
    #[must_use]
    pub fn area_mm2(&self, square_points: f64) -> f64 {
        let mm = self.per_point() / 1_000_000.0;
        square_points * mm * mm
    }

    /// The scale of another program's measurement; `None` for a unit it
    /// does not know.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "one inch of sheet is far inside the range"
    )]
    pub fn from_measure(measure: &Measure) -> Option<Self> {
        let unit = measure.unit.trim().to_lowercase();
        let unit = unit.trim_end_matches('.');
        let (_, nm) = UNITS.iter().find(|(name, _)| *name == unit)?;
        let real = Length::from_nm((72.0 * measure.factor * nm).round() as i64);
        (real > Length::ZERO).then_some(Self { points: 72.0, real })
    }

    /// This scale as a standard measure, in inches or millimeters.
    #[must_use]
    pub fn to_measure(&self, units: Units) -> Measure {
        let (unit, nm) = match units {
            Units::Imperial => ("in", UNITS[0].1),
            Units::Metric => ("mm", UNITS[4].1),
        };
        let inch = self.length(72.0);
        Measure {
            ratio: format!("1 in = {}", shown(inch, units)),
            unit: unit.to_owned(),
            factor: self.per_point() / nm,
        }
    }

    /// The scale as people read it: `1" = 48"`.
    #[must_use]
    pub fn label(&self, units: Units) -> String {
        format!("1\" = {}", shown(self.length(72.0), units))
    }
}

/// How a measured length is shown: to the nearest 1/16" or 0.1 mm.
#[must_use]
pub fn shown(length: Length, units: Units) -> String {
    let precision = match units {
        Units::Imperial => Length::from_inch_fraction(1, 16).unwrap_or(Length::from_mm(1)),
        Units::Metric => Length::from_nm(100_000),
    };
    length.show(units, precision).text
}

/// How an area in square millimeters is shown: square feet or square
/// meters, to two decimals.
#[must_use]
pub fn shown_area(mm2: f64, units: Units) -> String {
    match units {
        Units::Imperial => format!("{:.2} sq ft", mm2 / (304.8 * 304.8)),
        Units::Metric => format!("{:.2} m²", mm2 / 1_000_000.0),
    }
}

/// The length of the path through `points`, on the sheet.
#[must_use]
pub fn path_length(points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .sum()
}

/// The area inside the polygon through `points`, on the sheet.
#[must_use]
pub fn polygon_area(points: &[Point]) -> f64 {
    let n = points.len();
    if n < 3 {
        return 0.0;
    }
    let twice: f64 = (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    twice.abs() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_inch_scale_reads_feet() {
        // 1/4" = 1'-0": 18 points to the foot.
        let scale = Scale::from_line(
            Point::new(0.0, 0.0),
            Point::new(18.0, 0.0),
            Length::from_inches(12),
        )
        .unwrap();
        assert_eq!(scale.length(216.0), Length::from_inches(144));
        assert_eq!(scale.label(Units::Imperial), "1\" = 48\"");
        // A 10' × 12' room: 180 × 216 points.
        let area = scale.area_mm2(180.0 * 216.0);
        assert_eq!(shown_area(area, Units::Imperial), "120.00 sq ft");
        let measure = scale.to_measure(Units::Imperial);
        assert_eq!(measure.unit, "in");
        let back = Scale::from_measure(&measure).unwrap();
        assert_eq!(back.length(216.0), Length::from_inches(144));
    }

    #[test]
    fn other_units_are_read_and_unknown_ones_refused() {
        let measure = |unit: &str, factor: f64| Measure {
            ratio: String::new(),
            unit: unit.into(),
            factor,
        };
        let feet = Scale::from_measure(&measure("ft", 1.0 / 18.0)).unwrap();
        assert_eq!(feet.length(216.0), Length::from_inches(144));
        let meters = Scale::from_measure(&measure("m", 0.05)).unwrap();
        assert_eq!(meters.length(20.0), Length::from_mm(1000));
        assert_eq!(Scale::from_measure(&measure("furlong", 1.0)), None);
    }

    #[test]
    fn paths_and_areas_add_up() {
        let square = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        assert!((path_length(&square) - 30.0).abs() < 1e-12);
        assert!((polygon_area(&square) - 100.0).abs() < 1e-12);
        assert!(polygon_area(&square[..2]).abs() < 1e-12);
    }
}
