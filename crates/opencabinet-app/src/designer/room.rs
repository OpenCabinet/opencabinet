// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Editing the room: its walls, their openings, and where the cabinets are
//! placed on them.
//!
//! The window edits one room at a time; the cabinets belong to the project
//! and can be placed in any of them. Walls are added one after
//! another, each turning a right angle to the left of the one before, so a
//! straight, L or U kitchen is just a list of lengths; the file keeps the
//! corners as points (RFC-0001 §5).

use std::collections::BTreeMap;

use opencabinet_model::{Id, Length, Placement, Point, Room, Wall, WallOpening, WallOpeningKind};

use super::{CabinetKind, Designer, Dimension};

/// What can be cut into a wall from the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpeningKind {
    /// 36" wide, 48" high, its sill 42" up.
    Window,
    /// 36" wide, 80" high, from the floor.
    Door,
}

/// How the Walls tab shows the room.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WallView {
    /// The edited wall's elevation.
    #[default]
    Elevation,
    /// The room's plan.
    Plan,
    /// The plan and the elevation together.
    PlanAndElevation,
}

impl WallView {
    /// The view at `index` in the window's list, the elevation for any
    /// other.
    #[must_use]
    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Plan,
            2 => Self::PlanAndElevation,
            _ => Self::Elevation,
        }
    }

    /// Its place in the window's list.
    #[must_use]
    pub fn index(self) -> i32 {
        match self {
            Self::Elevation => 0,
            Self::Plan => 1,
            Self::PlanAndElevation => 2,
        }
    }
}

/// Which room, wall, opening and placement are being edited, and whether
/// the drawing shows the walls rather than the cabinet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RoomSelection {
    /// By its place in the project; the project may have no room yet.
    pub(super) room: usize,
    pub(super) wall: Option<usize>,
    pub(super) opening: Option<usize>,
    /// Among the edited cabinet's placements in the room.
    pub(super) placement: Option<usize>,
    pub(super) showing_walls: bool,
    /// How the walls are shown.
    pub(super) view: WallView,
}

/// A new wall: 10' long, 8' high, 4-1/2" thick.
fn new_wall_size() -> (Length, Length, Length) {
    (
        Length::from_inches(120),
        Length::from_inches(96),
        Length::from_inch_fraction(9, 2).expect("a valid fraction"),
    )
}

/// The bottom of a wall cabinet: 36" of counter and 18" of backsplash.
fn wall_cabinet_height() -> Length {
    Length::from_inches(54)
}

/// From a wall's start to its end, to the nanometer for walls along an
/// axis.
fn wall_length(wall: &Wall) -> Length {
    let (dx, dy) = (
        (wall.end.x - wall.start.x).nm(),
        (wall.end.y - wall.start.y).nm(),
    );
    if dx == 0 || dy == 0 {
        return Length::from_nm(dx.abs() + dy.abs());
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "a wall is far below 2^53 nm"
    )]
    Length::from_nm((dx as f64).hypot(dy as f64).round() as i64)
}

/// The direction of a wall along an axis: each of `x` and `y` is −1, 0
/// or 1. `None` for a wall at an angle.
fn axis(wall: &Wall) -> Option<(i64, i64)> {
    let (dx, dy) = (
        (wall.end.x - wall.start.x).nm(),
        (wall.end.y - wall.start.y).nm(),
    );
    (dx == 0 || dy == 0).then_some((dx.signum(), dy.signum()))
}

fn opening_name(kind: &WallOpeningKind) -> &'static str {
    match kind {
        WallOpeningKind::Window => "Window",
        WallOpeningKind::Door => "Door",
        WallOpeningKind::PassThrough | WallOpeningKind::Other(_) => "Opening",
    }
}

impl Designer {
    fn room(&self) -> Option<&Room> {
        self.document.project.rooms.get(self.room_selection.room)
    }

    fn room_mut(&mut self) -> Option<&mut Room> {
        self.document
            .project
            .rooms
            .get_mut(self.room_selection.room)
    }

    fn wall(&self) -> Option<&Wall> {
        self.room()?.walls.get(self.room_selection.wall?)
    }

    fn wall_mut(&mut self) -> Option<&mut Wall> {
        let index = self.room_selection.wall?;
        self.room_mut()?.walls.get_mut(index)
    }

    fn opening(&self) -> Option<&WallOpening> {
        self.wall()?.openings.get(self.room_selection.opening?)
    }

    fn opening_mut(&mut self) -> Option<&mut WallOpening> {
        let index = self.room_selection.opening?;
        self.wall_mut()?.openings.get_mut(index)
    }

    /// The edited cabinet's placements, by their index in the room.
    fn placement_indexes(&self) -> Vec<usize> {
        let id = self.product().id;
        self.room().map_or_else(Vec::new, |room| {
            room.placements
                .iter()
                .enumerate()
                .filter(|(_, p)| p.product == id)
                .map(|(i, _)| i)
                .collect()
        })
    }

    fn placement(&self) -> Option<&Placement> {
        let index = *self
            .placement_indexes()
            .get(self.room_selection.placement?)?;
        self.room()?.placements.get(index)
    }

    fn placement_mut(&mut self) -> Option<&mut Placement> {
        let index = *self
            .placement_indexes()
            .get(self.room_selection.placement?)?;
        self.room_mut()?.placements.get_mut(index)
    }

    /// After the edited cabinet changes, edit its first placement.
    pub(super) fn cabinet_changed(&mut self) {
        self.section = 0;
        let placed = !self.placement_indexes().is_empty();
        self.room_selection.placement = placed.then_some(0);
        self.room_selection.showing_walls = false;
        self.follow_cabinet_standard();
    }

    /// The selection a project starts with: its first room and wall, and
    /// the first placement of its first cabinet.
    pub(super) fn first_selection(&mut self) {
        self.room_selection = RoomSelection::default();
        self.select_room_walls();
        self.cabinet_changed();
    }

    /// After the edited room changes, edit its first wall.
    fn select_room_walls(&mut self) {
        let has_walls = self.room().is_some_and(|r| !r.walls.is_empty());
        self.room_selection.wall = has_walls.then_some(0);
        self.select_wall_openings();
    }

    /// The project's rooms, by name.
    #[must_use]
    pub fn rooms(&self) -> Vec<String> {
        self.document
            .project
            .rooms
            .iter()
            .map(|r| r.name.clone())
            .collect()
    }

    /// Which room is being edited; `None` while the project has none.
    #[must_use]
    pub fn selected_room(&self) -> Option<usize> {
        self.room().map(|_| self.room_selection.room)
    }

    /// Edits another room, and shows its first wall; returns whether there
    /// is one at `index`.
    pub fn select_room(&mut self, index: usize) -> bool {
        let exists = index < self.document.project.rooms.len();
        if exists {
            self.room_selection.room = index;
            self.select_room_walls();
            let placed = !self.placement_indexes().is_empty();
            self.room_selection.placement = placed.then_some(0);
            self.room_selection.showing_walls = true;
        }
        exists
    }

    /// Adds an empty room, "Room 2" for a project with one, and edits it.
    pub fn add_room(&mut self) {
        let rooms = &mut self.document.project.rooms;
        // Of these many names, the rooms can take all but one.
        let count = rooms.len();
        let name = (count + 1..=2 * count + 1)
            .map(|n| format!("Room {n}"))
            .find(|name| rooms.iter().all(|r| r.name != *name))
            .unwrap_or_else(|| format!("Room {}", count + 1));
        rooms.push(Room {
            id: Id::new(),
            name,
            walls: Vec::new(),
            placements: Vec::new(),
            unknown: BTreeMap::default(),
        });
        let index = rooms.len() - 1;
        self.select_room(index);
        self.modified = true;
    }

    /// The edited room's name; empty while the project has no room.
    #[must_use]
    pub fn room_name(&self) -> String {
        self.room().map(|r| r.name.clone()).unwrap_or_default()
    }

    /// Renames the edited room.
    ///
    /// # Errors
    /// A message for the user when the name is empty or there is no room;
    /// nothing then changes.
    pub fn set_room_name(&mut self, text: &str) -> Result<(), String> {
        let name = text.trim();
        if name.is_empty() {
            return Err("A room needs a name".into());
        }
        let room = self.room_mut().ok_or("Add a room first")?;
        if room.name != name {
            name.clone_into(&mut room.name);
            self.modified = true;
        }
        Ok(())
    }

    /// How many walls and placements the edited room has, to say what
    /// deleting it would remove.
    #[must_use]
    pub fn room_contents(&self) -> (usize, usize) {
        self.room()
            .map_or((0, 0), |r| (r.walls.len(), r.placements.len()))
    }

    /// Removes the edited room with its walls and placements; the cabinets
    /// stay in the project, and those no longer placed count by their
    /// quantity. Then edits the room that takes its place in the list.
    ///
    /// # Errors
    /// When there is no room to remove.
    pub fn delete_room(&mut self) -> Result<(), String> {
        let index = self.selected_room().ok_or("There is no room to delete")?;
        let rooms = &mut self.document.project.rooms;
        rooms.remove(index);
        let left = rooms.len();
        if left == 0 {
            self.room_selection = RoomSelection::default();
        } else {
            self.select_room(index.min(left - 1));
        }
        self.modified = true;
        Ok(())
    }

    fn select_wall_openings(&mut self) {
        let has_openings = self.wall().is_some_and(|w| !w.openings.is_empty());
        self.room_selection.opening = has_openings.then_some(0);
    }

    /// The room's walls as the list shows them: "Wall A · 120\"".
    #[must_use]
    pub fn walls(&self) -> Vec<String> {
        self.room().map_or_else(Vec::new, |room| {
            room.walls
                .iter()
                .map(|w| format!("{} · {}", w.name, self.show(wall_length(w))))
                .collect()
        })
    }

    /// Which wall is being edited.
    #[must_use]
    pub fn selected_wall(&self) -> Option<usize> {
        self.room_selection.wall
    }

    /// Edits another wall, and shows it; returns whether there is one at
    /// `index`.
    pub fn select_wall(&mut self, index: usize) -> bool {
        let exists = self.room().is_some_and(|r| index < r.walls.len());
        if exists {
            self.room_selection.wall = Some(index);
            self.room_selection.showing_walls = true;
            self.select_wall_openings();
        }
        exists
    }

    /// Whether the drawing shows the edited wall rather than the edited
    /// cabinet's.
    pub fn show_walls(&mut self, walls: bool) {
        self.room_selection.showing_walls = walls;
    }

    /// How the walls are shown: the edited wall's elevation, the room's
    /// plan, or both.
    pub fn choose_view(&mut self, view: WallView) {
        self.room_selection.view = view;
    }

    /// Whether the drawing can show the room's plan: while the walls are
    /// shown and there is a room.
    #[must_use]
    pub fn plan_available(&self) -> bool {
        self.room_selection.showing_walls && self.room().is_some()
    }

    /// How the drawing shows the walls now: in elevation whenever the
    /// plan is not available.
    #[must_use]
    pub fn view(&self) -> WallView {
        if self.plan_available() {
            self.room_selection.view
        } else {
            WallView::Elevation
        }
    }

    /// The edited room and wall, for the plan.
    pub(super) fn plan_selection(&self) -> Option<(Id, Option<Id>)> {
        Some((self.room()?.id, self.wall().map(|w| w.id)))
    }

    /// Adds a wall at the end of the last one, turning a right angle to
    /// the left, and edits it. The room is made with the first wall.
    pub fn add_wall(&mut self) {
        let (length, height, thickness) = new_wall_size();
        if self.room().is_none() {
            self.add_room();
        }
        let index = self.room_selection.room;
        let room = &mut self.document.project.rooms[index];
        let (start, (dx, dy)) = room.walls.last().map_or(
            (
                Point {
                    x: Length::ZERO,
                    y: Length::ZERO,
                },
                (1, 0),
            ),
            // Left of (x, y) is (−y, x). A wall at an angle is followed
            // along x.
            |last| (last.end, axis(last).map_or((1, 0), |(x, y)| (-y, x))),
        );
        let name = ('A'..='Z')
            .map(|letter| format!("Wall {letter}"))
            .find(|name| room.walls.iter().all(|w| w.name != *name))
            .unwrap_or_else(|| format!("Wall {}", room.walls.len() + 1));
        room.walls.push(Wall {
            id: Id::new(),
            name,
            start,
            end: Point {
                x: start.x + length * dx,
                y: start.y + length * dy,
            },
            height,
            thickness,
            openings: Vec::new(),
            unknown: BTreeMap::default(),
        });
        self.room_selection.wall = Some(room.walls.len() - 1);
        self.room_selection.opening = None;
        self.room_selection.showing_walls = true;
        self.modified = true;
    }

    /// Removes the edited wall, with its openings and the placements on
    /// it. The walls after it stay where they are.
    ///
    /// # Errors
    /// When there is no wall to remove.
    pub fn delete_wall(&mut self) -> Result<(), String> {
        let index = self
            .room_selection
            .wall
            .ok_or("There is no wall to delete")?;
        let room = self.room_mut().ok_or("There is no wall to delete")?;
        let removed = room.walls.remove(index);
        room.placements.retain(|p| p.wall != Some(removed.id));
        self.room_selection.wall =
            (!room.walls.is_empty()).then(|| index.min(room.walls.len() - 1));
        self.select_wall_openings();
        let placements = self.placement_indexes().len();
        self.room_selection.placement = (placements > 0).then_some(0);
        self.modified = true;
        Ok(())
    }

    /// Makes the edited wall `length` long from its start, along the same
    /// direction, and moves the walls joined after it with its end.
    fn set_wall_length(&mut self, length: Length) -> Result<(), String> {
        let index = self.room_selection.wall.ok_or("There is no wall to edit")?;
        let room = self.room_mut().ok_or("There is no wall to edit")?;
        let wall = &room.walls[index];
        let (dx, dy) =
            axis(wall).ok_or("Only walls along the plan's axes can change length here")?;
        let old_end = wall.end;
        let new_end = Point {
            x: wall.start.x + length * dx,
            y: wall.start.y + length * dy,
        };
        let shift = (new_end.x - old_end.x, new_end.y - old_end.y);
        room.walls[index].end = new_end;
        // The walls after it that start where the one before ended.
        let mut joint = old_end;
        for wall in &mut room.walls[index + 1..] {
            if wall.start != joint {
                break;
            }
            joint = wall.end;
            for point in [&mut wall.start, &mut wall.end] {
                point.x += shift.0;
                point.y += shift.1;
            }
        }
        Ok(())
    }

    /// The edited wall's openings as the list shows them:
    /// "Window · 36\" at 60\"".
    #[must_use]
    pub fn openings(&self) -> Vec<String> {
        self.wall().map_or_else(Vec::new, |wall| {
            wall.openings
                .iter()
                .map(|o| {
                    format!(
                        "{} · {} at {}",
                        opening_name(&o.kind),
                        self.show(o.width),
                        self.show(o.offset)
                    )
                })
                .collect()
        })
    }

    /// Which opening of the edited wall is being edited.
    #[must_use]
    pub fn selected_opening(&self) -> Option<usize> {
        self.room_selection.opening
    }

    /// Edits another opening of the wall; returns whether there is one at
    /// `index`.
    pub fn select_opening(&mut self, index: usize) -> bool {
        let exists = self.wall().is_some_and(|w| index < w.openings.len());
        if exists {
            self.room_selection.opening = Some(index);
        }
        exists
    }

    /// Cuts a window or a door into the middle of the edited wall, and
    /// edits it.
    ///
    /// # Errors
    /// When no wall is being edited.
    pub fn add_opening(&mut self, kind: OpeningKind) -> Result<(), String> {
        let (kind, width, height, sill) = match kind {
            OpeningKind::Window => (
                WallOpeningKind::Window,
                Length::from_inches(36),
                Length::from_inches(48),
                Length::from_inches(42),
            ),
            OpeningKind::Door => (
                WallOpeningKind::Door,
                Length::from_inches(36),
                Length::from_inches(80),
                Length::ZERO,
            ),
        };
        let wall = self.wall_mut().ok_or("Add a wall first")?;
        let offset = ((wall_length(wall) - width).div_round(2)).max(Length::ZERO);
        wall.openings.push(WallOpening {
            id: Id::new(),
            kind,
            offset,
            width,
            height,
            sill_height: sill,
            unknown: BTreeMap::default(),
        });
        let index = wall.openings.len() - 1;
        self.room_selection.opening = Some(index);
        self.room_selection.showing_walls = true;
        self.modified = true;
        Ok(())
    }

    /// Removes the edited opening.
    ///
    /// # Errors
    /// When there is none.
    pub fn delete_opening(&mut self) -> Result<(), String> {
        let index = self
            .room_selection
            .opening
            .ok_or("There is no opening to delete")?;
        let wall = self.wall_mut().ok_or("There is no opening to delete")?;
        wall.openings.remove(index);
        let left = wall.openings.len();
        self.room_selection.opening = (left > 0).then(|| index.min(left - 1));
        self.modified = true;
        Ok(())
    }

    /// The edited cabinet's placements as the list shows them:
    /// "Wall A at 30\", 0\" up", or "Free-standing".
    #[must_use]
    pub fn placements(&self) -> Vec<String> {
        let Some(room) = self.room() else {
            return Vec::new();
        };
        self.placement_indexes()
            .into_iter()
            .map(|i| {
                let placement = &room.placements[i];
                let wall = placement
                    .wall
                    .and_then(|id| room.walls.iter().find(|w| w.id == id));
                match wall {
                    Some(wall) => format!(
                        "{} at {}, {} up",
                        wall.name,
                        self.show(placement.offset.unwrap_or_default()),
                        self.show(placement.elevation.unwrap_or_default())
                    ),
                    None => "Free-standing".into(),
                }
            })
            .collect()
    }

    /// Which of the edited cabinet's placements is being edited.
    #[must_use]
    pub fn selected_placement(&self) -> Option<usize> {
        self.room_selection.placement
    }

    /// Edits another placement of the cabinet, and shows its wall;
    /// returns whether there is one at `index`.
    pub fn select_placement(&mut self, index: usize) -> bool {
        let exists = index < self.placement_indexes().len();
        if exists {
            self.room_selection.placement = Some(index);
            self.room_selection.showing_walls = false;
        }
        exists
    }

    /// Places the edited cabinet on the edited wall, or the first one,
    /// after the last cabinet at its height: on the floor, or for a wall
    /// cabinet 54" up. Then edits the new placement.
    ///
    /// # Errors
    /// When the room has no walls yet.
    pub fn place(&mut self) -> Result<(), String> {
        let product = self.product();
        let id = product.id;
        let hung = CabinetKind::of(&product.product_type).is_some_and(CabinetKind::hangs);
        let elevation = if hung {
            wall_cabinet_height()
        } else {
            Length::ZERO
        };
        let wall_index = self.room_selection.wall.unwrap_or(0);
        let room_index = self.room_selection.room;
        let project = &mut self.document.project;
        let room = project
            .rooms
            .get_mut(room_index)
            .filter(|r| !r.walls.is_empty())
            .ok_or("Add a wall first, on the Walls tab")?;
        let wall = room.walls[wall_index.min(room.walls.len() - 1)].id;
        let products = &project.products;
        let offset = room
            .placements
            .iter()
            .filter(|p| p.wall == Some(wall))
            .filter(|p| (p.elevation.unwrap_or_default() > Length::ZERO) == hung)
            .filter_map(|p| {
                let other = products.iter().find(|o| o.id == p.product)?;
                Some(p.offset.unwrap_or_default() + other.width)
            })
            .max()
            .unwrap_or(Length::ZERO);
        room.placements.push(Placement {
            id: Id::new(),
            product: id,
            wall: Some(wall),
            offset: Some(offset),
            elevation: Some(elevation),
            position: None,
            rotation: None,
            unknown: BTreeMap::default(),
        });
        // A placed cabinet counts its placements (RFC-0001 §6).
        self.product_mut().quantity = None;
        self.room_selection.placement = Some(self.placement_indexes().len() - 1);
        self.room_selection.showing_walls = false;
        self.modified = true;
        Ok(())
    }

    /// How many times the edited cabinet is placed, in all the rooms.
    #[must_use]
    pub fn placed_count(&self) -> usize {
        let id = self.product().id;
        self.document
            .project
            .rooms
            .iter()
            .flat_map(|r| &r.placements)
            .filter(|p| p.product == id)
            .count()
    }

    /// How many of the edited cabinet the job needs, when it is not placed.
    #[must_use]
    pub fn quantity(&self) -> u32 {
        self.product().quantity.unwrap_or(1)
    }

    /// Reads what the user typed as how many of the edited cabinet the job
    /// needs.
    ///
    /// # Errors
    /// A message for the user when it is not a whole number from 1 to 999
    /// or the cabinet is placed; nothing then changes.
    pub fn set_quantity(&mut self, text: &str) -> Result<(), String> {
        if self.placed_count() > 0 {
            return Err("A placed cabinet counts its placements".into());
        }
        let quantity = text
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|q| (1..=999).contains(q))
            .ok_or("Must be a whole number from 1 to 999")?;
        if quantity != self.quantity() {
            self.product_mut().quantity = (quantity > 1).then_some(quantity);
            self.modified = true;
        }
        Ok(())
    }

    /// Removes the edited placement; the cabinet stays in the project.
    ///
    /// # Errors
    /// When there is none.
    pub fn unplace(&mut self) -> Result<(), String> {
        let selected = self
            .room_selection
            .placement
            .ok_or("The cabinet is not placed")?;
        let index = *self
            .placement_indexes()
            .get(selected)
            .ok_or("The cabinet is not placed")?;
        if let Some(room) = self.room_mut() {
            room.placements.remove(index);
        }
        let left = self.placement_indexes().len();
        self.room_selection.placement = (left > 0).then(|| selected.min(left - 1));
        self.modified = true;
        Ok(())
    }

    /// The wall the edited placement is on, by its place in the walls list.
    #[must_use]
    pub fn placement_wall(&self) -> Option<usize> {
        let wall = self.placement()?.wall?;
        self.room()?.walls.iter().position(|w| w.id == wall)
    }

    /// Moves the edited placement onto another wall, keeping its offset
    /// and height; a free-standing one starts at the wall's start.
    /// Returns whether there is a wall at `index` and a placement to move.
    pub fn set_placement_wall(&mut self, index: usize) -> bool {
        let Some(wall) = self.room().and_then(|r| r.walls.get(index)).map(|w| w.id) else {
            return false;
        };
        let Some(placement) = self.placement_mut() else {
            return false;
        };
        if placement.wall != Some(wall) {
            placement.wall = Some(wall);
            placement.offset.get_or_insert(Length::ZERO);
            placement.elevation.get_or_insert(Length::ZERO);
            placement.position = None;
            placement.rotation = None;
            self.modified = true;
        }
        true
    }

    /// The stored value of a wall, opening or placement field, when one is
    /// being edited.
    pub(super) fn room_value(&self, dimension: Dimension) -> Option<Length> {
        match dimension {
            Dimension::WallLength => self.wall().map(wall_length),
            Dimension::WallHeight => self.wall().map(|w| w.height),
            Dimension::OpeningOffset => self.opening().map(|o| o.offset),
            Dimension::OpeningWidth => self.opening().map(|o| o.width),
            Dimension::OpeningHeight => self.opening().map(|o| o.height),
            Dimension::OpeningSill => self.opening().map(|o| o.sill_height),
            Dimension::PlacementOffset => self.placement().filter(|p| p.wall.is_some())?.offset,
            Dimension::PlacementElevation => {
                self.placement().filter(|p| p.wall.is_some())?.elevation
            }
            Dimension::Width
            | Dimension::Height
            | Dimension::Depth
            | Dimension::BlindWidth
            | Dimension::SectionHeight
            | Dimension::SheetNominal
            | Dimension::SheetActual
            | Dimension::SheetLength
            | Dimension::SheetWidth
            | Dimension::BandThickness
            | Dimension::BandWidth => None,
        }
    }

    /// Stores a wall, opening or placement field.
    pub(super) fn set_room_value(
        &mut self,
        dimension: Dimension,
        length: Length,
    ) -> Result<(), String> {
        if self.room_value(dimension) == Some(length) {
            return Ok(());
        }
        let missing = || "Nothing to edit".to_owned();
        match dimension {
            Dimension::WallLength => self.set_wall_length(length)?,
            Dimension::WallHeight => self.wall_mut().ok_or_else(missing)?.height = length,
            Dimension::OpeningOffset => self.opening_mut().ok_or_else(missing)?.offset = length,
            Dimension::OpeningWidth => self.opening_mut().ok_or_else(missing)?.width = length,
            Dimension::OpeningHeight => self.opening_mut().ok_or_else(missing)?.height = length,
            Dimension::OpeningSill => {
                self.opening_mut().ok_or_else(missing)?.sill_height = length;
            }
            Dimension::PlacementOffset => {
                self.placement_mut().ok_or_else(missing)?.offset = Some(length);
            }
            Dimension::PlacementElevation => {
                self.placement_mut().ok_or_else(missing)?.elevation = Some(length);
            }
            Dimension::Width
            | Dimension::Height
            | Dimension::Depth
            | Dimension::BlindWidth
            | Dimension::SectionHeight
            | Dimension::SheetNominal
            | Dimension::SheetActual
            | Dimension::SheetLength
            | Dimension::SheetWidth
            | Dimension::BandThickness
            | Dimension::BandWidth => return Err(missing()),
        }
        self.modified = true;
        Ok(())
    }

    /// The wall the drawing shows: the edited wall while the walls are
    /// shown, otherwise the wall of the edited placement.
    pub(super) fn shown_wall(&self) -> Option<Id> {
        if self.room_selection.showing_walls
            && let Some(wall) = self.wall()
        {
            return Some(wall.id);
        }
        self.placement()
            .or_else(|| {
                let first = *self.placement_indexes().first()?;
                self.room()?.placements.get(first)
            })?
            .wall
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencabinet_model::Units;

    /// A wall's start and end in inches.
    fn corners(designer: &Designer, index: usize) -> [(i64, i64); 2] {
        let wall = &designer.room().unwrap().walls[index];
        let inches = |l: Length| l.nm() / Length::NM_PER_INCH;
        [
            (inches(wall.start.x), inches(wall.start.y)),
            (inches(wall.end.x), inches(wall.end.y)),
        ]
    }

    #[test]
    fn a_cabinet_not_placed_has_a_quantity_and_a_placed_one_counts_its_placements() {
        let mut designer = Designer::default();
        assert_eq!(designer.quantity(), 1);
        designer.set_quantity(" 3 ").unwrap();
        assert_eq!(designer.quantity(), 3);
        assert!(designer.modified());
        assert_eq!(designer.cut_list().rows[0].quantity, "3");

        for text in ["0", "1000", "many", ""] {
            assert_eq!(
                designer.set_quantity(text),
                Err("Must be a whole number from 1 to 999".into())
            );
        }
        assert_eq!(designer.quantity(), 3);

        // A copy needs as many.
        designer.duplicate();
        assert_eq!(designer.quantity(), 3);
        designer.set_quantity("1").unwrap();
        assert_eq!(designer.product().quantity, None);

        // Placing it clears the quantity, which would be a validation error.
        designer.select(0);
        designer.add_wall();
        designer.place().unwrap();
        assert_eq!(designer.product().quantity, None);
        assert_eq!(designer.placed_count(), 1);
        assert_eq!(designer.issues(), []);
        assert_eq!(
            designer.set_quantity("2"),
            Err("A placed cabinet counts its placements".into())
        );
        designer.place().unwrap();
        assert_eq!(designer.placed_count(), 2);
        assert_eq!(designer.cut_list().rows[0].quantity, "2");
    }

    #[test]
    fn walls_follow_each_other_at_right_angles() {
        let mut designer = Designer::new(Units::Imperial);
        assert_eq!(designer.walls(), Vec::<String>::new());
        for _ in 0..3 {
            designer.add_wall();
        }
        assert_eq!(
            designer.walls(),
            ["Wall A · 120\"", "Wall B · 120\"", "Wall C · 120\""]
        );
        assert_eq!(corners(&designer, 1), [(120, 0), (120, 120)]);
        assert_eq!(corners(&designer, 2), [(120, 120), (0, 120)]);

        // Lengthening a wall moves the walls joined after it.
        designer.select_wall(0);
        designer.edit(Dimension::WallLength, "144").unwrap();
        assert_eq!(corners(&designer, 0), [(0, 0), (144, 0)]);
        assert_eq!(corners(&designer, 1), [(144, 0), (144, 120)]);
        assert_eq!(corners(&designer, 2), [(144, 120), (24, 120)]);
        assert_eq!(
            designer.edit(Dimension::WallLength, "0"),
            Err("Must be greater than zero".into())
        );
        designer.edit(Dimension::WallHeight, "8'").unwrap();
        assert_eq!(designer.reading(Dimension::WallHeight), "= 96\"");
        assert_eq!(designer.issues(), []);

        designer.delete_wall().unwrap();
        assert_eq!(designer.walls().len(), 2);
        assert_eq!(designer.selected_wall(), Some(0));
    }

    #[test]
    fn windows_and_doors_are_cut_into_the_middle_of_the_wall() {
        let mut designer = Designer::new(Units::Imperial);
        assert_eq!(
            designer.add_opening(OpeningKind::Window),
            Err("Add a wall first".into())
        );
        designer.add_wall();
        designer.edit(Dimension::WallLength, "144").unwrap();
        designer.add_opening(OpeningKind::Window).unwrap();
        // (144 − 36) / 2 from the start, its sill 42" up.
        assert_eq!(designer.openings(), ["Window · 36\" at 54\""]);
        assert_eq!(designer.reading(Dimension::OpeningSill), "= 42\"");
        designer.edit(Dimension::OpeningOffset, "60").unwrap();
        assert_eq!(
            designer.edit(Dimension::OpeningOffset, "-1"),
            Err("Cannot be negative".into())
        );
        designer.add_opening(OpeningKind::Door).unwrap();
        assert_eq!(
            designer.openings(),
            ["Window · 36\" at 60\"", "Door · 36\" at 54\""]
        );
        assert_eq!(designer.reading(Dimension::OpeningHeight), "= 80\"");
        designer.delete_opening().unwrap();
        assert_eq!(designer.selected_opening(), Some(0));
        assert_eq!(designer.issues(), []);
    }

    #[test]
    fn cabinets_are_placed_after_the_last_one_at_their_height() {
        let mut designer = Designer::new(Units::Imperial);
        assert_eq!(
            designer.place(),
            Err("Add a wall first, on the Walls tab".into())
        );
        assert_eq!(designer.drawing().title, "B1, not placed on a wall");
        designer.add_wall();
        designer.place().unwrap();
        assert_eq!(designer.placements(), ["Wall A at 0\", 0\" up"]);
        assert_eq!(designer.drawing().title, "Elevation of wall Wall A");

        // The next base cabinet goes after it; a wall cabinet starts the
        // row 54" up.
        designer.add();
        designer.place().unwrap();
        assert_eq!(designer.placements(), ["Wall A at 30\", 0\" up"]);
        designer.add();
        designer.set_kind(CabinetKind::Wall);
        designer.place().unwrap();
        assert_eq!(designer.placements(), ["Wall A at 0\", 54\" up"]);
        assert_eq!(designer.placement_problems(), Vec::<String>::new());

        // Moved onto the first base cabinet, it collides.
        designer.edit(Dimension::PlacementElevation, "20").unwrap();
        assert_eq!(designer.placement_problems(), ["B1 and W3 overlap"]);

        // Another wall: it keeps its offset and height.
        designer.add_wall();
        designer.select(2);
        assert!(designer.set_placement_wall(1));
        assert_eq!(designer.placements(), ["Wall B at 0\", 20\" up"]);
        designer.unplace().unwrap();
        assert_eq!(designer.placements(), Vec::<String>::new());
        assert_eq!(designer.unplace(), Err("The cabinet is not placed".into()));
        assert_eq!(designer.issues(), []);
    }

    #[test]
    fn the_drawing_shows_the_edited_wall_while_the_walls_are_edited() {
        let mut designer = Designer::new(Units::Imperial);
        designer.add_wall();
        designer.add_wall();
        designer.place().unwrap();
        // Choosing a wall shows it; going back to the cabinet shows its
        // wall again.
        designer.select_wall(0);
        assert_eq!(designer.drawing().title, "Elevation of wall Wall A");
        designer.show_walls(false);
        assert_eq!(designer.drawing().title, "Elevation of wall Wall B");

        // Deleting a wall removes the placements on it.
        designer.select_wall(1);
        designer.delete_wall().unwrap();
        assert_eq!(designer.placements(), Vec::<String>::new());
    }

    #[test]
    fn rooms_are_added_renamed_chosen_and_deleted() {
        let mut designer = Designer::default();
        assert_eq!(designer.rooms(), Vec::<String>::new());
        assert_eq!(designer.selected_room(), None);
        assert_eq!(designer.room_name(), "");
        assert_eq!(
            designer.set_room_name("Kitchen"),
            Err("Add a room first".into())
        );

        // The first wall makes the first room.
        designer.add_wall();
        assert_eq!(designer.rooms(), ["Room 1"]);
        assert_eq!(designer.selected_room(), Some(0));
        designer.place().unwrap();

        // A new room starts empty, and the cabinet is placed in it too.
        designer.add_room();
        assert_eq!(designer.rooms(), ["Room 1", "Room 2"]);
        assert_eq!(designer.selected_room(), Some(1));
        assert_eq!(designer.walls(), Vec::<String>::new());
        assert_eq!(designer.placements(), Vec::<String>::new());
        assert_eq!(
            designer.place(),
            Err("Add a wall first, on the Walls tab".into())
        );
        designer.add_wall();
        designer.place().unwrap();
        assert_eq!(designer.placements().len(), 1);
        assert_eq!(designer.placed_count(), 2);
        assert_eq!(designer.cut_list().rows[0].quantity, "2");

        designer.set_room_name("  Laundry ").unwrap();
        assert_eq!(designer.rooms(), ["Room 1", "Laundry"]);
        assert_eq!(
            designer.set_room_name(" "),
            Err("A room needs a name".into())
        );
        assert_eq!(designer.room_name(), "Laundry");

        assert!(designer.select_room(0));
        assert_eq!(designer.room_name(), "Room 1");
        assert_eq!(designer.walls().len(), 1);
        assert_eq!(designer.placements().len(), 1);
        assert!(!designer.select_room(2));
        assert_eq!(designer.selected_room(), Some(0));

        // A name already taken is skipped.
        designer.set_room_name("Room 3").unwrap();
        designer.add_room();
        assert_eq!(designer.rooms(), ["Room 3", "Laundry", "Room 4"]);
        assert_eq!(designer.room_contents(), (0, 0));
        designer.delete_room().unwrap();
        assert_eq!(designer.selected_room(), Some(1));

        // Deleting a room keeps the cabinet, placed in the other one.
        assert_eq!(designer.room_contents(), (1, 1));
        designer.delete_room().unwrap();
        assert_eq!(designer.rooms(), ["Room 3"]);
        assert_eq!(designer.selected_room(), Some(0));
        assert_eq!(designer.placed_count(), 1);
        assert_eq!(designer.cabinets().len(), 1);

        // Without rooms it counts by its quantity again.
        designer.delete_room().unwrap();
        assert_eq!(designer.rooms(), Vec::<String>::new());
        assert_eq!(designer.selected_room(), None);
        assert_eq!(designer.placed_count(), 0);
        assert_eq!(designer.cut_list().rows[0].quantity, "1");
        assert_eq!(designer.issues(), []);
        assert_eq!(
            designer.delete_room(),
            Err("There is no room to delete".into())
        );
    }
}
