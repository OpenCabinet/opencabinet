// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Products and their fronts (RFC-0001 §6), and overrides (§10).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Extensions, Id, Length, Unknown};

/// One cabinet, filler or end panel as the user designed it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    /// The product's identity.
    pub id: Id,
    /// The shop's item number, such as "B30".
    pub label: String,
    /// The product type and its major version, such as `base-cabinet@1`.
    #[serde(rename = "type")]
    pub product_type: String,
    /// The construction standard, a snapshot in the library.
    pub standard: Id,
    /// Overall width.
    pub width: Length,
    /// Overall height.
    pub height: Length,
    /// Overall depth.
    pub depth: Length,
    /// How many, for a product that is not placed; placed products count
    /// their placements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<u32>,
    /// Values for the parameters the product type declares. Their types
    /// come from that declaration, so they stay plain JSON here.
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
    /// Material role, such as `case`, to library item.
    #[serde(default)]
    pub materials: BTreeMap<String, Id>,
    /// The front layout; fillers and end panels have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front: Option<FrontNode>,
    /// Changes to the derived parts.
    #[serde(default)]
    pub overrides: Vec<Override>,
    /// Plugin data.
    #[serde(default)]
    pub extensions: Extensions,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// A node of the front split tree: a split with children, or an opening
/// with an insert. Validation checks that it is exactly one of the two.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontNode {
    /// The node's identity, which never changes as the tree is edited.
    pub id: Id,
    /// The node's size within its parent split; the root has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Size>,
    /// How the node is split, if it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<SplitDirection>,
    /// The parts of a split node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<FrontNode>,
    /// What fills an opening.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert: Option<Insert>,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// A child's size within its split: `{"length": nm}` or `{"share": n}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Size {
    /// A fixed length.
    Length(Length),
    /// A share of the space the fixed lengths leave.
    Share(u32),
}

/// How a node is split. Closed: changes only in a major version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SplitDirection {
    /// Stacked top to bottom.
    Horizontal,
    /// Side by side.
    Vertical,
}

/// What fills an opening of the front.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Insert {
    /// What kind of insert it is.
    pub kind: InsertKind,
    /// The style of a door, drawer front or false front.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<FrontStyle>,
    /// How many doors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u8>,
    /// The hinge side of a single door.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hinge_side: Option<Side>,
    /// How many shelves in an open section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shelves: Option<u8>,
    /// Changes to the hardware the standard places.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adjustments: Vec<Adjustment>,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// What kind of insert fills an opening. Open: see [`WallOpeningKind`](crate::WallOpeningKind).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsertKind {
    /// One or two doors.
    Doors,
    /// A drawer.
    Drawer,
    /// A fixed panel that looks like a drawer front.
    FalseFront,
    /// An open section, optionally with shelves.
    Open,
    /// An empty space for an appliance.
    Appliance,
    /// Nothing.
    None,
    /// A kind from a newer version.
    #[serde(untagged)]
    Other(String),
}

/// The style of a door or front. Open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrontStyle {
    /// A single panel.
    Slab,
    /// Two stiles, two rails and a panel.
    FivePiece,
    /// A style from a newer version.
    #[serde(untagged)]
    Other(String),
}

/// Left or right. Closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    /// Left, seen from the front.
    Left,
    /// Right, seen from the front.
    Right,
}

/// A change to the hardware the standard places on an insert.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Adjustment {
    /// The adjustment's identity.
    pub id: Id,
    /// The key of the derived hardware, such as `hinges/left`.
    pub target: String,
    /// What to do.
    pub action: AdjustmentAction,
    /// Along the insert's edge, for `add` and `move`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Length>,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// What an adjustment does. Open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AdjustmentAction {
    /// Adds a piece of hardware.
    Add,
    /// Moves one.
    Move,
    /// Removes one.
    Remove,
    /// An action from a newer version.
    #[serde(untagged)]
    Other(String),
}

/// A change to the derived data of one product (RFC-0001 §10).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Override {
    /// The override's identity.
    pub id: Id,
    /// The key of the derived part or assembly, such as `case/bottom`.
    pub target: String,
    /// What to do.
    pub action: OverrideAction,
    /// The new value or the added element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// Why, shown in reports.
    pub note: String,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// What an override does. Open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverrideAction {
    /// Sets a property: material, edge band or size.
    Set,
    /// Adds an operation.
    AddOperation,
    /// Removes an operation.
    RemoveOperation,
    /// Leaves the part out.
    Exclude,
    /// Adds a part.
    AddPart,
    /// An action from a newer version.
    #[serde(untagged)]
    Other(String),
}
