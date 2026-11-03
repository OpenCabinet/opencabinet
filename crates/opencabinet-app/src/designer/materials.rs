// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Editing the project's sheet materials and edge bands, and choosing which
//! of them each part of a cabinet is made of.
//!
//! A project keeps its own copy of everything it uses (RFC-0001 §8), so these
//! edits change this project alone. Items keep their identity when edited,
//! and fields from a newer version stay in the file.

use std::collections::BTreeMap;

use opencabinet_model::{
    EdgeBand, FrontStyle, Grain, Id, InsertKind, Length, Method, Product, SheetMaterial, Units,
};
use opencabinet_reports::BOARD_WIDTH;

use super::{CabinetKind, Designer, Dimension, inches, visit};

/// What a part of a cabinet can be made of. The first choice of all but
/// `Sheets` is to have none of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleChoices {
    /// A sheet material; the cabinet cannot be evaluated without it.
    Sheets,
    /// A sheet material, or the doors'.
    SheetsOrDoors,
    /// A sheet material, or the case's.
    SheetsOrCase,
    /// An edge band, or none: the edges stay bare.
    Bands,
}

/// A part of the edited cabinet that takes a material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialRole {
    /// Its key in the file, such as `drawerBox`.
    pub key: &'static str,
    /// What the window calls it.
    pub label: &'static str,
    /// What it can be made of.
    pub choices: RoleChoices,
    /// Its place among the choices; `None` when it has no material, or one
    /// that is not in the library.
    pub chosen: Option<usize>,
}

/// A part that not every cabinet has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Feature {
    Fronts,
    DrawerOrFalseFronts,
    Drawers,
    Shelves,
    FivePiece,
    Blind,
    FaceFrame,
}

/// A role the window offers, and what a cabinet needs to have it.
#[derive(Clone, Copy, Debug)]
struct Role {
    key: &'static str,
    label: &'static str,
    choices: RoleChoices,
    needs: Option<Feature>,
}

const fn role(
    key: &'static str,
    label: &'static str,
    choices: RoleChoices,
    needs: Option<Feature>,
) -> Role {
    Role {
        key,
        label,
        choices,
        needs,
    }
}

/// Every role the window offers, in the order it lists them.
const ROLES: [Role; 13] = {
    use Feature::{Blind, DrawerOrFalseFronts, Drawers, FaceFrame, FivePiece, Fronts, Shelves};
    use RoleChoices::{Bands, Sheets, SheetsOrCase, SheetsOrDoors};
    [
        role("case", "Case", Sheets, None),
        role("edgeBandCase", "Case edge band", Bands, None),
        role("back", "Back", Sheets, None),
        role("faceFrame", "Face frame", Sheets, Some(FaceFrame)),
        role("shelves", "Shelves", SheetsOrCase, Some(Shelves)),
        role("doors", "Doors", Sheets, Some(Fronts)),
        role(
            "drawerFronts",
            "Drawer and false fronts",
            SheetsOrDoors,
            Some(DrawerOrFalseFronts),
        ),
        role("edgeBandFronts", "Front edge band", Bands, Some(Fronts)),
        role("blindPanel", "Blind panel", SheetsOrDoors, Some(Blind)),
        role(
            "frontFrames",
            "Five-piece frames",
            SheetsOrDoors,
            Some(FivePiece),
        ),
        role(
            "frontPanels",
            "Five-piece panels",
            SheetsOrDoors,
            Some(FivePiece),
        ),
        role("drawerBox", "Drawer boxes", Sheets, Some(Drawers)),
        role("drawerBottom", "Drawer bottoms", Sheets, Some(Drawers)),
    ]
};

/// Which sheet and which edge band are being edited, read within the ones
/// the project has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MaterialSelection {
    sheet: usize,
    band: usize,
}

/// A sheet with a grain along its length and the standard 97" × 49" size.
pub(super) fn new_sheet(name: &str, nominal: Length, actual: Length) -> SheetMaterial {
    SheetMaterial {
        id: Id::new(),
        version: 1,
        name: name.into(),
        nominal_thickness: nominal,
        actual_thickness: actual,
        length: Length::from_inches(97),
        width: Length::from_inches(49),
        grain: Grain::Length,
        unknown: BTreeMap::default(),
    }
}

/// A 0.5 mm edge band, 22 mm wide.
pub(super) fn new_band(name: &str) -> EdgeBand {
    EdgeBand {
        id: Id::new(),
        version: 1,
        name: name.into(),
        thickness: Length::from_nm(500_000),
        width: Length::from_mm(22),
        unknown: BTreeMap::default(),
    }
}

impl Dimension {
    /// Whether it belongs to the edited sheet or edge band.
    pub(super) fn is_library(self) -> bool {
        matches!(
            self,
            Self::SheetNominal
                | Self::SheetActual
                | Self::SheetLength
                | Self::SheetWidth
                | Self::BandThickness
                | Self::BandWidth
        )
    }
}

impl Designer {
    /// The project's sheets as the list shows them: name and nominal
    /// thickness, such as `Plywood · 3/4"`.
    #[must_use]
    pub fn sheets(&self) -> Vec<String> {
        self.document
            .library
            .materials
            .iter()
            .map(|m| format!("{} · {}", m.name, self.show(m.nominal_thickness)))
            .collect()
    }

    /// Which sheet is being edited; `None` when the project has none.
    #[must_use]
    pub fn selected_sheet(&self) -> Option<usize> {
        let count = self.document.library.materials.len();
        (count > 0).then(|| self.material_selection.sheet.min(count - 1))
    }

    /// Edits another sheet, by its place in the list; returns whether there
    /// is one there.
    pub fn select_sheet(&mut self, index: usize) -> bool {
        let exists = index < self.document.library.materials.len();
        if exists {
            self.material_selection.sheet = index;
        }
        exists
    }

    /// Adds a 3/4" sheet after the edited one, and edits it.
    pub fn add_sheet(&mut self) {
        let name = self.free_name("New sheet");
        self.insert_sheet(new_sheet(&name, inches(3, 4), Length::from_mm(18)));
    }

    /// Adds a copy of the edited sheet after it, and edits it. The copy has
    /// an identity of its own.
    pub fn duplicate_sheet(&mut self) {
        let Some(sheet) = self.sheet() else {
            return;
        };
        let mut copy = sheet.clone();
        copy.id = Id::new();
        copy.version = 1;
        copy.name = self.free_name(&format!("{} copy", sheet.name));
        self.insert_sheet(copy);
    }

    fn insert_sheet(&mut self, sheet: SheetMaterial) {
        let index = self.selected_sheet().map_or(0, |i| i + 1);
        self.document.library.materials.insert(index, sheet);
        self.material_selection.sheet = index;
        self.modified = true;
    }

    /// Removes the edited sheet.
    ///
    /// # Errors
    /// A message for the user naming the cabinets that still use it, which
    /// then keeps it.
    pub fn delete_sheet(&mut self) -> Result<(), String> {
        let Some(index) = self.selected_sheet() else {
            return Ok(());
        };
        let sheet = &self.document.library.materials[index];
        let id = sheet.id;
        self.refuse_if_used(id, &sheet.name)?;
        self.forget_material(id);
        self.document.library.materials.remove(index);
        self.modified = true;
        Ok(())
    }

    /// The edited sheet's name; empty when there is none.
    #[must_use]
    pub fn sheet_name(&self) -> &str {
        self.sheet().map_or("", |m| &m.name)
    }

    /// Renames the edited sheet.
    ///
    /// # Errors
    /// A message for the user when the name is empty or another sheet or
    /// band has it; the name is then left unchanged.
    pub fn set_sheet_name(&mut self, text: &str) -> Result<(), String> {
        let Some(index) = self.selected_sheet() else {
            return Err("There is no sheet to rename".into());
        };
        let id = self.document.library.materials[index].id;
        let name = self.checked_name(id, text)?;
        let sheet = &mut self.document.library.materials[index];
        if sheet.name != name {
            sheet.name = name;
            self.modified = true;
        }
        Ok(())
    }

    /// Whether the edited sheet has a grain; `None` when there is no sheet.
    #[must_use]
    pub fn sheet_has_grain(&self) -> Option<bool> {
        self.sheet().map(|m| m.grain == Grain::Length)
    }

    /// Gives the edited sheet a grain along its length, or none.
    pub fn set_sheet_grain(&mut self, grain: bool) {
        let grain = if grain { Grain::Length } else { Grain::None };
        if let Some(index) = self.selected_sheet() {
            let sheet = &mut self.document.library.materials[index];
            if sheet.grain != grain {
                sheet.grain = grain;
                self.modified = true;
            }
        }
    }

    /// How the bill of materials counts the edited sheet, when it is not
    /// by the sheet.
    #[must_use]
    pub fn sheet_note(&self) -> String {
        match self.sheet() {
            Some(sheet) if sheet.width < BOARD_WIDTH => format!(
                "Narrower than {}: bought as solid wood, by length",
                self.show(BOARD_WIDTH)
            ),
            _ => String::new(),
        }
    }

    /// The project's edge bands as the list shows them: name and width.
    #[must_use]
    pub fn bands(&self) -> Vec<String> {
        self.document
            .library
            .edge_bands
            .iter()
            .map(|b| format!("{} · {}", b.name, self.show(b.width)))
            .collect()
    }

    /// Which edge band is being edited; `None` when the project has none.
    #[must_use]
    pub fn selected_band(&self) -> Option<usize> {
        let count = self.document.library.edge_bands.len();
        (count > 0).then(|| self.material_selection.band.min(count - 1))
    }

    /// Edits another edge band, by its place in the list; returns whether
    /// there is one there.
    pub fn select_band(&mut self, index: usize) -> bool {
        let exists = index < self.document.library.edge_bands.len();
        if exists {
            self.material_selection.band = index;
        }
        exists
    }

    /// Adds an edge band after the edited one, and edits it.
    pub fn add_band(&mut self) {
        let name = self.free_name("New edge band");
        self.insert_band(new_band(&name));
    }

    /// Adds a copy of the edited edge band after it, and edits it. The copy
    /// has an identity of its own.
    pub fn duplicate_band(&mut self) {
        let Some(band) = self.band() else {
            return;
        };
        let mut copy = band.clone();
        copy.id = Id::new();
        copy.version = 1;
        copy.name = self.free_name(&format!("{} copy", band.name));
        self.insert_band(copy);
    }

    fn insert_band(&mut self, band: EdgeBand) {
        let index = self.selected_band().map_or(0, |i| i + 1);
        self.document.library.edge_bands.insert(index, band);
        self.material_selection.band = index;
        self.modified = true;
    }

    /// Removes the edited edge band.
    ///
    /// # Errors
    /// A message for the user naming the cabinets that still use it, which
    /// then keeps it.
    pub fn delete_band(&mut self) -> Result<(), String> {
        let Some(index) = self.selected_band() else {
            return Ok(());
        };
        let band = &self.document.library.edge_bands[index];
        let id = band.id;
        self.refuse_if_used(id, &band.name)?;
        self.forget_material(id);
        self.document.library.edge_bands.remove(index);
        self.modified = true;
        Ok(())
    }

    /// The edited edge band's name; empty when there is none.
    #[must_use]
    pub fn band_name(&self) -> &str {
        self.band().map_or("", |b| &b.name)
    }

    /// Renames the edited edge band.
    ///
    /// # Errors
    /// A message for the user when the name is empty or another sheet or
    /// band has it; the name is then left unchanged.
    pub fn set_band_name(&mut self, text: &str) -> Result<(), String> {
        let Some(index) = self.selected_band() else {
            return Err("There is no edge band to rename".into());
        };
        let id = self.document.library.edge_bands[index].id;
        let name = self.checked_name(id, text)?;
        let band = &mut self.document.library.edge_bands[index];
        if band.name != name {
            band.name = name;
            self.modified = true;
        }
        Ok(())
    }

    /// The parts of the edited cabinet that take a material. What it chose
    /// for parts it no longer has stays in the file.
    #[must_use]
    pub fn material_roles(&self) -> Vec<MaterialRole> {
        let features = self.features_of(self.product());
        ROLES
            .iter()
            .filter(|role| role.needs.is_none_or(|f| features.contains(&f)))
            .map(|role| MaterialRole {
                key: role.key,
                label: role.label,
                choices: role.choices,
                chosen: self.chosen(role.key, role.choices),
            })
            .collect()
    }

    /// The names a role of this kind chooses from, in the window's order.
    #[must_use]
    pub fn role_choices(&self, choices: RoleChoices) -> Vec<String> {
        let library = &self.document.library;
        let first = match choices {
            RoleChoices::Sheets => None,
            RoleChoices::SheetsOrDoors => Some("Same as doors"),
            RoleChoices::SheetsOrCase => Some("Same as case"),
            RoleChoices::Bands => Some("None"),
        };
        let names: Vec<&String> = if choices == RoleChoices::Bands {
            library.edge_bands.iter().map(|b| &b.name).collect()
        } else {
            library.materials.iter().map(|m| &m.name).collect()
        };
        first
            .into_iter()
            .map(str::to_owned)
            .chain(names.into_iter().cloned())
            .collect()
    }

    /// Makes a part of the edited cabinet out of another of its choices.
    pub fn set_material_role(&mut self, key: &str, choice: usize) {
        let Some(&Role { key, choices, .. }) = ROLES.iter().find(|role| role.key == key) else {
            return;
        };
        let ids = self.role_ids(choices);
        let offset = usize::from(choices != RoleChoices::Sheets);
        let new = if offset == 1 && choice == 0 {
            None
        } else {
            let Some(&id) = ids.get(choice - offset) else {
                return;
            };
            Some(id)
        };
        let materials = &mut self.product_mut().materials;
        if materials.get(key).copied() == new {
            return;
        }
        match new {
            Some(id) => materials.insert(key.to_owned(), id),
            None => materials.remove(key),
        };
        self.modified = true;
    }

    /// Gives every other cabinet the edited cabinet's materials; returns
    /// how many changed. A part it has no material for keeps the other
    /// cabinet's, unless having none is a choice, such as the same as doors.
    pub fn apply_materials_to_all(&mut self) -> usize {
        let selected = self.selected;
        let source = self.product().materials.clone();
        let optional = ROLES
            .iter()
            .filter(|role| role.choices != RoleChoices::Sheets)
            .map(|role| role.key);
        let mut changed = 0;
        for (index, product) in self.document.project.products.iter_mut().enumerate() {
            if index == selected {
                continue;
            }
            let before = product.materials.clone();
            product.materials.extend(source.clone());
            for key in optional.clone() {
                if !source.contains_key(key) {
                    product.materials.remove(key);
                }
            }
            changed += usize::from(product.materials != before);
        }
        if changed > 0 {
            self.modified = true;
        }
        changed
    }

    /// A length of the edited sheet or edge band.
    pub(super) fn library_value(&self, dimension: Dimension) -> Option<Length> {
        match dimension {
            Dimension::SheetNominal => self.sheet().map(|m| m.nominal_thickness),
            Dimension::SheetActual => self.sheet().map(|m| m.actual_thickness),
            Dimension::SheetLength => self.sheet().map(|m| m.length),
            Dimension::SheetWidth => self.sheet().map(|m| m.width),
            Dimension::BandThickness => self.band().map(|b| b.thickness),
            Dimension::BandWidth => self.band().map(|b| b.width),
            _ => None,
        }
    }

    /// Stores a length of the edited sheet or edge band.
    pub(super) fn set_library_value(
        &mut self,
        dimension: Dimension,
        length: Length,
    ) -> Result<(), String> {
        let sheet = self.selected_sheet();
        let band = self.selected_band();
        let library = &mut self.document.library;
        let sheet = sheet.map(|i| &mut library.materials[i]);
        let band = band.map(|i| &mut library.edge_bands[i]);
        let field = match dimension {
            Dimension::SheetNominal => sheet.map(|m| &mut m.nominal_thickness),
            Dimension::SheetActual => sheet.map(|m| &mut m.actual_thickness),
            Dimension::SheetLength => sheet.map(|m| &mut m.length),
            Dimension::SheetWidth => sheet.map(|m| &mut m.width),
            Dimension::BandThickness => band.map(|b| &mut b.thickness),
            Dimension::BandWidth => band.map(|b| &mut b.width),
            _ => None,
        };
        let Some(field) = field else {
            return Err("There is no material to edit".into());
        };
        if *field != length {
            *field = length;
            self.modified = true;
        }
        Ok(())
    }

    /// A length of a sheet or edge band as text to put in its field: in
    /// the current units when they show it exactly, otherwise in
    /// millimeters, so that 18 mm plywood never reads as 23/32".
    pub(super) fn exact_text(&self, length: Length) -> String {
        let shown = length.show(self.units(), self.precision());
        if shown.exact {
            shown.text
        } else {
            length.show(Units::Metric, Length::from_nm(1)).text
        }
    }

    fn sheet(&self) -> Option<&SheetMaterial> {
        self.selected_sheet()
            .map(|i| &self.document.library.materials[i])
    }

    fn band(&self) -> Option<&EdgeBand> {
        self.selected_band()
            .map(|i| &self.document.library.edge_bands[i])
    }

    /// `name`, or `name 2`, `name 3`… when a sheet or band already has it.
    fn free_name(&self, name: &str) -> String {
        let taken = |candidate: &str| {
            let library = &self.document.library;
            library.materials.iter().any(|m| m.name == candidate)
                || library.edge_bands.iter().any(|b| b.name == candidate)
        };
        if !taken(name) {
            return name.to_owned();
        }
        // With n items, one of the first n + 1 numbers is free.
        let items = self.document.library.materials.len() + self.document.library.edge_bands.len();
        (2..=items + 2)
            .map(|n| format!("{name} {n}"))
            .find(|candidate| !taken(candidate))
            .expect("one of the first n + 1 numbers is free")
    }

    /// What the user typed as a name for the item `id`, trimmed.
    fn checked_name(&self, id: Id, text: &str) -> Result<String, String> {
        let name = text.trim();
        if name.is_empty() {
            return Err("A name cannot be empty".into());
        }
        let library = &self.document.library;
        let taken = library
            .materials
            .iter()
            .map(|m| (m.id, &m.name))
            .chain(library.edge_bands.iter().map(|b| (b.id, &b.name)))
            .any(|(other, other_name)| other != id && other_name == name);
        if taken {
            return Err(format!("Another material is already named {name}"));
        }
        Ok(name.to_owned())
    }

    /// Removes the item `id` from parts cabinets no longer have, which kept
    /// it only in case they came back.
    fn forget_material(&mut self, id: Id) {
        for product in &mut self.document.project.products {
            product.materials.retain(|_, material| *material != id);
        }
    }

    /// Says which cabinets use the item `id`, when any does: for a part they
    /// have, or a part the window does not know, so never for one that is
    /// out of sight.
    fn refuse_if_used(&self, id: Id, name: &str) -> Result<(), String> {
        let text = id.to_string();
        let users: Vec<&str> = self
            .document
            .project
            .products
            .iter()
            .filter(|p| {
                let features = self.features_of(p);
                let in_sight = |key: &str| {
                    ROLES
                        .iter()
                        .find(|role| role.key == key)
                        .is_none_or(|role| role.needs.is_none_or(|f| features.contains(&f)))
                };
                p.materials
                    .iter()
                    .any(|(key, material)| *material == id && in_sight(key))
                    || p.overrides.iter().any(|o| {
                        o.value
                            .as_ref()
                            .is_some_and(|v| v.to_string().contains(&text))
                    })
            })
            .map(|p| p.label.as_str())
            .collect();
        let shown = users.iter().take(3).copied().collect::<Vec<_>>().join(", ");
        match users.len() {
            0 => Ok(()),
            1 => Err(format!("Cannot delete {name}: {shown} uses it")),
            2 | 3 => Err(format!("Cannot delete {name}: {shown} use it")),
            n => Err(format!(
                "Cannot delete {name}: {shown} and {} more use it",
                n - 3
            )),
        }
    }

    fn features_of(&self, product: &Product) -> Vec<Feature> {
        let mut features = Vec::new();
        let blind =
            CabinetKind::of(&product.product_type).is_some_and(|k| k.usual_blind_width().is_some());
        if blind {
            features.extend([Feature::Blind, Feature::Fronts]);
        }
        let library = &self.document.library;
        if library
            .standards
            .iter()
            .any(|s| s.id == product.standard && s.method == Method::FaceFrame)
        {
            features.push(Feature::FaceFrame);
        }
        if let Some(front) = &product.front {
            visit(front, &mut |insert| {
                match insert.kind {
                    InsertKind::Doors => features.push(Feature::Fronts),
                    InsertKind::Drawer => features.extend([
                        Feature::Fronts,
                        Feature::DrawerOrFalseFronts,
                        Feature::Drawers,
                    ]),
                    InsertKind::FalseFront => {
                        features.extend([Feature::Fronts, Feature::DrawerOrFalseFronts]);
                    }
                    _ => {}
                }
                if insert.shelves.unwrap_or(0) > 0 {
                    features.push(Feature::Shelves);
                }
                if insert.style == Some(FrontStyle::FivePiece) {
                    features.push(Feature::FivePiece);
                }
            });
        }
        features
    }

    fn role_ids(&self, choices: RoleChoices) -> Vec<Id> {
        let library = &self.document.library;
        if choices == RoleChoices::Bands {
            library.edge_bands.iter().map(|b| b.id).collect()
        } else {
            library.materials.iter().map(|m| m.id).collect()
        }
    }

    fn chosen(&self, key: &str, choices: RoleChoices) -> Option<usize> {
        let offset = usize::from(choices != RoleChoices::Sheets);
        match self.product().materials.get(key) {
            Some(id) => self
                .role_ids(choices)
                .iter()
                .position(|i| i == id)
                .map(|i| i + offset),
            None => (offset == 1).then_some(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use opencabinet_eval::{Part, evaluate};
    use opencabinet_format::Document;

    use super::super::SectionKind;
    use super::*;

    /// Millimeters with only the decimals they need.
    fn mm(length: Length) -> String {
        let nm = length.nm();
        let text = format!("{}.{:06}", nm / 1_000_000, (nm % 1_000_000).abs());
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    }

    /// The edited cabinet's first part with `role`.
    fn part(designer: &Designer, role: &str) -> Part {
        let product = designer.product().id;
        evaluate(&designer.document.project, &designer.document.library)
            .parts
            .into_iter()
            .find(|p| p.product == product && p.role == role)
            .unwrap_or_else(|| panic!("no part with role {role}"))
    }

    fn keys(designer: &Designer) -> Vec<&'static str> {
        designer.material_roles().iter().map(|r| r.key).collect()
    }

    fn role(designer: &Designer, key: &str) -> MaterialRole {
        designer
            .material_roles()
            .into_iter()
            .find(|r| r.key == key)
            .unwrap_or_else(|| panic!("no role {key}"))
    }

    /// Makes `key` out of the sheet named `name`.
    fn choose(designer: &mut Designer, key: &str, name: &str) {
        let choices = role(designer, key).choices;
        let choice = designer
            .role_choices(choices)
            .iter()
            .position(|n| n == name)
            .unwrap_or_else(|| panic!("no choice {name}"));
        designer.set_material_role(key, choice);
    }

    fn kitchen() -> Designer {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kitchen.ocab");
        Designer::open(&fixture).unwrap()
    }

    #[test]
    fn the_real_thickness_changes_the_parts_by_hand_worked_sizes() {
        // The starter B1 is 30" wide with 18 mm sides, so its bottom is
        // 762 - 2 × 18 = 726 long.
        let mut designer = Designer::default();
        assert_eq!(mm(part(&designer, "bottom").length), "726");

        // Plywood that really is 3/4": 762 - 2 × 19.05 = 723.9.
        designer.edit(Dimension::SheetActual, "3/4").unwrap();
        assert!(designer.modified());
        let bottom = part(&designer, "bottom");
        assert_eq!(
            (mm(bottom.length), mm(bottom.thickness)),
            ("723.9".to_owned(), "19.05".to_owned())
        );

        // The nominal thickness is only what it is sold as.
        designer.edit(Dimension::SheetNominal, "1").unwrap();
        assert_eq!(mm(part(&designer, "bottom").length), "723.9");
        assert_eq!(designer.sheets()[0], "Plywood 3/4\" · 1\"");
    }

    #[test]
    fn a_new_sheet_chosen_for_the_case_changes_only_what_it_makes() {
        let mut designer = Designer::default();
        let door_before = mm(part(&designer, "door").length);
        designer.add_sheet();
        assert_eq!(designer.selected_sheet(), Some(1));
        assert_eq!(designer.sheet_name(), "New sheet");
        designer.set_sheet_name("Melamine 5/8\"").unwrap();
        designer.edit(Dimension::SheetNominal, "5/8").unwrap();
        designer.edit(Dimension::SheetActual, "5/8").unwrap();
        choose(&mut designer, "case", "Melamine 5/8\"");

        // 762 - 2 × 15.875 = 730.25.
        let bottom = part(&designer, "bottom");
        assert_eq!(
            (mm(bottom.length), mm(bottom.thickness)),
            ("730.25".to_owned(), "15.875".to_owned())
        );
        let door = part(&designer, "door");
        assert_eq!(mm(door.length), door_before);
        assert_eq!(mm(door.thickness), "18");
    }

    #[test]
    fn a_part_without_its_own_material_follows_the_one_it_falls_back_on() {
        let mut designer = Designer::default();
        designer.add_sheet();
        let new = designer.document.library.materials[1].id;
        let plywood = designer.document.library.materials[0].id;
        assert_eq!(role(&designer, "drawerFronts").chosen, Some(0));
        assert_eq!(part(&designer, "drawerFront").material, plywood);

        choose(&mut designer, "drawerFronts", "New sheet");
        assert_eq!(part(&designer, "drawerFront").material, new);
        assert_eq!(part(&designer, "door").material, plywood);

        // Same as doors again: it follows them when they change.
        choose(&mut designer, "drawerFronts", "Same as doors");
        assert!(!designer.product().materials.contains_key("drawerFronts"));
        choose(&mut designer, "doors", "New sheet");
        assert_eq!(part(&designer, "drawerFront").material, new);
    }

    #[test]
    fn removing_a_band_leaves_the_edges_bare_and_says_so() {
        let mut designer = Designer::default();
        assert_eq!(role(&designer, "edgeBandFronts").chosen, Some(1));
        choose(&mut designer, "edgeBandFronts", "None");
        assert_eq!(role(&designer, "edgeBandFronts").chosen, Some(0));
        let evaluation = evaluate(&designer.document.project, &designer.document.library);
        assert!(
            evaluation
                .problems
                .iter()
                .any(|p| p.message.contains("edgeBandFronts") && !p.blocking),
            "{:?}",
            evaluation.problems
        );
    }

    #[test]
    fn only_the_parts_a_cabinet_has_are_offered_and_the_rest_stay_in_the_file() {
        let mut designer = Designer::default();
        // A drawer over doors, slab, no shelves.
        assert_eq!(
            keys(&designer),
            [
                "case",
                "edgeBandCase",
                "back",
                "doors",
                "drawerFronts",
                "edgeBandFronts",
                "drawerBox",
                "drawerBottom"
            ]
        );
        designer.set_five_piece(true);
        assert!(keys(&designer).contains(&"frontFrames"));
        assert!(keys(&designer).contains(&"frontPanels"));
        designer.set_five_piece(false);

        // The starter's five-piece panels are 1/4" plywood: not offered
        // on slab fronts, but kept.
        assert!(!keys(&designer).contains(&"frontPanels"));
        assert!(designer.product().materials.contains_key("frontPanels"));

        designer.select_section(1);
        designer.set_shelves("2").unwrap();
        assert!(keys(&designer).contains(&"shelves"));

        designer.select_section(0);
        designer.set_section_kind(SectionKind::Open);
        assert!(!keys(&designer).contains(&"drawerBox"));
        assert!(designer.product().materials.contains_key("drawerBox"));

        designer.set_kind(CabinetKind::BlindBase);
        assert!(keys(&designer).contains(&"blindPanel"));

        // A face frame standard asks for the frame's material; one that
        // the cabinet does not have yet shows no choice.
        let mut kitchen = kitchen();
        kitchen.select(3);
        assert_eq!(kitchen.label(), "W30");
        assert!(role(&kitchen, "faceFrame").chosen.is_some());
        kitchen.product_mut().materials.remove("faceFrame");
        assert_eq!(role(&kitchen, "faceFrame").chosen, None);
    }

    #[test]
    fn applying_to_all_gives_every_cabinet_these_materials() {
        let mut designer = Designer::default();
        designer.add();
        designer.add_sheet();
        let new = designer.document.library.materials[1].id;
        let box_material = designer.document.project.products[0].materials["drawerBox"];
        choose(&mut designer, "case", "New sheet");
        choose(&mut designer, "drawerFronts", "New sheet");
        designer.product_mut().materials.remove("drawerBox");
        // The other cabinet had its own shelves, which it loses, as this
        // one follows the case.
        let first = &mut designer.document.project.products[0];
        first.materials.insert("shelves".into(), box_material);

        assert_eq!(designer.apply_materials_to_all(), 1);
        let first = &designer.document.project.products[0].materials;
        assert_eq!(first["case"], new);
        assert_eq!(first["drawerFronts"], new);
        assert!(!first.contains_key("shelves"));
        // A part this cabinet has no material for keeps the other's.
        assert_eq!(first["drawerBox"], box_material);

        assert_eq!(designer.apply_materials_to_all(), 0);
    }

    #[test]
    fn a_material_in_use_cannot_be_deleted_until_nothing_uses_it() {
        let mut designer = Designer::default();
        let library = |d: &Designer| d.document.library.materials.len();
        assert_eq!(
            designer.delete_sheet(),
            Err("Cannot delete Plywood 3/4\": B1 uses it".into())
        );
        assert_eq!(library(&designer), 4);

        designer.add();
        designer.add();
        designer.add();
        assert_eq!(
            designer.delete_sheet(),
            Err("Cannot delete Plywood 3/4\": B1, B2, B3 and 1 more use it".into())
        );

        designer.add_sheet();
        assert_eq!(designer.selected_sheet(), Some(1));
        designer.delete_sheet().unwrap();
        assert_eq!(library(&designer), 4);
        assert_eq!(designer.selected_sheet(), Some(1));

        assert_eq!(
            designer.delete_band(),
            Err("Cannot delete Edge band 0.5 mm: B1, B2, B3 and 1 more use it".into())
        );
        designer.add_band();
        designer.delete_band().unwrap();
        assert_eq!(designer.selected_band(), Some(0));
    }

    #[test]
    fn a_part_out_of_sight_never_stops_a_delete() {
        let mut designer = Designer::default();
        // Only the frameless cabinet's hidden face frame role uses it.
        designer.select_sheet(3);
        assert_eq!(designer.sheet_name(), "Solid wood 3/4\"");
        assert!(!keys(&designer).contains(&"faceFrame"));
        designer.delete_sheet().unwrap();
        assert!(!designer.product().materials.contains_key("faceFrame"));

        // Once it matters, the list is empty and the evaluator says so.
        designer.set_cabinet_standard(1);
        assert_eq!(role(&designer, "faceFrame").chosen, None);
        let evaluation = evaluate(&designer.document.project, &designer.document.library);
        assert!(
            evaluation
                .problems
                .iter()
                .any(|p| p.blocking && p.message.contains("faceFrame")),
            "{:?}",
            evaluation.problems
        );
    }

    #[test]
    fn names_must_be_given_and_not_repeated() {
        let mut designer = Designer::default();
        assert_eq!(
            designer.set_sheet_name("  "),
            Err("A name cannot be empty".into())
        );
        assert_eq!(
            designer.set_sheet_name("Plywood 1/4\""),
            Err("Another material is already named Plywood 1/4\"".into())
        );
        assert_eq!(
            designer.set_sheet_name("Edge band 0.5 mm"),
            Err("Another material is already named Edge band 0.5 mm".into())
        );
        assert_eq!(designer.sheet_name(), "Plywood 3/4\"");
        designer.set_sheet_name(" Birch 3/4\" ").unwrap();
        assert_eq!(designer.sheet_name(), "Birch 3/4\"");

        designer.duplicate_sheet();
        assert_eq!(designer.sheet_name(), "Birch 3/4\" copy");
        designer.select_sheet(0);
        designer.duplicate_sheet();
        assert_eq!(designer.sheet_name(), "Birch 3/4\" copy 2");
        designer.duplicate_band();
        assert_eq!(designer.band_name(), "Edge band 0.5 mm copy");
        designer.set_band_name("Maple band").unwrap();
        assert_eq!(designer.bands()[1], "Maple band · 7/8\"");
    }

    #[test]
    fn exact_sizes_are_never_replaced_by_rounded_ones() {
        let mut designer = Designer::default();
        // 18 mm is not a 32nd of an inch.
        assert_eq!(designer.text(Dimension::SheetActual), "18 mm");
        assert_eq!(designer.reading(Dimension::SheetActual), "≈ 23/32\"");
        assert_eq!(designer.text(Dimension::SheetNominal), "3/4\"");
        assert_eq!(designer.text(Dimension::BandThickness), "0.5 mm");

        designer.set_units(Units::Metric);
        assert_eq!(designer.text(Dimension::SheetNominal), "19.05 mm");
        assert_eq!(designer.text(Dimension::SheetActual), "18 mm");

        // Reading a field back changes nothing.
        let before = designer.document.library.clone();
        let text = designer.text(Dimension::SheetNominal);
        designer.edit(Dimension::SheetNominal, &text).unwrap();
        assert_eq!(designer.document.library, before);
    }

    #[test]
    fn sizes_must_be_positive_and_grain_and_notes_follow_the_sheet() {
        let mut designer = Designer::default();
        assert_eq!(
            designer.edit(Dimension::SheetActual, "0"),
            Err("Must be greater than zero".into())
        );
        assert_eq!(designer.text(Dimension::SheetActual), "18 mm");
        assert_eq!(designer.sheet_has_grain(), Some(true));
        designer.set_sheet_grain(false);
        assert_eq!(designer.sheet_has_grain(), Some(false));

        assert_eq!(designer.sheet_note(), "");
        designer.edit(Dimension::SheetWidth, "5-1/2").unwrap();
        assert_eq!(
            designer.sheet_note(),
            "Narrower than 12\": bought as solid wood, by length"
        );
        // The bill of materials counts it by length too.
        let bom = designer.bill_of_materials();
        assert!(
            bom.rows.iter().any(|r| r.quantity.ends_with("ft")),
            "{:?}",
            bom.rows
        );
    }

    #[test]
    fn edits_keep_identity_and_newer_fields_through_a_save() {
        let dir =
            std::env::temp_dir().join(format!("opencabinet-materials-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("materials.ocab");

        let mut designer = Designer::default();
        designer.document.library.materials[0]
            .unknown
            .insert("futureField".into(), 7.into());
        let id = designer.document.library.materials[0].id;
        designer.set_sheet_name("Birch 3/4\"").unwrap();
        designer.edit(Dimension::SheetActual, "3/4").unwrap();
        designer.save_as(&path).unwrap();

        let reopened = Document::load(&path).unwrap();
        let sheet = &reopened.library.materials[0];
        assert_eq!(sheet.id, id);
        assert_eq!(sheet.name, "Birch 3/4\"");
        assert_eq!(sheet.actual_thickness, inches(3, 4));
        assert_eq!(sheet.unknown["futureField"], 7);
        std::fs::remove_dir_all(&dir).ok();
    }
}
