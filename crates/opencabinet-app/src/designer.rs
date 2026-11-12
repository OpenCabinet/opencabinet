// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the window shows and edits, without any window: the open project,
//! where it is saved, its cabinets and the one being edited. Keeping it
//! free of Slint lets it be tested directly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use opencabinet_eval::{elevation, front_view, placement_problems, plan};
use opencabinet_format::Document;
use opencabinet_model::validate::{Issue, validate};
use opencabinet_model::{
    FrontNode, FrontStyle, Id, Insert, InsertKind, Length, Library, Method, Product, Project,
    Settings, SheetMaterial, Side, Size, SplitDirection, Standard, Units,
};
use opencabinet_reports::elevations::elevations_dxf;
use opencabinet_reports::{
    Bom, CutList, Fonts, Paper, Separator, bill_of_materials, cut_list, cut_list_pdf,
    elevations_pdf, materials_to_csv, parts_dxf, to_csv,
};

use crate::NAME_AND_VERSION;
use crate::drawing::{self, Drawing};

mod front;
mod materials;
mod room;
mod standard;
pub mod template;

pub use front::SectionKind;
pub use materials::{MaterialRole, RoleChoices};
pub use room::{OpeningKind, WallView};
pub use standard::{RuleKind, RuleRow};

/// A length the window edits: the cabinet's overall dimensions, the edited
/// wall's and opening's, and where the edited placement puts the cabinet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimension {
    /// The cabinet, left to right.
    Width,
    /// The cabinet, floor to top.
    Height,
    /// The cabinet, front to back.
    Depth,
    /// The wall, from its start to its end.
    WallLength,
    /// The wall, floor to ceiling.
    WallHeight,
    /// From the wall's start to the opening.
    OpeningOffset,
    /// The opening, side to side.
    OpeningWidth,
    /// The opening, bottom to top.
    OpeningHeight,
    /// From the floor to the opening's bottom.
    OpeningSill,
    /// From the wall's start to the cabinet's left side.
    PlacementOffset,
    /// From the floor to the cabinet's bottom.
    PlacementElevation,
    /// How much of a blind corner's front is hidden, from its blind side.
    BlindWidth,
    /// The edited section of the front, top to bottom.
    SectionHeight,
    /// The thickness the edited sheet is sold as.
    SheetNominal,
    /// The thickness the edited sheet really has.
    SheetActual,
    /// The edited sheet, along its grain.
    SheetLength,
    /// The edited sheet, across its grain.
    SheetWidth,
    /// The edited edge band, how thick it is.
    BandThickness,
    /// The edited edge band, how wide it is.
    BandWidth,
}

impl Dimension {
    /// Every dimension, in the order the window shows them.
    pub const ALL: [Self; 19] = [
        Self::Width,
        Self::Height,
        Self::Depth,
        Self::WallLength,
        Self::WallHeight,
        Self::OpeningOffset,
        Self::OpeningWidth,
        Self::OpeningHeight,
        Self::OpeningSill,
        Self::PlacementOffset,
        Self::PlacementElevation,
        Self::BlindWidth,
        Self::SectionHeight,
        Self::SheetNominal,
        Self::SheetActual,
        Self::SheetLength,
        Self::SheetWidth,
        Self::BandThickness,
        Self::BandWidth,
    ];

    /// Whether it may be zero: offsets and heights above the floor may,
    /// sizes may not.
    fn may_be_zero(self) -> bool {
        matches!(
            self,
            Self::OpeningOffset
                | Self::OpeningSill
                | Self::PlacementOffset
                | Self::PlacementElevation
        )
    }
}

/// The editing state of the window: a project, the file it lives in, and
/// which of its cabinets is being edited.
#[derive(Clone, Debug, PartialEq)]
pub struct Designer {
    document: Document,
    path: Option<PathBuf>,
    modified: bool,
    /// The index of the product being edited; a project always has one.
    selected: usize,
    /// The edited section of its front, from the top; read within the
    /// sections it has.
    section: usize,
    room_selection: room::RoomSelection,
    material_selection: materials::MaterialSelection,
    /// The standard the Standard tab edits, read within the ones the
    /// project has.
    standard: usize,
}

impl Default for Designer {
    /// A new project in inches.
    fn default() -> Self {
        Self::new(Units::Imperial)
    }
}

impl Designer {
    /// A new, unsaved project with one 30" × 34-1/2" × 24" base cabinet,
    /// shown in `units`.
    #[must_use]
    pub fn new(units: Units) -> Self {
        Self::with_method(units, Method::Frameless)
    }

    /// A new, unsaved project with both starter standards, whose first
    /// cabinet is built by `method`.
    #[must_use]
    pub fn with_method(units: Units, method: Method) -> Self {
        Self::around(new_document(units, method))
    }

    /// A new, unsaved project holding `document`.
    fn around(document: Document) -> Self {
        let mut designer = Self {
            document,
            path: None,
            modified: false,
            selected: 0,
            section: 0,
            room_selection: room::RoomSelection::default(),
            material_selection: materials::MaterialSelection::default(),
            standard: 0,
        };
        designer.follow_cabinet_standard();
        designer
    }

    /// Opens a project file.
    ///
    /// # Errors
    /// A message for the user when the file cannot be read or has no
    /// product to show; the current project is then left unchanged.
    pub fn open(path: &Path) -> Result<Self, String> {
        let document = Document::load(path).map_err(|e| format!("Cannot open the file: {e}"))?;
        if document.project.products.is_empty() {
            return Err("The project has no products to show yet".into());
        }
        let mut designer = Self {
            document,
            path: Some(path.to_owned()),
            modified: false,
            selected: 0,
            section: 0,
            room_selection: room::RoomSelection::default(),
            material_selection: materials::MaterialSelection::default(),
            standard: 0,
        };
        designer.first_selection();
        designer.follow_cabinet_standard();
        Ok(designer)
    }

    /// Saves the project to `path`, which becomes its file.
    ///
    /// # Errors
    /// A message for the user when the file cannot be written.
    pub fn save_as(&mut self, path: &Path) -> Result<(), String> {
        self.document.generator = NAME_AND_VERSION.into();
        self.document
            .save_zip(path)
            .map_err(|e| format!("Cannot save the file: {e}"))?;
        self.path = Some(path.to_owned());
        self.modified = false;
        Ok(())
    }

    /// The file the project lives in, once it has been saved or opened.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether there are changes that are not saved.
    #[must_use]
    pub fn modified(&self) -> bool {
        self.modified
    }

    /// The window title: the file's name, `*` when modified, and the program.
    #[must_use]
    pub fn title(&self) -> String {
        let name = self
            .path
            .as_deref()
            .and_then(Path::file_name)
            .map_or_else(|| "Untitled".into(), |n| n.to_string_lossy().into_owned());
        let mark = if self.modified { "*" } else { "" };
        format!("{name}{mark} — OpenCabinet Designer")
    }

    /// Problems found in the project's authored data.
    #[must_use]
    pub fn issues(&self) -> Vec<Issue> {
        validate(&self.document.project, &self.document.library)
    }

    /// The project's name, such as "Smith kitchen".
    #[must_use]
    pub fn project_name(&self) -> &str {
        &self.document.project.name
    }

    /// The label of the product being edited, such as "B30".
    #[must_use]
    pub fn label(&self) -> &str {
        &self.product().label
    }

    /// Relabels the cabinet being edited.
    ///
    /// # Errors
    /// A message for the user when the label is empty or another cabinet
    /// has it; the label is then left unchanged.
    pub fn set_label(&mut self, text: &str) -> Result<(), String> {
        let label = text.trim();
        if label.is_empty() {
            return Err("A label cannot be empty".into());
        }
        if self.cabinet_labeled(label) {
            return Err(format!("Another cabinet is already labeled {label}"));
        }
        if self.product().label != label {
            label.clone_into(&mut self.product_mut().label);
            self.modified = true;
        }
        Ok(())
    }

    /// The project's cabinets as the list shows them: label, kind and
    /// width, such as "B1 · Base · 30\"".
    #[must_use]
    pub fn cabinets(&self) -> Vec<String> {
        self.document
            .project
            .products
            .iter()
            .map(|p| {
                let kind = CabinetKind::of(&p.product_type).map_or_else(
                    || {
                        p.product_type
                            .split('@')
                            .next()
                            .unwrap_or_default()
                            .to_owned()
                    },
                    |k| k.name().to_owned(),
                );
                let width = p.width.show(self.units(), self.precision()).text;
                format!("{} · {kind} · {width}", p.label)
            })
            .collect()
    }

    /// Which cabinet is being edited, by its place in the list.
    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Edits another cabinet, by its place in the list; returns whether
    /// there is one there.
    pub fn select(&mut self, index: usize) -> bool {
        let exists = index < self.document.project.products.len();
        if exists {
            self.selected = index;
            self.cabinet_changed();
        }
        exists
    }

    /// Adds a base cabinet after the one being edited, with its standard
    /// and materials, and edits it.
    pub fn add(&mut self) {
        let source = self.product();
        let kind = CabinetKind::Base;
        let (height, depth) = kind.usual_height_and_depth();
        let product = Product {
            id: Id::new(),
            label: self.free_label(kind.label_prefix()),
            product_type: kind.product_type().into(),
            standard: source.standard,
            width: Length::from_inches(30),
            height,
            depth,
            quantity: None,
            parameters: BTreeMap::default(),
            materials: source.materials.clone(),
            front: Some(kind.usual_front()),
            overrides: Vec::new(),
            extensions: BTreeMap::default(),
            unknown: BTreeMap::default(),
        };
        self.insert(product);
    }

    /// Adds a copy of the cabinet being edited after it, with the next
    /// free label of its letters, and edits it. Copies have identities of
    /// their own.
    pub fn duplicate(&mut self) {
        let mut product = self.product().clone();
        let letters: String = product
            .label
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect();
        product.id = Id::new();
        product.label = self.free_label(&letters);
        if let Some(front) = &mut product.front {
            renew_ids(front);
        }
        self.insert(product);
    }

    /// Removes the cabinet being edited, with its placements in the
    /// rooms, and edits the one that takes its place in the list.
    ///
    /// # Errors
    /// A message for the user when it is the only cabinet: the window
    /// always has one to edit.
    pub fn delete(&mut self) -> Result<(), String> {
        let project = &mut self.document.project;
        if project.products.len() == 1 {
            return Err("A project keeps at least one cabinet".into());
        }
        let removed = project.products.remove(self.selected);
        for room in &mut project.rooms {
            room.placements.retain(|p| p.product != removed.id);
        }
        self.selected = self.selected.min(project.products.len() - 1);
        self.cabinet_changed();
        self.modified = true;
        Ok(())
    }

    fn insert(&mut self, product: Product) {
        self.selected += 1;
        self.document
            .project
            .products
            .insert(self.selected, product);
        self.cabinet_changed();
        self.modified = true;
    }

    /// `prefix` and the smallest number no cabinet uses with it:
    /// B1, B2… A cabinet without letters gets C1, C2…
    fn free_label(&self, prefix: &str) -> String {
        let prefix = if prefix.is_empty() { "C" } else { prefix };
        // With n cabinets, one of the first n + 1 numbers is free.
        (1..=self.document.project.products.len() + 1)
            .map(|n| format!("{prefix}{n}"))
            .find(|label| {
                !self
                    .document
                    .project
                    .products
                    .iter()
                    .any(|p| p.label == *label)
            })
            .expect("one of the first n + 1 numbers is free")
    }

    /// The kind of cabinet being edited, if the window offers it.
    #[must_use]
    pub fn kind(&self) -> Option<CabinetKind> {
        CabinetKind::of(&self.product().product_type)
    }

    /// Makes the cabinet another kind, with that kind's usual height, depth
    /// and front, and a label that starts with its letters. The width
    /// stays, except that a blind corner takes its usual width and blind
    /// part; the materials and the style of the fronts stay.
    pub fn set_kind(&mut self, kind: CabinetKind) {
        if self.kind() == Some(kind) {
            return;
        }
        let five_piece = self.five_piece();
        let number: String = self
            .label()
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .collect();
        let wanted = format!("{}{number}", kind.label_prefix());
        // Keep the number unless another cabinet has that label.
        let label = if number.is_empty() || self.cabinet_labeled(&wanted) {
            self.free_label(kind.label_prefix())
        } else {
            wanted
        };
        let product = self.product_mut();
        let (height, depth) = kind.usual_height_and_depth();
        product.product_type = kind.product_type().into();
        product.height = height;
        product.depth = depth;
        if let Some(width) = kind.usual_width() {
            product.width = width;
        }
        product.front = Some(kind.usual_front());
        product.label = label;
        self.section = 0;
        let product = self.product_mut();
        // Only a blind corner has a blind part, on the right at first.
        product.parameters.remove(BLIND_SIDE);
        product.parameters.remove(BLIND_WIDTH);
        if let Some(blind) = kind.usual_blind_width() {
            product.parameters.insert(BLIND_SIDE.into(), "right".into());
            product
                .parameters
                .insert(BLIND_WIDTH.into(), blind.nm().into());
        }
        // The new front keeps the style the fronts had.
        self.set_five_piece(five_piece);
        self.modified = true;
    }

    /// Which side of a blind corner is hidden; `None` for other kinds.
    #[must_use]
    pub fn blind_side(&self) -> Option<Side> {
        if self.kind().is_none_or(|k| k.usual_blind_width().is_none()) {
            return None;
        }
        let side = self.product().parameters.get(BLIND_SIDE)?;
        serde_json::from_value(side.clone()).ok()
    }

    /// Hides the other side of a blind corner. A single door's stored
    /// hinge side is cleared, so that it hinges away from the blind part.
    pub fn set_blind_side(&mut self, side: Side) {
        if self.blind_side().is_none() || self.blind_side() == Some(side) {
            return;
        }
        let product = self.product_mut();
        let value = match side {
            Side::Left => "left",
            Side::Right => "right",
        };
        product.parameters.insert(BLIND_SIDE.into(), value.into());
        if let Some(front) = &mut product.front {
            visit_mut(front, &mut |insert| {
                if insert.kind == InsertKind::Doors && insert.count.unwrap_or(1) == 1 {
                    insert.hinge_side = None;
                }
            });
        }
        self.modified = true;
    }

    /// Whether the cabinet's fronts are five-piece rather than slabs.
    #[must_use]
    pub fn five_piece(&self) -> bool {
        let mut five_piece = false;
        if let Some(front) = &self.product().front {
            visit(front, &mut |insert| {
                five_piece |= insert.style == Some(FrontStyle::FivePiece);
            });
        }
        five_piece
    }

    /// Makes every door and drawer front of the cabinet five-piece, or slab.
    pub fn set_five_piece(&mut self, five_piece: bool) {
        if self.five_piece() == five_piece {
            return;
        }
        let style = if five_piece {
            FrontStyle::FivePiece
        } else {
            FrontStyle::Slab
        };
        if let Some(front) = &mut self.product_mut().front {
            visit_mut(front, &mut |insert| {
                if matches!(
                    insert.kind,
                    InsertKind::Doors | InsertKind::Drawer | InsertKind::FalseFront
                ) {
                    insert.style = Some(style.clone());
                }
            });
        }
        self.modified = true;
    }

    /// The units lengths are typed and shown in.
    #[must_use]
    pub fn units(&self) -> Units {
        self.document.project.settings.units
    }

    /// Changes the units lengths are shown in, with that system's usual
    /// precision. Stored lengths never change, but the preference is saved
    /// with the project.
    pub fn set_units(&mut self, units: Units) {
        let settings = &mut self.document.project.settings;
        if settings.units != units {
            settings.units = units;
            settings.precision = usual_precision(units);
            self.modified = true;
        }
    }

    /// A dimension's stored value; `None` for a wall, opening or placement
    /// when none is being edited.
    #[must_use]
    pub fn get(&self, dimension: Dimension) -> Option<Length> {
        let product = self.product();
        match dimension {
            Dimension::Width => Some(product.width),
            Dimension::Height => Some(product.height),
            Dimension::Depth => Some(product.depth),
            Dimension::BlindWidth => {
                self.blind_side()?;
                product
                    .parameters
                    .get(BLIND_WIDTH)
                    .and_then(serde_json::Value::as_i64)
                    .map(Length::from_nm)
            }
            Dimension::SectionHeight => self.section_height(),
            d if d.is_library() => self.library_value(d),
            _ => self.room_value(dimension),
        }
    }

    /// Reads what the user typed into a dimension and stores it. An empty
    /// section height lets the section take the rest of the front.
    ///
    /// # Errors
    /// A message for the user when the text is not a length, is not
    /// positive (zero is fine for offsets and heights above the floor), or
    /// has nothing to apply to; the stored value is then left unchanged.
    pub fn edit(&mut self, dimension: Dimension, text: &str) -> Result<(), String> {
        if dimension == Dimension::SectionHeight && text.trim().is_empty() {
            return self.set_section_height(None);
        }
        let length = Length::parse(text, self.units()).map_err(|e| capitalize(&e.to_string()))?;
        if dimension.may_be_zero() {
            if length < Length::ZERO {
                return Err("Cannot be negative".into());
            }
        } else if length <= Length::ZERO {
            return Err("Must be greater than zero".into());
        }
        if dimension.is_library() {
            return self.set_library_value(dimension, length);
        }
        if dimension == Dimension::BlindWidth {
            return self.set_blind_width(length);
        }
        if dimension == Dimension::SectionHeight {
            return self.set_section_height(Some(length));
        }
        let product = self.product_mut();
        let field = match dimension {
            Dimension::Width => &mut product.width,
            Dimension::Height => &mut product.height,
            Dimension::Depth => &mut product.depth,
            _ => return self.set_room_value(dimension, length),
        };
        if *field != length {
            *field = length;
            self.modified = true;
        }
        Ok(())
    }

    /// Stores a blind corner's blind width, which must leave room for the
    /// doors.
    fn set_blind_width(&mut self, length: Length) -> Result<(), String> {
        let Some(old) = self.get(Dimension::BlindWidth) else {
            return Err("Only a blind corner has a blind width".into());
        };
        if length >= self.product().width {
            return Err("Must leave room for the doors".into());
        }
        if old != length {
            let product = self.product_mut();
            product
                .parameters
                .insert(BLIND_WIDTH.into(), length.nm().into());
            self.modified = true;
        }
        Ok(())
    }

    /// A dimension as text to put in its field, in the current units;
    /// empty when there is nothing to edit.
    #[must_use]
    pub fn text(&self, dimension: Dimension) -> String {
        self.get(dimension)
            .map(|l| {
                if dimension.is_library() {
                    self.exact_text(l)
                } else {
                    self.show(l)
                }
            })
            .unwrap_or_default()
    }

    /// How a dimension was read, for the line under its field: `= 30-1/2"`,
    /// or `≈ 30-1/2"` when the display precision rounds it.
    #[must_use]
    pub fn reading(&self, dimension: Dimension) -> String {
        let Some(length) = self.get(dimension) else {
            return if dimension == Dimension::SectionHeight {
                self.section_rest_reading()
            } else {
                String::new()
            };
        };
        let shown = length.show(self.units(), self.precision());
        let sign = if shown.exact { '=' } else { '≈' };
        format!("{sign} {}", shown.text)
    }

    /// A length in the current units and precision.
    fn show(&self, length: Length) -> String {
        length.show(self.units(), self.precision()).text
    }

    /// The project's cut list, in the current units.
    #[must_use]
    pub fn cut_list(&self) -> CutList {
        let document = &self.document;
        cut_list(
            &document.project,
            &document.library,
            self.units(),
            self.precision(),
        )
    }

    /// What to buy for the project, in the current units.
    #[must_use]
    pub fn bill_of_materials(&self) -> Bom {
        let document = &self.document;
        bill_of_materials(
            &document.project,
            &document.library,
            self.units(),
            self.precision(),
        )
    }

    /// Writes the bill of materials as CSV, with `separator` between
    /// fields.
    ///
    /// # Errors
    /// A message for the user when the file cannot be written.
    pub fn export_materials(&self, path: &Path, separator: Separator) -> Result<(), String> {
        std::fs::write(path, materials_to_csv(&self.bill_of_materials(), separator))
            .map_err(|e| format!("Cannot export the materials: {e}"))
    }

    /// Writes the cut list as CSV, with `separator` between fields.
    ///
    /// # Errors
    /// A message for the user when the file cannot be written.
    pub fn export_cut_list(&self, path: &Path, separator: Separator) -> Result<(), String> {
        std::fs::write(path, to_csv(&self.cut_list(), separator))
            .map_err(|e| format!("Cannot export the cut list: {e}"))
    }

    /// Writes the cut list as a PDF to print, on the paper usual for the
    /// project's units.
    ///
    /// # Errors
    /// A message for the user when the PDF cannot be made or written.
    pub fn export_cut_list_pdf(&self, path: &Path, fonts: &Fonts) -> Result<(), String> {
        let title = &self.document.project.name;
        let paper = Paper::for_units(self.units());
        let pdf = cut_list_pdf(
            &self.cut_list(),
            &self.bill_of_materials(),
            title,
            NAME_AND_VERSION,
            paper,
            fonts,
        )?;
        std::fs::write(path, pdf).map_err(|e| format!("Cannot export the cut list: {e}"))
    }

    /// Writes a DXF of every part, and one with all of them, into
    /// `folder`, replacing files of the same name; gives how many parts.
    ///
    /// # Errors
    /// A message for the user when a file cannot be written.
    pub fn export_parts_dxf(&self, folder: &Path) -> Result<usize, String> {
        let document = &self.document;
        let files = parts_dxf(&document.project, &document.library)?;
        let fail = |e: std::io::Error| format!("Cannot export the parts as DXF: {e}");
        std::fs::create_dir_all(folder).map_err(fail)?;
        for file in &files {
            std::fs::write(folder.join(&file.name), &file.contents).map_err(fail)?;
        }
        Ok(files.len() - 1)
    }

    /// Writes the elevations of every wall as a PDF, one wall to a page, on
    /// the paper usual for the project's units.
    ///
    /// # Errors
    /// A message for the user when there are no walls, or the PDF cannot
    /// be made or written.
    pub fn export_elevations_pdf(&self, path: &Path, fonts: &Fonts) -> Result<(), String> {
        let (project, library) = (&self.document.project, &self.document.library);
        let show = |length: Length| self.show(length);
        let paper = Paper::for_units(self.units());
        let pdf = elevations_pdf(
            project,
            library,
            self.units(),
            &show,
            &project.name,
            NAME_AND_VERSION,
            paper,
            fonts,
        )?;
        std::fs::write(path, pdf).map_err(|e| format!("Cannot export the elevations: {e}"))
    }

    /// Writes a DXF of every wall's elevation into `folder`, replacing
    /// files of the same name; gives how many walls.
    ///
    /// # Errors
    /// A message for the user when there are no walls, or a file cannot
    /// be written.
    pub fn export_elevations_dxf(&self, folder: &Path) -> Result<usize, String> {
        let show = |length: Length| self.show(length);
        let files = elevations_dxf(&self.document.project, &self.document.library, &show)?;
        let fail = |e: std::io::Error| format!("Cannot export the elevations: {e}");
        std::fs::create_dir_all(folder).map_err(fail)?;
        for file in &files {
            std::fs::write(folder.join(&file.name), &file.contents).map_err(fail)?;
        }
        Ok(files.len())
    }

    /// What the drawing area shows: the edited room's plan or wall while
    /// the walls are shown, otherwise the wall the edited cabinet is
    /// placed on, or the cabinet alone when it is not placed on a wall.
    #[must_use]
    pub fn drawing(&self) -> Drawing {
        if self.view() != WallView::Elevation
            && let Some(plan) = self.plan_drawing()
        {
            return plan;
        }
        self.elevation_drawing()
    }

    /// The drawing beside the first one when the plan and the elevation
    /// are shown together: the elevation.
    #[must_use]
    pub fn second_drawing(&self) -> Option<Drawing> {
        (self.view() == WallView::PlanAndElevation).then(|| self.elevation_drawing())
    }

    fn plan_drawing(&self) -> Option<Drawing> {
        let (room, wall) = self.plan_selection()?;
        let plan = plan(&self.document.project, room)?;
        let show = |length: Length| self.show(length);
        Some(drawing::plan(&plan, wall, self.product().id, &show))
    }

    /// The edited wall's elevation, or the cabinet alone.
    fn elevation_drawing(&self) -> Drawing {
        let (project, library) = (&self.document.project, &self.document.library);
        let product = self.product();
        let show = |length: Length| self.show(length);
        if let Some(elevation) = self
            .shown_wall()
            .and_then(|w| elevation(project, library, w))
        {
            return drawing::wall(&elevation, product.id, &show);
        }
        let view = front_view(product, library).ok();
        drawing::cabinet(&product.label, product.width, product.height, view.as_ref())
    }

    /// What is wrong with where the cabinets are placed on the walls.
    #[must_use]
    pub fn placement_problems(&self) -> Vec<String> {
        placement_problems(&self.document.project)
    }

    fn product(&self) -> &Product {
        &self.document.project.products[self.selected]
    }

    fn product_mut(&mut self) -> &mut Product {
        &mut self.document.project.products[self.selected]
    }

    /// Whether a cabinet other than the one being edited has `label`.
    fn cabinet_labeled(&self, label: &str) -> bool {
        let selected = self.selected;
        self.document
            .project
            .products
            .iter()
            .enumerate()
            .any(|(i, p)| i != selected && p.label == label)
    }

    /// The project's display precision, or the units' usual one when the
    /// file's is not usable.
    fn precision(&self) -> Length {
        let precision = self.document.project.settings.precision;
        if precision > Length::ZERO {
            precision
        } else {
            usual_precision(self.units())
        }
    }
}

/// Calls `f` with every insert of a front tree.
fn visit(node: &FrontNode, f: &mut impl FnMut(&Insert)) {
    if let Some(insert) = &node.insert {
        f(insert);
    }
    for child in &node.children {
        visit(child, f);
    }
}

/// Gives every node of a front tree a new identity, for a copy.
fn renew_ids(node: &mut FrontNode) {
    node.id = Id::new();
    for child in &mut node.children {
        renew_ids(child);
    }
}

/// Calls `f` with every insert of a front tree, to change it.
fn visit_mut(node: &mut FrontNode, f: &mut impl FnMut(&mut Insert)) {
    if let Some(insert) = &mut node.insert {
        f(insert);
    }
    for child in &mut node.children {
        visit_mut(child, f);
    }
}

/// 1/32" or 0.1 mm, the usual shop precisions.
fn usual_precision(units: Units) -> Length {
    match units {
        Units::Imperial => Length::from_inch_fraction(1, 32).expect("a valid fraction"),
        Units::Metric => Length::from_nm(100_000),
    }
}

/// A project with the materials, edge band and standards a base cabinet
/// needs, and one base cabinet built by `method`: a drawer over a pair of
/// doors.
fn new_document(units: Units, method: Method) -> Document {
    let library = starter_library();
    let standard = library
        .standards
        .iter()
        .find(|s| s.method == method)
        .expect("the starter library has both standards")
        .id;
    let product = base_cabinet(&library, standard);
    let project = Project {
        id: Id::new(),
        name: "Untitled".into(),
        notes: String::new(),
        settings: Settings {
            units,
            precision: usual_precision(units),
            default_standard: standard,
            unknown: BTreeMap::default(),
        },
        rooms: Vec::new(),
        products: vec![product],
        revisions: Vec::new(),
        extensions: BTreeMap::default(),
        unknown: BTreeMap::default(),
    };
    let mut document = Document::new(project, library);
    document.generator = NAME_AND_VERSION.into();
    document
}

fn inches(numerator: i64, denominator: i64) -> Length {
    Length::from_inch_fraction(numerator, denominator).expect("a valid fraction")
}

/// 3/4" plywood for the case and fronts, 1/4" plywood for the back and
/// drawer bottoms, 1/2" plywood for drawer boxes, solid wood for face frames,
/// a 0.5 mm edge band, a cup hinge and a frameless and a face frame standard,
/// in that order.
fn starter_library() -> Library {
    let hinge = standard::new_hinge();
    let solid_wood = SheetMaterial {
        length: Length::from_inches(96),
        width: inches(11, 2),
        ..materials::new_sheet("Solid wood 3/4\"", inches(3, 4), inches(3, 4))
    };
    Library {
        materials: vec![
            materials::new_sheet("Plywood 3/4\"", inches(3, 4), Length::from_mm(18)),
            materials::new_sheet("Plywood 1/4\"", inches(1, 4), Length::from_mm(6)),
            materials::new_sheet("Plywood 1/2\"", inches(1, 2), Length::from_mm(12)),
            solid_wood,
        ],
        edge_bands: vec![materials::new_band("Edge band 0.5 mm")],
        standards: vec![
            new_standard("Frameless", Method::Frameless, hinge.id),
            new_standard("Face frame", Method::FaceFrame, hinge.id),
        ],
        hardware: vec![hinge],
        unknown: BTreeMap::default(),
    }
}

/// A standard built by `method` with the usual rules and `hinge` for its doors.
fn new_standard(name: &str, method: Method, hinge: Id) -> Standard {
    Standard {
        id: Id::new(),
        version: 1,
        name: name.into(),
        derived_from: None,
        method,
        rules: serde_json::from_value(standard::usual_rules(method, hinge))
            .expect("rules are a JSON object"),
        unknown: BTreeMap::default(),
    }
}

/// A 30" × 34-1/2" × 24" base cabinet from the starter library.
fn base_cabinet(library: &Library, standard: Id) -> Product {
    let (plywood, thin, box_sides, solid, band) = (
        library.materials[0].id,
        library.materials[1].id,
        library.materials[2].id,
        library.materials[3].id,
        library.edge_bands[0].id,
    );
    usual_cabinet(
        standard,
        [
            ("case", plywood),
            ("back", thin),
            ("doors", plywood),
            ("drawerBox", box_sides),
            ("drawerBottom", thin),
            ("frontPanels", thin),
            ("faceFrame", solid),
            ("edgeBandCase", band),
            ("edgeBandFronts", band),
        ]
        .into_iter()
        .map(|(role, id)| (role.to_owned(), id))
        .collect(),
    )
}

/// A new project's 30" × 34-1/2" × 24" base cabinet, with `materials`.
fn usual_cabinet(standard: Id, materials: BTreeMap<String, Id>) -> Product {
    let (height, depth) = CabinetKind::Base.usual_height_and_depth();
    Product {
        id: Id::new(),
        label: "B1".into(),
        product_type: CabinetKind::Base.product_type().into(),
        standard,
        width: Length::from_inches(30),
        height,
        depth,
        quantity: None,
        parameters: BTreeMap::default(),
        materials,
        front: Some(CabinetKind::Base.usual_front()),
        overrides: Vec::new(),
        extensions: BTreeMap::default(),
        unknown: BTreeMap::default(),
    }
}

/// The parameters of a blind corner (RFC-0001 §6).
const BLIND_SIDE: &str = "blindSide";
const BLIND_WIDTH: &str = "blindWidth";

/// The kinds of cabinet the window offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CabinetKind {
    /// Stands on the floor, under the counter.
    Base,
    /// Hangs on the wall.
    Wall,
    /// Floor to near the ceiling: pantries and oven towers.
    Tall,
    /// A base cabinet in a corner, partly hidden by the cabinet on the
    /// adjoining wall.
    BlindBase,
    /// A wall cabinet in a corner, partly hidden by the cabinet on the
    /// adjoining wall.
    BlindWall,
}

impl CabinetKind {
    /// Every kind, in the order the window lists them.
    pub const ALL: [Self; 5] = [
        Self::Base,
        Self::Wall,
        Self::Tall,
        Self::BlindBase,
        Self::BlindWall,
    ];

    /// The name the window shows.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Base => "Base",
            Self::Wall => "Wall",
            Self::Tall => "Tall",
            Self::BlindBase => "Blind corner base",
            Self::BlindWall => "Blind corner wall",
        }
    }

    fn product_type(self) -> &'static str {
        match self {
            Self::Base => "base-cabinet@1",
            Self::Wall => "wall-cabinet@1",
            Self::Tall => "tall-cabinet@1",
            Self::BlindBase => "blind-corner-base@1",
            Self::BlindWall => "blind-corner-wall@1",
        }
    }

    /// Whether it hangs on the wall rather than standing on the floor.
    pub(crate) fn hangs(self) -> bool {
        matches!(self, Self::Wall | Self::BlindWall)
    }

    /// How much of a blind corner's front is hidden at first: as much as
    /// the adjoining cabinet is deep, 24" for a base and 12" for a wall
    /// cabinet. Other kinds have none.
    fn usual_blind_width(self) -> Option<Length> {
        match self {
            Self::BlindBase => Some(Length::from_inches(24)),
            Self::BlindWall => Some(Length::from_inches(12)),
            Self::Base | Self::Wall | Self::Tall => None,
        }
    }

    /// The width a blind corner starts at, 42" or 36", so its door is a
    /// usual 18" or 24"; other kinds keep the width they had.
    fn usual_width(self) -> Option<Length> {
        match self {
            Self::BlindBase => Some(Length::from_inches(42)),
            Self::BlindWall => Some(Length::from_inches(36)),
            Self::Base | Self::Wall | Self::Tall => None,
        }
    }

    fn of(product_type: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.product_type() == product_type)
    }

    /// The letters of the shop's usual labels: B30, W30, T24, BC42,
    /// WBC36.
    fn label_prefix(self) -> &'static str {
        match self {
            Self::Base => "B",
            Self::Wall => "W",
            Self::Tall => "T",
            Self::BlindBase => "BC",
            Self::BlindWall => "WBC",
        }
    }

    /// 34-1/2" × 24" for a base cabinet, 30" × 12" for a wall cabinet,
    /// 84" × 24" for a tall one; blind corners as base and wall cabinets.
    fn usual_height_and_depth(self) -> (Length, Length) {
        match self {
            Self::Base | Self::BlindBase => (inches(69, 2), Length::from_inches(24)),
            Self::Wall | Self::BlindWall => (Length::from_inches(30), Length::from_inches(12)),
            Self::Tall => (Length::from_inches(84), Length::from_inches(24)),
        }
    }

    /// A drawer over a pair of doors for a base cabinet; a pair of doors
    /// with two shelves for a wall cabinet, or four for a tall one. A blind
    /// corner has one door, which hinges away from the blind part, with a
    /// shelf behind it, or two in a wall cabinet.
    fn usual_front(self) -> FrontNode {
        let leaf = |size, kind, count, shelves| FrontNode {
            id: Id::new(),
            size,
            split: None,
            children: Vec::new(),
            insert: Some(Insert {
                kind,
                style: Some(FrontStyle::Slab),
                count,
                hinge_side: None,
                shelves,
                adjustments: Vec::new(),
                unknown: BTreeMap::default(),
            }),
            unknown: BTreeMap::default(),
        };
        match self {
            Self::Base => FrontNode {
                id: Id::new(),
                size: None,
                split: Some(SplitDirection::Horizontal),
                children: vec![
                    leaf(
                        Some(Size::Length(Length::from_inches(6))),
                        InsertKind::Drawer,
                        None,
                        None,
                    ),
                    leaf(Some(Size::Share(1)), InsertKind::Doors, Some(2), None),
                ],
                insert: None,
                unknown: BTreeMap::default(),
            },
            Self::Wall => leaf(None, InsertKind::Doors, Some(2), Some(2)),
            Self::Tall => leaf(None, InsertKind::Doors, Some(2), Some(4)),
            Self::BlindBase => leaf(None, InsertKind::Doors, Some(1), Some(1)),
            Self::BlindWall => leaf(None, InsertKind::Doors, Some(1), Some(2)),
        }
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("opencabinet-app-{}-{test}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_new_project_is_valid_and_unmodified() {
        let designer = Designer::default();
        assert_eq!(designer.issues(), []);
        assert!(!designer.modified());
        assert_eq!(designer.title(), "Untitled — OpenCabinet Designer");
        assert_eq!(designer.label(), "B1");
    }

    #[test]
    fn the_cut_list_follows_edits_and_units() {
        let mut designer = Designer::default();
        let list = designer.cut_list();
        assert_eq!(list.problems, Vec::<String>::new());
        let names: Vec<&str> = list.rows.iter().map(|r| r.part.as_str()).collect();
        assert_eq!(
            names,
            [
                "Left side",
                "Right side",
                "Bottom",
                "Front stretcher",
                "Back stretcher",
                "Back",
                "Drawer front",
                "Drawer box, left side",
                "Drawer box, right side",
                "Drawer box, front",
                "Drawer box, back",
                "Drawer box, bottom",
                "Door 1",
                "Door 2",
            ]
        );
        // The bottom spans the inside: 30" less two 18 mm sides.
        assert_eq!(list.rows[2].length, "28-19/32\"");
        assert_eq!(list.rows[2].edges, "1 edge");

        designer.edit(Dimension::Width, "36").unwrap();
        designer.set_units(Units::Metric);
        let rows = designer.cut_list().rows;
        assert_eq!(
            (rows[2].length.as_str(), rows[2].cut_length.as_str()),
            ("878.4 mm", "878.4 mm")
        );
        assert_eq!(rows[12].edges, "4 edges");
    }

    #[test]
    fn choosing_a_kind_gives_its_usual_sizes_and_front() {
        let mut designer = Designer::default();
        designer.set_kind(CabinetKind::Wall);
        assert_eq!(designer.label(), "W1");
        assert_eq!(designer.reading(Dimension::Height), "= 30\"");
        assert_eq!(designer.reading(Dimension::Depth), "= 12\"");
        let list = designer.cut_list();
        assert_eq!(list.problems, Vec::<String>::new());
        let names: Vec<&str> = list.rows.iter().map(|r| r.part.as_str()).collect();
        assert!(
            names.contains(&"Top") && names.contains(&"Shelf 2"),
            "{names:?}"
        );

        designer.set_kind(CabinetKind::Tall);
        assert_eq!(designer.label(), "T1");
        assert_eq!(designer.reading(Dimension::Height), "= 84\"");
        assert_eq!(
            designer
                .cut_list()
                .rows
                .iter()
                .filter(|r| r.part.starts_with("Shelf"))
                .count(),
            4
        );
        assert!(designer.modified());
    }

    #[test]
    fn a_blind_corner_starts_with_its_usual_width_and_blind_part() {
        let mut designer = Designer::default();
        assert_eq!(designer.blind_side(), None);
        assert_eq!(designer.text(Dimension::BlindWidth), "");

        designer.set_kind(CabinetKind::BlindBase);
        assert_eq!(designer.label(), "BC1");
        assert_eq!(designer.reading(Dimension::Width), "= 42\"");
        assert_eq!(designer.blind_side(), Some(Side::Right));
        assert_eq!(designer.reading(Dimension::BlindWidth), "= 24\"");
        let list = designer.cut_list();
        assert_eq!(list.problems, Vec::<String>::new());
        let fronts: Vec<&str> = list
            .rows
            .iter()
            .map(|r| r.part.as_str())
            .filter(|p| p.starts_with("Blind") || p.starts_with("Door") || p.starts_with("Shelf"))
            .collect();
        assert_eq!(fronts, ["Blind panel", "Door 1", "Shelf 1"]);

        designer.set_kind(CabinetKind::BlindWall);
        assert_eq!(designer.label(), "WBC1");
        assert_eq!(designer.reading(Dimension::Width), "= 36\"");
        assert_eq!(designer.reading(Dimension::BlindWidth), "= 12\"");
        assert_eq!(designer.cut_list().problems, Vec::<String>::new());

        // Another kind keeps the width and drops the blind part.
        designer.set_kind(CabinetKind::Base);
        assert_eq!(designer.reading(Dimension::Width), "= 36\"");
        assert!(designer.product().parameters.is_empty());
        assert_eq!(designer.blind_side(), None);
    }

    #[test]
    fn the_blind_width_must_leave_room_for_the_doors() {
        let mut designer = Designer::default();
        assert_eq!(
            designer.edit(Dimension::BlindWidth, "12"),
            Err("Only a blind corner has a blind width".into())
        );
        designer.set_kind(CabinetKind::BlindBase);
        assert_eq!(
            designer.edit(Dimension::BlindWidth, "42"),
            Err("Must leave room for the doors".into())
        );
        assert_eq!(
            designer.edit(Dimension::BlindWidth, "0"),
            Err("Must be greater than zero".into())
        );
        designer.edit(Dimension::BlindWidth, "27").unwrap();
        assert_eq!(designer.reading(Dimension::BlindWidth), "= 27\"");
        // A door 42" − 27" = 15" wide, less the 1/8" gap.
        let list = designer.cut_list();
        let door = list.rows.iter().find(|r| r.part == "Door 1").unwrap();
        assert_eq!(door.width, "14-7/8\"");
    }

    #[test]
    fn hiding_the_other_side_rehangs_the_door() {
        // The sample's BC42 is blind on the right, its door hinged on the
        // left.
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kitchen.ocab");
        let mut designer = Designer::open(&fixture).unwrap();
        designer.select(2);
        assert_eq!(designer.kind(), Some(CabinetKind::BlindBase));
        assert_eq!(designer.blind_side(), Some(Side::Right));
        let hinge = |designer: &Designer| {
            designer
                .product()
                .front
                .as_ref()
                .unwrap()
                .insert
                .as_ref()
                .unwrap()
                .hinge_side
        };
        assert_eq!(hinge(&designer), Some(Side::Left));

        designer.set_blind_side(Side::Left);
        assert_eq!(designer.blind_side(), Some(Side::Left));
        assert_eq!(hinge(&designer), None);
        assert!(designer.modified());
        let view = designer.drawing();
        assert!(!view.marks.is_empty(), "the door's hinge marks are drawn");
    }

    #[test]
    fn five_piece_fronts_replace_slab_doors_and_drawer_fronts() {
        let mut designer = Designer::default();
        assert!(!designer.five_piece());
        designer.set_five_piece(true);
        assert!(designer.five_piece());
        let list = designer.cut_list();
        assert_eq!(list.problems, Vec::<String>::new());
        let names: Vec<&str> = list.rows.iter().map(|r| r.part.as_str()).collect();
        for name in [
            "Drawer front, top rail",
            "Door 1, left stile",
            "Door 2, panel",
        ] {
            assert!(names.contains(&name), "{name} in {names:?}");
        }
        assert!(!names.contains(&"Door 1"));
        designer.set_five_piece(false);
        assert!(designer.cut_list().rows.iter().any(|r| r.part == "Door 1"));
    }

    #[test]
    fn edits_store_exact_values_and_keep_the_last_good_one() {
        let mut designer = Designer::default();
        designer.edit(Dimension::Width, "30 1/2").unwrap();
        assert_eq!(
            designer.get(Dimension::Width),
            Some(Length::from_nm(774_700_000))
        );
        assert_eq!(designer.reading(Dimension::Width), "= 30-1/2\"");
        assert!(designer.modified());

        assert_eq!(
            designer.edit(Dimension::Width, "thirty"),
            Err("Not a length".into())
        );
        assert_eq!(
            designer.edit(Dimension::Width, "0"),
            Err("Must be greater than zero".into())
        );
        assert_eq!(
            designer.get(Dimension::Width),
            Some(Length::from_nm(774_700_000))
        );
    }

    #[test]
    fn switching_units_changes_only_the_display() {
        let mut designer = Designer::default();
        designer.set_units(Units::Metric);
        assert_eq!(designer.text(Dimension::Width), "762 mm");
        assert_eq!(designer.reading(Dimension::Height), "= 876.3 mm");
        designer.edit(Dimension::Depth, "600").unwrap();
        designer.set_units(Units::Imperial);
        // 600 mm is 23.622…": shown rounded, but stored exactly.
        assert_eq!(designer.reading(Dimension::Depth), "≈ 23-5/8\"");
        assert_eq!(designer.get(Dimension::Depth), Some(Length::from_mm(600)));
    }

    #[test]
    fn a_saved_project_reopens_with_its_edits_and_units() {
        let path = scratch("save").join("cabinet.ocab");
        let mut designer = Designer::default();
        designer.edit(Dimension::Width, "33").unwrap();
        designer.set_units(Units::Metric);
        designer.save_as(&path).unwrap();
        assert!(!designer.modified());
        assert_eq!(designer.title(), "cabinet.ocab — OpenCabinet Designer");

        let reopened = Designer::open(&path).unwrap();
        assert_eq!(
            reopened.get(Dimension::Width),
            Some(Length::from_inches(33))
        );
        assert_eq!(reopened.units(), Units::Metric);
        assert_eq!(reopened, designer);
    }

    #[test]
    fn the_kitchen_users_get_opens_without_a_problem() {
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-kitchen.ocab");
        let designer = Designer::open(&fixture).unwrap();
        assert_eq!(designer.issues().len(), 0);
        let problems =
            opencabinet_eval::evaluate(&designer.document.project, &designer.document.library)
                .problems;
        assert_eq!(problems, []);
    }

    #[test]
    fn opening_something_that_is_not_a_project_explains_why() {
        let path = scratch("open").join("notes.ocab");
        std::fs::write(&path, "not a project").unwrap();
        let error = Designer::open(&path).unwrap_err();
        assert!(error.starts_with("Cannot open the file:"), "{error}");
    }

    #[test]
    fn cabinets_are_added_copied_and_removed() {
        let mut designer = Designer::default();
        designer.add();
        assert_eq!(designer.selected(), 1);
        assert_eq!(designer.label(), "B2");
        designer.edit(Dimension::Width, "18").unwrap();
        designer.duplicate();
        assert_eq!(designer.label(), "B3");
        assert_eq!(
            designer.cabinets(),
            ["B1 · Base · 30\"", "B2 · Base · 18\"", "B3 · Base · 18\""]
        );
        // A copy has identities of its own, so the project stays valid, and
        // the cut list covers every cabinet.
        assert_eq!(designer.issues(), []);
        let products: Vec<String> = designer
            .cut_list()
            .rows
            .iter()
            .map(|r| r.product.clone())
            .filter(|p| p != "B1")
            .collect();
        assert_eq!(products.len(), 28);

        designer.delete().unwrap();
        assert_eq!((designer.selected(), designer.label()), (1, "B2"));
        designer.select(0);
        designer.delete().unwrap();
        assert_eq!(designer.label(), "B2");
        assert_eq!(
            designer.delete(),
            Err("A project keeps at least one cabinet".into())
        );
        assert!(!designer.select(1));
        assert!(designer.modified());
    }

    #[test]
    fn labels_are_unique_and_not_empty() {
        let mut designer = Designer::default();
        designer.add();
        assert_eq!(
            designer.set_label("B1"),
            Err("Another cabinet is already labeled B1".into())
        );
        assert_eq!(
            designer.set_label("  "),
            Err("A label cannot be empty".into())
        );
        designer.set_label(" Sink ").unwrap();
        assert_eq!(designer.label(), "Sink");
        // Copies of a cabinet without a number count from 1.
        designer.duplicate();
        assert_eq!(designer.label(), "Sink1");

        // A new kind keeps the number unless another cabinet has it.
        designer.set_label("W1").unwrap();
        designer.select(0);
        designer.set_kind(CabinetKind::Wall);
        assert_eq!(designer.label(), "W2");
    }

    #[test]
    fn the_drawing_shows_the_wall_of_the_cabinet_being_edited() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kitchen.ocab");
        let mut designer = Designer::open(&fixture).unwrap();
        let drawing = designer.drawing();
        assert_eq!(drawing.title, "Elevation of wall North");
        let labels: Vec<&str> = drawing.labels.iter().map(|l| l.text.as_str()).collect();
        // Every cabinet on the wall, then its dimensions in the project's
        // units: the B30s' widths and the whole wall.
        assert_eq!(&labels[..6], ["B30", "B30", "SB36", "F3", "BC42", "W30"]);
        assert_eq!(labels.iter().filter(|&&l| l == "30\"").count(), 4);
        assert!(labels.contains(&"144\""), "{labels:?}");
        assert!(!drawing.highlight.is_empty() && !drawing.marks.is_empty());
        // The BC42 stops short of the corner, with nothing in front of its
        // blind part.
        assert_eq!(
            designer.placement_problems(),
            ["BC42's blind panel shows: no cabinet stands in front of all of it"]
        );

        // The island stands free, not on a wall: it is drawn alone.
        designer.select(4);
        assert_eq!(designer.drawing().title, "IS48, not placed on a wall");
    }

    #[test]
    fn the_walls_show_in_plan_in_elevation_or_both() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kitchen.ocab");
        let mut designer = Designer::open(&fixture).unwrap();
        designer.choose_view(WallView::PlanAndElevation);
        // Not while the cabinet is shown.
        assert_eq!(designer.view(), WallView::Elevation);
        assert!(designer.second_drawing().is_none());

        designer.show_walls(true);
        assert_eq!(designer.view(), WallView::PlanAndElevation);
        assert_eq!(designer.drawing().title, "Plan of Kitchen");
        assert_eq!(
            designer.second_drawing().unwrap().title,
            "Elevation of wall North"
        );
        designer.choose_view(WallView::Plan);
        assert_eq!(designer.drawing().title, "Plan of Kitchen");
        assert!(designer.second_drawing().is_none());
        for index in [0, 1, 2] {
            assert_eq!(WallView::from_index(index).index(), index);
        }
        assert_eq!(WallView::from_index(7), WallView::Elevation);
    }

    #[test]
    fn removing_a_cabinet_removes_its_placements() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kitchen.ocab");
        let mut designer = Designer::open(&fixture).unwrap();
        let placed = |d: &Designer| d.document.project.rooms[0].placements.len();
        assert_eq!(placed(&designer), 7);
        // B30 is placed twice.
        designer.delete().unwrap();
        assert_eq!(placed(&designer), 5);
        assert_eq!(designer.label(), "SB36");
    }
}
