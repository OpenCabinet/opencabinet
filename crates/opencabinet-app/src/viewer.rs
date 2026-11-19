// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The 3D view in the window: what it shows, the camera, the mouse, and
//! the picture the renderer draws for the window to show.
//!
//! - Drag with the left button to turn around the scene; with the right or
//!   middle button, or with Shift, to slide it; scroll to come closer.
//! - Click a part to select it and read what it is; click empty space to
//!   clear the selection.
//! - Double-click a cabinet to see it alone; double-click empty space, or
//!   choose Room, to see the room again.

use opencabinet_eval::{Face, Operation};
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

/// The 3D view's state.
#[derive(Debug, Default)]
pub struct Viewer {
    /// The renderer, made the first time the view is shown; the error
    /// when none can be made.
    renderer: Option<Result<Renderer, String>>,
    /// The scene, `None` when the project changed since it was built.
    model: Option<Model>,
    /// The room it was built for.
    room: Option<usize>,
    /// What the view shows.
    pub view: View,
    camera: Camera,
    /// Whether the camera has been placed on the scene yet.
    framed: bool,
    /// Whether the mesh must be sent to the GPU again.
    stale_mesh: bool,
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
            self.stale_mesh = true;
        }
        self.model.as_ref().expect("just built")
    }

    /// Changes what the view shows.
    pub fn change(&mut self, change: impl FnOnce(&mut View)) {
        change(&mut self.view);
        self.stale_mesh = true;
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

    /// Draws the view; the picture, or why it could not be drawn.
    ///
    /// # Errors
    /// When there is no renderer or it fails: a message for the user.
    pub fn draw(&mut self, designer: &Designer) -> Result<Image, String> {
        if !self.framed {
            self.fit(designer);
        }
        let view = self.view;
        let mesh = self.stale_mesh.then(|| self.model(designer).mesh(&view));
        let renderer = self
            .renderer
            .get_or_insert_with(Renderer::new)
            .as_mut()
            .map_err(|e| e.clone())?;
        if let Some(mesh) = mesh {
            renderer.upload(&mesh);
            self.stale_mesh = false;
        }
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
        renderer.draw(&frame, width, height, buffer.make_mut_bytes())?;
        Ok(Image::from_rgba8(buffer))
    }

    /// The renderer's adapter, once there is one, for the user.
    #[must_use]
    pub fn adapter(&self) -> Option<String> {
        match &self.renderer {
            Some(Ok(renderer)) if renderer.software => Some(format!(
                "Drawn without a graphics card ({}), which is slower",
                renderer.adapter
            )),
            Some(Ok(renderer)) => Some(renderer.adapter.clone()),
            _ => None,
        }
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

    /// The button came up; whether it was a click, which selects the part
    /// under the mouse.
    pub fn release(&mut self, x: f32, y: f32, designer: &Designer) -> bool {
        let Some(drag) = self.drag.take() else {
            return false;
        };
        if drag.moved >= CLICK || drag.pan {
            return false;
        }
        let picked = self.pick(x, y, designer);
        self.change(|view| view.selected = picked);
        true
    }

    /// The part under `(x, y)`, if any.
    pub fn pick(&mut self, x: f32, y: f32, designer: &Designer) -> Option<Pick> {
        let ray = self.camera.ray(x, y, self.size.0, self.size.1);
        let view = self.view;
        self.model(designer).pick(&view, ray)
    }

    /// The cabinet under `(x, y)` is double-clicked: it is shown alone, or
    /// with nothing there, the room again. Returns the product's index in
    /// the project, to edit it.
    pub fn double_click(&mut self, x: f32, y: f32, designer: &Designer) -> Option<usize> {
        let picked = self.pick(x, y, designer);
        let product = picked.and_then(|p| {
            let id = self.model.as_ref()?.instances[p.instance].product;
            designer.project().products.iter().position(|q| q.id == id)
        });
        match picked {
            Some(pick) => self.change(|view| {
                view.isolated = Some(pick.instance);
                view.selected = Some(pick);
            }),
            None => self.change(|view| {
                view.isolated = None;
                view.selected = None;
                view.explode = 0.0;
            }),
        }
        self.fit(designer);
        product
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
        let mut holes = 0;
        let mut grooves = 0;
        for operation in &part.operations {
            match operation {
                Operation::Hole { .. } => holes += 1,
                Operation::HolePattern { count, .. } => holes += count,
                Operation::Groove { .. } => grooves += 1,
            }
        }
        let faces: Vec<&str> = part
            .operations
            .iter()
            .map(|o| match o {
                Operation::Hole { face, .. }
                | Operation::HolePattern { face, .. }
                | Operation::Groove { face, .. } => match face {
                    Face::A => "A",
                    Face::B => "B",
                },
            })
            .collect();
        let machined = match (holes, grooves) {
            (0, 0) => "nothing machined".to_owned(),
            _ => format!(
                "{holes} {}, {grooves} {} on face {}",
                if holes == 1 { "hole" } else { "holes" },
                if grooves == 1 { "groove" } else { "grooves" },
                if faces.contains(&"B") { "A and B" } else { "A" }
            ),
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
