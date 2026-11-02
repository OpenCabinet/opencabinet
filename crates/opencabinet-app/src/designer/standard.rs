// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Editing the project's construction standards, and choosing the one each
//! cabinet is built by.
//!
//! The window shows the rules from a table, and edits the file's JSON in
//! place, so rules it does not show stay as they are. A value is kept only if
//! the evaluator can still read the standard. Sizes that do not work for one
//! cabinet are that cabinet's problem, which the cut list reports.

use std::collections::BTreeMap;

use opencabinet_eval::rules::Rules;
use opencabinet_model::{Hardware, Id, Length, Method, Standard};
use serde_json::{Value, json};

use super::{Designer, capitalize, inches};

/// The most hinges a door takes.
const MAX_HINGES: u32 = 10;

/// What a rule holds, and how it is typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Holds {
    /// A length greater than zero.
    Size,
    /// A length that may be zero.
    SizeOrZero,
    /// A length greater than zero, or nothing; the text says what nothing
    /// means.
    OptionalSize(&'static str),
    /// A whole percentage.
    Percent,
    /// Lengths separated by commas, kept shortest first.
    Sizes,
    /// `height = count` pairs separated by commas, shortest first.
    HingeTable,
    /// The name of the standard's hinge.
    HingeName,
    /// A size of the standard's hinge.
    HingeSize,
    /// One of a list, which the code knows how to store.
    Choice(&'static [&'static str]),
}

struct Field {
    key: &'static str,
    label: &'static str,
    holds: Holds,
}

struct Group {
    key: &'static str,
    title: &'static str,
    fields: &'static [Field],
}

const fn field(key: &'static str, label: &'static str, holds: Holds) -> Field {
    Field { key, label, holds }
}

const BACKS: &[&str] = &["Grooved", "Applied"];
const OVERLAYS: &[&str] = &["Partial overlay", "Inset"];

/// The keys in the file of a hinge's sizes.
const CUP_DIAMETER: &str = "cupDiameter";
const CUP_DEPTH: &str = "cupDepth";

/// Every rule the evaluator reads, in the order the window lists them.
const GROUPS: &[Group] = &[
    Group {
        key: "toeKick",
        title: "Toe kick",
        fields: &[field("height", "Toe kick height", Holds::SizeOrZero)],
    },
    Group {
        key: "case",
        title: "Case",
        fields: &[field("stretcherWidth", "Stretcher width", Holds::Size)],
    },
    Group {
        key: "back",
        title: "Back",
        fields: &[
            field("construction", "Back construction", Holds::Choice(BACKS)),
            field("grooveDepth", "Groove depth", Holds::Size),
            field(
                "setback",
                "Groove from rear edge",
                Holds::OptionalSize("Empty: the groove is not machined"),
            ),
        ],
    },
    Group {
        key: "fronts",
        title: "Fronts",
        fields: &[
            field("overlay", "How fronts sit", Holds::Choice(OVERLAYS)),
            field("overlayAmount", "Overlay on the frame", Holds::Size),
            field("gap", "Gap between fronts", Holds::SizeOrZero),
        ],
    },
    Group {
        key: "faceFrame",
        title: "Face frame",
        fields: &[
            field("stileWidth", "Stile width", Holds::Size),
            field("railWidth", "Rail width", Holds::Size),
            field(
                "topRailWidth",
                "Top rail width",
                Holds::OptionalSize("Empty: the rail width"),
            ),
            field(
                "bottomRailWidth",
                "Bottom rail width",
                Holds::OptionalSize("Empty: the rail width"),
            ),
            field(
                "mullionWidth",
                "Mullion width",
                Holds::OptionalSize("Empty: the stile width"),
            ),
            field("overhang", "Overhang past the case", Holds::SizeOrZero),
        ],
    },
    Group {
        key: "drawers",
        title: "Drawers",
        fields: &[
            field("sideClearance", "Slide room, each side", Holds::SizeOrZero),
            field(
                "verticalClearance",
                "Box lower than opening",
                Holds::SizeOrZero,
            ),
            field("rearClearance", "Room behind the box", Holds::SizeOrZero),
            field("slideLengths", "Slide lengths, with commas", Holds::Sizes),
            field("bottomGrooveDepth", "Bottom groove depth", Holds::Size),
            field(
                "bottomGrooveSetback",
                "Bottom groove from edge",
                Holds::OptionalSize("Empty: the groove is not machined"),
            ),
        ],
    },
    Group {
        key: "shelves",
        title: "Shelves",
        fields: &[
            field(
                "sideClearance",
                "Narrower than inside, total",
                Holds::SizeOrZero,
            ),
            field("frontSetback", "Back from the front", Holds::SizeOrZero),
            field("backAllowance", "Room for the back", Holds::SizeOrZero),
        ],
    },
    Group {
        key: "fivePiece",
        title: "Five-piece fronts",
        fields: &[
            field("stileWidth", "Five-piece stile width", Holds::Size),
            field("railWidth", "Five-piece rail width", Holds::Size),
            field("jointDepth", "Joint depth", Holds::Size),
            field("panelGrooveDepth", "Panel groove depth", Holds::Size),
            field("panelClearance", "Panel room, total", Holds::SizeOrZero),
        ],
    },
    Group {
        key: "systemHoles",
        title: "Line boring",
        fields: &[
            field("frontSetback", "Front column setback", Holds::SizeOrZero),
            field("rearSetback", "Rear column setback", Holds::SizeOrZero),
            field("spacing", "Hole spacing", Holds::Size),
            field("diameter", "Hole diameter", Holds::Size),
            field("depth", "Hole depth", Holds::Size),
            field("endClearance", "No holes at the ends", Holds::SizeOrZero),
        ],
    },
    Group {
        key: "hinges",
        title: "Hinges",
        fields: &[
            field("@name", "Hinge name", Holds::HingeName),
            field(CUP_DIAMETER, "Cup diameter", Holds::HingeSize),
            field(CUP_DEPTH, "Cup depth", Holds::HingeSize),
            field("boringDistance", "Cup from door edge", Holds::Size),
            field("endDistance", "Cup from door end", Holds::Size),
            field("counts", "Door height = hinges", Holds::HingeTable),
        ],
    },
    Group {
        key: "billOfMaterials",
        title: "Bill of materials",
        fields: &[
            field("wastePercent", "Waste, percent", Holds::Percent),
            field(
                "edgeBandAllowance",
                "Band added per edge",
                Holds::SizeOrZero,
            ),
        ],
    },
];

/// How a row of the Standard tab is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleKind {
    /// The title of a group of rules.
    Group,
    /// The title of a group the standard does not have, which can be added.
    MissingGroup,
    /// A field to type in.
    Field,
    /// A list to choose from.
    Choice {
        /// What it offers.
        options: &'static [&'static str],
        /// Which it has.
        chosen: Option<usize>,
    },
}

/// A row of the Standard tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleRow {
    /// Which rule, for editing it: `toeKick.height`, or `toeKick` for a
    /// group.
    pub key: String,
    /// What the window calls it.
    pub label: &'static str,
    /// How it is shown.
    pub kind: RuleKind,
    /// The text to put in its field.
    pub text: String,
    /// The line under its field.
    pub reading: String,
}

/// A 35 mm cup hinge, 13 mm deep.
pub(super) fn new_hinge() -> Hardware {
    Hardware {
        id: Id::new(),
        version: 1,
        category: "hinge".into(),
        name: "Cup hinge 35 mm".into(),
        dimensions: [
            (CUP_DIAMETER.to_owned(), Length::from_mm(35)),
            (CUP_DEPTH.to_owned(), Length::from_mm(13)),
        ]
        .into(),
        unknown: BTreeMap::default(),
    }
}

/// The usual rules of a standard built by `method`, with `hinge` for its
/// doors. A shop is expected to change them.
pub(super) fn usual_rules(method: Method, hinge: Id) -> Value {
    // The drawer slides the shop stocks, 12" to 22" by 2".
    let slides = [12, 14, 16, 18, 20, 22].map(Length::from_inches);
    let mut rules = json!({
        "back": {
            "construction": "grooved",
            "grooveDepth": inches(3, 8),
            "setback": inches(1, 4),
        },
        "case": { "stretcherWidth": Length::from_inches(4) },
        "drawers": {
            "sideClearance": inches(1, 2),
            "verticalClearance": inches(3, 2),
            "rearClearance": Length::from_inches(1),
            "slideLengths": slides,
            "bottomGrooveDepth": inches(1, 4),
            "bottomGrooveSetback": inches(1, 2),
        },
        "fivePiece": {
            "stileWidth": inches(9, 4),
            "railWidth": inches(9, 4),
            "jointDepth": inches(3, 8),
            "panelGrooveDepth": inches(3, 8),
            "panelClearance": inches(1, 8),
        },
        "fronts": { "overlay": "full", "gap": inches(1, 8) },
        "shelves": {
            "sideClearance": inches(1, 16),
            "frontSetback": inches(1, 4),
            "backAllowance": inches(3, 4),
        },
        "toeKick": { "height": Length::from_inches(4) },
        "billOfMaterials": {
            "wastePercent": 15,
            "edgeBandAllowance": Length::from_inches(1),
        },
        "systemHoles": {
            "frontSetback": Length::from_mm(37),
            "rearSetback": Length::from_mm(37),
            "spacing": Length::from_mm(32),
            "diameter": Length::from_mm(5),
            "depth": Length::from_mm(12),
            "endClearance": Length::from_inches(2),
        },
        "hinges": {
            "hardware": hinge,
            "boringDistance": Length::from_mm(4),
            "endDistance": Length::from_inches(3),
            "counts": [
                { "upTo": Length::from_inches(40), "count": 2 },
                { "upTo": Length::from_inches(60), "count": 3 },
                { "upTo": Length::from_inches(80), "count": 4 },
            ],
        },
    });
    if method == Method::FaceFrame {
        rules["fronts"] = json!({
            "overlay": "partial",
            "overlayAmount": inches(1, 2),
            "gap": inches(1, 8),
        });
        rules["faceFrame"] = json!({
            "stileWidth": inches(3, 2),
            "railWidth": inches(3, 2),
            "joinery": "butt",
        });
    }
    rules
}

fn length_of(value: &Value) -> Option<Length> {
    serde_json::from_value(value.clone()).ok()
}

/// Whether the standard shows `field` of `group` as things stand: only what
/// it can use is offered, and the rest stays in the file.
fn shown(standard: &Standard, group: &str, field: &str) -> bool {
    let rule = |key: &str| standard.rules.get(group).and_then(|g| g.get(key));
    match (group, field) {
        ("back", "grooveDepth" | "setback") => {
            rule("construction").and_then(Value::as_str) != Some("applied")
        }
        ("fronts", "overlay") => standard.method == Method::FaceFrame,
        ("fronts", "overlayAmount") => {
            standard.method == Method::FaceFrame
                && rule("overlay").and_then(Value::as_str) == Some("partial")
        }
        _ => true,
    }
}

fn parse_size(text: &str, designer: &Designer) -> Result<Length, String> {
    Length::parse(text.trim(), designer.units()).map_err(|e| capitalize(&e.to_string()))
}

impl Designer {
    /// The names of the project's standards.
    #[must_use]
    pub fn standards(&self) -> Vec<String> {
        self.document
            .library
            .standards
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }

    /// Which standard the Standard tab edits; `None` when the project has
    /// none.
    #[must_use]
    pub fn selected_standard(&self) -> Option<usize> {
        let count = self.document.library.standards.len();
        (count > 0).then(|| self.standard.min(count - 1))
    }

    /// Edits another standard, by its place in the list.
    pub fn select_standard(&mut self, index: usize) {
        if index < self.document.library.standards.len() {
            self.standard = index;
        }
    }

    /// Makes the Standard tab edit the standard of the edited cabinet.
    pub(super) fn follow_cabinet_standard(&mut self) {
        if let Some(index) = self.cabinet_standard() {
            self.standard = index;
        }
    }

    /// Which standard the edited cabinet is built by; `None` when it is not
    /// in the library.
    #[must_use]
    pub fn cabinet_standard(&self) -> Option<usize> {
        let id = self.product().standard;
        self.document
            .library
            .standards
            .iter()
            .position(|s| s.id == id)
    }

    /// Builds the edited cabinet by another standard, and edits that one.
    pub fn set_cabinet_standard(&mut self, index: usize) {
        let Some(id) = self.document.library.standards.get(index).map(|s| s.id) else {
            return;
        };
        if self.product().standard != id {
            self.product_mut().standard = id;
            self.modified = true;
        }
        self.standard = index;
    }

    /// The edited standard's name; empty when there is none.
    #[must_use]
    pub fn standard_name(&self) -> &str {
        self.edited_standard().map_or("", |s| &s.name)
    }

    /// Renames the edited standard.
    ///
    /// # Errors
    /// A message for the user when the name is empty or another standard
    /// has it; the name is then left unchanged.
    pub fn set_standard_name(&mut self, text: &str) -> Result<(), String> {
        let Some(index) = self.selected_standard() else {
            return Err("There is no standard to rename".into());
        };
        let name = text.trim();
        if name.is_empty() {
            return Err("A name cannot be empty".into());
        }
        let standards = &mut self.document.library.standards;
        let taken = standards
            .iter()
            .enumerate()
            .any(|(i, s)| i != index && s.name == name);
        if taken {
            return Err(format!("Another standard is already named {name}"));
        }
        if standards[index].name != name {
            name.clone_into(&mut standards[index].name);
            self.modified = true;
        }
        Ok(())
    }

    /// How the edited standard builds, and how many cabinets it builds.
    #[must_use]
    pub fn standard_note(&self) -> String {
        let Some(standard) = self.edited_standard() else {
            return String::new();
        };
        let method = match standard.method {
            Method::Frameless => "Frameless",
            Method::FaceFrame => "Face frame",
        };
        let products = &self.document.project.products;
        let using = products
            .iter()
            .filter(|p| p.standard == standard.id)
            .count();
        format!("{method} · used by {using} of {} cabinets", products.len())
    }

    /// The rows of the Standard tab for the edited standard.
    #[must_use]
    pub fn rule_rows(&self) -> Vec<RuleRow> {
        let Some(standard) = self.edited_standard() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for group in GROUPS {
            if group.key == "faceFrame" && standard.method != Method::FaceFrame {
                continue;
            }
            let present = standard.rules.contains_key(group.key);
            rows.push(RuleRow {
                key: group.key.to_owned(),
                label: group.title,
                kind: if present {
                    RuleKind::Group
                } else {
                    RuleKind::MissingGroup
                },
                text: String::new(),
                reading: String::new(),
            });
            if present {
                rows.extend(
                    group
                        .fields
                        .iter()
                        .filter(|f| shown(standard, group.key, f.key))
                        .filter_map(|f| self.rule_row(standard, group, f)),
                );
            }
        }
        rows
    }

    /// Stores what the user typed into the rule `key`.
    ///
    /// # Errors
    /// A message for the user when it cannot be read, or the standard could
    /// not be read with it; the rule is then left unchanged.
    pub fn edit_rule(&mut self, key: &str, text: &str) -> Result<(), String> {
        let (group, field) = find_field(key).ok_or("There is no such rule")?;
        match field.holds {
            Holds::HingeName => self.set_hinge_name(text),
            Holds::HingeSize => {
                let size = parse_size(text, self)?;
                if size <= Length::ZERO {
                    return Err("Must be greater than zero".into());
                }
                self.set_hinge_size(field.key, size)
            }
            Holds::Choice(_) => Err("Choose from the list".into()),
            holds => {
                let value = parse_rule(holds, text, self)?;
                self.store_rule(group.key, field.key, value)
            }
        }
    }

    /// Chooses from the list of the rule `key`. A choice that needs a size
    /// the standard does not have yet gets the usual one.
    pub fn choose_rule(&mut self, key: &str, choice: usize) {
        let changes: [(&str, &str, Option<Value>); 2] = match (key, choice) {
            ("back.construction", 0) => [
                ("construction", "grooved", None),
                ("grooveDepth", "", Some(json!(inches(3, 8)))),
            ],
            ("back.construction", 1) => {
                [("construction", "applied", None), ("grooveDepth", "", None)]
            }
            ("fronts.overlay", 0) => [
                ("overlay", "partial", None),
                ("overlayAmount", "", Some(json!(inches(1, 2)))),
            ],
            ("fronts.overlay", 1) => [("overlay", "inset", None), ("overlayAmount", "", None)],
            _ => return,
        };
        let Some(index) = self.selected_standard() else {
            return;
        };
        let group = key.split('.').next().unwrap_or_default();
        let Some(Value::Object(rules)) =
            self.document.library.standards[index].rules.get_mut(group)
        else {
            return;
        };
        let (rule, value, _) = &changes[0];
        if rules.get(*rule).and_then(Value::as_str) == Some(value) {
            return;
        }
        rules.insert((*rule).to_owned(), Value::from(*value));
        let (rule, _, usual) = &changes[1];
        if let Some(usual) = usual {
            rules.entry((*rule).to_owned()).or_insert(usual.clone());
        }
        self.modified = true;
    }

    /// Gives the edited standard the usual rules of a group it does not
    /// have. Hinges take the library's first, or a new one.
    pub fn add_rule_group(&mut self, group: &str) {
        let Some(index) = self.selected_standard() else {
            return;
        };
        let library = &mut self.document.library;
        if library.standards[index].rules.contains_key(group) {
            return;
        }
        let existing = library
            .hardware
            .iter()
            .find(|h| h.category == "hinge")
            .map(|h| h.id);
        let hinge = existing.unwrap_or_else(|| {
            let hinge = new_hinge();
            let id = hinge.id;
            if group == "hinges" {
                library.hardware.push(hinge);
            }
            id
        });
        let usual = usual_rules(library.standards[index].method, hinge);
        if let Some(rules) = usual.get(group) {
            library.standards[index]
                .rules
                .insert(group.to_owned(), rules.clone());
            self.modified = true;
        }
    }

    fn edited_standard(&self) -> Option<&Standard> {
        self.selected_standard()
            .map(|i| &self.document.library.standards[i])
    }

    /// A row for `field`; `None` for a hinge the library does not have.
    fn rule_row(&self, standard: &Standard, group: &Group, field: &Field) -> Option<RuleRow> {
        let rule = standard.rules.get(group.key).and_then(|g| g.get(field.key));
        let size = |length: Length| (self.exact_text(length), self.rule_reading(length));
        let (kind, text, reading) = match field.holds {
            Holds::Choice(options) => {
                let stored = rule.and_then(Value::as_str).unwrap_or_default();
                let chosen = match field.key {
                    "construction" => Some(usize::from(stored == "applied")),
                    _ => match stored {
                        "partial" => Some(0),
                        "inset" => Some(1),
                        _ => None,
                    },
                };
                let kind = RuleKind::Choice { options, chosen };
                (kind, String::new(), String::new())
            }
            Holds::Size | Holds::SizeOrZero => {
                let (text, reading) = rule.and_then(length_of).map(size).unwrap_or_default();
                (RuleKind::Field, text, reading)
            }
            Holds::OptionalSize(when_empty) => {
                let (text, reading) = rule
                    .and_then(length_of)
                    .map_or((String::new(), when_empty.to_owned()), size);
                (RuleKind::Field, text, reading)
            }
            Holds::Percent => {
                let text = rule.map(ToString::to_string).unwrap_or_default();
                (RuleKind::Field, text, String::new())
            }
            Holds::Sizes => {
                let sizes = rule.and_then(Value::as_array).map(|items| {
                    items
                        .iter()
                        .filter_map(length_of)
                        .map(|l| self.exact_text(l))
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                (RuleKind::Field, sizes.unwrap_or_default(), String::new())
            }
            Holds::HingeTable => {
                let table = rule.and_then(Value::as_array).map(|items| {
                    items
                        .iter()
                        .filter_map(|row| {
                            let up_to = length_of(row.get("upTo")?)?;
                            let count = row.get("count")?.as_u64()?;
                            Some(format!("{} = {count}", self.exact_text(up_to)))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                (RuleKind::Field, table.unwrap_or_default(), String::new())
            }
            Holds::HingeName => {
                let hinge = self.standard_hinge(standard)?;
                (RuleKind::Field, hinge.name.clone(), String::new())
            }
            Holds::HingeSize => {
                let hinge = self.standard_hinge(standard)?;
                let (text, reading) = hinge
                    .dimensions
                    .get(field.key)
                    .copied()
                    .map(size)
                    .unwrap_or_default();
                (RuleKind::Field, text, reading)
            }
        };
        Some(RuleRow {
            key: format!("{}.{}", group.key, field.key),
            label: field.label,
            kind,
            text,
            reading,
        })
    }

    /// The line under a size: `= 1/8"`, or `≈ 23/32"` when the display
    /// precision rounds it.
    fn rule_reading(&self, length: Length) -> String {
        let shown = length.show(self.units(), self.precision());
        let sign = if shown.exact { '=' } else { '≈' };
        format!("{sign} {}", shown.text)
    }

    fn standard_hinge(&self, standard: &Standard) -> Option<&Hardware> {
        let id = standard.rules.get("hinges")?.get("hardware")?;
        let id: Id = serde_json::from_value(id.clone()).ok()?;
        self.document.library.hardware.iter().find(|h| h.id == id)
    }

    fn standard_hinge_mut(&mut self) -> Result<&mut Hardware, String> {
        let id = self
            .edited_standard()
            .and_then(|s| self.standard_hinge(s))
            .map(|h| h.id)
            .ok_or("The standard's hinge is not in the library")?;
        self.document
            .library
            .hardware
            .iter_mut()
            .find(|h| h.id == id)
            .ok_or_else(|| "The standard's hinge is not in the library".into())
    }

    fn set_hinge_name(&mut self, text: &str) -> Result<(), String> {
        let name = text.trim();
        if name.is_empty() {
            return Err("A name cannot be empty".into());
        }
        let hinge = self.standard_hinge_mut()?;
        if hinge.name != name {
            name.clone_into(&mut hinge.name);
            self.modified = true;
        }
        Ok(())
    }

    fn set_hinge_size(&mut self, key: &str, size: Length) -> Result<(), String> {
        let hinge = self.standard_hinge_mut()?;
        if hinge.dimensions.insert(key.to_owned(), size) != Some(size) {
            self.modified = true;
        }
        Ok(())
    }

    /// Stores a rule, or removes it when `value` is `None`, unless the
    /// evaluator could no longer read the standard.
    fn store_rule(&mut self, group: &str, field: &str, value: Option<Value>) -> Result<(), String> {
        let index = self
            .selected_standard()
            .ok_or("There is no standard to edit")?;
        let standard = &mut self.document.library.standards[index];
        let Some(Value::Object(rules)) = standard.rules.get_mut(group) else {
            return Err("The standard does not have these rules".into());
        };
        let old = rules.get(field).cloned();
        if old == value {
            return Ok(());
        }
        let set = |rules: &mut serde_json::Map<String, Value>, value: Option<Value>| match value {
            Some(value) => rules.insert(field.to_owned(), value),
            None => rules.remove(field),
        };
        set(rules, value);
        if let Err(message) = Rules::of(standard) {
            if let Some(Value::Object(rules)) = standard.rules.get_mut(group) {
                set(rules, old);
            }
            return Err(capitalize(&message));
        }
        self.modified = true;
        Ok(())
    }
}

fn find_field(key: &str) -> Option<(&'static Group, &'static Field)> {
    let (group, field) = key.split_once('.')?;
    let group = GROUPS.iter().find(|g| g.key == group)?;
    let field = group.fields.iter().find(|f| f.key == field)?;
    Some((group, field))
}

/// What the user typed for a rule that holds `holds`, as the file keeps it.
fn parse_rule(holds: Holds, text: &str, designer: &Designer) -> Result<Option<Value>, String> {
    let positive = |text: &str| {
        let size = parse_size(text, designer)?;
        if size <= Length::ZERO {
            return Err("Must be greater than zero".to_owned());
        }
        Ok(size)
    };
    Ok(Some(match holds {
        Holds::Size => json!(positive(text)?),
        Holds::SizeOrZero => {
            let size = parse_size(text, designer)?;
            if size < Length::ZERO {
                return Err("Cannot be negative".into());
            }
            json!(size)
        }
        Holds::OptionalSize(_) => {
            if text.trim().is_empty() {
                return Ok(None);
            }
            json!(positive(text)?)
        }
        Holds::Percent => match text.trim().parse::<u32>() {
            Ok(percent) if percent <= 100 => json!(percent),
            _ => return Err("Must be a whole number from 0 to 100".into()),
        },
        Holds::Sizes => {
            let mut sizes = text
                .split(',')
                .map(positive)
                .collect::<Result<Vec<_>, _>>()?;
            sizes.sort_unstable();
            sizes.dedup();
            json!(sizes)
        }
        Holds::HingeTable => {
            let mut rows = text
                .split(',')
                .map(|pair| {
                    let (height, count) = pair
                        .split_once('=')
                        .ok_or("Write each height and its hinges as 40 = 2, 60 = 3")?;
                    let height = positive(height)?;
                    match count.trim().parse::<u32>() {
                        Ok(count) if (1..=MAX_HINGES).contains(&count) => Ok((height, count)),
                        _ => Err(format!("A door takes 1 to {MAX_HINGES} hinges")),
                    }
                })
                .collect::<Result<Vec<_>, String>>()?;
            rows.sort_unstable();
            json!(
                rows.iter()
                    .map(|(height, count)| json!({ "upTo": height, "count": count }))
                    .collect::<Vec<_>>()
            )
        }
        Holds::HingeName | Holds::HingeSize | Holds::Choice(_) => {
            return Err("Not a value for this rule".into());
        }
    }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use opencabinet_eval::rules::Back;
    use opencabinet_eval::{Part, evaluate};
    use opencabinet_format::Document;
    use opencabinet_model::Units;

    use super::*;

    /// Millimeters with only the decimals they need.
    fn mm(length: Length) -> String {
        let nm = length.nm();
        let text = format!("{}.{:06}", nm / 1_000_000, (nm % 1_000_000).abs());
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    }

    /// The edited cabinet's parts with `role`.
    fn parts(designer: &Designer, role: &str) -> Vec<Part> {
        let product = designer.product().id;
        evaluate(&designer.document.project, &designer.document.library)
            .parts
            .into_iter()
            .filter(|p| p.product == product && p.role == role)
            .collect()
    }

    /// The `length × width` of the edited cabinet's first part with `role`.
    fn size(designer: &Designer, role: &str) -> String {
        let part = &parts(designer, role)[0];
        format!("{} × {}", mm(part.length), mm(part.width))
    }

    /// What stops a cabinet from being evaluated.
    fn problems(designer: &Designer) -> Vec<String> {
        evaluate(&designer.document.project, &designer.document.library)
            .problems
            .into_iter()
            .filter(|p| p.blocking)
            .map(|p| p.message)
            .collect()
    }

    fn row(designer: &Designer, key: &str) -> RuleRow {
        designer
            .rule_rows()
            .into_iter()
            .find(|r| r.key == key)
            .unwrap_or_else(|| panic!("no row {key}"))
    }

    fn keys(designer: &Designer) -> Vec<String> {
        designer.rule_rows().into_iter().map(|r| r.key).collect()
    }

    fn rules(designer: &Designer) -> Rules {
        Rules::of(designer.edited_standard().unwrap()).unwrap()
    }

    fn kitchen() -> Designer {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kitchen.ocab");
        Designer::open(&fixture).unwrap()
    }

    #[test]
    fn a_new_face_frame_project_matches_the_sizes_worked_out_by_hand() {
        let designer = Designer::with_method(Units::Imperial, Method::FaceFrame);
        assert!(designer.issues().is_empty(), "{:?}", designer.issues());
        assert!(problems(&designer).is_empty(), "{:?}", problems(&designer));
        assert_eq!(designer.cabinet_standard(), Some(1));
        assert_eq!(designer.standard_name(), "Face frame");

        // 30" × 34-1/2" with a 4" toe kick: a face of 762 × 774.7.
        // Stiles 38.1 wide run the full height.
        assert_eq!(size(&designer, "frameStile"), "774.7 × 38.1");
        // The rails fit between them: 762 - 2 × 38.1 = 685.8.
        assert_eq!(size(&designer, "frameRail"), "685.8 × 38.1");
        // The opening is 685.8 × (774.7 - 2 × 38.1) = 698.5. A 6" drawer
        // and a 38.1 rail leave 508 for the doors.
        // The drawer front covers the frame by 12.7 all around.
        assert_eq!(size(&designer, "drawerFront"), "711.2 × 177.8");
        // A pair of doors shares 685.8 + 2 × 12.7 = 711.2 with a 3.175 gap:
        // each is 354.0125 wide and 508 + 2 × 12.7 = 533.4 high.
        assert_eq!(size(&designer, "door"), "533.4 × 354.0125");
        // The frame comes from the solid wood.
        let solid = designer.document.library.materials[3].id;
        assert_eq!(parts(&designer, "frameStile")[0].material, solid);
        // Frameless is still the default.
        assert_eq!(Designer::default().standard_name(), "Frameless");
    }

    #[test]
    fn a_cabinet_can_change_standard_and_keep_its_materials() {
        let mut designer = Designer::default();
        assert_eq!(designer.standards(), ["Frameless", "Face frame"]);
        // A frameless pair: (762 - 2 × 3.175) / 2 = 377.825 wide.
        assert!(size(&designer, "door").ends_with("× 377.825"));
        assert_eq!(parts(&designer, "frameStile").len(), 0);

        designer.set_cabinet_standard(1);
        assert!(designer.modified());
        assert_eq!(designer.selected_standard(), Some(1));
        assert!(size(&designer, "door").ends_with("× 354.0125"));
        assert!(problems(&designer).is_empty(), "{:?}", problems(&designer));
        assert!(
            designer
                .material_roles()
                .iter()
                .any(|r| r.key == "faceFrame")
        );

        // The tab follows the cabinet being edited.
        designer.add();
        designer.set_cabinet_standard(0);
        assert_eq!(designer.selected_standard(), Some(0));
        designer.select(0);
        assert_eq!(designer.selected_standard(), Some(1));
        assert_eq!(
            designer.standard_note(),
            "Face frame · used by 1 of 2 cabinets"
        );
    }

    #[test]
    fn rules_change_the_parts_by_hand_worked_sizes() {
        let mut designer = Designer::default();
        // Frameless pair with a 1/4" gap: (762 - 2 × 6.35) / 2 = 374.65.
        designer.edit_rule("fronts.gap", "1/4").unwrap();
        assert!(designer.modified());
        assert!(size(&designer, "door").ends_with("× 374.65"));
        assert_eq!(row(&designer, "fronts.gap").reading, "= 1/4\"");

        // A 3" toe kick makes the sides 25.4 longer: 876.3 - 76.2 = 800.1.
        let before = parts(&designer, "side")[0].length;
        assert_eq!(mm(before), "774.7");
        designer.edit_rule("toeKick.height", "3").unwrap();
        assert_eq!(mm(parts(&designer, "side")[0].length), "800.1");

        // A face frame's wider stiles narrow the rails between them:
        // 762 - 2 × 50.8 = 660.4.
        designer.set_cabinet_standard(1);
        designer.edit_rule("faceFrame.stileWidth", "2").unwrap();
        assert_eq!(size(&designer, "frameRail"), "660.4 × 38.1");
        // The top rail can differ; empty goes back to the rail width.
        designer.edit_rule("faceFrame.topRailWidth", "3").unwrap();
        assert_eq!(
            rules(&designer).face_frame.unwrap().top_rail_width,
            Some(Length::from_inches(3))
        );
        designer.edit_rule("faceFrame.topRailWidth", " ").unwrap();
        assert_eq!(rules(&designer).face_frame.unwrap().top_rail_width, None);
        assert_eq!(
            row(&designer, "faceFrame.topRailWidth").reading,
            "Empty: the rail width"
        );
    }

    #[test]
    fn impossible_values_are_refused_and_change_nothing() {
        let mut designer = Designer::default();
        let before = designer.document.library.clone();
        let refused = [
            ("toeKick.height", "tall", "Not a length"),
            ("toeKick.height", "-1", "Cannot be negative"),
            ("case.stretcherWidth", "0", "Must be greater than zero"),
            ("systemHoles.spacing", "0", "Must be greater than zero"),
            (
                "billOfMaterials.wastePercent",
                "101",
                "Must be a whole number from 0 to 100",
            ),
            ("drawers.slideLengths", "12, wide", "Not a length"),
            (
                "hinges.counts",
                "40",
                "Write each height and its hinges as 40 = 2, 60 = 3",
            ),
            ("hinges.counts", "40 = 0", "A door takes 1 to 10 hinges"),
            ("hinges.@name", " ", "A name cannot be empty"),
            ("hinges.cupDiameter", "0", "Must be greater than zero"),
            ("back.construction", "1", "Choose from the list"),
            ("nothing.here", "1", "There is no such rule"),
        ];
        for (key, text, message) in refused {
            assert_eq!(
                designer.edit_rule(key, text),
                Err(message.to_owned()),
                "{key}"
            );
        }
        assert_eq!(designer.document.library, before);
        assert!(!designer.modified());
        // Zero is fine where it means none.
        designer.edit_rule("toeKick.height", "0").unwrap();
    }

    #[test]
    fn lists_are_kept_shortest_first_and_reach_the_evaluator() {
        let mut designer = Designer::default();
        designer
            .edit_rule("drawers.slideLengths", "21, 15, 18, 15")
            .unwrap();
        assert_eq!(
            row(&designer, "drawers.slideLengths").text,
            "15\", 18\", 21\""
        );
        // A 24" deep cabinet keeps 1" behind the box: 21" slides, 533.4.
        assert_eq!(mm(parts(&designer, "drawerSide")[0].length), "533.4");

        assert_eq!(
            row(&designer, "hinges.counts").text,
            "40\" = 2, 60\" = 3, 80\" = 4"
        );
        designer
            .edit_rule("hinges.counts", "60 = 4, 20 = 3")
            .unwrap();
        assert_eq!(row(&designer, "hinges.counts").text, "20\" = 3, 60\" = 4");
        let counts = rules(&designer).hinges.unwrap().counts;
        assert_eq!(counts[0].up_to, Length::from_inches(20));
        assert_eq!(counts[0].count, 3);
    }

    #[test]
    fn the_hinge_is_edited_in_the_library() {
        let mut designer = Designer::default();
        assert_eq!(row(&designer, "hinges.@name").text, "Cup hinge 35 mm");
        designer.edit_rule("hinges.@name", "Soft close").unwrap();
        designer.edit_rule("hinges.cupDepth", "12 mm").unwrap();
        let hinge = &designer.document.library.hardware[0];
        assert_eq!(hinge.name, "Soft close");
        assert_eq!(hinge.dimensions[CUP_DEPTH], Length::from_mm(12));
        // Both starter standards share it.
        designer.select_standard(1);
        assert_eq!(row(&designer, "hinges.@name").text, "Soft close");
    }

    #[test]
    fn only_what_the_standard_can_use_is_offered() {
        let mut designer = Designer::default();
        let frameless = keys(&designer);
        assert!(frameless.contains(&"fronts.gap".to_owned()));
        assert!(!frameless.contains(&"fronts.overlay".to_owned()));
        assert!(!frameless.contains(&"faceFrame".to_owned()));

        designer.choose_rule("back.construction", 1);
        assert!(designer.modified());
        assert!(!keys(&designer).contains(&"back.grooveDepth".to_owned()));
        assert_eq!(rules(&designer).back, Back::Applied);
        // What was typed comes back with the choice.
        designer.edit_rule("toeKick.height", "4").unwrap();
        designer.choose_rule("back.construction", 0);
        assert_eq!(row(&designer, "back.grooveDepth").text, "3/8\"");

        designer.select_standard(1);
        assert_eq!(
            row(&designer, "fronts.overlay").kind,
            RuleKind::Choice {
                options: OVERLAYS,
                chosen: Some(0)
            }
        );
        designer.choose_rule("fronts.overlay", 1);
        assert!(!keys(&designer).contains(&"fronts.overlayAmount".to_owned()));
        // An inset door stands back by the gap: 685.8 wide opening,
        // (685.8 - 3 × 3.175) / 2 = 338.1375.
        designer.set_cabinet_standard(1);
        assert!(size(&designer, "door").ends_with("× 338.1375"));
        designer.choose_rule("fronts.overlay", 0);
        assert_eq!(row(&designer, "fronts.overlayAmount").text, "1/2\"");
    }

    #[test]
    fn a_group_the_standard_lacks_can_be_added_and_the_rest_is_kept() {
        let mut designer = Designer::default();
        let standard = &mut designer.document.library.standards[0];
        standard.rules.remove("drawers");
        standard.rules.remove("hinges");
        standard
            .rules
            .insert("futureRules".into(), json!({ "a": 1 }));
        if let Some(Value::Object(toe)) = standard.rules.get_mut("toeKick") {
            toe.insert("futureField".into(), json!(7));
        }
        assert_eq!(row(&designer, "drawers").kind, RuleKind::MissingGroup);
        assert!(!keys(&designer).contains(&"drawers.slideLengths".to_owned()));
        assert!(problems(&designer).iter().any(|p| p.contains("drawers")));

        designer.add_rule_group("drawers");
        assert_eq!(row(&designer, "drawers").kind, RuleKind::Group);
        assert_eq!(
            rules(&designer).drawers.unwrap().rear_clearance,
            Length::from_inches(1)
        );
        designer.add_rule_group("hinges");
        assert_eq!(row(&designer, "hinges.@name").text, "Cup hinge 35 mm");
        assert_eq!(designer.document.library.hardware.len(), 1);

        // Rules the window does not show survive editing and saving.
        designer.edit_rule("toeKick.height", "3").unwrap();
        let dir = std::env::temp_dir().join(format!("opencabinet-standard-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("standard.ocab");
        designer.save_as(&path).unwrap();
        let reopened = Document::load(&path).unwrap();
        let rules = &reopened.library.standards[0].rules;
        assert_eq!(rules["futureRules"], json!({ "a": 1 }));
        assert_eq!(rules["toeKick"]["futureField"], 7);
        assert_eq!(rules["toeKick"]["height"], json!(Length::from_inches(3)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_shop_file_shows_its_own_standard() {
        let mut designer = kitchen();
        designer.select(3);
        assert_eq!(designer.label(), "W30");
        assert_eq!(designer.standard_name(), "Shop face frame");
        assert_eq!(row(&designer, "fronts.overlayAmount").text, "1/2\"");
        assert_eq!(row(&designer, "faceFrame.stileWidth").text, "1-1/2\"");
        // 37 mm is not a 32nd of an inch, and stays exact.
        assert_eq!(row(&designer, "systemHoles.frontSetback").text, "37 mm");
        assert!(
            row(&designer, "systemHoles.frontSetback")
                .reading
                .starts_with('≈')
        );
    }

    #[test]
    fn names_must_be_given_and_not_repeated() {
        let mut designer = Designer::default();
        assert_eq!(
            designer.set_standard_name(" "),
            Err("A name cannot be empty".into())
        );
        assert_eq!(
            designer.set_standard_name("Face frame"),
            Err("Another standard is already named Face frame".into())
        );
        designer.set_standard_name(" Shop 32 mm ").unwrap();
        assert_eq!(designer.standard_name(), "Shop 32 mm");
    }
}
