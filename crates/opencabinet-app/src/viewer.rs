// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A 3D view in the window: what it shows, its camera, the mouse, and the
//! picture drawn for the window to show. Each view of the window that
//! shows 3D has its own; they share one renderer.
//!
//! - Drag with the left button to turn around the scene; with the right or
//!   middle button, or with Shift, to slide it; scroll to come closer.
//! - Click a cabinet to edit it; click empty space to edit the project.
//! - Double-click a cabinet to go inside it: there, a click selects a part
//!   and describes it. Double-click empty space, or choose Room, to see the
//!   room again.

use opencabinet_eval::{Face, Operation};
use opencabinet_model::Id;
use opencabinet_view3d::{Camera, Frame, Mode, Model, Pick, Renderer, View};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

use crate::designer::Designer;

/// The view's display modes, in the order the window lists them.
pub const MODES: [(&str, Mode); 4] = [
    ("Textured", Mode::Textured),
    ("Shaded", Mode::Shaded),
    ("X-ray", Mode::XRay),
    ("Wireframe", Mode::Wireframe),
];

/// How far the mouse may move, in pixels, for a press and release to be a
/// click rather than a drag.
const CLICK: f32 = 4.0;

/// A drag in progress.
#[derive(Clone, Copy, Debug)]
struct Drag {
    /// Whether it slides the view rather than turning it.
    pan: bool,
    /// Where the mouse was last, in logical pixels.
    last: (f32, f32),
    /// How far it has moved since the press.
    moved: f32,
}

/// What a click in the view found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Click {
    /// A cabinet in the room, by its index in the project.
    Cabinet(usize),
    /// A part of the cabinet the view is inside.
    Part,
    /// Nothing; with the point of the floor under the mouse, in the
    /// room's plan millimeters.
    Empty(Option<(f64, f64)>),
}

/// A 3D view's state.
#[derive(Debug, Default)]
pub struct Viewer {
    /// The scene, `None` when the project changed since it was built.
    model: Option<Model>,
    /// The room it was built for.
    room: Option<usize>,
    /// What the view shows.
    pub view: View,
    camera: Camera,
    /// Whether the camera has been placed on the scene yet.
    framed: bool,
    /// The view's size in logical pixels, and pixels per logical pixel.
    size: (f32, f32),
    scale: f32,
    drag: Option<Drag>,
}

impl Viewer {
    /// The project changed: the scene is built again when next drawn.
    pub fn invalidate(&mut self) {
        self.model = None;
    }

    /// The view is `width` by `height` logical pixels, `scale` physical
    /// pixels each.
    pub fn resize(&mut self, width: f32, height: f32, scale: f32) {
        self.size = (width.max(1.0), height.max(1.0));
        self.scale = scale.max(0.5);
    }

    /// The scene of the project, built if needed.
    fn model(&mut self, designer: &Designer) -> &Model {
        let room = designer.selected_room();
        if self.model.is_none() || self.room != room {
            if self.room != room {
                self.view.isolated = None;
                self.framed = false;
            }
            let model = Model::build(designer.project(), designer.library(), room);
            // A selection or isolation that no longer exists is dropped.
            let instances = model.instances.len();
            if self.view.isolated.is_some_and(|i| i >= instances) {
                self.view.isolated = None;
            }
            if self
                .view
                .selected
                .is_some_and(|p| p.instance >= instances || p.part >= model.parts.len())
            {
                self.view.selected = None;
            }
            self.model = Some(model);
            self.room = room;
        }
        self.model.as_ref().expect("just built")
    }

    /// Changes what the view shows.
    pub fn change(&mut self, change: impl FnOnce(&mut View)) {
        change(&mut self.view);
    }

    /// Outlines the product `id`, the one being edited, if any.
    pub fn highlight(&mut self, id: Option<Id>) {
        self.change(|view| view.highlight = id);
    }

    /// Places the camera so the view shows all it shows.
    pub fn fit(&mut self, designer: &Designer) {
        let view = self.view;
        let (low, high) = self.model(designer).bounds(&view);
        self.camera.frame(low, high);
        if view.isolated.is_none() {
            self.camera.pitch = self.camera.pitch.max(0.55);
        }
        self.framed = true;
    }

    /// Draws the view with `renderer`, as its view `slot`.
    ///
    /// # Errors
    /// When the renderer fails: a message for the user.
    pub fn draw(
        &mut self,
        designer: &Designer,
        renderer: &mut Renderer,
        slot: usize,
    ) -> Result<Image, String> {
        if !self.framed {
            self.fit(designer);
        }
        let view = self.view;
        // A mesh is per slot, so a view that moved to another slot sends
        // its mesh again; building it is quick.
        let mesh = self.model(designer).mesh(&view);
        renderer.upload(slot, &mesh);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a window's size in pixels"
        )]
        let (width, height) = (
            (self.size.0 * self.scale).round().max(1.0) as u32,
            (self.size.1 * self.scale).round().max(1.0) as u32,
        );
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
        #[allow(clippy::cast_precision_loss, reason = "a window's size in pixels")]
        let frame = Frame {
            view_projection: self.camera.view_projection(width as f32 / height as f32),
            eye: self.camera.eye().to_array(),
            textured: view.mode == Mode::Textured,
            ghost_opacity: 0.22,
        };
        renderer.draw(slot, &frame, width, height, buffer.make_mut_bytes())?;
        Ok(Image::from_rgba8(buffer))
    }

    /// A mouse button went down at `(x, y)`: `button` 0 left, 1 right, 2
    /// middle.
    pub fn press(&mut self, x: f32, y: f32, button: i32, shift: bool) {
        self.drag = Some(Drag {
            pan: button != 0 || shift,
            last: (x, y),
            moved: 0.0,
        });
    }

    /// The mouse moved to `(x, y)` with a button down; whether the view
    /// changed.
    pub fn drag_to(&mut self, x: f32, y: f32) -> bool {
        let Some(drag) = &mut self.drag else {
            return false;
        };
        let (dx, dy) = (x - drag.last.0, y - drag.last.1);
        drag.last = (x, y);
        drag.moved += dx.abs() + dy.abs();
        if drag.moved < CLICK {
            return false;
        }
        if drag.pan {
            self.camera.pan(dx, dy, self.size.1);
        } else {
            self.camera.orbit(dx, dy);
        }
        true
    }

    /// The button came up at `(x, y)`: what it clicked, or `None` after a
    /// drag. Inside a cabinet, a click selects a part or clears the
    /// selection.
    pub fn release(&mut self, x: f32, y: f32, designer: &Designer) -> Option<Click> {
        let drag = self.drag.take()?;
        if drag.moved >= CLICK || drag.pan {
            return None;
        }
        let picked = self.pick(x, y, designer);
        if self.view.isolated.is_some() {
            self.change(|view| view.selected = picked);
            return Some(if picked.is_some() {
                Click::Part
            } else {
                Click::Empty(None)
            });
        }
        self.change(|view| view.selected = None);
        Some(match picked.and_then(|p| self.product_index(p, designer)) {
            Some(index) => Click::Cabinet(index),
            None => Click::Empty(self.floor_point(x, y)),
        })
    }

    /// The point of the floor under `(x, y)`, in the plan's millimeters.
    #[must_use]
    pub fn floor_point(&self, x: f32, y: f32) -> Option<(f64, f64)> {
        let ray = self.camera.ray(x, y, self.size.0, self.size.1);
        if ray.direction.y >= -1e-6 {
            return None;
        }
        let t = -ray.origin.y / ray.direction.y;
        let p = ray.origin + ray.direction * t;
        Some((f64::from(p.x), f64::from(p.z)))
    }

    fn product_index(&self, pick: Pick, designer: &Designer) -> Option<usize> {
        let id = self.model.as_ref()?.instances.get(pick.instance)?.product;
        designer.project().products.iter().position(|q| q.id == id)
    }

    /// The part under `(x, y)`, if any.
    pub fn pick(&mut self, x: f32, y: f32, designer: &Designer) -> Option<Pick> {
        let ray = self.camera.ray(x, y, self.size.0, self.size.1);
        let view = self.view;
        self.model(designer).pick(&view, ray)
    }

    /// `(x, y)` is double-clicked: the view goes inside the cabinet there,
    /// or with nothing there, back to the room. Returns the cabinet's index
    /// in the project, to edit it.
    pub fn double_click(&mut self, x: f32, y: f32, designer: &Designer) -> Option<usize> {
        let picked = self.pick(x, y, designer);
        let product = picked.and_then(|p| self.product_index(p, designer));
        match picked {
            Some(pick) => self.change(|view| {
                view.isolated = Some(pick.instance);
                view.selected = None;
            }),
            None => self.leave(),
        }
        self.fit(designer);
        product
    }

    /// Back to the room from inside a cabinet.
    pub fn leave(&mut self) {
        self.change(|view| {
            view.isolated = None;
            view.selected = None;
            view.explode = 0.0;
        });
    }

    /// Comes closer by `steps` notches of the wheel.
    pub fn zoom(&mut self, steps: f32) {
        self.camera.zoom(steps);
    }

    /// The selected part, described for the user.
    #[must_use]
    pub fn describe(&self, designer: &Designer) -> String {
        let (Some(pick), Some(model)) = (self.view.selected, &self.model) else {
            return String::new();
        };
        let Some(part) = model.parts.get(pick.part) else {
            return String::new();
        };
        let instance = &model.instances[pick.instance];
        let material = designer
            .library()
            .materials
            .iter()
            .find(|m| m.id == part.material)
            .map_or("", |m| m.name.as_str());
        let banded = part.edges.iter().filter(|e| e.is_some()).count();
        let (mut holes, mut grooves, mut face_b, mut edge_holes) = (0, 0, false, 0);
        for operation in &part.operations {
            let face = match operation {
                Operation::EdgeHole { .. } => {
                    edge_holes += 1;
                    continue;
                }
                Operation::Hole { face, .. } => {
                    holes += 1;
                    face
                }
                Operation::HolePattern { count, face, .. } => {
                    holes += count;
                    face
                }
                Operation::Groove { face, .. } => {
                    grooves += 1;
                    face
                }
            };
            face_b |= *face == Face::B;
        }
        let mut machined = match (holes, grooves) {
            (0, 0) => Vec::new(),
            _ => vec![format!(
                "{holes} {}, {grooves} {} on face {}",
                if holes == 1 { "hole" } else { "holes" },
                if grooves == 1 { "groove" } else { "grooves" },
                if face_b { "A and B" } else { "A" }
            )],
        };
        if edge_holes > 0 {
            machined.push(format!(
                "{edge_holes} {} in edges",
                if edge_holes == 1 { "hole" } else { "holes" }
            ));
        }
        let machined = if machined.is_empty() {
            "nothing machined".to_owned()
        } else {
            machined.join(", ")
        };
        format!(
            "{} · {}\n{material}\n{} × {} × {}\n{} · {machined}",
            instance.label,
            opencabinet_reports::part_name(part),
            designer.show(part.length),
            designer.show(part.width),
            designer.show(part.thickness),
            match banded {
                0 => "no edges banded".to_owned(),
                1 => "1 edge banded".to_owned(),
                n => format!("{n} edges banded"),
            },
        )
    }
}
