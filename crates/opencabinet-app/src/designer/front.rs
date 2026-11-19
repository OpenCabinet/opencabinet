// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Editing a cabinet's front as sections stacked from top to bottom.
//!
//! The file keeps a split tree (RFC-0001 §6): one section is an opening at
//! its root, and several are the openings of a horizontal split. Every
//! section keeps its node's identity when it moves or changes kind. A front
//! the window cannot show as a stack, such as one with sections side by
//! side, is kept as it is.

use std::collections::BTreeMap;

use opencabinet_eval::front_openings;
use opencabinet_model::{
    FrontNode, FrontStyle, Id, Insert, InsertKind, Length, Product, Side, Size, SplitDirection,
};

use super::{Designer, capitalize};

/// The most shelves a section takes in the window.
const MAX_SHELVES: u8 = 20;

/// What fills a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKind {
    /// One or two doors.
    Doors,
    /// A drawer.
    Drawer,
    /// A panel that looks like a drawer front.
    FalseFront,
    /// No front, with or without shelves.
    Open,
    /// An empty space for an oven or a microwave.
    Appliance,
}

impl SectionKind {
    /// The name the window shows.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Doors => "Doors",
            Self::Drawer => "Drawer",
            Self::FalseFront => "False front",
            Self::Open => "Open",
            Self::Appliance => "Appliance space",
        }
    }

    fn of(kind: &InsertKind) -> Option<Self> {
        match kind {
            InsertKind::Doors => Some(Self::Doors),
            InsertKind::Drawer => Some(Self::Drawer),
            InsertKind::FalseFront => Some(Self::FalseFront),
            InsertKind::Open => Some(Self::Open),
            InsertKind::Appliance => Some(Self::Appliance),
            InsertKind::None | InsertKind::Other(_) => None,
        }
    }

    fn insert_kind(self) -> InsertKind {
        match self {
            Self::Doors => InsertKind::Doors,
            Self::Drawer => InsertKind::Drawer,
            Self::FalseFront => InsertKind::FalseFront,
            Self::Open => InsertKind::Open,
            Self::Appliance => InsertKind::Appliance,
        }
    }
}

/// Whether a node is an opening the window can edit.
fn is_section(node: &FrontNode) -> bool {
    node.children.is_empty()
        && node.split.is_none()
        && node
            .insert
            .as_ref()
            .is_some_and(|i| SectionKind::of(&i.kind).is_some())
}

/// Whether a front is one section, or sections stacked top to bottom.
fn is_stack(front: &FrontNode) -> bool {
    is_section(front)
        || (front.split == Some(SplitDirection::Horizontal)
            && front.insert.is_none()
            && !front.children.is_empty()
            && front.children.iter().all(is_section))
}

/// Whether a front has sections side by side anywhere in its tree.
fn has_side_by_side(node: &FrontNode) -> bool {
    node.split == Some(SplitDirection::Vertical) || node.children.iter().any(has_side_by_side)
}

/// The sections of a stack, from top to bottom.
fn sections_of(front: &FrontNode) -> &[FrontNode] {
    if front.children.is_empty() {
        std::slice::from_ref(front)
    } else {
        &front.children
    }
}

/// Section `index` of a stack.
fn section_mut(front: &mut FrontNode, index: usize) -> Option<&mut FrontNode> {
    if front.children.is_empty() {
        (index == 0).then_some(front)
    } else {
        front.children.get_mut(index)
    }
}

/// What a section holds, for the list: "2 doors, 4 shelves".
fn describe(insert: &Insert) -> String {
    let shelves = match insert.shelves.unwrap_or(0) {
        0 => String::new(),
        1 => ", 1 shelf".to_owned(),
        count => format!(", {count} shelves"),
    };
    match &insert.kind {
        InsertKind::Doors => match (insert.count.unwrap_or(1), insert.hinge_side) {
            (1, Some(Side::Left)) => format!("1 door, hinged left{shelves}"),
            (1, Some(Side::Right)) => format!("1 door, hinged right{shelves}"),
            (1, None) => format!("1 door{shelves}"),
            (count, _) => format!("{count} doors{shelves}"),
        },
        InsertKind::Open => format!("Open{shelves}"),
        kind => SectionKind::of(kind)
            .map_or("", SectionKind::name)
            .to_owned(),
    }
}

impl Designer {
    /// The sections of the edited cabinet's front, when it is a stack.
    fn section_nodes(&self) -> Option<&[FrontNode]> {
        let front = self.product().front.as_ref().filter(|f| is_stack(f))?;
        Some(sections_of(front))
    }

    /// The edited section's node.
    fn section_node(&self) -> Option<&FrontNode> {
        let nodes = self.section_nodes()?;
        nodes.get(self.section.min(nodes.len() - 1))
    }

    /// The edited section's insert.
    fn section_insert(&self) -> Option<&Insert> {
        self.section_node()?.insert.as_ref()
    }

    /// Why the cabinet's front cannot be edited here, if it cannot.
    #[must_use]
    pub fn front_note(&self) -> Option<&'static str> {
        match &self.product().front {
            None => Some("This cabinet has no front to edit"),
            Some(front) if is_stack(front) => None,
            Some(front) if has_side_by_side(front) => {
                Some("This front has side-by-side sections and cannot be edited here")
            }
            Some(_) => Some("This front cannot be edited here"),
        }
    }

    /// The kinds of section this cabinet can take: a blind corner has no
    /// side to mount a drawer's slides on next to its blind part.
    #[must_use]
    pub fn section_kinds(&self) -> Vec<SectionKind> {
        let mut kinds = vec![SectionKind::Doors];
        if self.blind_side().is_none() {
            kinds.push(SectionKind::Drawer);
        }
        kinds.extend([
            SectionKind::FalseFront,
            SectionKind::Open,
            SectionKind::Appliance,
        ]);
        kinds
    }

    /// The sections as the list shows them, from top to bottom:
    /// "Drawer · 6\"", "2 doors · rest, 24-1/2\"".
    #[must_use]
    pub fn sections(&self) -> Vec<String> {
        let Some(nodes) = self.section_nodes() else {
            return Vec::new();
        };
        nodes
            .iter()
            .map(|node| {
                let what = node.insert.as_ref().map_or_else(String::new, describe);
                let height = self.opening_height(node.id).map(|h| self.show(h));
                match (node.size, height) {
                    (Some(Size::Length(_)), Some(height)) => format!("{what} · {height}"),
                    (_, Some(height)) => format!("{what} · rest, {height}"),
                    (_, None) => format!("{what} · rest"),
                }
            })
            .collect()
    }

    /// How high the opening of the node `id` comes out, when the front can
    /// be laid out.
    fn opening_height(&self, id: Id) -> Option<Length> {
        let openings = front_openings(self.product(), &self.document.library).ok()?;
        openings
            .into_iter()
            .find(|(opening, _)| *opening == id)
            .map(|(_, rect)| rect.height)
    }

    /// Which section is being edited, by its place from the top.
    #[must_use]
    pub fn selected_section(&self) -> Option<usize> {
        let nodes = self.section_nodes()?;
        Some(self.section.min(nodes.len() - 1))
    }

    /// Edits another section; returns whether there is one there.
    pub fn select_section(&mut self, index: usize) -> bool {
        let exists = self.section_nodes().is_some_and(|n| index < n.len());
        if exists {
            self.section = index;
        }
        exists
    }

    /// The edited section's kind.
    #[must_use]
    pub fn section_kind(&self) -> Option<SectionKind> {
        SectionKind::of(&self.section_insert()?.kind)
    }

    /// How many doors the edited section has, if it has doors.
    #[must_use]
    pub fn door_count(&self) -> Option<u8> {
        let insert = self.section_insert()?;
        (insert.kind == InsertKind::Doors).then(|| insert.count.unwrap_or(1))
    }

    /// The stored hinge side of the edited section's single door.
    #[must_use]
    pub fn hinge_side(&self) -> Option<Side> {
        self.section_insert()?.hinge_side
    }

    /// How many shelves the edited section has, if it can have them.
    #[must_use]
    pub fn shelves(&self) -> Option<u8> {
        let insert = self.section_insert()?;
        matches!(insert.kind, InsertKind::Doors | InsertKind::Open)
            .then(|| insert.shelves.unwrap_or(0))
    }

    /// The fixed height of the edited section; `None` when it takes the
    /// rest.
    pub(super) fn section_height(&self) -> Option<Length> {
        match self.section_node()?.size {
            Some(Size::Length(length)) => Some(length),
            _ => None,
        }
    }

    /// What the line under the height field says when the section takes
    /// the rest of the front.
    pub(super) fn section_rest_reading(&self) -> String {
        self.section_node()
            .and_then(|node| self.opening_height(node.id))
            .map_or_else(String::new, |h| format!("takes the rest: {}", self.show(h)))
    }

    /// Stores `candidate` as the edited cabinet, unless it breaks a front
    /// that could be laid out.
    fn commit_front(&mut self, candidate: Product) -> Result<(), String> {
        let library = &self.document.library;
        if let Err(message) = front_openings(&candidate, library)
            && front_openings(self.product(), library).is_ok()
        {
            return Err(capitalize(&message));
        }
        if *self.product() != candidate {
            *self.product_mut() = candidate;
            self.modified = true;
        }
        Ok(())
    }

    /// A copy of the edited cabinet with `change` applied to section
    /// `index` of its front, and the insert there.
    fn with_insert(&self, change: impl FnOnce(&mut Insert)) -> Option<Product> {
        let index = self.selected_section()?;
        let mut candidate = self.product().clone();
        let node = section_mut(candidate.front.as_mut()?, index)?;
        change(node.insert.as_mut()?);
        Some(candidate)
    }

    /// The style a new door, drawer or false front takes: the cabinet's.
    fn front_style(&self) -> FrontStyle {
        if self.five_piece() {
            FrontStyle::FivePiece
        } else {
            FrontStyle::Slab
        }
    }

    /// The hinge side a new single door stores: the left, except that a
    /// blind corner's door hinges away from the blind part by itself.
    fn usual_hinge(&self) -> Option<Side> {
        self.blind_side().is_none().then_some(Side::Left)
    }

    /// Adds a 6" section below the edited one, and edits it: a drawer, or
    /// in a blind corner a false front.
    ///
    /// # Errors
    /// A message for the user when the front cannot be edited here or has
    /// no room for it.
    pub fn add_section(&mut self) -> Result<(), String> {
        let selected = self
            .selected_section()
            .ok_or("This front cannot be edited here")?;
        let kind = if self.blind_side().is_none() {
            InsertKind::Drawer
        } else {
            InsertKind::FalseFront
        };
        let section = FrontNode {
            id: Id::new(),
            size: Some(Size::Length(Length::from_inches(6))),
            split: None,
            children: Vec::new(),
            insert: Some(Insert {
                kind,
                style: Some(self.front_style()),
                count: None,
                hinge_side: None,
                shelves: None,
                adjustments: Vec::new(),
                unknown: BTreeMap::default(),
            }),
            unknown: BTreeMap::default(),
        };
        let mut candidate = self.product().clone();
        let front = candidate
            .front
            .as_mut()
            .ok_or("This cabinet has no front")?;
        if front.children.is_empty() {
            // The one section keeps its identity under a new split, and
            // takes what the new section leaves.
            let split = FrontNode {
                id: Id::new(),
                size: None,
                split: Some(SplitDirection::Horizontal),
                children: Vec::new(),
                insert: None,
                unknown: BTreeMap::default(),
            };
            let mut only = std::mem::replace(front, split);
            only.size = Some(Size::Share(1));
            front.children.push(only);
        }
        front.children.insert(selected + 1, section);
        self.commit_front(candidate)?;
        self.section = selected + 1;
        Ok(())
    }

    /// Removes the edited section. When none would take the rest of the
    /// front, the last one does.
    ///
    /// # Errors
    /// A message for the user when it is the only section, or the front
    /// cannot be edited here.
    pub fn delete_section(&mut self) -> Result<(), String> {
        let selected = self
            .selected_section()
            .ok_or("This front cannot be edited here")?;
        let mut candidate = self.product().clone();
        let front = candidate
            .front
            .as_mut()
            .ok_or("This cabinet has no front")?;
        if front.children.is_empty() {
            return Err("A front keeps at least one section".into());
        }
        front.children.remove(selected);
        if front.children.len() == 1 {
            let mut only = front.children.remove(0);
            only.size = None;
            *front = only;
        } else if !front
            .children
            .iter()
            .any(|c| matches!(c.size, Some(Size::Share(_))))
            && let Some(last) = front.children.last_mut()
        {
            last.size = Some(Size::Share(1));
        }
        self.commit_front(candidate)?;
        self.section = selected.saturating_sub(1);
        Ok(())
    }

    /// Moves the edited section one place up or down, if it can, and
    /// keeps editing it.
    pub fn move_section(&mut self, up: bool) {
        let Some(selected) = self.selected_section() else {
            return;
        };
        let count = self.section_nodes().map_or(0, <[FrontNode]>::len);
        let target = if up {
            selected.checked_sub(1)
        } else {
            Some(selected + 1).filter(|&t| t < count)
        };
        let Some(target) = target else {
            return;
        };
        if let Some(front) = &mut self.product_mut().front {
            front.children.swap(selected, target);
            self.section = target;
            self.modified = true;
        }
    }

    /// Changes what fills the edited section, with that kind's usual
    /// contents; its height and identity stay. Shelves stay between doors
    /// and an open section.
    pub fn set_section_kind(&mut self, kind: SectionKind) {
        if self.section_kind() == Some(kind) {
            return;
        }
        let (style, hinge) = (self.front_style(), self.usual_hinge());
        let candidate = self.with_insert(|insert| {
            insert.kind = kind.insert_kind();
            insert.adjustments.clear();
            insert.count = (kind == SectionKind::Doors).then_some(1);
            insert.hinge_side = if kind == SectionKind::Doors {
                hinge
            } else {
                None
            };
            insert.style = matches!(
                kind,
                SectionKind::Doors | SectionKind::Drawer | SectionKind::FalseFront
            )
            .then_some(style);
            if !matches!(kind, SectionKind::Doors | SectionKind::Open) {
                insert.shelves = None;
            }
        });
        if let Some(candidate) = candidate {
            *self.product_mut() = candidate;
            self.modified = true;
        }
    }

    /// Gives the edited section one door or a pair. A pair hinges on its
    /// outer edges, so it stores no hinge side.
    pub fn set_door_count(&mut self, count: u8) {
        if self.door_count().is_none_or(|c| c == count) || !(1..=2).contains(&count) {
            return;
        }
        let hinge = if count == 1 { self.usual_hinge() } else { None };
        let candidate = self.with_insert(|insert| {
            insert.count = Some(count);
            insert.hinge_side = hinge;
        });
        if let Some(candidate) = candidate {
            *self.product_mut() = candidate;
            self.modified = true;
        }
    }

    /// Hinges the edited section's single door on `side`.
    pub fn set_hinge_side(&mut self, side: Side) {
        if self.door_count() != Some(1) || self.hinge_side() == Some(side) {
            return;
        }
        if let Some(candidate) = self.with_insert(|insert| insert.hinge_side = Some(side)) {
            *self.product_mut() = candidate;
            self.modified = true;
        }
    }

    /// Reads what the user typed as how many shelves the edited section
    /// has.
    ///
    /// # Errors
    /// A message for the user when it is not a whole number the window
    /// takes, or the section has no shelves; nothing then changes.
    pub fn set_shelves(&mut self, text: &str) -> Result<(), String> {
        let Some(old) = self.shelves() else {
            return Err("This section has no shelves".into());
        };
        let count = text
            .trim()
            .parse::<u8>()
            .ok()
            .filter(|&c| c <= MAX_SHELVES)
            .ok_or_else(|| format!("Must be a whole number from 0 to {MAX_SHELVES}"))?;
        if count != old
            && let Some(candidate) =
                self.with_insert(|insert| insert.shelves = (count > 0).then_some(count))
        {
            *self.product_mut() = candidate;
            self.modified = true;
        }
        Ok(())
    }

    /// Fixes the edited section's height, or with `None` lets it take the
    /// rest of the front.
    pub(super) fn set_section_height(&mut self, height: Option<Length>) -> Result<(), String> {
        let selected = self
            .selected_section()
            .ok_or("This front cannot be edited here")?;
        let mut candidate = self.product().clone();
        let front = candidate
            .front
            .as_mut()
            .ok_or("This cabinet has no front")?;
        if front.children.is_empty() {
            return match height {
                None => Ok(()),
                Some(_) => Err("The only section takes the whole front".into()),
            };
        }
        let node = front
            .children
            .get_mut(selected)
            .ok_or("This front cannot be edited here")?;
        node.size = Some(height.map_or(Size::Share(1), Size::Length));
        self.commit_front(candidate)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use opencabinet_eval::{HardwareKind, Part, evaluate};

    use super::super::{CabinetKind, Dimension};
    use super::*;

    /// Millimeters with only the decimals they need.
    fn mm(length: Length) -> String {
        let nm = length.nm();
        let text = format!("{}.{:06}", nm / 1_000_000, (nm % 1_000_000).abs());
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    }

    /// The parts of the edited cabinet with `role`.
    fn parts(designer: &Designer, role: &str) -> Vec<Part> {
        let product = designer.product().id;
        evaluate(&designer.document.project, &designer.document.library)
            .parts
            .into_iter()
            .filter(|p| p.product == product && p.role == role)
            .collect()
    }

    /// The `length × width` of the edited cabinet's parts with `role`.
    fn sizes(designer: &Designer, role: &str) -> Vec<String> {
        parts(designer, role)
            .iter()
            .map(|p| format!("{} × {}", mm(p.length), mm(p.width)))
            .collect()
    }

    /// What stops the edited cabinet from being evaluated.
    fn blocking(designer: &Designer) -> Vec<String> {
        evaluate(&designer.document.project, &designer.document.library)
            .problems
            .into_iter()
            .filter(|p| p.blocking && p.product == designer.label())
            .map(|p| p.message)
            .collect()
    }

    /// A front with two sections side by side.
    fn side_by_side(designer: &Designer) -> FrontNode {
        let mut front = designer.product().front.clone().unwrap();
        front.split = Some(SplitDirection::Vertical);
        for child in &mut front.children {
            child.size = Some(Size::Share(1));
        }
        front
    }

    #[test]
    fn a_base_with_three_drawers_matches_the_sizes_worked_out_by_hand() {
        // The starter B1: 30" × 34-1/2" × 24", toe kick 4", so the face is
        // 762 × 774.7, with gaps of 3.175.
        let mut designer = Designer::default();
        assert_eq!(
            designer.sections(),
            ["Drawer · 6\"", "2 doors · rest, 24-1/2\""]
        );
        designer.select_section(1);
        designer.set_section_kind(SectionKind::Drawer);
        designer.edit(Dimension::SectionHeight, "").unwrap();
        designer.select_section(0);
        designer.add_section().unwrap();
        assert_eq!(designer.selected_section(), Some(1));
        designer.edit(Dimension::SectionHeight, "8").unwrap();
        assert_eq!(
            designer.sections(),
            ["Drawer · 6\"", "Drawer · 8\"", "Drawer · rest, 16-1/2\"",]
        );
        // Openings 152.4, 203.2 and 774.7 − 355.6 = 419.1, each front 3.175
        // less, its grain across.
        assert_eq!(
            sizes(&designer, "drawerFront"),
            [
                "758.825 × 149.225",
                "758.825 × 200.025",
                "758.825 × 415.925",
            ]
        );
        // Each box is 1-1/2" lower than its opening. It runs on a 22" slide,
        // the longest in 24" less the 1" clearance.
        assert_eq!(
            sizes(&designer, "drawerSide"),
            [
                "558.8 × 114.3",
                "558.8 × 114.3",
                "558.8 × 165.1",
                "558.8 × 165.1",
                "558.8 × 381",
                "558.8 × 381",
            ]
        );
        assert_eq!(sizes(&designer, "door"), Vec::<String>::new());
        assert_eq!(blocking(&designer), Vec::<String>::new());
    }

    #[test]
    fn a_tower_with_an_oven_matches_the_sizes_worked_out_by_hand() {
        // A 30" × 84" tall cabinet, its face 2032 high: doors take what a
        // 30" oven space and a 10" drawer leave, 2032 − 762 − 254 = 1016.
        let mut designer = Designer::default();
        designer.set_kind(CabinetKind::Tall);
        designer.add_section().unwrap();
        designer.set_section_kind(SectionKind::Appliance);
        designer.edit(Dimension::SectionHeight, "30").unwrap();
        designer.add_section().unwrap();
        designer.edit(Dimension::SectionHeight, "10").unwrap();
        assert_eq!(
            designer.sections(),
            [
                "2 doors, 4 shelves · rest, 40\"",
                "Appliance space · 30\"",
                "Drawer · 10\"",
            ]
        );
        assert_eq!(
            sizes(&designer, "door"),
            ["1012.825 × 377.825", "1012.825 × 377.825"]
        );
        assert_eq!(sizes(&designer, "drawerFront"), ["758.825 × 250.825"]);
        assert_eq!(parts(&designer, "shelf").len(), 4);
        assert_eq!(blocking(&designer), Vec::<String>::new());
    }

    #[test]
    fn a_face_frame_section_is_the_height_of_its_frame_opening() {
        // The sample's W30, made tall: 762 wide, its frame of 38.1 members
        // around an opening of 685.8 × (2032 − 76.2 = 1955.8). Two rails
        // come out of that, and a 6" false front and a 30" oven leave the
        // doors 1955.8 − 76.2 − 152.4 − 762 = 965.2. Fronts overlay the
        // frame by 12.7 on each edge.
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kitchen.ocab");
        let mut designer = Designer::open(&fixture).unwrap();
        designer.select(3);
        assert_eq!(designer.label(), "W30");
        designer.set_five_piece(false);
        designer.set_kind(CabinetKind::Tall);
        designer.set_door_count(1);
        designer.add_section().unwrap();
        designer.set_section_kind(SectionKind::FalseFront);
        designer.add_section().unwrap();
        designer.set_section_kind(SectionKind::Appliance);
        designer.edit(Dimension::SectionHeight, "30").unwrap();
        assert_eq!(sizes(&designer, "door"), ["990.6 × 711.2"]);
        assert_eq!(sizes(&designer, "falseFront"), ["711.2 × 177.8"]);
        // The top and bottom rails, then the two between the sections.
        assert_eq!(
            sizes(&designer, "frameRail"),
            [
                "685.8 × 38.1",
                "685.8 × 38.1",
                "685.8 × 38.1",
                "685.8 × 38.1",
            ]
        );
        assert_eq!(blocking(&designer), Vec::<String>::new());
    }

    #[test]
    fn sections_keep_their_identity_as_the_front_changes() {
        let mut designer = Designer::default();
        designer.set_kind(CabinetKind::Wall);
        let ids =
            |d: &Designer| -> Vec<Id> { d.section_nodes().unwrap().iter().map(|n| n.id).collect() };
        let first = ids(&designer)[0];

        // The first section stays the same node under the new split.
        designer.add_section().unwrap();
        let both = ids(&designer);
        assert_eq!(both[0], first);

        designer.move_section(true);
        assert_eq!(ids(&designer), [both[1], both[0]]);
        assert_eq!(designer.selected_section(), Some(0));
        designer.move_section(true);
        assert_eq!(designer.selected_section(), Some(0));

        designer.set_section_kind(SectionKind::Open);
        assert_eq!(ids(&designer), [both[1], both[0]]);

        // Back to one section, the node is the front again.
        designer.delete_section().unwrap();
        assert_eq!(ids(&designer), [first]);
        let front = designer.product().front.as_ref().unwrap();
        assert_eq!((front.id, front.size), (first, None));
        assert_eq!(
            designer.delete_section(),
            Err("A front keeps at least one section".into())
        );
    }

    #[test]
    fn deleting_the_section_that_takes_the_rest_gives_it_to_the_last_one() {
        let mut designer = Designer::default();
        designer.select_section(0);
        designer.add_section().unwrap();
        // Drawer, drawer, then doors that take the rest.
        designer.select_section(2);
        designer.delete_section().unwrap();
        assert_eq!(
            designer.sections(),
            ["Drawer · 6\"", "Drawer · rest, 24-1/2\""]
        );
        assert_eq!(designer.selected_section(), Some(1));
    }

    #[test]
    fn heights_that_do_not_fit_are_refused_and_nothing_changes() {
        let mut designer = Designer::default();
        let before = designer.product().clone();
        assert_eq!(
            designer.edit(Dimension::SectionHeight, "40"),
            Err("The front's fixed sizes are larger than the opening".into())
        );
        assert_eq!(
            designer.edit(Dimension::SectionHeight, "tall"),
            Err("Not a length".into())
        );
        assert_eq!(
            designer.edit(Dimension::SectionHeight, "0"),
            Err("Must be greater than zero".into())
        );
        // The doors are the only section left to take the rest.
        designer.select_section(1);
        assert_eq!(
            designer.edit(Dimension::SectionHeight, "12"),
            Err("The front's fixed sizes do not fill the opening".into())
        );
        assert_eq!(*designer.product(), before);

        assert_eq!(
            designer.reading(Dimension::SectionHeight),
            "takes the rest: 24-1/2\""
        );
        designer.select_section(0);
        assert_eq!(designer.reading(Dimension::SectionHeight), "= 6\"");

        // Fixed heights that fill the face exactly are fine: 774.7 − 152.4.
        designer.select_section(1);
        designer.edit(Dimension::SectionHeight, "24.5").unwrap();
        assert_eq!(designer.sections()[1], "2 doors · 24-1/2\"");

        // One section always takes the whole front.
        let mut designer = Designer::default();
        designer.set_kind(CabinetKind::Wall);
        assert_eq!(
            designer.edit(Dimension::SectionHeight, "12"),
            Err("The only section takes the whole front".into())
        );
        assert_eq!(designer.edit(Dimension::SectionHeight, ""), Ok(()));
    }

    #[test]
    fn doors_choose_their_count_hinges_and_shelves() {
        let mut designer = Designer::default();
        designer.set_kind(CabinetKind::Wall);
        assert_eq!(designer.door_count(), Some(2));
        assert_eq!(designer.shelves(), Some(2));
        assert_eq!(designer.hinge_side(), None);

        designer.set_door_count(1);
        assert_eq!(designer.hinge_side(), Some(Side::Left));
        designer.set_hinge_side(Side::Right);
        assert_eq!(
            designer.sections(),
            ["1 door, hinged right, 2 shelves · rest, 30\""]
        );
        let door = &parts(&designer, "door")[0];
        // Right hinged: cups 4 + 17.5 from the door's right edge.
        let Some(opencabinet_eval::Operation::Hole { at, .. }) = door.operations.first() else {
            panic!("the door is bored for its hinges");
        };
        assert_eq!(at.y, door.width - Length::from_nm(21_500_000));

        designer.set_shelves(" 3 ").unwrap();
        assert_eq!(parts(&designer, "shelf").len(), 3);
        designer.set_shelves("0").unwrap();
        assert_eq!(designer.section_insert().unwrap().shelves, None);
        assert_eq!(
            designer.set_shelves("many"),
            Err("Must be a whole number from 0 to 20".into())
        );
        assert_eq!(
            designer.set_shelves("21"),
            Err("Must be a whole number from 0 to 20".into())
        );

        designer.set_door_count(2);
        assert_eq!(designer.hinge_side(), None);
        assert_eq!(parts(&designer, "door").len(), 2);

        // A drawer has no doors or shelves to set.
        designer.set_section_kind(SectionKind::Drawer);
        assert_eq!((designer.door_count(), designer.shelves()), (None, None));
        assert_eq!(
            designer.set_shelves("1"),
            Err("This section has no shelves".into())
        );
    }

    #[test]
    fn shelves_stay_between_doors_and_open_sections_only() {
        let mut designer = Designer::default();
        designer.set_kind(CabinetKind::Wall);
        designer.set_section_kind(SectionKind::Open);
        assert_eq!(designer.shelves(), Some(2));
        assert_eq!(designer.sections(), ["Open, 2 shelves · rest, 30\""]);
        assert_eq!(parts(&designer, "door").len(), 0);
        designer.set_section_kind(SectionKind::Doors);
        assert_eq!(designer.shelves(), Some(2));
        assert_eq!(designer.hinge_side(), Some(Side::Left));
        designer.set_section_kind(SectionKind::Appliance);
        assert_eq!(designer.shelves(), None);
        designer.set_section_kind(SectionKind::Doors);
        assert_eq!(designer.shelves(), Some(0));
    }

    #[test]
    fn new_sections_take_the_cabinets_style() {
        let mut designer = Designer::default();
        designer.set_five_piece(true);
        designer.add_section().unwrap();
        let insert = designer.section_insert().unwrap();
        assert_eq!(insert.style, Some(FrontStyle::FivePiece));
        designer.set_section_kind(SectionKind::Doors);
        assert_eq!(
            designer.section_insert().unwrap().style,
            Some(FrontStyle::FivePiece)
        );
        designer.set_section_kind(SectionKind::Open);
        assert_eq!(designer.section_insert().unwrap().style, None);
    }

    #[test]
    fn a_blind_corner_offers_no_drawers_and_its_door_hinges_by_itself() {
        let mut designer = Designer::default();
        assert!(designer.section_kinds().contains(&SectionKind::Drawer));
        designer.set_kind(CabinetKind::BlindBase);
        assert!(!designer.section_kinds().contains(&SectionKind::Drawer));
        assert_eq!(designer.sections(), ["1 door, 1 shelf · rest, 30-1/2\""]);

        designer.add_section().unwrap();
        assert_eq!(designer.section_kind(), Some(SectionKind::FalseFront));
        designer.set_section_kind(SectionKind::Doors);
        assert_eq!(designer.hinge_side(), None);
        // A door 6" high is too short for its hinges: the evaluator says so.
        assert_eq!(
            blocking(&designer),
            ["the door is too short for its hinges"]
        );
        designer.edit(Dimension::SectionHeight, "12").unwrap();
        assert_eq!(blocking(&designer), Vec::<String>::new());
    }

    #[test]
    fn a_front_with_sections_side_by_side_is_kept_as_it_is() {
        let mut designer = Designer::default();
        assert_eq!(designer.front_note(), None);
        let mut unknown = designer.product().front.clone().unwrap();
        unknown.children[0].insert.as_mut().unwrap().kind = InsertKind::None;
        let before = designer.product_mut().front.replace(unknown);
        assert_eq!(
            designer.front_note(),
            Some("This front cannot be edited here")
        );
        designer.product_mut().front = before;

        let front = side_by_side(&designer);
        designer.product_mut().front = Some(front.clone());
        assert_eq!(
            designer.front_note(),
            Some("This front has side-by-side sections and cannot be edited here")
        );
        assert_eq!(designer.sections(), Vec::<String>::new());
        assert_eq!(designer.selected_section(), None);
        assert!(designer.add_section().is_err());
        assert!(designer.delete_section().is_err());
        designer.move_section(true);
        designer.set_section_kind(SectionKind::Open);
        designer.set_door_count(1);
        designer.set_hinge_side(Side::Right);
        assert!(designer.set_shelves("1").is_err());
        assert!(designer.edit(Dimension::SectionHeight, "6").is_err());
        assert_eq!(designer.product().front.as_ref(), Some(&front));
    }

    #[test]
    fn an_edited_front_saves_and_reopens_as_it_was() {
        let mut designer = Designer::default();
        designer.add_section().unwrap();
        designer.set_section_kind(SectionKind::Open);
        designer.set_shelves("2").unwrap();
        designer.select_section(2);
        designer.set_door_count(1);
        designer.set_hinge_side(Side::Right);

        let dir = std::env::temp_dir().join(format!("opencabinet-front-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("front.ocab");
        designer.save_as(&path).unwrap();
        let reopened = Designer::open(&path).unwrap();
        assert_eq!(reopened.product(), designer.product());
        assert_eq!(reopened.issues(), []);
    }

    #[test]
    fn the_hardware_follows_the_sections() {
        // Three pairs of slides for three drawers, no hinges.
        let mut designer = Designer::default();
        designer.select_section(1);
        designer.set_section_kind(SectionKind::Drawer);
        designer.add_section().unwrap();
        designer.set_section_kind(SectionKind::Drawer);
        let id = designer.product().id;
        let hardware = evaluate(&designer.document.project, &designer.document.library).hardware;
        let used: Vec<(HardwareKind, u32)> = hardware
            .into_iter()
            .filter(|h| h.product == id)
            .map(|h| (h.kind, h.quantity))
            .collect();
        assert_eq!(
            used,
            [(HardwareKind::SlidePair(Length::from_inches(22)), 3)]
        );
    }
}
