// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bill of materials: what to buy for a job, from the same parts and
//! hardware as the cut list.
//!
//! - **Sheets:** the parts' area at their cut size plus the standard's
//!   waste, divided by the area of one sheet and rounded up. It is an
//!   estimate: planning the cuts on each sheet is cut optimization's job.
//! - **Solid wood,** materials whose stock is narrower than 12": the parts'
//!   length plus the waste, in boards of the stock's length.
//! - **Edge band:** the length of every banded edge plus the standard's
//!   allowance for trimming each one.
//! - **Hardware:** the hinges, drawer slides and shelf pins the evaluator
//!   placed.
//!
//! The waste and the allowance come from the `billOfMaterials` rules of
//! each product's standard.

#![allow(
    clippy::cast_precision_loss,
    reason = "areas and lengths of a job are far below 2^53 nm"
)]

use std::collections::HashMap;

use opencabinet_eval::rules::{BillOfMaterials, Rules};
use opencabinet_eval::{HardwareKind, HardwareUse, Part, evaluate};
use opencabinet_model::{Id, Length, Library, Project, Units};

/// The bill of materials, as rows of text in the user's units.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bom {
    /// One row per material, edge band and piece of hardware.
    pub rows: Vec<BomRow>,
    /// What the user should know about how it was counted.
    pub notes: Vec<String>,
}

/// One thing to buy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BomRow {
    /// Sheets, solid wood, edge band or hardware.
    pub section: String,
    /// What it is: a material's, edge band's or hardware's name.
    pub item: String,
    /// How many: "3 sheets", "41.6 ft", "12".
    pub quantity: String,
    /// How it was counted.
    pub detail: String,
}

impl BomRow {
    /// The column titles, in the order of [`BomRow::cells`].
    pub const TITLES: [&str; 4] = ["Section", "Item", "Quantity", "Detail"];

    /// The row's cells, in the order of [`BomRow::TITLES`].
    #[must_use]
    pub fn cells(&self) -> [&str; 4] {
        [&self.section, &self.item, &self.quantity, &self.detail]
    }
}

/// Stock narrower than this is counted by length, as boards.
pub const BOARD_WIDTH: Length = Length::from_inches(12);

/// A count that is whole, rounded up; a hair over a whole number is that
/// number, so floating point never asks for an extra sheet.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a small positive count"
)]
fn whole(count: f64) -> u64 {
    (count - 1e-9).ceil().max(0.0) as u64
}

fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The bill of materials of a project, in `units`, with sheet and board
/// sizes shown at `precision`.
#[must_use]
pub fn bill_of_materials(
    project: &Project,
    library: &Library,
    units: Units,
    precision: Length,
) -> Bom {
    let evaluation = evaluate(project, library);
    let mut notes = Vec::new();
    let rules: HashMap<Id, BillOfMaterials> = project
        .products
        .iter()
        .filter_map(|product| {
            let found = library
                .standards
                .iter()
                .find(|s| s.id == product.standard)
                .and_then(|s| Rules::of(s).ok())
                .and_then(|r| r.bill_of_materials);
            if found.is_none() {
                notes.push(format!(
                    "{}: its standard has no `billOfMaterials` rules, so no waste or edge \
                     band allowance is added",
                    product.label
                ));
            }
            Some((product.id, found?))
        })
        .collect();
    let count = Counting {
        library,
        rules,
        units,
        precision,
    };
    let mut rows = count.materials(&evaluation.parts);
    rows.extend(count.edge_bands(&evaluation.parts));
    rows.extend(count.hardware(&evaluation.hardware));
    notes.push(
        "Sheet counts are estimates from the parts' area; pulls, legs, screws and glue are \
         not counted yet"
            .into(),
    );
    Bom { rows, notes }
}

/// What counting needs: the library, each product's rules, and how to
/// show lengths.
struct Counting<'l> {
    library: &'l Library,
    rules: HashMap<Id, BillOfMaterials>,
    units: Units,
    precision: Length,
}

impl Counting<'_> {
    fn show(&self, length: Length) -> String {
        length.show(self.units, self.precision).text
    }

    /// One plus the waste of the part's standard, as a factor.
    fn waste(&self, part: &Part) -> f64 {
        let percent = self.rules.get(&part.product).map_or(0, |r| r.waste_percent);
        1.0 + f64::from(percent) / 100.0
    }

    fn allowance(&self, part: &Part) -> Length {
        self.rules
            .get(&part.product)
            .map_or(Length::ZERO, |r| r.edge_band_allowance)
    }

    /// A row per material, in the order the parts first use them: sheets
    /// by area, solid stock by length.
    fn materials(&self, all: &[Part]) -> Vec<BomRow> {
        let mut materials: Vec<Id> = Vec::new();
        for part in all {
            if !materials.contains(&part.material) {
                materials.push(part.material);
            }
        }
        let mut rows = Vec::new();
        for id in materials {
            let Some(material) = self.library.materials.iter().find(|m| m.id == id) else {
                continue;
            };
            let parts: Vec<&Part> = all.iter().filter(|p| p.material == id).collect();
            if material.width < BOARD_WIDTH {
                let length = |p: &&Part| mm(p.cut_length) * f64::from(p.quantity);
                let net: f64 = parts.iter().map(length).sum();
                let bought: f64 = parts.iter().map(|p| length(p) * self.waste(p)).sum();
                let boards = whole(bought / mm(material.length).max(1.0));
                rows.push(BomRow {
                    section: "Solid wood".into(),
                    item: material.name.clone(),
                    quantity: plural(boards, "board", "boards"),
                    detail: format!(
                        "{} of parts, {} with waste, in {} boards",
                        lineal(net, self.units),
                        lineal(bought, self.units),
                        self.show(material.length)
                    ),
                });
            } else {
                let net: f64 = parts.iter().map(|p| part_area(p)).sum();
                let bought: f64 = parts.iter().map(|p| part_area(p) * self.waste(p)).sum();
                let sheet = mm(material.length) * mm(material.width);
                let sheets = whole(bought / sheet.max(1.0));
                rows.push(BomRow {
                    section: "Sheets".into(),
                    item: material.name.clone(),
                    quantity: plural(sheets, "sheet", "sheets"),
                    detail: format!(
                        "{} of parts, {} with waste, on {} × {} sheets",
                        surface(net, self.units),
                        surface(bought, self.units),
                        self.show(material.length),
                        self.show(material.width)
                    ),
                });
            }
        }
        rows
    }

    /// A row per edge band: every banded edge plus the allowance for
    /// trimming each.
    fn edge_bands(&self, parts: &[Part]) -> Vec<BomRow> {
        let mut bands: Vec<(Id, f64, u64)> = Vec::new();
        for part in parts {
            for (edge, band) in part.edges.iter().enumerate() {
                let Some(band) = band else { continue };
                // The ends, x0 and x1, run across the width; the sides
                // along the length.
                let along = if edge < 2 { part.width } else { part.length };
                let length = mm(along + self.allowance(part)) * f64::from(part.quantity);
                let copies = u64::from(part.quantity);
                match bands.iter_mut().find(|(id, _, _)| id == band) {
                    Some((_, total, edges)) => {
                        *total += length;
                        *edges += copies;
                    }
                    None => bands.push((*band, length, copies)),
                }
            }
        }
        bands
            .into_iter()
            .map(|(id, length, edges)| BomRow {
                section: "Edge band".into(),
                item: self
                    .library
                    .edge_bands
                    .iter()
                    .find(|b| b.id == id)
                    .map_or_else(String::new, |b| b.name.clone()),
                quantity: lineal(length, self.units),
                detail: format!(
                    "{}, with the allowance for trimming",
                    plural(edges, "edge", "edges")
                ),
            })
            .collect()
    }

    /// A row per kind of hardware, in the order it was first placed.
    fn hardware(&self, used: &[HardwareUse]) -> Vec<BomRow> {
        let mut kinds: Vec<(HardwareKind, u64)> = Vec::new();
        for use_ in used {
            let count = u64::from(use_.quantity);
            match kinds.iter_mut().find(|(kind, _)| *kind == use_.kind) {
                Some((_, total)) => *total += count,
                None => kinds.push((use_.kind, count)),
            }
        }
        kinds
            .into_iter()
            .map(|(kind, count)| {
                let (item, quantity, detail) = match kind {
                    HardwareKind::Hinge(id) => (
                        self.library
                            .hardware
                            .iter()
                            .find(|h| h.id == id)
                            .map_or_else(|| "Hinge".to_owned(), |h| h.name.clone()),
                        count.to_string(),
                        "with mounting plates".to_owned(),
                    ),
                    HardwareKind::SlidePair(length) => (
                        format!("Drawer slides {}", self.show(length)),
                        plural(count, "pair", "pairs"),
                        String::new(),
                    ),
                    HardwareKind::ShelfPin => (
                        "Shelf pins".to_owned(),
                        count.to_string(),
                        "4 for each adjustable shelf".to_owned(),
                    ),
                };
                BomRow {
                    section: "Hardware".into(),
                    item,
                    quantity,
                    detail,
                }
            })
            .collect()
    }
}

fn mm(length: Length) -> f64 {
    length.nm() as f64 / 1_000_000.0
}

/// A part's area at its cut size, for every copy, in square millimeters.
fn part_area(part: &Part) -> f64 {
    mm(part.cut_length) * mm(part.cut_width) * f64::from(part.quantity)
}

/// Square millimeters as square feet or meters, to a tenth or a
/// hundredth.
fn surface(area: f64, units: Units) -> String {
    match units {
        Units::Imperial => format!("{:.1} ft²", area / 92_903.04),
        Units::Metric => format!("{:.2} m²", area / 1_000_000.0),
    }
}

/// Millimeters as feet or meters, rounded up to a tenth or a hundredth, so
/// what is bought is never short.
fn lineal(length: f64, units: Units) -> String {
    match units {
        Units::Imperial => format!("{:.1} ft", whole(length / 304.8 * 10.0) as f64 / 10.0),
        Units::Metric => format!("{:.2} m", whole(length / 1000.0 * 100.0) as f64 / 100.0),
    }
}
