// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reports made from a project's evaluation (RFC-0002's reports layer):
//! the cut list and the bill of materials as rows of text in the user's
//! units, as CSV and as PDF; the parts as DXF; and the walls' elevations as
//! DXF and PDF.

mod bill_of_materials;
mod csv;
mod cut_list;
pub mod elevations;
mod parts_dxf;
mod pdf;
mod sketch;

pub use bill_of_materials::{BOARD_WIDTH, Bom, BomRow, bill_of_materials};
pub use csv::{Separator, materials_to_csv, to_csv};
pub use cut_list::{CutList, CutRow, cut_list};
pub use parts_dxf::{ALL_PARTS, parts_dxf};
pub use pdf::{Fonts, Paper, cut_list_pdf, elevations_pdf};
pub use sketch::{DxfFile, Shape};
