// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The checks of RFC-0001 §12 that need no evaluator: identity, references
//! and the shape of the authored data.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::product::{FrontNode, FrontStyle, InsertKind, Product};
use crate::project::{Placement, Room, Wall};
use crate::{Angle, Extensions, Id, Length, Library, Project};

/// How serious an issue is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The product cannot be evaluated; reports are blocked for it.
    Error,
    /// Worth the user's attention; nothing is blocked.
    Warning,
}

/// A problem found in a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// How serious it is.
    pub severity: Severity,
    /// Where it is, such as `product B30`.
    pub location: String,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{severity}: {}: {}", self.location, self.message)
    }
}

/// Checks a project against its library. An empty result means the
/// authored data is consistent; it says nothing yet about whether products
/// can be built, which is the evaluator's job.
#[must_use]
pub fn validate(project: &Project, library: &Library) -> Vec<Issue> {
    let mut checker = Checker::new(project, library);
    checker.run();
    checker.issues
}

struct Checker<'a> {
    project: &'a Project,
    library: &'a Library,
    issues: Vec<Issue>,
    ids: HashSet<Id>,
    standards: HashSet<Id>,
    /// Materials and edge bands, which material roles can point to.
    materials: HashSet<Id>,
    placements: HashMap<Id, usize>,
}

impl<'a> Checker<'a> {
    fn new(project: &'a Project, library: &'a Library) -> Self {
        let mut placements = HashMap::new();
        for placement in project.rooms.iter().flat_map(|r| &r.placements) {
            *placements.entry(placement.product).or_default() += 1;
        }
        Self {
            project,
            library,
            issues: Vec::new(),
            ids: HashSet::new(),
            standards: library.standards.iter().map(|s| s.id).collect(),
            materials: library
                .materials
                .iter()
                .map(|m| m.id)
                .chain(library.edge_bands.iter().map(|e| e.id))
                .collect(),
            placements,
        }
    }

    fn run(&mut self) {
        let (project, library) = (self.project, self.library);
        self.id(project.id, "project");
        let library_items = library
            .materials
            .iter()
            .map(|m| (m.id, &m.name))
            .chain(library.edge_bands.iter().map(|e| (e.id, &e.name)))
            .chain(library.hardware.iter().map(|h| (h.id, &h.name)))
            .chain(library.standards.iter().map(|s| (s.id, &s.name)));
        for (id, name) in library_items {
            self.id(id, &format!("library item {name}"));
        }
        for material in &library.materials {
            if material.actual_thickness <= Length::ZERO {
                let location = format!("material {}", material.name);
                self.error(&location, "thickness must be positive");
            }
        }

        let settings = &project.settings;
        if settings.precision <= Length::ZERO {
            self.error("settings", "precision must be positive");
        }
        if !self.standards.contains(&settings.default_standard) {
            self.error("settings", "default standard is not in the library");
        }
        self.extensions(&project.extensions, "project");
        for room in &project.rooms {
            self.room(room);
        }
        let products: HashSet<Id> = project.products.iter().map(|p| p.id).collect();
        for placement in project.rooms.iter().flat_map(|r| &r.placements) {
            if !products.contains(&placement.product) {
                let location = format!("placement {}", placement.id);
                self.error(&location, "product does not exist");
            }
        }
        for product in &project.products {
            self.product(product);
        }
    }

    fn room(&mut self, room: &Room) {
        self.id(room.id, &format!("room {}", room.name));
        for wall in &room.walls {
            self.wall(wall);
        }
        let walls: HashSet<Id> = room.walls.iter().map(|w| w.id).collect();
        for placement in &room.placements {
            self.placement(placement, &walls);
        }
    }

    fn wall(&mut self, wall: &Wall) {
        let location = format!("wall {}", wall.name);
        self.id(wall.id, &location);
        if wall.start == wall.end || wall.height <= Length::ZERO || wall.thickness <= Length::ZERO {
            self.error(&location, "wall has no length, height or thickness");
        }
        // Compare squared lengths in 128-bit integers: exact, no square root.
        let dx = i128::from((wall.end.x - wall.start.x).nm());
        let dy = i128::from((wall.end.y - wall.start.y).nm());
        let length_squared = dx * dx + dy * dy;
        for opening in &wall.openings {
            self.id(opening.id, &location);
            let end = i128::from((opening.offset + opening.width).nm());
            if opening.offset < Length::ZERO || end * end > length_squared {
                let message = format!("opening {} is outside the wall", opening.id);
                self.error(&location, &message);
            }
        }
    }

    fn placement(&mut self, placement: &Placement, walls: &HashSet<Id>) {
        let location = format!("placement {}", placement.id);
        self.id(placement.id, &location);
        let on_wall = [
            placement.wall.is_some(),
            placement.offset.is_some(),
            placement.elevation.is_some(),
        ];
        let free = [placement.position.is_some(), placement.rotation.is_some()];
        match (on_wall, free) {
            ([true, true, true], [false, false]) => {
                if !placement.wall.is_some_and(|w| walls.contains(&w)) {
                    self.error(&location, "wall is not in the same room");
                }
            }
            ([false, false, false], [true, true]) => {
                let rotation = placement.rotation.unwrap_or_default();
                if rotation < Angle::ZERO || rotation >= Angle::FULL_TURN {
                    self.error(&location, "rotation must be in [0°, 360°)");
                }
            }
            _ => self.error(
                &location,
                "use either wall, offset and elevation, or position and rotation",
            ),
        }
    }

    fn product(&mut self, product: &Product) {
        let location = format!("product {}", product.label);
        self.id(product.id, &location);
        if [product.width, product.height, product.depth]
            .iter()
            .any(|l| *l <= Length::ZERO)
        {
            self.error(&location, "dimensions must be positive");
        }
        if !self.standards.contains(&product.standard) {
            self.error(&location, "standard is not in the library");
        }
        let type_is_valid = product
            .product_type
            .split_once('@')
            .is_some_and(|(name, major)| !name.is_empty() && major.parse::<u32>().is_ok());
        if !type_is_valid {
            self.error(&location, "type must look like `base-cabinet@1`");
        }
        for (role, id) in &product.materials {
            if !self.materials.contains(id) {
                let message = format!("material for role `{role}` is not in the library");
                self.error(&location, &message);
            }
        }
        let placed = self.placements.get(&product.id).copied().unwrap_or(0);
        match product.quantity {
            Some(0) => self.error(&location, "quantity must be at least 1"),
            Some(quantity) if placed > 0 && quantity != 1 => self.error(
                &location,
                "a placed product counts its placements; remove the quantity",
            ),
            _ => {}
        }
        if let Some(front) = &product.front {
            if front.size.is_some() {
                self.error(&location, "the root of the front has no size");
            }
            self.front(front, &location);
        }
        for o in &product.overrides {
            self.id(o.id, &location);
        }
        self.extensions(&product.extensions, &location);
    }

    fn front(&mut self, node: &FrontNode, location: &str) {
        self.id(node.id, location);
        match (&node.split, &node.insert) {
            (Some(_), None) if !node.children.is_empty() => {
                for child in &node.children {
                    if child.size.is_none() {
                        let message = format!("front node {} needs a size", child.id);
                        self.error(location, &message);
                    }
                    self.front(child, location);
                }
            }
            (None, Some(insert)) if node.children.is_empty() => {
                // Kept, but not evaluated: a newer version wrote it.
                if let InsertKind::Other(kind) = &insert.kind {
                    let message = format!("unknown insert `{kind}`; open with a newer OpenCabinet");
                    self.error(location, &message);
                }
                if let Some(FrontStyle::Other(style)) = &insert.style {
                    self.error(location, &format!("unknown front style `{style}`"));
                }
                for adjustment in &insert.adjustments {
                    self.id(adjustment.id, location);
                }
            }
            _ => {
                let message = format!(
                    "front node {} must be a split with children or an insert",
                    node.id
                );
                self.error(location, &message);
            }
        }
    }

    fn extensions(&mut self, extensions: &Extensions, location: &str) {
        for namespace in extensions.keys() {
            let labels: Vec<&str> = namespace.split('.').collect();
            let valid = labels.len() >= 2
                && labels.iter().all(|l| {
                    !l.is_empty()
                        && l.chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                });
            if !valid {
                let message = format!("extension `{namespace}` is not a reverse-domain namespace");
                self.warning(location, &message);
            }
        }
    }

    /// Records an identifier, which must be a UUIDv7 used once.
    fn id(&mut self, id: Id, location: &str) {
        if !id.is_v7() {
            self.error(location, &format!("{id} is not a UUIDv7"));
        }
        if !self.ids.insert(id) {
            self.error(location, &format!("{id} is used more than once"));
        }
    }

    fn error(&mut self, location: &str, message: &str) {
        self.push(Severity::Error, location, message);
    }

    fn warning(&mut self, location: &str, message: &str) {
        self.push(Severity::Warning, location, message);
    }

    fn push(&mut self, severity: Severity, location: &str, message: &str) {
        self.issues.push(Issue {
            severity,
            location: location.into(),
            message: message.into(),
        });
    }
}
