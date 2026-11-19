// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The project, its settings and its rooms (RFC-0001 §4 and §5).

use serde::{Deserialize, Serialize};

use crate::product::Product;
use crate::{Angle, Extensions, Id, Length, Units, Unknown};

/// A job: everything a shop designs for one client, except the library
/// snapshots, which are kept apart in [`Library`](crate::Library).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// The project's identity.
    pub id: Id,
    /// The job name.
    pub name: String,
    /// Free text about the job. The model has no fields for personal data.
    #[serde(default)]
    pub notes: String,
    /// How the project is shown and what new products start from.
    pub settings: Settings,
    /// The rooms, with their walls and the products placed in them.
    #[serde(default)]
    pub rooms: Vec<Room>,
    /// Every product, placed or not.
    #[serde(default)]
    pub products: Vec<Product>,
    /// The project's revision log.
    #[serde(default)]
    pub revisions: Vec<Revision>,
    /// Plugin data.
    #[serde(default)]
    pub extensions: Extensions,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// Display preferences and defaults; they never change stored values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The units lengths are typed and shown in.
    pub units: Units,
    /// The display precision, itself a length: 1/32" or 0.1 mm.
    pub precision: Length,
    /// The construction standard new products start with.
    pub default_standard: Id,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// One entry of the revision log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    /// The revision's label, such as "A".
    pub label: String,
    /// The date, as `YYYY-MM-DD`.
    pub date: String,
    /// Who issued it.
    pub author: String,
    /// What changed.
    pub description: String,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// A room with its walls and the products placed in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    /// The room's identity.
    pub id: Id,
    /// The room's name, such as "Kitchen".
    pub name: String,
    /// The room's walls.
    #[serde(default)]
    pub walls: Vec<Wall>,
    /// The products placed in the room.
    #[serde(default)]
    pub placements: Vec<Placement>,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// A point in plan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    /// Along the plan's x axis.
    pub x: Length,
    /// Along the plan's y axis.
    pub y: Length,
}

/// A point in space, z up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point3 {
    /// Along the plan's x axis.
    pub x: Length,
    /// Along the plan's y axis.
    pub y: Length,
    /// Up from the floor.
    pub z: Length,
}

/// A wall in plan. Its front side is on the left walking from start to end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wall {
    /// The wall's identity.
    pub id: Id,
    /// The wall's name, such as "North".
    pub name: String,
    /// Where the wall starts.
    pub start: Point,
    /// Where the wall ends.
    pub end: Point,
    /// Floor to ceiling.
    pub height: Length,
    /// Front face to back face.
    pub thickness: Length,
    /// Windows, doors and pass-throughs.
    #[serde(default)]
    pub openings: Vec<WallOpening>,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// A window, door or pass-through in a wall.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallOpening {
    /// The opening's identity.
    pub id: Id,
    /// What kind of opening it is.
    pub kind: WallOpeningKind,
    /// From the wall's start to the opening's left side.
    pub offset: Length,
    /// The opening's width.
    pub width: Length,
    /// The opening's height.
    pub height: Length,
    /// From the floor to the opening's bottom.
    pub sill_height: Length,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}

/// What kind of opening a wall has. Open: new kinds may arrive in a minor
/// version and are kept as [`WallOpeningKind::Other`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WallOpeningKind {
    /// A window.
    Window,
    /// A door.
    Door,
    /// An opening without a door.
    PassThrough,
    /// A kind from a newer version.
    #[serde(untagged)]
    Other(String),
}

/// Puts a product in a room: on a wall (`wall`, `offset`, `elevation`) or
/// free (`position`, `rotation`). Validation checks that exactly one form
/// is used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    /// The placement's identity.
    pub id: Id,
    /// The product placed.
    pub product: Id,
    /// The wall the product's back rests against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall: Option<Id>,
    /// From the wall's start to the product's left side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<Length>,
    /// From the floor to the product's bottom.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation: Option<Length>,
    /// The product's origin, for islands and peninsulas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Point3>,
    /// The product's rotation about z, in [0°, 360°).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Angle>,
    /// Fields from a newer version, kept on save.
    #[serde(flatten)]
    pub unknown: Unknown,
}
