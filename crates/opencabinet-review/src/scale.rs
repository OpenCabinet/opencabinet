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
        let nm = unit_nm(&measure.unit)?;
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
        match units {
            Units::Imperial => {
                // An architect's scale: so much sheet to a foot, when that
                // is a plain fraction of an inch.
                let sheet = self.sheet_for(Length::from_inches(12));
                let fine = Length::from_inch_fraction(1, 64).unwrap_or(Length::from_mm(1));
                let shown_sheet = sheet.show(Units::Imperial, fine);
                if shown_sheet.exact && sheet <= Length::from_inches(12) {
                    format!("{} = 1'-0\"", shown_sheet.text.replace('-', " "))
                } else {
                    format!("1\" = {}", shown(self.length(72.0), units))
                }
            }
            Units::Metric => {
                let ratio = self.ratio();
                if (ratio - ratio.round()).abs() < 0.01 && ratio >= 1.0 {
                    format!("1:{}", ratio.round())
                } else {
                    format!("1 mm = {}", shown(self.length(72.0 / 25.4), units))
                }
            }
        }
    }

    /// How long `real` is on the sheet.
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "lengths far inside both ranges"
    )]
    fn sheet_for(&self, real: Length) -> Length {
        let points = self.points * real.nm() as f64 / self.real.nm() as f64;
        Length::from_nm((points / 72.0 * Length::NM_PER_INCH as f64).round() as i64)
    }

    /// Real length per length on the sheet.
    #[expect(clippy::cast_precision_loss, reason = "lengths far below 2^52 nm")]
    fn ratio(&self) -> f64 {
        self.real.nm() as f64 / (self.points / 72.0 * Length::NM_PER_INCH as f64)
    }

    /// A scale where `sheet` on paper stands for `real`.
    #[must_use]
    #[expect(clippy::cast_precision_loss, reason = "lengths far below 2^52 nm")]
    pub fn of(sheet: Length, real: Length) -> Self {
        Self {
            points: sheet.nm() as f64 / Length::NM_PER_INCH as f64 * 72.0,
            real,
        }
    }
}

/// The common scales: architects' and engineers' in imperial units, and
/// the metric ratios, with how they are written.
#[must_use]
pub fn presets() -> Vec<(String, Scale)> {
    let inch = |n: i64, d: i64| Length::from_inch_fraction(n, d).unwrap_or(Length::ZERO);
    let foot = Length::from_inches(12);
    let mut out = Vec::new();
    for (n, d) in [
        (1, 16),
        (3, 32),
        (1, 8),
        (3, 16),
        (1, 4),
        (3, 8),
        (1, 2),
        (3, 4),
        (1, 1),
        (3, 2),
        (3, 1),
    ] {
        let scale = Scale::of(inch(n, d), foot);
        out.push((scale.label(Units::Imperial), scale));
    }
    for feet in [10, 20, 30, 40, 50, 60, 100] {
        out.push((
            format!("1\" = {feet}'"),
            Scale::of(inch(1, 1), Length::from_inches(12 * feet)),
        ));
    }
    for ratio in [1, 2, 5, 10, 20, 25, 50, 100, 200, 500] {
        out.push((
            format!("1:{ratio}"),
            Scale::of(Length::from_mm(1), Length::from_mm(ratio)),
        ));
    }
    out
}

/// Nanometers in a unit as programs write it: `ft`, `ft'`, `'`, `feet`,
/// `in`, `"`, `inches`, `mm`, `m`, and so on.
fn unit_nm(unit: &str) -> Option<f64> {
    let unit = unit.trim().to_lowercase();
    let unit = unit.trim_end_matches('.');
    let name = match unit {
        "'" | "ft'" | "feet" | "foot" | "ft-in" | "ft' in\"" | "ft'-in\"" => "ft",
        "\"" | "in\"" | "inch" | "inches" => "in",
        "yard" | "yards" => "yd",
        "mile" | "miles" => "mi",
        "millimeter" | "millimeters" | "millimetre" | "millimetres" => "mm",
        "centimeter" | "centimeters" | "centimetre" | "centimetres" => "cm",
        "meter" | "meters" | "metre" | "metres" => "m",
        "kilometer" | "kilometers" | "kilometre" | "kilometres" => "km",
        other => other,
    };
    UNITS.iter().find(|(n, _)| *n == name).map(|&(_, nm)| nm)
}

/// How a measured length is shown: to the nearest 1/16" or 0.1 mm.
#[must_use]
pub fn shown(length: Length, units: Units) -> String {
    match units {
        Units::Imperial => feet_and_inches(length),
        Units::Metric => length.show(units, Length::from_nm(100_000)).text,
    }
}

/// A length in feet and inches to the nearest 1/16", as drawings write
/// it: `12'-6 1/2"`, or `6 1/2"` under a foot.
fn feet_and_inches(length: Length) -> String {
    let sixteenth = Length::from_inch_fraction(1, 16).unwrap_or(Length::from_mm(1));
    let negative = length < Length::ZERO;
    let nm = length.nm().abs();
    let steps = (nm + sixteenth.nm() / 2) / sixteenth.nm();
    let (feet, rest) = (steps / 192, steps % 192);
    let inches = Length::from_nm(rest * sixteenth.nm())
        .show(Units::Imperial, sixteenth)
        .text
        .replacen('-', " ", 1);
    let sign = if negative { "-" } else { "" };
    if feet == 0 {
        format!("{sign}{inches}")
    } else {
        format!("{sign}{feet}'-{inches}")
    }
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

/// The length around the closed shape through `points`, on the sheet.
#[must_use]
pub fn perimeter(points: &[Point]) -> f64 {
    match points {
        [first, .., last] if points.len() > 2 => path_length(points) + last.distance(*first),
        _ => path_length(points),
    }
}

/// The angle at the second of `points`, between the first and the third,
/// in degrees from 0 to 180; `None` until there are three distinct points.
#[must_use]
pub fn angle(points: &[Point]) -> Option<f64> {
    let [a, corner, b, ..] = points else {
        return None;
    };
    let (u, v) = (
        (a.x - corner.x, a.y - corner.y),
        (b.x - corner.x, b.y - corner.y),
    );
    let (lu, lv) = (u.0.hypot(u.1), v.0.hypot(v.1));
    if lu < f64::EPSILON || lv < f64::EPSILON {
        return None;
    }
    let cos = ((u.0 * v.0 + u.1 * v.1) / (lu * lv)).clamp(-1.0, 1.0);
    Some(cos.acos().to_degrees())
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
        assert_eq!(scale.label(Units::Imperial), "1/4\" = 1'-0\"");
        assert_eq!(shown(scale.length(216.0), Units::Imperial), "12'-0\"");
        assert_eq!(
            shown(Length::from_inch_fraction(301, 2).unwrap(), Units::Imperial),
            "12'-6 1/2\""
        );
        assert_eq!(
            shown(Length::from_inch_fraction(13, 2).unwrap(), Units::Imperial),
            "6 1/2\""
        );
        // A 10' × 12' room: 180 × 216 points.
        let area = scale.area_mm2(180.0 * 216.0);
        assert_eq!(shown_area(area, Units::Imperial), "120.00 sq ft");
        let measure = scale.to_measure(Units::Imperial);
        assert_eq!(measure.unit, "in");
        let back = Scale::from_measure(&measure).unwrap();
        assert_eq!(back.length(216.0), Length::from_inches(144));
    }

    #[test]
    fn the_common_scales_are_offered_by_name() {
        let presets = presets();
        let named = |name: &str| presets.iter().find(|(n, _)| n == name).map(|(_, s)| *s);
        let quarter = named("1/4\" = 1'-0\"").unwrap();
        assert!((quarter.length(18.0).nm() - Length::from_inches(12).nm()).abs() < 2);
        assert!(named("1 1/2\" = 1'-0\"").is_some());
        assert!(named("1\" = 20'").is_some());
        assert_eq!(named("1:50").unwrap().label(Units::Metric), "1:50");
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
        let quoted = Scale::from_measure(&measure("ft'", 1.0 / 18.0)).unwrap();
        assert_eq!(quoted.length(216.0), Length::from_inches(144));
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
