// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The project's snapshots of library items (RFC-0001 §7 and §8).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Id, Length, Unknown};

/// Snapshots of every library item the project uses. An item's `id` and
/// `version` are those of the shop's master copy, so the software can
/// offer an update when the master changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    /// Sheet goods.
    #[serde(default)]
    pub materials: Vec<SheetMaterial>,
    /// Edge banding.
    #[serde(default)]
    pub edge_bands: Vec<EdgeBand>,
    /// Hinges, slides, pulls and the like.
    #[serde(default)]
    pub hardware: Vec<Hardware>,
    /// Construction standards.
    #[serde(default)]
    pub standards: Vec<Standard>,
    /// Sections from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// A sheet material such as plywood or melamine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetMaterial {
    /// The master's identity.
    pub id: Id,
    /// The master's version when the snapshot was taken.
    pub version: u32,
    /// The material's name.
    pub name: String,
    /// The thickness it is sold as, such as 3/4".
    pub nominal_thickness: Length,
    /// The thickness it really has; every derived size uses this one.
    pub actual_thickness: Length,
    /// Sheet length.
    pub length: Length,
    /// Sheet width.
    pub width: Length,
    /// Whether the sheet has a grain direction.
    pub grain: Grain,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// A sheet's grain. Closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Grain {
    /// No grain: parts may turn freely.
    None,
    /// Grain along the sheet's length.
    Length,
}

/// An edge band.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeBand {
    /// The master's identity.
    pub id: Id,
    /// The master's version when the snapshot was taken.
    pub version: u32,
    /// The band's name.
    pub name: String,
    /// Its thickness.
    pub thickness: Length,
    /// Its width.
    pub width: Length,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// A piece of hardware the evaluator places.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    /// The master's identity.
    pub id: Id,
    /// The master's version when the snapshot was taken.
    pub version: u32,
    /// Hinge, slide, pull, leveler, shelf pin or connector.
    pub category: String,
    /// The name the user gave it.
    pub name: String,
    /// The dimensions the evaluator needs, such as `cupDiameter`.
    #[serde(default)]
    pub dimensions: BTreeMap<String, Length>,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// A construction standard: how a shop builds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Standard {
    /// The master's identity.
    pub id: Id,
    /// The master's version when the snapshot was taken.
    pub version: u32,
    /// The standard's name.
    pub name: String,
    /// The standard this one was copied from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<Id>,
    /// Frameless or face frame.
    pub method: Method,
    /// The rule groups of RFC-0001 §7. Their schema belongs to the format
    /// specification, so they stay plain JSON until the evaluator reads them.
    #[serde(default)]
    pub rules: BTreeMap<String, Value>,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// The construction method. Closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Method {
    /// European-style boxes without a face frame.
    Frameless,
    /// Boxes with a face frame.
    FaceFrame,
}
