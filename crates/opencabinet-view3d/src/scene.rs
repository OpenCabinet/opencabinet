// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the viewer shows, as triangles and lines: the room's floor and
//! walls, every placed product part by part, what is machined into the
//! parts, and the hardware, as simple shapes.
//!
//! The world is in millimeters: `x` and `z` are the plan's `x` and `y`, so
//! that seen from above it looks as the plan is drawn, and `y` is up from
//! the floor. A product's frame (see `opencabinet_eval::assembly`) turns
//! with its wall: its `x` runs along the wall from the wall's start, its
//! `z` toward the room.
//!
//! A [`Model`] is built once for each change to the project; a [`Mesh`] is
//! built from it for each change to the [`View`]: what is isolated,
//! selected, exploded or hidden, and how surfaces are drawn.

#![allow(
    clippy::many_single_char_names,
    reason = "geometry reads best as u, v, w and x, y, z"
)]

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use opencabinet_eval::{Axis, Edge, Face, FastenerKind, Operation, Part, evaluate, plan};
use opencabinet_model::{Id, Length, Library, Project, Wall};

use crate::math::{Ray, Vec3};

/// A corner of a triangle, as the GPU reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    /// Where it is, in the world.
    pub position: [f32; 3],
    /// The surface's normal.
    pub normal: [f32; 3],
    /// Millimeters on the surface: along the grain, then across it.
    pub uv: [f32; 2],
    /// The surface's color, linear, and its opacity.
    pub color: [f32; 4],
    /// What the surface is ([`Surface`]), a seed that varies its look, and
    /// its [flags](SELECTED).
    pub params: [f32; 4],
}

/// The end of a line, as the GPU reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct LineVertex {
    /// Where it is, in the world.
    pub position: [f32; 3],
    /// Its color, linear, and its opacity.
    pub color: [f32; 4],
}

/// A flag of [`Vertex::params`]: the surface belongs to the selected part.
pub const SELECTED: u32 = 1;
/// A flag of [`Vertex::params`]: the surface is seen through.
pub const GHOST: u32 = 2;

/// What a surface is, and so how it is shaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Surface {
    /// A plain color: paint, melamine, MDF.
    Plain = 0,
    /// Wood, its grain along the first coordinate.
    Wood = 1,
    /// A floor of planks.
    Floor = 2,
    /// A painted wall.
    Wall = 3,
    /// Window glass.
    Glass = 4,
    /// The inside of a hole or groove.
    Hole = 5,
    /// Steel.
    Metal = 6,
    /// The edge of plywood, its plies across the second coordinate.
    PlyEdge = 7,
    /// The ground under a product seen on its own, with a grid.
    Ground = 8,
    /// The soft shadow under a product.
    Shadow = 9,
}

impl Surface {
    /// The surface as the GPU reads it.
    #[must_use]
    pub const fn code(self) -> f32 {
        match self {
            Self::Plain => 0.0,
            Self::Wood => 1.0,
            Self::Floor => 2.0,
            Self::Wall => 3.0,
            Self::Glass => 4.0,
            Self::Hole => 5.0,
            Self::Metal => 6.0,
            Self::PlyEdge => 7.0,
            Self::Ground => 8.0,
            Self::Shadow => 9.0,
        }
    }
}

/// How surfaces are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Wood grain, melamine and steel, as they look.
    #[default]
    Textured,
    /// Each material in its color, without texture.
    Shaded,
    /// Everything seen through, to show what is inside.
    XRay,
    /// Only the parts' edges.
    Wireframe,
}

/// What the viewer shows, and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// How surfaces are drawn.
    pub mode: Mode,
    /// The instance shown on its own, if any.
    pub isolated: Option<usize>,
    /// The selected part.
    pub selected: Option<Pick>,
    /// Whether doors and drawer fronts are shown.
    pub fronts: bool,
    /// How far the parts are pulled apart, from 0 (assembled) to 1.
    pub explode: f32,
    /// The product whose edges are drawn in the selection's color: the
    /// one being edited.
    pub highlight: Option<Id>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            mode: Mode::Textured,
            isolated: None,
            selected: None,
            fronts: true,
            explode: 0.0,
            highlight: None,
        }
    }
}

/// A part of an instance, picked with the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pick {
    /// The instance, in [`Model::instances`].
    pub instance: usize,
    /// The part, in [`Model::parts`].
    pub part: usize,
}

/// A product where it stands: one placement of it in the room, or the
/// product alone when the project has no room.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    /// The placement, if it is placed.
    pub placement: Option<Id>,
    /// The product.
    pub product: Id,
    /// Its label, such as "B30".
    pub label: String,
    /// Its left, back, bottom corner.
    pub origin: Vec3,
    /// Its `x` axis in the world: along its wall.
    pub x: Vec3,
    /// Its `z` axis in the world: toward the room.
    pub z: Vec3,
    /// Its width, height and depth.
    pub size: Vec3,
    /// Its parts, in [`Model::parts`].
    pub parts: Vec<usize>,
    /// How high its sides stand off the floor: its toe kick.
    pub toe_kick: f32,
}

impl Instance {
    /// The point `p` of the product's frame, in the world.
    #[must_use]
    pub fn world(&self, p: Vec3) -> Vec3 {
        self.origin + self.x * p.x + Vec3::UP * p.y + self.z * p.z
    }

    /// The direction `d` of the product's frame, in the world.
    #[must_use]
    pub fn direction(&self, d: Vec3) -> Vec3 {
        self.x * d.x + Vec3::UP * d.y + self.z * d.z
    }

    /// The point `p` of the world, in the product's frame.
    #[must_use]
    pub fn local(&self, p: Vec3) -> Vec3 {
        let d = p - self.origin;
        Vec3::new(d.dot(self.x), d.y, d.dot(self.z))
    }
}

/// A piece of wall: four corners on the floor, front and back, raised from
/// `bottom` to `top`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallPiece {
    /// Front start, front end, back end, back start, at `y = 0`.
    pub corners: [Vec3; 4],
    /// Where it starts above the floor.
    pub bottom: f32,
    /// Where it ends above the floor.
    pub top: f32,
}

/// What a project shows in 3D, before any view of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    /// Every placed product, or every product when there is no room.
    pub instances: Vec<Instance>,
    /// Every part of every product, from the evaluation.
    pub parts: Vec<Part>,
    /// The room's walls, in pieces around their openings.
    pub walls: Vec<WallPiece>,
    /// The room's windows: a pane in the middle of the wall each.
    pub windows: Vec<[Vec3; 4]>,
    /// The room's floor, from its lowest to its highest corner.
    pub floor: Option<(Vec3, Vec3)>,
    /// How each material looks.
    looks: HashMap<Id, Look>,
}

/// How a material looks, until materials say so themselves (RFC-0003).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Look {
    surface: Surface,
    /// Linear.
    color: [f32; 3],
    /// What its cut edges show when they are not banded.
    edge: (Surface, [f32; 3]),
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a room is far below 2^24 mm"
)]
fn mm(length: Length) -> f32 {
    (length.nm() as f64 / 1_000_000.0) as f32
}

/// An sRGB color, linear.
fn linear([r, g, b]: [f32; 3]) -> [f32; 3] {
    [r.powf(2.2), g.powf(2.2), b.powf(2.2)]
}

/// How a material looks, from words in its name: a species, plywood,
/// melamine and its color, MDF.
fn look(name: &str) -> Look {
    let name = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    let core = (Surface::Plain, linear([0.72, 0.63, 0.49]));
    if has(&["melamine", "laminate", "thermofoil"]) {
        let color = if has(&["black"]) {
            [0.09, 0.09, 0.10]
        } else if has(&["gray", "grey"]) {
            [0.56, 0.57, 0.58]
        } else if has(&["almond", "beige"]) {
            [0.87, 0.81, 0.70]
        } else {
            [0.94, 0.94, 0.92]
        };
        return Look {
            surface: Surface::Plain,
            color: linear(color),
            edge: core,
        };
    }
    if has(&["mdf", "hardboard", "particle"]) {
        let color = linear([0.70, 0.58, 0.42]);
        return Look {
            surface: Surface::Plain,
            color,
            edge: (Surface::Plain, color),
        };
    }
    let species: [(&str, [f32; 3]); 10] = [
        ("walnut", [0.38, 0.24, 0.15]),
        ("cherry", [0.64, 0.36, 0.22]),
        ("mahogany", [0.52, 0.27, 0.17]),
        ("oak", [0.74, 0.57, 0.37]),
        ("hickory", [0.80, 0.63, 0.43]),
        ("alder", [0.78, 0.56, 0.39]),
        ("maple", [0.88, 0.75, 0.56]),
        ("birch", [0.89, 0.78, 0.60]),
        ("pine", [0.89, 0.74, 0.50]),
        ("poplar", [0.80, 0.77, 0.57]),
    ];
    let color = species
        .iter()
        .find(|(word, _)| name.contains(word))
        .map_or([0.84, 0.70, 0.50], |(_, c)| *c);
    let color = linear(color);
    let edge = if has(&["plywood", "ply "]) || name.ends_with("ply") {
        (Surface::PlyEdge, color)
    } else {
        (Surface::Wood, color)
    };
    Look {
        surface: Surface::Wood,
        color,
        edge,
    }
}

/// A small number that varies a part's grain from its neighbors'.
#[allow(clippy::cast_precision_loss, reason = "a seed, not a value")]
fn seed(text: &str, salt: u128) -> f32 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes().chain(salt.to_le_bytes()) {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    (hash % 4096) as f32 / 64.0
}

fn vec(axis: Axis) -> Vec3 {
    let [x, y, z] = axis.unit();
    #[allow(clippy::cast_precision_loss, reason = "-1, 0 or 1")]
    Vec3::new(x as f32, y as f32, z as f32)
}

/// Whether a part is a door, a drawer front or one of their members.
fn is_front(part: &Part) -> bool {
    matches!(
        part.role,
        "door" | "drawerFront" | "falseFront" | "blindPanel" | "stile" | "rail" | "panel"
    )
}

impl Model {
    /// The products of `project` in 3D: those placed in the room at
    /// `room`, or with no room, every product side by side.
    #[must_use]
    pub fn build(project: &Project, library: &Library, room: Option<usize>) -> Self {
        let evaluation = evaluate(project, library);
        let mut by_product: HashMap<Id, Vec<usize>> = HashMap::new();
        for (i, part) in evaluation.parts.iter().enumerate() {
            by_product.entry(part.product).or_default().push(i);
        }
        let looks = library
            .materials
            .iter()
            .map(|m| (m.id, look(&m.name)))
            .collect();
        let mut model = Self {
            parts: evaluation.parts.clone(),
            looks,
            ..Self::default()
        };
        let parts_of = |id: Id| by_product.get(&id).cloned().unwrap_or_default();
        if let Some(room) = room.and_then(|r| project.rooms.get(r)) {
            let Some(plan) = plan(project, room.id) else {
                return model;
            };
            for cabinet in &plan.cabinets {
                let Some(product) = project.products.iter().find(|p| p.id == cabinet.product)
                else {
                    continue;
                };
                let placement = room.placements.iter().find(|p| p.id == cabinet.placement);
                let bottom = placement
                    .map(|p| {
                        if p.wall.is_some() {
                            p.elevation.unwrap_or_default()
                        } else {
                            p.position.map(|q| q.z).unwrap_or_default()
                        }
                    })
                    .unwrap_or_default();
                let at = |(x, y): (f64, f64)| {
                    #[allow(clippy::cast_possible_truncation, reason = "millimeters")]
                    Vec3::new(x as f32, 0.0, y as f32)
                };
                let [left_back, right_back, _, left_front] = cabinet.corners.map(at);
                let origin = left_back + Vec3::UP * mm(bottom);
                model.instances.push(model.instance(
                    Some(cabinet.placement),
                    product,
                    origin,
                    (
                        (right_back - left_back).normalized(),
                        (left_front - left_back).normalized(),
                    ),
                    parts_of(product.id),
                ));
            }
            for wall in &room.walls {
                model.add_wall(wall, &plan);
            }
            model.floor = model.floor_of_walls();
        } else {
            let mut x = 0.0;
            for product in &project.products {
                let origin = Vec3::new(x, 0.0, 0.0);
                let axes = (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
                let instance = model.instance(None, product, origin, axes, parts_of(product.id));
                x += instance.size.x + 300.0;
                model.instances.push(instance);
            }
        }
        model
    }

    fn instance(
        &self,
        placement: Option<Id>,
        product: &opencabinet_model::Product,
        origin: Vec3,
        (x, z): (Vec3, Vec3),
        parts: Vec<usize>,
    ) -> Instance {
        let toe_kick = parts
            .iter()
            .map(|&i| &self.parts[i])
            .find(|p| p.key == "case/side.left")
            .map_or(0.0, |side| mm(side.placement.origin[1]));
        Instance {
            placement,
            product: product.id,
            label: product.label.clone(),
            origin,
            x,
            z,
            size: Vec3::new(mm(product.width), mm(product.height), mm(product.depth)),
            parts,
            toe_kick,
        }
    }

    /// The pieces of `wall` around its openings, and a pane for each
    /// window.
    fn add_wall(&mut self, wall: &Wall, plan: &opencabinet_eval::Plan) {
        let Some(seen) = plan.walls.iter().find(|w| w.wall == wall.id) else {
            return;
        };
        let length = mm(seen.length);
        if length <= 0.0 {
            return;
        }
        let height = mm(wall.height);
        #[allow(clippy::cast_possible_truncation, reason = "millimeters")]
        let at = |(x, y): (f64, f64)| Vec3::new(x as f32, 0.0, y as f32);
        let (start, end) = (at(seen.front[0]), at(seen.front[1]));
        let along = (end - start).normalized();
        #[allow(clippy::cast_possible_truncation, reason = "millimeters")]
        let back =
            Vec3::new(seen.outward.0 as f32, 0.0, seen.outward.1 as f32) * seen.thickness as f32;
        // Where the back meets the walls at the ends, from the plan.
        let near = |a: Vec3, b: Vec3| (a - b).length() < 0.5;
        let mut back_start = start + back;
        let mut back_end = end + back;
        for piece in &seen.pieces {
            let piece = piece.map(at);
            if near(piece[0], start) {
                back_start = piece[3];
            }
            if near(piece[1], end) {
                back_end = piece[2];
            }
        }
        let front = |a: f32| start + along * a;
        let rear = |a: f32| {
            if a <= 0.0 {
                back_start
            } else if a >= length {
                back_end
            } else {
                start + along * a + back
            }
        };
        // Breaks along the wall where openings start and end.
        let mut openings: Vec<(f32, f32, f32, f32, bool)> = wall
            .openings
            .iter()
            .map(|o| {
                let from = mm(o.offset).clamp(0.0, length);
                let to = mm(o.offset + o.width).clamp(from, length);
                let sill = mm(o.sill_height);
                let window = !matches!(
                    o.kind,
                    opencabinet_model::WallOpeningKind::Door
                        | opencabinet_model::WallOpeningKind::PassThrough
                );
                (from, to, sill, sill + mm(o.height), window)
            })
            .filter(|o| o.1 > o.0)
            .collect();
        openings.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut breaks: Vec<f32> = vec![0.0, length];
        for o in &openings {
            breaks.extend([o.0, o.1]);
        }
        breaks.sort_by(f32::total_cmp);
        breaks.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        for pair in breaks.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let middle = f32::midpoint(a, b);
            let corners = [front(a), front(b), rear(b), rear(a)];
            let mut piece = |bottom: f32, top: f32| {
                if top > bottom {
                    self.walls.push(WallPiece {
                        corners,
                        bottom,
                        top,
                    });
                }
            };
            match openings.iter().find(|o| o.0 <= middle && middle <= o.1) {
                Some(&(_, _, sill, head, window)) => {
                    piece(0.0, sill.min(height));
                    piece(head.min(height), height);
                    if window {
                        let half = back * 0.5;
                        let (p, q) = (front(a) + half, front(b) + half);
                        let (low, high) = (Vec3::UP * sill, Vec3::UP * head.min(height));
                        self.windows.push([p + low, q + low, q + high, p + high]);
                    }
                }
                None => piece(0.0, height),
            }
        }
    }

    fn floor_of_walls(&self) -> Option<(Vec3, Vec3)> {
        let mut corners = self.walls.iter().flat_map(|w| w.corners);
        let first = corners.next()?;
        let (low, high) = corners.fold((first, first), |(l, h), c| (l.min(c), h.max(c)));
        Some((low, high))
    }

    /// Where the explode setting moves `part` of `instance`, in the
    /// product's frame.
    fn offset(&self, instance: &Instance, part: &Part, explode: f32) -> Vec3 {
        if explode <= 0.0 {
            return Vec3::default();
        }
        let (low, high) = self.bounds_of(part);
        let center = (low + high) * 0.5;
        let middle = instance.size * 0.5;
        let mut offset = (center - middle) * (explode * 0.9);
        // Nothing is pulled down through the floor.
        offset.y = offset.y.max(-(instance.origin.y + low.y));
        offset
    }

    /// A part's box in its product's frame.
    #[must_use]
    pub fn bounds_of(&self, part: &Part) -> (Vec3, Vec3) {
        let (low, high) = part
            .placement
            .bounds(part.length, part.width, part.thickness);
        let v = |p: [Length; 3]| Vec3::new(mm(p[0]), mm(p[1]), mm(p[2]));
        (v(low), v(high))
    }

    fn visible(view: &View, instance: usize, part: &Part) -> bool {
        view.isolated.is_none_or(|i| i == instance) && (view.fronts || !is_front(part))
    }

    /// The part under the ray, if any: the nearest the ray enters.
    #[must_use]
    pub fn pick(&self, view: &View, ray: Ray) -> Option<Pick> {
        let mut best: Option<(f32, Pick)> = None;
        for (i, instance) in self.instances.iter().enumerate() {
            let local = Ray {
                origin: instance.local(ray.origin),
                direction: {
                    let d = ray.direction;
                    Vec3::new(d.dot(instance.x), d.y, d.dot(instance.z))
                },
            };
            for &p in &instance.parts {
                let part = &self.parts[p];
                if !Self::visible(view, i, part) {
                    continue;
                }
                let offset = self.offset(instance, part, view.explode);
                let (low, high) = self.bounds_of(part);
                if let Some(t) = local.hits_box(low + offset, high + offset)
                    && best.is_none_or(|(b, _)| t < b)
                {
                    best = Some((
                        t,
                        Pick {
                            instance: i,
                            part: p,
                        },
                    ));
                }
            }
        }
        best.map(|(_, pick)| pick)
    }

    /// The lowest and highest corners of what `view` shows.
    #[must_use]
    pub fn bounds(&self, view: &View) -> (Vec3, Vec3) {
        let mut low = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut high = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        let mut cover = |p: Vec3| {
            low = low.min(p);
            high = high.max(p);
        };
        for (i, instance) in self.instances.iter().enumerate() {
            if view.isolated.is_some_and(|j| j != i) {
                continue;
            }
            // Pulled apart, the parts reach out further from the middle.
            let (s, middle) = (instance.size, instance.size * 0.5);
            let spread = 1.0 + view.explode.max(0.0) * 0.9;
            for corner in [
                Vec3::default(),
                Vec3::new(s.x, 0.0, 0.0),
                Vec3::new(0.0, s.y, 0.0),
                Vec3::new(0.0, 0.0, s.z),
                s,
            ] {
                cover(instance.world(middle + (corner - middle) * spread));
            }
        }
        if view.isolated.is_none() {
            for wall in &self.walls {
                for c in wall.corners {
                    cover(c);
                    cover(c + Vec3::UP * wall.top);
                }
            }
        }
        if low.x > high.x {
            return (Vec3::default(), Vec3::new(1000.0, 1000.0, 1000.0));
        }
        (low, high)
    }

    /// The triangles and lines that show the model as `view` says.
    #[must_use]
    pub fn mesh(&self, view: &View) -> Mesh {
        let mut mesh = Mesh::default();
        self.room_mesh(view, &mut mesh);
        for i in 0..self.instances.len() {
            if view.isolated.is_none_or(|j| j == i) {
                self.instance_mesh(view, i, &mut mesh);
            }
        }
        mesh
    }

    /// The floor, the walls and their windows; or the ground under a
    /// product seen alone.
    fn room_mesh(&self, view: &View, mesh: &mut Mesh) {
        let ghost_all = view.mode == Mode::XRay;
        let lines_only = view.mode == Mode::Wireframe;
        if view.isolated.is_none() {
            if let Some((low, high)) = self.floor {
                let (a, b) = (
                    low - Vec3::new(400.0, 0.0, 400.0),
                    high + Vec3::new(400.0, 0.0, 400.0),
                );
                mesh.floor(a, b, Surface::Floor, linear([0.66, 0.50, 0.34]));
            }
            for wall in &self.walls {
                mesh.wall(wall, lines_only || ghost_all);
            }
            for pane in &self.windows {
                mesh.quad(
                    *pane,
                    Vec3::UP,
                    [[0.0, 0.0]; 4],
                    [0.55, 0.70, 0.80, 0.25],
                    [Surface::Glass.code(), 0.0, flag(GHOST), 0.0],
                    true,
                );
            }
        }
        if (view.isolated.is_some() || self.floor.is_none()) && !self.instances.is_empty() {
            let (low, high) = self.bounds(view);
            let margin = Vec3::new(1500.0, 0.0, 1500.0);
            mesh.floor(
                low - margin,
                high + margin,
                Surface::Ground,
                linear([0.90, 0.90, 0.89]),
            );
        }
    }

    /// The parts of the instance `i`, what is machined into them and their
    /// hardware; a see-through box for a product that could not be
    /// evaluated.
    fn instance_mesh(&self, view: &View, i: usize, mesh: &mut Mesh) {
        let instance = &self.instances[i];
        let ghost_all = view.mode == Mode::XRay;
        let lines_only = view.mode == Mode::Wireframe;
        let assembled = view.explode <= 0.0 && !lines_only;
        if instance.parts.is_empty() {
            mesh.placeholder(instance);
            return;
        }
        if instance.origin.y < 1.0 && assembled {
            mesh.shadow(instance);
        }
        if instance.toe_kick > 1.0 && assembled {
            mesh.toe_kick(instance, ghost_all);
        }
        {
            for &p in &instance.parts {
                let part = &self.parts[p];
                if !Self::visible(view, i, part) {
                    continue;
                }
                let selected = view.selected
                    == Some(Pick {
                        instance: i,
                        part: p,
                    });
                let ghost = (ghost_all || lines_only) && !selected;
                let offset = self.offset(instance, part, view.explode);
                let look = self
                    .looks
                    .get(&part.material)
                    .copied()
                    .unwrap_or_else(|| look(""));
                let shape = PartShape {
                    instance,
                    part,
                    offset,
                    look,
                    seed: seed(&part.key, instance.product.uuid().as_u128()),
                    flags: if selected { SELECTED } else { 0 } | if ghost { GHOST } else { 0 },
                };
                if !lines_only || selected {
                    mesh.part(&shape, view.mode == Mode::Shaded);
                    mesh.machining(&shape);
                    mesh.hardware(&shape);
                    mesh.fasteners(&shape);
                }
                let edge = if selected {
                    [0.10, 0.42, 1.0, 1.0]
                } else if view.highlight == Some(instance.product) {
                    [0.10, 0.42, 1.0, 0.85]
                } else if lines_only {
                    [0.05, 0.07, 0.10, 0.9]
                } else {
                    [0.06, 0.05, 0.04, 0.55]
                };
                mesh.part_edges(&shape, edge, selected);
            }
        }
    }
}

/// A face of a box: its four corners, each the low (`false`) or high
/// (`true`) end of `x`, `y` and `z`, and its normal.
type BoxFace = ([bool; 3], [bool; 3], [bool; 3], [bool; 3], Vec3);

/// A part ready to draw.
struct PartShape<'m> {
    instance: &'m Instance,
    part: &'m Part,
    offset: Vec3,
    look: Look,
    seed: f32,
    flags: u32,
}

impl PartShape<'_> {
    /// The point `(u, v, w)` of the part, in the world.
    fn point(&self, u: f32, v: f32, w: f32) -> Vec3 {
        let p = &self.part.placement;
        let origin = Vec3::new(mm(p.origin[0]), mm(p.origin[1]), mm(p.origin[2]));
        let [a, b, c] = p.axes.map(vec);
        self.instance
            .world(origin + a * u + b * v + c * w + self.offset)
    }

    /// The part's axis `i` (length, width, thickness) in the world.
    fn axis(&self, i: usize) -> Vec3 {
        self.instance.direction(vec(self.part.placement.axes[i]))
    }

    fn size(&self) -> (f32, f32, f32) {
        (
            mm(self.part.length),
            mm(self.part.width),
            mm(self.part.thickness),
        )
    }
}

/// Triangles and lines for the GPU.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    /// The corners of every triangle.
    pub vertices: Vec<Vertex>,
    /// Opaque triangles, three corners each.
    pub solid: Vec<u32>,
    /// Triangles seen through, three corners each, drawn after the others.
    pub ghost: Vec<u32>,
    /// Edges, two ends each, hidden behind what is in front of them.
    pub lines: Vec<LineVertex>,
    /// Edges drawn over everything: the selected part's.
    pub overlay: Vec<LineVertex>,
}

impl Mesh {
    /// A quad with its corners in order around it.
    fn quad(
        &mut self,
        corners: [Vec3; 4],
        normal: Vec3,
        uv: [[f32; 2]; 4],
        color: [f32; 4],
        params: [f32; 4],
        ghost: bool,
    ) {
        let Ok(first) = u32::try_from(self.vertices.len()) else {
            return;
        };
        for (corner, uv) in corners.iter().zip(uv) {
            self.vertices.push(Vertex {
                position: corner.to_array(),
                normal: normal.to_array(),
                uv,
                color,
                params,
            });
        }
        let indices = if ghost {
            &mut self.ghost
        } else {
            &mut self.solid
        };
        indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }

    fn line(&mut self, a: Vec3, b: Vec3, color: [f32; 4], overlay: bool) {
        let list = if overlay {
            &mut self.overlay
        } else {
            &mut self.lines
        };
        list.push(LineVertex {
            position: a.to_array(),
            color,
        });
        list.push(LineVertex {
            position: b.to_array(),
            color,
        });
    }

    fn floor(&mut self, low: Vec3, high: Vec3, surface: Surface, color: [f32; 3]) {
        let (y, a, b) = (0.0, low, high);
        let corners = [
            Vec3::new(a.x, y, a.z),
            Vec3::new(b.x, y, a.z),
            Vec3::new(b.x, y, b.z),
            Vec3::new(a.x, y, b.z),
        ];
        let uv = corners.map(|c| [c.x, c.z]);
        self.quad(
            corners,
            Vec3::UP,
            uv,
            [color[0], color[1], color[2], 1.0],
            [surface.code(), 3.0, 0.0, 0.0],
            false,
        );
    }

    fn wall(&mut self, wall: &WallPiece, ghost: bool) {
        let color = linear([0.90, 0.88, 0.84]);
        let rgba = [color[0], color[1], color[2], 1.0];
        let flags = if ghost { GHOST } else { 0 };
        let params = [Surface::Wall.code(), 0.0, flag(flags), 0.0];
        let (lo, hi) = (Vec3::UP * wall.bottom, Vec3::UP * wall.top);
        let c = wall.corners;
        for i in 0..4 {
            let (a, b) = (c[i], c[(i + 1) % 4]);
            let normal = (b - a).cross(Vec3::UP).normalized();
            let along = (b - a).length();
            self.quad(
                [a + lo, b + lo, b + hi, a + hi],
                -normal,
                [
                    [0.0, wall.bottom],
                    [along, wall.bottom],
                    [along, wall.top],
                    [0.0, wall.top],
                ],
                rgba,
                params,
                ghost,
            );
        }
        for (y, n) in [(hi, Vec3::UP), (lo, -Vec3::UP)] {
            self.quad(
                [c[0] + y, c[1] + y, c[2] + y, c[3] + y],
                n,
                [[0.0; 2]; 4],
                rgba,
                params,
                ghost,
            );
        }
        let edge = [0.35, 0.34, 0.32, 0.5];
        for i in 0..4 {
            let (a, b) = (c[i], c[(i + 1) % 4]);
            self.line(a + hi, b + hi, edge, false);
            self.line(a + lo, a + hi, edge, false);
        }
    }

    /// A soft shadow on the floor under a product.
    fn shadow(&mut self, instance: &Instance) {
        let s = instance.size;
        let m = 70.0;
        let corners = [
            Vec3::new(-m, 0.6, -m),
            Vec3::new(s.x + m, 0.6, -m),
            Vec3::new(s.x + m, 0.6, s.z + m),
            Vec3::new(-m, 0.6, s.z + m),
        ]
        .map(|p| instance.world(p));
        let uv = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        self.quad(
            corners,
            Vec3::UP,
            uv,
            [0.0, 0.0, 0.0, 0.45],
            [Surface::Shadow.code(), 0.0, flag(GHOST), 0.0],
            true,
        );
    }

    /// The base a cabinet stands on, set back under its front: not a part
    /// of the cabinet, but what it would look like without one.
    fn toe_kick(&mut self, instance: &Instance, ghost: bool) {
        let s = instance.size;
        let low = Vec3::new(0.0, 0.0, 0.0);
        let high = Vec3::new(s.x, instance.toe_kick, (s.z - 76.0).max(0.0));
        let color = linear([0.16, 0.15, 0.14]);
        self.solid_box(
            instance,
            (low, high),
            [color[0], color[1], color[2], 1.0],
            Surface::Plain,
            (if ghost { GHOST } else { 0 }, ghost),
        );
    }

    /// A product that could not be evaluated: its outline, seen through.
    fn placeholder(&mut self, instance: &Instance) {
        let color = linear([0.70, 0.72, 0.75]);
        self.solid_box(
            instance,
            (Vec3::default(), instance.size),
            [color[0], color[1], color[2], 0.35],
            Surface::Plain,
            (0, true),
        );
        let s = instance.size;
        let edge = [0.25, 0.27, 0.30, 0.8];
        let p = |x: f32, y: f32, z: f32| instance.world(Vec3::new(x, y, z));
        for (x, z) in [(0.0, 0.0), (s.x, 0.0), (s.x, s.z), (0.0, s.z)] {
            self.line(p(x, 0.0, z), p(x, s.y, z), edge, false);
        }
        for y in [0.0, s.y] {
            let c = [
                p(0.0, y, 0.0),
                p(s.x, y, 0.0),
                p(s.x, y, s.z),
                p(0.0, y, s.z),
            ];
            for k in 0..4 {
                self.line(c[k], c[(k + 1) % 4], edge, false);
            }
        }
    }

    /// An axis-aligned box in a product's frame.
    fn solid_box(
        &mut self,
        instance: &Instance,
        (low, high): (Vec3, Vec3),
        color: [f32; 4],
        surface: Surface,
        (flags, see_through): (u32, bool),
    ) {
        let corner = |x: bool, y: bool, z: bool| {
            instance.world(Vec3::new(
                if x { high.x } else { low.x },
                if y { high.y } else { low.y },
                if z { high.z } else { low.z },
            ))
        };
        let params = [surface.code(), 0.0, flag(flags), 0.0];
        let faces: [BoxFace; 6] = [
            (
                [false, false, true],
                [true, false, true],
                [true, true, true],
                [false, true, true],
                Vec3::new(0.0, 0.0, 1.0),
            ),
            (
                [true, false, false],
                [false, false, false],
                [false, true, false],
                [true, true, false],
                Vec3::new(0.0, 0.0, -1.0),
            ),
            (
                [true, false, true],
                [true, false, false],
                [true, true, false],
                [true, true, true],
                Vec3::new(1.0, 0.0, 0.0),
            ),
            (
                [false, false, false],
                [false, false, true],
                [false, true, true],
                [false, true, false],
                Vec3::new(-1.0, 0.0, 0.0),
            ),
            (
                [false, true, true],
                [true, true, true],
                [true, true, false],
                [false, true, false],
                Vec3::UP,
            ),
            (
                [false, false, false],
                [true, false, false],
                [true, false, true],
                [false, false, true],
                -Vec3::UP,
            ),
        ];
        for (a, b, c, d, n) in faces {
            let p = |k: [bool; 3]| corner(k[0], k[1], k[2]);
            self.quad(
                [p(a), p(b), p(c), p(d)],
                instance.direction(n),
                [[0.0; 2]; 4],
                color,
                params,
                see_through,
            );
        }
    }

    /// The six faces of a part, each with its grain and edge look.
    fn part(&mut self, shape: &PartShape<'_>, plain: bool) {
        let (l, w, t) = shape.size();
        let look = shape.look;
        let surface = |s: Surface| {
            if plain && matches!(s, Surface::Wood | Surface::PlyEdge) {
                Surface::Plain
            } else {
                s
            }
        };
        let ghost = shape.flags & GHOST != 0;
        let params = |s: Surface| [surface(s).code(), shape.seed, flag(shape.flags), 0.0];
        let rgba = |c: [f32; 3]| [c[0], c[1], c[2], 1.0];
        let p = |u: f32, v: f32, z: f32| shape.point(u, v, z);
        let (du, dv, dw) = (shape.axis(0), shape.axis(1), shape.axis(2));
        // Faces A and B: the grain along the length.
        for (z, n) in [(0.0, -dw), (t, dw)] {
            self.quad(
                [p(0.0, 0.0, z), p(l, 0.0, z), p(l, w, z), p(0.0, w, z)],
                n,
                [[0.0, 0.0], [l, 0.0], [l, w], [0.0, w]],
                rgba(look.color),
                params(look.surface),
                ghost,
            );
        }
        // An edge shows its band, which matches the faces, or its core.
        let edge = |banded: bool| {
            if banded {
                (look.surface, look.color)
            } else {
                look.edge
            }
        };
        let edges = shape.part.edges;
        // The sides of the width, y0 and y1: along the length.
        for (v, n, banded) in [(0.0, -dv, edges[2].is_some()), (w, dv, edges[3].is_some())] {
            let (s, c) = edge(banded);
            self.quad(
                [p(0.0, v, 0.0), p(l, v, 0.0), p(l, v, t), p(0.0, v, t)],
                n,
                [[0.0, 0.0], [l, 0.0], [l, t], [0.0, t]],
                rgba(c),
                params(s),
                ghost,
            );
        }
        // The ends, x0 and x1: across the width.
        for (u, n, banded) in [(0.0, -du, edges[0].is_some()), (l, du, edges[1].is_some())] {
            let (s, c) = edge(banded);
            self.quad(
                [p(u, 0.0, 0.0), p(u, w, 0.0), p(u, w, t), p(u, 0.0, t)],
                n,
                [[0.0, 0.0], [w, 0.0], [w, t], [0.0, t]],
                rgba(c),
                params(s),
                ghost,
            );
        }
    }

    fn part_edges(&mut self, shape: &PartShape<'_>, color: [f32; 4], overlay: bool) {
        let (l, w, t) = shape.size();
        let p = |u: f32, v: f32, z: f32| shape.point(u, v, z);
        for (a, b) in [
            ((0.0, 0.0), (l, 0.0)),
            ((l, 0.0), (l, w)),
            ((l, w), (0.0, w)),
            ((0.0, w), (0.0, 0.0)),
        ] {
            for z in [0.0, t] {
                self.line(p(a.0, a.1, z), p(b.0, b.1, z), color, overlay);
            }
            self.line(p(a.0, a.1, 0.0), p(a.0, a.1, t), color, overlay);
        }
    }

    /// A disc on a face of a part, `lift` off it.
    fn disc(
        &mut self,
        shape: &PartShape<'_>,
        (u, v, z): (f32, f32, f32),
        radius: f32,
        out: f32,
        color: [f32; 3],
        surface: Surface,
    ) {
        let (du, dv, dw) = (shape.axis(0), shape.axis(1), shape.axis(2));
        let center = shape.point(u, v, z) + dw * out;
        let params = [surface.code(), shape.seed, flag(shape.flags & GHOST), 0.0];
        let rgba = [color[0], color[1], color[2], 1.0];
        let ghost = shape.flags & GHOST != 0;
        self.fan(
            (center, du, dv, dw * out.signum()),
            radius,
            rgba,
            params,
            ghost,
        );
    }

    /// A flat disc around `center` in the plane of `a` and `b`, facing
    /// `normal`.
    fn fan(
        &mut self,
        (center, a, b, normal): (Vec3, Vec3, Vec3, Vec3),
        radius: f32,
        rgba: [f32; 4],
        params: [f32; 4],
        ghost: bool,
    ) {
        let segments = 20;
        let Ok(first) = u32::try_from(self.vertices.len()) else {
            return;
        };
        self.vertices.push(Vertex {
            position: center.to_array(),
            normal: normal.to_array(),
            uv: [0.0, 0.0],
            color: rgba,
            params,
        });
        for k in 0..segments {
            #[allow(clippy::cast_precision_loss, reason = "a few segments")]
            let angle = k as f32 / segments as f32 * std::f32::consts::TAU;
            let (s, c) = angle.sin_cos();
            let p = center + a * (c * radius) + b * (s * radius);
            self.vertices.push(Vertex {
                position: p.to_array(),
                normal: normal.to_array(),
                uv: [1.0, 0.0],
                color: rgba,
                params,
            });
        }
        let indices = if ghost {
            &mut self.ghost
        } else {
            &mut self.solid
        };
        for k in 0..segments {
            let next = (k + 1) % segments;
            indices.extend([first, first + 1 + k, first + 1 + next]);
        }
    }

    /// A round rod from `a` to `b`, points of the world, `radius` thick,
    /// with flat ends; `x` and `y` are two directions square to it.
    fn rod(
        &mut self,
        (a, b): (Vec3, Vec3),
        (x, y): (Vec3, Vec3),
        radius: f32,
        (rgba, params): ([f32; 4], [f32; 4]),
        ghost: bool,
    ) {
        let segments = 12;
        let along = (b - a).normalized();
        let ring = |k: i32| {
            #[allow(clippy::cast_precision_loss, reason = "a few segments")]
            let angle = k as f32 / segments as f32 * std::f32::consts::TAU;
            let (s, c) = angle.sin_cos();
            x * c + y * s
        };
        for k in 0..segments {
            let (r0, r1) = (ring(k), ring(k + 1));
            let normal = (r0 + r1).normalized();
            self.quad(
                [
                    a + r0 * radius,
                    a + r1 * radius,
                    b + r1 * radius,
                    b + r0 * radius,
                ],
                normal,
                [[0.0; 2]; 4],
                rgba,
                params,
                ghost,
            );
        }
        self.fan((a, x, y, -along), radius, rgba, params, ghost);
        self.fan((b, x, y, along), radius, rgba, params, ghost);
    }

    /// The dowels and screws set into a part: light wooden rods and steel
    /// ones, a screw's head a short wide rod where it sits in the side.
    /// They stay solid in X-ray, where the parts around them are seen
    /// through, so the joinery shows.
    fn fasteners(&mut self, shape: &PartShape<'_>) {
        let ghost = false;
        for fastener in &shape.part.fasteners {
            let p = |q: [Length; 3]| shape.point(mm(q[0]), mm(q[1]), mm(q[2]));
            let (a, b) = (p(fastener.start), p(fastener.end));
            // A fastener lies along one of the part's axes; the other two
            // are square to it.
            let run = (0..3)
                .max_by_key(|&i| (fastener.end[i] - fastener.start[i]).nm().abs())
                .unwrap_or(0);
            let across = (shape.axis((run + 1) % 3), shape.axis((run + 2) % 3));
            let radius = mm(fastener.diameter) / 2.0;
            let params = |surface: Surface| [surface.code(), 0.0, 0.0, 0.0];
            match fastener.kind {
                FastenerKind::Dowel => {
                    let wood = [0.86, 0.70, 0.47, 1.0];
                    self.rod(
                        (a, b),
                        across,
                        radius,
                        (wood, params(Surface::Plain)),
                        ghost,
                    );
                }
                FastenerKind::Confirmat { head } => {
                    let steel = ([0.30, 0.31, 0.33, 1.0], params(Surface::Metal));
                    self.rod((a, b), across, radius * 0.8, steel, ghost);
                    let neck = a + (b - a).normalized() * 4.0;
                    self.rod((a, neck), across, mm(head) / 2.0, steel, ghost);
                }
            }
        }
    }

    /// Holes and grooves, as dark marks on their face.
    fn machining(&mut self, shape: &PartShape<'_>) {
        let (_, _, t) = shape.size();
        let dark = [0.03, 0.025, 0.02];
        let side = |face: Face| match face {
            Face::A => (t, 0.4),
            Face::B => (0.0, -0.4),
        };
        for operation in &shape.part.operations {
            match *operation {
                Operation::Hole {
                    face,
                    at,
                    diameter,
                    countersink,
                    ..
                } => {
                    let (z, out) = side(face);
                    let r = mm(countersink.unwrap_or(diameter)) / 2.0;
                    self.disc(shape, (mm(at.x), mm(at.y), z), r, out, dark, Surface::Hole);
                }
                Operation::EdgeHole {
                    edge,
                    along,
                    diameter,
                    ..
                } => {
                    let (l, w, _) = shape.size();
                    let (du, dv, dw) = (shape.axis(0), shape.axis(1), shape.axis(2));
                    let along = mm(along);
                    let (center, a, b, normal) = match edge {
                        Edge::X0 => (shape.point(-0.4, along, t / 2.0), dv, dw, -du),
                        Edge::X1 => (shape.point(l + 0.4, along, t / 2.0), dv, dw, du),
                        Edge::Y0 => (shape.point(along, -0.4, t / 2.0), du, dw, -dv),
                        Edge::Y1 => (shape.point(along, w + 0.4, t / 2.0), du, dw, dv),
                    };
                    let params = [
                        Surface::Hole.code(),
                        shape.seed,
                        flag(shape.flags & GHOST),
                        0.0,
                    ];
                    let rgba = [dark[0], dark[1], dark[2], 1.0];
                    let ghost = shape.flags & GHOST != 0;
                    self.fan(
                        (center, a, b, normal),
                        mm(diameter) / 2.0,
                        rgba,
                        params,
                        ghost,
                    );
                }
                Operation::HolePattern {
                    face,
                    start,
                    step,
                    count,
                    diameter,
                    ..
                } => {
                    let (z, out) = side(face);
                    let r = mm(diameter) / 2.0;
                    for k in 0..count {
                        #[allow(clippy::cast_precision_loss, reason = "a few holes")]
                        let k = k as f32;
                        let (u, v) = (mm(start.x) + mm(step.x) * k, mm(start.y) + mm(step.y) * k);
                        self.disc(shape, (u, v, z), r, out, dark, Surface::Hole);
                    }
                }
                Operation::Groove {
                    face,
                    start,
                    end,
                    width,
                    ..
                } => {
                    let (z, out) = side(face);
                    let (a, b) = ((mm(start.x), mm(start.y)), (mm(end.x), mm(end.y)));
                    let along = Vec3::new(b.0 - a.0, b.1 - a.1, 0.0).normalized();
                    let half = mm(width) / 2.0;
                    let across = (-along.y * half, along.x * half);
                    let lift = shape.axis(2) * out;
                    let p = |u: f32, v: f32| shape.point(u, v, z) + lift;
                    let corners = [
                        p(a.0 - across.0, a.1 - across.1),
                        p(b.0 - across.0, b.1 - across.1),
                        p(b.0 + across.0, b.1 + across.1),
                        p(a.0 + across.0, a.1 + across.1),
                    ];
                    let length = (b.0 - a.0).hypot(b.1 - a.1);
                    self.quad(
                        corners,
                        shape.axis(2) * out.signum(),
                        [[0.0, 0.0], [length, 0.0], [length, 1.0], [0.0, 1.0]],
                        [dark[0], dark[1], dark[2], 1.0],
                        [Surface::Hole.code(), 0.0, flag(shape.flags & GHOST), 1.0],
                        shape.flags & GHOST != 0,
                    );
                }
            }
        }
    }

    /// The hardware a part carries, as simple steel shapes: a hinge's cup
    /// and arm behind each cup bored in a door, the slides under a drawer
    /// box's sides, the pins under a shelf's corners.
    fn hardware(&mut self, shape: &PartShape<'_>) {
        let (l, w, t) = shape.size();
        let steel = [0.62, 0.64, 0.67, 1.0];
        let ghost = shape.flags & GHOST != 0;
        let local_box = |mesh: &mut Self, u: (f32, f32), v: (f32, f32), z: (f32, f32)| {
            mesh.part_box(shape, (u, v, z), steel, Surface::Metal, ghost);
        };
        match shape.part.role {
            "door" | "stile" => {
                for operation in &shape.part.operations {
                    if let Operation::Hole {
                        face: Face::A,
                        at,
                        diameter,
                        ..
                    } = *operation
                        && mm(diameter) >= 25.0
                    {
                        let (u, v) = (mm(at.x), mm(at.y));
                        let r = mm(diameter) / 2.0;
                        self.disc(
                            shape,
                            (u, v, t),
                            r - 1.5,
                            0.8,
                            [0.70, 0.72, 0.75],
                            Surface::Metal,
                        );
                        // The arm runs from the cup back into the cabinet.
                        local_box(self, (u - 9.0, u + 9.0), (v - 7.0, v + 7.0), (t, t + 48.0));
                    }
                }
            }
            "drawerSide" => {
                // Undermount slides under the box's bottom, inside the side.
                local_box(self, (15.0, l - 5.0), (1.0, 11.0), (t, t + 22.0));
            }
            "shelf" => {
                for u in [-3.0, l - 5.0] {
                    for v in [30.0, w - 36.0] {
                        local_box(self, (u, u + 8.0), (v, v + 6.0), (-6.0, 0.0));
                    }
                }
            }
            _ => {}
        }
    }

    /// A box in a part's own coordinates.
    fn part_box(
        &mut self,
        shape: &PartShape<'_>,
        ((u0, u1), (v0, v1), (w0, w1)): ((f32, f32), (f32, f32), (f32, f32)),
        color: [f32; 4],
        surface: Surface,
        ghost: bool,
    ) {
        let p = |u: f32, v: f32, z: f32| shape.point(u, v, z);
        let (du, dv, dw) = (shape.axis(0), shape.axis(1), shape.axis(2));
        let params = [
            surface.code(),
            shape.seed,
            flag(if ghost { GHOST } else { 0 }),
            0.0,
        ];
        let faces = [
            (
                [p(u0, v0, w0), p(u1, v0, w0), p(u1, v1, w0), p(u0, v1, w0)],
                -dw,
            ),
            (
                [p(u0, v0, w1), p(u1, v0, w1), p(u1, v1, w1), p(u0, v1, w1)],
                dw,
            ),
            (
                [p(u0, v0, w0), p(u1, v0, w0), p(u1, v0, w1), p(u0, v0, w1)],
                -dv,
            ),
            (
                [p(u0, v1, w0), p(u1, v1, w0), p(u1, v1, w1), p(u0, v1, w1)],
                dv,
            ),
            (
                [p(u0, v0, w0), p(u0, v1, w0), p(u0, v1, w1), p(u0, v0, w1)],
                -du,
            ),
            (
                [p(u1, v0, w0), p(u1, v1, w0), p(u1, v1, w1), p(u1, v0, w1)],
                du,
            ),
        ];
        for (corners, normal) in faces {
            self.quad(corners, normal, [[0.0; 2]; 4], color, params, ghost);
        }
    }
}

/// Flags as the GPU reads them.
#[allow(clippy::cast_precision_loss, reason = "a few bits")]
fn flag(flags: u32) -> f32 {
    flags as f32
}
