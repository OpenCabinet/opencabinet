// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! OpenCabinet's 3D view (ADR-0016): the room, its walls and floor, and
//! every placed product built part by part, with the grain of its wood,
//! its edges, what is machined into it and its hardware.
//!
//! [`scene`] turns a project into triangles and lines and finds the part
//! under the mouse; [`camera`] orbits it; [`render`] draws it with wgpu
//! into an image the window shows. Only [`render`] needs a GPU, so the
//! rest is tested anywhere.

pub mod camera;
pub mod math;
pub mod render;
pub mod scene;

pub use crate::camera::Camera;
pub use crate::render::{Frame, Renderer};
pub use crate::scene::{Instance, Mesh, Mode, Model, Pick, View};
