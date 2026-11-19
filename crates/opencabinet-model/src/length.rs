// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exact lengths in integer nanometers, as ADR-0009 decides.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// A length: a signed count of nanometers.
///
/// One inch is exactly 25,400,000 nm, so every fraction of an inch down to
/// 1/64" and every metric value down to a millionth of a millimeter is an
/// exact integer. Addition and subtraction are exact; division rounds half
/// to even, the one rounding rule of the model.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Length(i64);

/// The unit system a user enters and reads lengths in. It never changes
/// stored values (RFC-0001, invariant 8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Units {
    /// Inches and feet with fractions: the default (ADR-0008).
    #[default]
    Imperial,
    /// Millimeters.
    Metric,
}

/// A length as shown to the user, rounded to a display precision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    /// For example `23-7/32"` or `587.4 mm`.
    pub text: String,
    /// Whether `text` is the exact value. The interface must flag a value
    /// that is not (ADR-0009).
    pub exact: bool,
}

/// Why a typed length could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseLengthError {
    /// Nothing was typed.
    Empty,
    /// The text is not a length.
    Invalid,
    /// A fraction has a zero denominator.
    ZeroDenominator,
    /// The value does not fit in the model's range.
    OutOfRange,
}

impl fmt::Display for ParseLengthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "no length entered",
            Self::Invalid => "not a length",
            Self::ZeroDenominator => "a fraction cannot have a zero denominator",
            Self::OutOfRange => "length too large",
        })
    }
}

impl std::error::Error for ParseLengthError {}

const NM_PER_FOOT: i64 = 12 * Length::NM_PER_INCH;

impl Length {
    /// No length.
    pub const ZERO: Self = Self(0);
    /// Nanometers in a millimeter.
    pub const NM_PER_MM: i64 = 1_000_000;
    /// Nanometers in an inch: exact by the 1959 international agreement.
    pub const NM_PER_INCH: i64 = 25_400_000;

    /// A length of `nm` nanometers.
    #[must_use]
    pub const fn from_nm(nm: i64) -> Self {
        Self(nm)
    }

    /// The length in nanometers, as stored in files.
    #[must_use]
    pub const fn nm(self) -> i64 {
        self.0
    }

    /// A whole number of millimeters.
    #[must_use]
    pub const fn from_mm(mm: i64) -> Self {
        Self(mm * Self::NM_PER_MM)
    }

    /// A whole number of inches.
    #[must_use]
    pub const fn from_inches(inches: i64) -> Self {
        Self(inches * Self::NM_PER_INCH)
    }

    /// `numerator / denominator` inches, rounded half to even when it is
    /// not a whole number of nanometers (finer than 1/64"). `None` for a
    /// zero denominator or a value out of range.
    #[must_use]
    pub fn from_inch_fraction(numerator: i64, denominator: i64) -> Option<Self> {
        if denominator == 0 {
            return None;
        }
        let nm = div_round_half_even(
            i128::from(numerator) * i128::from(Self::NM_PER_INCH),
            i128::from(denominator),
        );
        i64::try_from(nm).ok().map(Self)
    }

    /// This length divided by `divisor`, rounded half to even.
    ///
    /// # Panics
    /// If `divisor` is zero.
    #[must_use]
    pub fn div_round(self, divisor: i64) -> Self {
        assert!(divisor != 0, "division of a length by zero");
        let nm = div_round_half_even(i128::from(self.0), i128::from(divisor));
        Self(i64::try_from(nm).expect("a quotient is never larger than its dividend"))
    }

    /// Splits this length into `parts` lengths that add up to it exactly.
    /// The nanometers left over go one each to the first parts, so the
    /// result is deterministic.
    ///
    /// # Panics
    /// If `parts` is zero.
    #[must_use]
    pub fn split(self, parts: u32) -> Vec<Self> {
        assert!(parts != 0, "splitting a length into zero parts");
        let n = i64::from(parts);
        let (base, remainder) = (self.0.div_euclid(n), self.0.rem_euclid(n));
        (0..n)
            .map(|i| Self(base + i64::from(i < remainder)))
            .collect()
    }

    /// Reads a length as a person types it.
    ///
    /// Inches accept whole numbers, decimals and fractions, with or without
    /// the inch mark: `30`, `30.5`, `30-1/2`, `30 1/2"`, `1/2in`. Feet come
    /// first, with `'` or `ft`: `8'`, `2' 6"`, `2'-6 1/2"`. Metric values
    /// take a unit: `762 mm`, `76.2 cm`, `2.4 m`. A bare number uses
    /// `default_units`: inches for imperial, millimeters for metric.
    ///
    /// # Errors
    /// When the text is empty, not a length, divides by zero or is out of
    /// range.
    pub fn parse(input: &str, default_units: Units) -> Result<Self, ParseLengthError> {
        let text = input.trim();
        if text.is_empty() {
            return Err(ParseLengthError::Empty);
        }
        let (negative, text) = match text.strip_prefix('-') {
            Some(rest) => (true, rest.trim_start()),
            None => (false, text),
        };
        let nm = parse_unsigned(&text.to_ascii_lowercase(), default_units)?;
        let nm = if negative { -nm } else { nm };
        i64::try_from(nm)
            .map(Self)
            .map_err(|_| ParseLengthError::OutOfRange)
    }

    /// Shows this length rounded to `precision`, which must be positive.
    ///
    /// Imperial lengths show inches. With a precision of 1/2, 1/4, … 1/64"
    /// they show a fraction in lowest terms, such as `23-7/32"`; with
    /// 1/10, 1/100, … of an inch, decimals, such as `23.22"`; with any other
    /// precision, inches to four decimals. Metric lengths show millimeters
    /// with only the decimals they need, such as `587.4 mm`.
    ///
    /// # Panics
    /// If `precision` is not positive.
    #[must_use]
    pub fn show(self, units: Units, precision: Self) -> Shown {
        assert!(precision.0 > 0, "display precision must be positive");
        let steps = div_round_half_even(i128::from(self.0), i128::from(precision.0));
        let exact = steps * i128::from(precision.0) == i128::from(self.0);
        let rounded = steps * i128::from(precision.0);
        let per_inch = (Self::NM_PER_INCH % precision.0 == 0)
            .then(|| i128::from(Self::NM_PER_INCH / precision.0));
        let (text, exact) = match (units, per_inch) {
            (Units::Imperial, Some(d)) if d.count_ones() == 1 => {
                (fraction_text(steps, d) + "\"", exact)
            }
            (Units::Imperial, Some(d)) if is_power_of_ten(d) => {
                (decimal_text(steps, d) + "\"", exact)
            }
            (Units::Imperial, _) => {
                let ten_thousandths =
                    div_round_half_even(rounded * 10_000, i128::from(Self::NM_PER_INCH));
                let text = decimal_text(ten_thousandths, 10_000) + "\"";
                (
                    text,
                    exact && ten_thousandths * i128::from(Self::NM_PER_INCH) == rounded * 10_000,
                )
            }
            (Units::Metric, _) => (
                decimal_text(rounded, i128::from(Self::NM_PER_MM)) + " mm",
                exact,
            ),
        };
        Shown { text, exact }
    }
}

impl fmt::Display for Length {
    /// The raw value, for logs and debugging: `762000000 nm`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} nm", self.0)
    }
}

impl Add for Length {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl AddAssign for Length {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl Sub for Length {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl SubAssign for Length {
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0;
    }
}

impl Neg for Length {
    type Output = Self;

    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Mul<i64> for Length {
    type Output = Self;

    fn mul(self, factor: i64) -> Self {
        Self(self.0 * factor)
    }
}

impl Sum for Length {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Add::add)
    }
}

/// `numerator / denominator` rounded to the nearest integer, ties to even.
/// `denominator` may be negative but not zero.
fn div_round_half_even(numerator: i128, denominator: i128) -> i128 {
    let (n, d) = if denominator < 0 {
        (-numerator, -denominator)
    } else {
        (numerator, denominator)
    };
    let (quotient, remainder) = (n.div_euclid(d), n.rem_euclid(d));
    match (2 * remainder).cmp(&d) {
        std::cmp::Ordering::Less => quotient,
        std::cmp::Ordering::Greater => quotient + 1,
        std::cmp::Ordering::Equal => quotient + (quotient & 1),
    }
}

/// Parses a length without its sign, in nanometers.
fn parse_unsigned(text: &str, default_units: Units) -> Result<i128, ParseLengthError> {
    for (suffix, nm_per_unit) in [
        ("mm", Length::NM_PER_MM),
        ("cm", 10 * Length::NM_PER_MM),
        ("m", 1000 * Length::NM_PER_MM),
    ] {
        if let Some(number) = text.strip_suffix(suffix) {
            return decimal(number.trim_end(), nm_per_unit);
        }
    }

    let feet_mark = text
        .find('\'')
        .map(|i| (i, 1))
        .or_else(|| text.find("ft").map(|i| (i, 2)));
    if let Some((at, mark_len)) = feet_mark {
        let feet = decimal(text[..at].trim_end(), NM_PER_FOOT)?;
        // What follows the feet is inches, optionally after a dash: 2'-6".
        let rest = text[at + mark_len..].trim_start();
        let rest = rest.strip_prefix('-').unwrap_or(rest).trim_start();
        let inches = if rest.is_empty() { 0 } else { inches(rest)? };
        return Ok(feet + inches);
    }

    let explicit_inches = text.ends_with('"') || text.ends_with("in");
    match default_units {
        Units::Metric if !explicit_inches => decimal(text, Length::NM_PER_MM),
        _ => inches(text),
    }
}

/// Parses inches: `30`, `30.5`, `30-1/2`, `30 1/2`, `1/2`, with an optional
/// trailing `"` or `in`.
fn inches(text: &str) -> Result<i128, ParseLengthError> {
    let text = text
        .strip_suffix('"')
        .or_else(|| text.strip_suffix("in"))
        .unwrap_or(text)
        .trim_end();
    let Some((before, denominator)) = text.split_once('/') else {
        return decimal(text, Length::NM_PER_INCH);
    };
    let denominator = digits(denominator.trim())?;
    if denominator == 0 {
        return Err(ParseLengthError::ZeroDenominator);
    }
    // The whole number and the numerator are separated by a dash or spaces.
    let before = before.trim_end();
    let (whole, numerator) = match before.rfind(['-', ' ']) {
        Some(at) => (digits(before[..at].trim_end())?, digits(&before[at + 1..])?),
        None => (0, digits(before)?),
    };
    let total = (whole * denominator + numerator) * i128::from(Length::NM_PER_INCH);
    Ok(div_round_half_even(total, denominator))
}

/// Parses an unsigned decimal number of units, such as `30.5`, in
/// nanometers, rounded half to even beyond a nanometer.
fn decimal(text: &str, nm_per_unit: i64) -> Result<i128, ParseLengthError> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() && fraction.is_empty() {
        return Err(ParseLengthError::Invalid);
    }
    // Eighteen decimals are already far below a nanometer.
    if fraction.len() > 18 {
        return Err(ParseLengthError::Invalid);
    }
    let whole = if whole.is_empty() { 0 } else { digits(whole)? };
    let scale = 10_i128.pow(u32::try_from(fraction.len()).expect("at most 18"));
    let fraction = if fraction.is_empty() {
        0
    } else {
        digits(fraction)?
    };
    let units = whole
        .checked_mul(scale)
        .and_then(|w| w.checked_add(fraction))
        .and_then(|u| u.checked_mul(i128::from(nm_per_unit)))
        .ok_or(ParseLengthError::OutOfRange)?;
    Ok(div_round_half_even(units, scale))
}

/// Parses ASCII digits only: no sign, no spaces.
fn digits(text: &str) -> Result<i128, ParseLengthError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseLengthError::Invalid);
    }
    text.parse().map_err(|_| ParseLengthError::OutOfRange)
}

/// `steps` of `1/denominator` inch, as `-23-7/32`.
fn fraction_text(steps: i128, denominator: i128) -> String {
    let sign = if steps < 0 { "-" } else { "" };
    let steps = steps.abs();
    let (whole, rest) = (steps / denominator, steps % denominator);
    if rest == 0 {
        return format!("{sign}{whole}");
    }
    let common = gcd(rest, denominator);
    let (numerator, denominator) = (rest / common, denominator / common);
    if whole == 0 {
        format!("{sign}{numerator}/{denominator}")
    } else {
        format!("{sign}{whole}-{numerator}/{denominator}")
    }
}

/// `count / scale` in decimal, with only the decimals it needs: `587.4`,
/// `24`. `scale` must be a power of ten.
fn decimal_text(count: i128, scale: i128) -> String {
    let sign = if count < 0 { "-" } else { "" };
    let count = count.abs();
    let (whole, rest) = (count / scale, count % scale);
    if rest == 0 {
        return format!("{sign}{whole}");
    }
    let width = scale.to_string().len() - 1;
    let decimals = format!("{rest:0width$}");
    format!("{sign}{whole}.{}", decimals.trim_end_matches('0'))
}

fn is_power_of_ten(mut n: i128) -> bool {
    while n > 1 && n % 10 == 0 {
        n /= 10;
    }
    n == 1
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIXTY_FOURTH: Length = Length::from_nm(396_875);
    const THIRTY_SECOND: Length = Length::from_nm(793_750);

    fn inch(text: &str) -> Length {
        Length::parse(text, Units::Imperial).unwrap()
    }

    #[test]
    fn every_sixty_fourth_round_trips_exactly() {
        for steps in -256..=256_i64 {
            let length = SIXTY_FOURTH * steps;
            let shown = length.show(Units::Imperial, SIXTY_FOURTH);
            assert!(shown.exact, "{}", shown.text);
            assert_eq!(inch(&shown.text), length, "{}", shown.text);
        }
    }

    #[test]
    fn reads_the_ways_shops_write_inches() {
        let thirty_and_a_half = Length::from_nm(774_700_000);
        for text in [
            "30.5",
            "30-1/2",
            "30 1/2",
            "30 1/2\"",
            "30-1/2 in",
            " 30  1/2 ",
        ] {
            assert_eq!(inch(text), thirty_and_a_half, "{text}");
        }
        assert_eq!(inch("1/2\""), Length::from_nm(12_700_000));
        assert_eq!(inch("-3/4"), Length::from_nm(-19_050_000));
        assert_eq!(inch(".75"), Length::from_nm(19_050_000));
        assert_eq!(inch("24"), Length::from_inches(24));
    }

    #[test]
    fn reads_feet_and_inches() {
        assert_eq!(inch("8'"), Length::from_inches(96));
        assert_eq!(inch("2' 6\""), Length::from_inches(30));
        assert_eq!(inch("2'-6 1/2\""), inch("30.5"));
        assert_eq!(inch("1.5ft"), Length::from_inches(18));
    }

    #[test]
    fn reads_metric_with_a_unit_or_by_default() {
        let metric = |text| Length::parse(text, Units::Metric).unwrap();
        assert_eq!(metric("762"), Length::from_mm(762));
        assert_eq!(metric("587.4"), Length::from_nm(587_400_000));
        assert_eq!(inch("762 mm"), Length::from_mm(762));
        assert_eq!(inch("76.2cm"), Length::from_mm(762));
        assert_eq!(inch("2.4 m"), Length::from_mm(2400));
        // An inch mark wins over the default units.
        assert_eq!(metric("30\""), Length::from_inches(30));
    }

    #[test]
    fn rejects_what_is_not_a_length() {
        use ParseLengthError::{Empty, Invalid, OutOfRange, ZeroDenominator};
        let parse = |text| Length::parse(text, Units::Imperial);
        assert_eq!(parse("  "), Err(Empty));
        for text in [
            "abc", "1/", "/2", ".", "1.2.3", "1-", "--1", "1/2/3", "1 x/2",
        ] {
            assert_eq!(parse(text), Err(Invalid), "{text}");
        }
        // A trailing point is still a whole number.
        assert_eq!(parse("3."), Ok(Length::from_inches(3)));
        assert_eq!(parse("1/0"), Err(ZeroDenominator));
        assert_eq!(parse("99999999999999 ft"), Err(OutOfRange));
    }

    #[test]
    fn finer_than_a_nanometer_rounds_half_to_even() {
        // 1/128" is 198,437.5 nm: the tie goes to the even neighbor.
        assert_eq!(
            Length::from_inch_fraction(1, 128),
            Some(Length::from_nm(198_438))
        );
        assert_eq!(
            Length::from_inch_fraction(3, 128),
            Some(Length::from_nm(595_312))
        );
        assert_eq!(Length::from_inch_fraction(1, 0), None);
        assert_eq!(Length::from_nm(5).div_round(2), Length::from_nm(2));
        assert_eq!(Length::from_nm(7).div_round(2), Length::from_nm(4));
        assert_eq!(Length::from_nm(-7).div_round(2), Length::from_nm(-4));
        assert_eq!(Length::from_nm(7).div_round(-2), Length::from_nm(-4));
    }

    #[test]
    fn splits_add_up_to_the_whole() {
        let width = Length::from_nm(1_000_000_001);
        let doors = width.split(3);
        assert_eq!(
            doors,
            [333_333_334, 333_333_334, 333_333_333].map(Length::from_nm)
        );
        assert_eq!(doors.into_iter().sum::<Length>(), width);
        let negative = Length::from_nm(-7).split(2);
        assert_eq!(
            negative.iter().copied().sum::<Length>(),
            Length::from_nm(-7)
        );
    }

    #[test]
    fn shows_rounded_values_and_flags_them() {
        let length = inch("23-7/32");
        let at_32nds = length.show(Units::Imperial, THIRTY_SECOND);
        assert_eq!(
            at_32nds,
            Shown {
                text: "23-7/32\"".into(),
                exact: true
            }
        );
        // 7/32 is 3.5 sixteenths: the tie goes to 4/16, reduced to 1/4.
        let sixteenth = Length::from_inch_fraction(1, 16).unwrap();
        let at_16ths = length.show(Units::Imperial, sixteenth);
        assert_eq!(
            at_16ths,
            Shown {
                text: "23-1/4\"".into(),
                exact: false
            }
        );
        let tenth_mm = Length::from_nm(100_000);
        assert_eq!(length.show(Units::Metric, tenth_mm).text, "589.8 mm");
        assert_eq!(
            Length::from_mm(762).show(Units::Metric, tenth_mm).text,
            "762 mm"
        );
        assert_eq!(
            Length::ZERO.show(Units::Imperial, THIRTY_SECOND).text,
            "0\""
        );
        assert_eq!(
            (-inch("1/2")).show(Units::Imperial, THIRTY_SECOND).text,
            "-1/2\""
        );
        // Hundredths of an inch show decimals; a metric precision shown in
        // inches shows four decimals.
        let hundredth = Length::from_nm(254_000);
        assert_eq!(
            inch("23-7/32").show(Units::Imperial, hundredth).text,
            "23.22\""
        );
        let shown = Length::from_mm(10).show(Units::Imperial, Length::from_mm(1));
        assert_eq!(
            shown,
            Shown {
                text: "0.3937\"".into(),
                exact: false
            }
        );
    }

    #[test]
    fn serializes_as_a_plain_integer() {
        let json = serde_json::to_string(&Length::from_inches(30)).unwrap();
        assert_eq!(json, "762000000");
        assert_eq!(
            serde_json::from_str::<Length>(&json).unwrap(),
            Length::from_inches(30)
        );
        assert_eq!(serde_json::to_string(&Units::Metric).unwrap(), "\"metric\"");
    }
}
