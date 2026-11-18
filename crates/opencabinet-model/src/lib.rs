// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The data model shared by every OpenCabinet module (RFC-0001).
//!
//! This crate has no I/O: it defines values and the rules they obey.
//!
//! - Quantities: [`Length`] in integer nanometers (ADR-0009), [`Angle`] in
//!   integer microdegrees, and [`Id`], the stable identity of an authored
//!   entity.
//! - Authored data: the [`Project`] with its rooms and products, and the
//!   [`Library`] of snapshots it uses.
//! - [`validate()`]: the checks that need no evaluator.
//!
//! Every object keeps the fields this version does not know in `unknown`,
//! and every open enumeration keeps values it does not know in `Other`, so
//! a file written by a newer minor version or a plugin survives a save
//! (RFC-0001, invariant 7). Struct fields are written in declaration order.

mod angle;
mod id;
mod length;
mod library;
mod product;
mod project;
pub mod validate;

use std::collections::BTreeMap;

pub use angle::Angle;
pub use id::{Id, ParseIdError};
pub use length::{Length, ParseLengthError, Shown, Units};
pub use library::{EdgeBand, Grain, Hardware, Library, Method, SheetMaterial, Standard};
pub use product::{
    Adjustment, AdjustmentAction, FrontNode, FrontStyle, Insert, InsertKind, Override,
    OverrideAction, Product, Side, Size, SplitDirection,
};
pub use project::{
    Placement, Point, Point3, Project, Revision, Room, Settings, Wall, WallOpening, WallOpeningKind,
};
pub use validate::validate;

/// Fields this version does not know, written back unchanged after the
/// known ones, in key order.
pub type Unknown = BTreeMap<String, serde_json::Value>;

/// Plugin data keyed by a reverse-domain namespace (RFC-0001 §11).
pub type Extensions = BTreeMap<String, serde_json::Value>;
