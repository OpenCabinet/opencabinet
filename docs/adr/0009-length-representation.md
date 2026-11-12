<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# 0009 — Represent lengths as integer nanometers

- **Status:** proposed
- **Date:** 2026-10-25
- **Deciders:** Andrés (@A-newGP)

## Context and problem statement

[ADR-0008](0008-imperial-default-units.md) makes fractional inches the
default and metric a first-class option. Every dimension in the data model
([RFC-0001](../rfcs/0001-project-data-model.md)) is a length, and cut lists
must be exact: a panel entered as 23-7/32" must be stored, computed, saved
and displayed as 23-7/32", on every machine, forever. How are lengths
represented in the model and in files?

## Decision drivers

- **Exactness for user input:** fractional inches down to 1/64" and metric
  values down to 0.001 mm must be stored without rounding.
- **Determinism:** the same project produces the same cut list bit for bit on
  every platform and language (required for reproducible tests and for
  comparing revisions).
- **Interoperability:** other software reading the open file format must
  understand values without a special library.
- **Simplicity and speed:** lengths are the most common value in the model.

## Considered options

1. Floating point (`f64`) in inches
2. Floating point (`f64`) in millimeters
3. Decimal floating point (`decimal128`)
4. Exact rational numbers (numerator/denominator)
5. Integer count of a base unit of 1/16000 mm
6. **Integer count of nanometers (`i64`)**

## Decision outcome

Chosen option: **6 — a length is a signed 64-bit integer count of
nanometers.**

- 1 inch is exactly 25,400,000 nm, so every fraction down to 1/64"
  (396,875 nm) is an exact integer; so is every metric value down to
  0.000001 mm.
- Range: ±9.2 million km, far beyond any project.
- Files store lengths as JSON integers in nanometers; the unit is
  self-explanatory to anyone implementing the format.
- Addition and subtraction are exact. Operations that divide (centering,
  splitting a width among several doors) round to the nearest nanometer
  using a single documented rule (round half to even), and the evaluator
  distributes remainders deterministically so that parts always add up to
  the whole.
- Display and input are separate concerns: values are rounded only when
  shown, to the precision the user chooses (for example the nearest 1/32"
  or 0.1 mm), and the user interface must flag a value that is not exact at
  the displayed precision.
- Areas, volumes and costs are **not** lengths: they use their own types
  (for example `f64` square meters for material yield), because squaring
  nanometers overflows 64 bits for large panels and those values do not
  need to be exact.
- Angles follow the same idea: integer **microdegrees** (90° is
  90,000,000), exact for every angle shops use and deterministic, as
  RFC-0001 §2 describes. v0.1 needs them only to rotate a placed product in
  plan.

### Consequences

- Good, because user input is exact in both unit systems, and arithmetic is
  integer arithmetic: fast, portable and deterministic in every language.
- Good, because the file format is trivial to read correctly.
- Bad, because 1/128" and finer fractions are not exact (1/128" =
  198,437.5 nm). Shop dimensions are not specified that finely; such values
  round by half a nanometer, which is invisible at any display precision.
- Bad, because implementations must keep lengths in 64-bit integer types.
  A JavaScript `number` is exact only up to 2^53 nm (about 9,000 km): enough
  for any single length, but intermediate products silently lose precision.
  The stack ADR must account for this.

## Pros and cons of the options

### 1 and 2 — `f64` in inches or millimeters

- Good, because it is the default in most languages and graphics libraries.
- Bad, because neither unit system is exact in the other: 0.1 mm is not
  exact in binary, and 1/64" = 0.396875 mm is not exact either. Rounding
  errors accumulate across operations and differ between platforms.

### 3 — Decimal floating point

- Good, because metric decimals are exact.
- Bad, because 1/3 and many fraction conversions are still inexact, support
  differs between languages and it is slower.

### 4 — Exact rationals

- Good, because every value, including thirds, is exact.
- Bad, because numerators and denominators grow without bound in chained
  calculations, arithmetic is much slower, and every reader of the file
  format needs rational arithmetic.

### 5 — Integer count of 1/16000 mm

- Good, because 1/128" is also exact (3,175 units).
- Bad, because the unit is obscure: every file reader must learn it, for a
  gain (1/128") that shop dimensions do not need. It was the founder-facing
  proposal before this analysis; nanometers win on interoperability.

## Sources

- International yard and pound agreement (1959): 1 inch = 25.4 mm exactly.
- IEEE 754-2019, Standard for Floating-Point Arithmetic.
- David Goldberg, "What Every Computer Scientist Should Know About
  Floating-Point Arithmetic", ACM Computing Surveys, 1991.
