// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! OpenCabinet Designer's window, as a library so tests can drive it
//! without a window system (see `tests/window.rs`).

pub mod designer;
pub mod dialogs;
pub mod drawing;
pub mod fonts;
pub mod perf;

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use opencabinet_model::{Side, Units};
use slint::{
    CloseRequestResponse, ComponentHandle, ModelRc, SharedString, StandardListViewItem, VecModel,
};

use crate::designer::{CabinetKind, Designer, Dimension, OpeningKind};
use crate::dialogs::{Dialogs, FileKind, SystemDialogs};
use opencabinet_reports::Separator;

/// The code Slint generates from `ui/app.slint`, which follows Slint's
/// conventions rather than this workspace's lints.
#[allow(missing_docs, clippy::pedantic)]
mod ui {
    slint::include_modules!();
}

pub use ui::AppWindow;

/// The name and version shown to users and written into saved files.
pub const NAME_AND_VERSION: &str = concat!("OpenCabinet Designer ", env!("CARGO_PKG_VERSION"));

/// Creates the window, asking the user through the system's dialogs.
///
/// # Errors
/// When the platform cannot create a window.
pub fn create() -> Result<AppWindow, slint::PlatformError> {
    create_with(Rc::new(SystemDialogs))
}

/// Creates the window and connects it to a new [`Designer`], asking the
/// user through `dialogs`.
///
/// # Errors
/// When the platform cannot create a window.
pub fn create_with(dialogs: Rc<dyn Dialogs>) -> Result<AppWindow, slint::PlatformError> {
    let window = AppWindow::new()?;
    let app = Rc::new(App {
        window: window.as_weak(),
        designer: RefCell::new(Designer::default()),
        dialogs,
    });
    app.show(&window, true);

    for dimension in Dimension::ALL {
        let app = Rc::clone(&app);
        on_edited(&window, dimension, move |text| app.edit(dimension, &text));
    }
    connect_room(&window, &app);
    connect_front(&window, &app);
    let handle = Rc::clone(&app);
    window.on_units_chosen(move |metric| handle.choose_units(metric));
    let handle = Rc::clone(&app);
    window.on_cabinet_chosen(move |index| handle.choose_cabinet(index));
    let handle = Rc::clone(&app);
    window.on_add_cabinet(move || {
        handle.change_cabinets(|d| {
            d.add();
            Ok(())
        });
    });
    let handle = Rc::clone(&app);
    window.on_duplicate_cabinet(move || {
        handle.change_cabinets(|d| {
            d.duplicate();
            Ok(())
        });
    });
    let handle = Rc::clone(&app);
    window.on_delete_cabinet(move || handle.change_cabinets(Designer::delete));
    let handle = Rc::clone(&app);
    window.on_label_edited(move |text| handle.edit_label(&text));
    window.set_kinds(ModelRc::new(VecModel::from(
        CabinetKind::ALL
            .map(|k| slint::SharedString::from(k.name()))
            .to_vec(),
    )));
    let handle = Rc::clone(&app);
    window.on_kind_chosen(move |index| handle.choose_kind(index));
    let handle = Rc::clone(&app);
    window.on_style_chosen(move |five_piece| handle.choose_style(five_piece));
    let handle = Rc::clone(&app);
    window.on_blind_side_chosen(move |index| handle.choose_blind_side(index));
    let handle = Rc::clone(&app);
    window.on_new_project(move || handle.new_project());
    let handle = Rc::clone(&app);
    window.on_open_project(move || handle.open());
    let handle = Rc::clone(&app);
    window.on_save_project(move || {
        handle.save(false);
    });
    let handle = Rc::clone(&app);
    window.on_save_project_as(move || {
        handle.save(true);
    });
    connect_exports(&window, &app);
    let handle = Rc::clone(&app);
    window.on_about(move || {
        handle.status(&format!(
            "{NAME_AND_VERSION} — free software under the GNU GPL, version 3 or later"
        ));
    });
    let handle = Rc::clone(&app);
    window.on_quit(move || {
        if handle.may_discard() {
            // Fails only when no event loop runs, as in tests: nothing to stop.
            slint::quit_event_loop().ok();
        }
    });
    let handle = Rc::clone(&app);
    window.window().on_close_requested(move || {
        if handle.may_discard() {
            CloseRequestResponse::HideWindow
        } else {
            CloseRequestResponse::KeepWindowShown
        }
    });
    Ok(window)
}

/// The window's state and what it does with it.
struct App {
    window: slint::Weak<AppWindow>,
    designer: RefCell<Designer>,
    dialogs: Rc<dyn Dialogs>,
}

impl App {
    fn edit(&self, dimension: Dimension, text: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let result = self.designer.borrow_mut().edit(dimension, text);
        set_error(&window, dimension, result.err().unwrap_or_default());
        // Keep what the user is typing; only the readings change.
        self.show(&window, false);
    }

    fn choose_units(&self, metric: bool) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let units = if metric {
            Units::Metric
        } else {
            Units::Imperial
        };
        self.designer.borrow_mut().set_units(units);
        clear_errors(&window);
        self.show(&window, true);
    }

    fn choose_cabinet(&self, index: i32) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let chosen = usize::try_from(index).is_ok_and(|i| self.designer.borrow_mut().select(i));
        if chosen {
            clear_errors(&window);
            self.show(&window, true);
        }
    }

    /// Adds, copies or removes a cabinet with `change`, then shows the one
    /// to edit, or says why it could not.
    fn change_cabinets(&self, change: impl FnOnce(&mut Designer) -> Result<(), String>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let result = change(&mut self.designer.borrow_mut());
        clear_errors(&window);
        self.show(&window, true);
        if let Err(message) = result {
            self.status(&message);
        }
    }

    fn edit_label(&self, text: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let result = self.designer.borrow_mut().set_label(text);
        window.set_label_error(result.err().unwrap_or_default().into());
        self.show(&window, false);
    }

    fn choose_kind(&self, index: i32) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let Some(&kind) = usize::try_from(index)
            .ok()
            .and_then(|i| CabinetKind::ALL.get(i))
        else {
            return;
        };
        self.designer.borrow_mut().set_kind(kind);
        clear_errors(&window);
        self.show(&window, true);
    }

    fn choose_blind_side(&self, index: i32) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let side = if index == 0 { Side::Left } else { Side::Right };
        self.designer.borrow_mut().set_blind_side(side);
        self.show(&window, false);
    }

    /// Edits another section, unless it is the one being edited: the
    /// list also reports the section the window has just set.
    fn choose_section(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        if self.designer.borrow().selected_section() != Some(index) {
            self.update(|designer| {
                designer.select_section(index);
            });
        }
    }

    fn edit_shelves(&self, text: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let result = self.designer.borrow_mut().set_shelves(text);
        window.set_shelves_error(result.err().unwrap_or_default().into());
        self.show(&window, false);
    }

    fn edit_quantity(&self, text: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let result = self.designer.borrow_mut().set_quantity(text);
        window.set_quantity_error(result.err().unwrap_or_default().into());
        self.show(&window, false);
    }

    fn choose_style(&self, five_piece: bool) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        self.designer.borrow_mut().set_five_piece(five_piece);
        self.show(&window, false);
    }

    fn new_project(&self) {
        if !self.may_discard() {
            return;
        }
        let units = self.designer.borrow().units();
        self.replace(Designer::new(units));
    }

    fn open(&self) {
        if !self.may_discard() {
            return;
        }
        let Some(path) = self.dialogs.open_path() else {
            return;
        };
        match Designer::open(&path) {
            Ok(designer) => {
                let problems = designer.issues().len();
                self.replace(designer);
                let opened = format!("Opened {}", file_name(&path));
                let message = match problems {
                    0 => opened,
                    n => format!("{opened} · {n} problems found"),
                };
                self.status(&message);
            }
            Err(message) => self.status(&message),
        }
    }

    /// Saves to the project's file, or asks where when it has none or
    /// `ask` is set. Returns whether the project was saved.
    fn save(&self, ask: bool) -> bool {
        let known = self.designer.borrow().path().map(Path::to_owned);
        let path = match known {
            Some(path) if !ask => path,
            _ => {
                let suggested = format!("{}.ocab", self.designer.borrow().label());
                let Some(path) = self.dialogs.save_path(&suggested, FileKind::Project) else {
                    return false;
                };
                path
            }
        };
        let result = self.designer.borrow_mut().save_as(&path);
        let Some(window) = self.window.upgrade() else {
            return false;
        };
        self.show(&window, false);
        match result {
            Ok(()) => {
                self.status(&format!("Saved {}", file_name(&path)));
                true
            }
            Err(message) => {
                self.status(&message);
                false
            }
        }
    }

    fn export_materials(&self, separator: Separator) {
        let suggested = format!("{} materials.csv", self.designer.borrow().project_name());
        let Some(path) = self.dialogs.save_path(&suggested, FileKind::Materials) else {
            return;
        };
        match self.designer.borrow().export_materials(&path, separator) {
            Ok(()) => self.status(&format!("Exported the materials to {}", file_name(&path))),
            Err(message) => self.status(&message),
        }
    }

    fn export_cut_list(&self, separator: Separator) {
        let suggested = format!("{} cut list.csv", self.designer.borrow().label());
        let Some(path) = self.dialogs.save_path(&suggested, FileKind::CutList) else {
            return;
        };
        match self.designer.borrow().export_cut_list(&path, separator) {
            Ok(()) => self.status(&format!("Exported the cut list to {}", file_name(&path))),
            Err(message) => self.status(&message),
        }
    }

    fn export_cut_list_pdf(&self) {
        let fonts = match fonts::load() {
            Ok(fonts) => fonts,
            Err(message) => return self.status(&message),
        };
        let suggested = format!("{} cut list.pdf", self.designer.borrow().label());
        let Some(path) = self.dialogs.save_path(&suggested, FileKind::CutListPdf) else {
            return;
        };
        match self.designer.borrow().export_cut_list_pdf(&path, &fonts) {
            Ok(()) => self.status(&format!("Exported the cut list to {}", file_name(&path))),
            Err(message) => self.status(&message),
        }
    }

    fn export_parts_dxf(&self) {
        let Some(folder) = self.dialogs.folder("Export parts as DXF: choose a folder") else {
            return;
        };
        match self.designer.borrow().export_parts_dxf(&folder) {
            Ok(parts) => self.status(&format!(
                "Exported {parts} parts as DXF to {}",
                file_name(&folder)
            )),
            Err(message) => self.status(&message),
        }
    }

    fn export_elevations_pdf(&self) {
        let fonts = match fonts::load() {
            Ok(fonts) => fonts,
            Err(message) => return self.status(&message),
        };
        let suggested = format!("{} elevations.pdf", self.designer.borrow().project_name());
        let Some(path) = self.dialogs.save_path(&suggested, FileKind::ElevationsPdf) else {
            return;
        };
        match self.designer.borrow().export_elevations_pdf(&path, &fonts) {
            Ok(()) => self.status(&format!("Exported the elevations to {}", file_name(&path))),
            Err(message) => self.status(&message),
        }
    }

    fn export_elevations_dxf(&self) {
        let Some(folder) = self
            .dialogs
            .folder("Export elevations as DXF: choose a folder")
        else {
            return;
        };
        match self.designer.borrow().export_elevations_dxf(&folder) {
            Ok(walls) => self.status(&format!(
                "Exported the elevations of {walls} {} as DXF to {}",
                if walls == 1 { "wall" } else { "walls" },
                file_name(&folder)
            )),
            Err(message) => self.status(&message),
        }
    }

    /// Whether the current project may be closed: it has no unsaved
    /// changes, or the user agrees to lose them.
    fn may_discard(&self) -> bool {
        !self.designer.borrow().modified() || self.dialogs.discard_changes()
    }

    fn replace(&self, designer: Designer) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        *self.designer.borrow_mut() = designer;
        clear_errors(&window);
        self.show(&window, true);
    }

    /// Changes the walls, openings or placements with `change`, then shows
    /// the result with the fields rewritten, or says why it could not.
    fn change_room(&self, change: impl FnOnce(&mut Designer) -> Result<(), String>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let result = change(&mut self.designer.borrow_mut());
        clear_errors(&window);
        self.show(&window, true);
        if let Err(message) = result {
            self.status(&message);
        }
    }

    /// Changes the walls, openings or placements with `change`, which
    /// cannot fail, then shows the result with the fields rewritten.
    fn update(&self, change: impl FnOnce(&mut Designer)) {
        self.change_room(|designer| {
            change(designer);
            Ok(())
        });
    }

    /// Shows the designer's state; with `texts`, also rewrites the fields.
    fn show(&self, window: &AppWindow, texts: bool) {
        let designer = self.designer.borrow();
        if texts {
            window.set_label_text(designer.label().into());
        }
        for dimension in Dimension::ALL {
            if texts {
                set_text(window, dimension, designer.text(dimension).into());
            }
            set_reading(window, dimension, designer.reading(dimension).into());
        }
        show_room(window, &designer);
        show_front(window, &designer, texts);
        show_drawing(window, &designer);
        let list = designer.cut_list();
        let rows: Vec<ModelRc<StandardListViewItem>> = list
            .rows
            .iter()
            .map(|row| {
                let cells: Vec<StandardListViewItem> = row
                    .cells()
                    .into_iter()
                    .map(StandardListViewItem::from)
                    .collect();
                ModelRc::new(VecModel::from(cells))
            })
            .collect();
        window.set_cut_rows(ModelRc::new(VecModel::from(rows)));
        window.set_cut_problems(list.problems.join("\n").into());
        let materials = designer.bill_of_materials();
        let rows: Vec<ModelRc<StandardListViewItem>> = materials
            .rows
            .iter()
            .map(|row| {
                let cells: Vec<StandardListViewItem> = row
                    .cells()
                    .into_iter()
                    .map(StandardListViewItem::from)
                    .collect();
                ModelRc::new(VecModel::from(cells))
            })
            .collect();
        window.set_material_rows(ModelRc::new(VecModel::from(rows)));
        window.set_material_notes(materials.notes.join("\n").into());
        let cabinets: Vec<StandardListViewItem> = designer
            .cabinets()
            .iter()
            .map(|c| StandardListViewItem::from(c.as_str()))
            .collect();
        window.set_cabinets(ModelRc::new(VecModel::from(cabinets)));
        window.set_cabinet_index(i32::try_from(designer.selected()).unwrap_or(0));
        window.set_window_title(designer.title().into());
        window.set_cabinet_label(designer.label().into());
        let kind = designer
            .kind()
            .and_then(|k| CabinetKind::ALL.iter().position(|&c| c == k));
        window.set_kind_index(kind.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
        window.set_style_index(i32::from(designer.five_piece()));
        let blind = designer.blind_side();
        window.set_blind(blind.is_some());
        window.set_blind_side_index(i32::from(blind == Some(Side::Right)));
        let metric = designer.units() == Units::Metric;
        window.set_metric(metric);
        let units = if metric { "millimeters" } else { "inches" };
        window.set_status(format!("{NAME_AND_VERSION} · {units}").into());
    }

    fn status(&self, message: &str) {
        if let Some(window) = self.window.upgrade() {
            window.set_status(message.into());
        }
    }
}

/// The drawing area: the elevation or the cabinet, and what is wrong with
/// the placements.
fn show_drawing(window: &AppWindow, designer: &Designer) {
    let drawing = designer.drawing();
    // An empty path is not drawn; a move alone draws nothing.
    let path = |commands: String| {
        if commands.is_empty() {
            "M 0 0".into()
        } else {
            commands.into()
        }
    };
    window.set_drawing_title(drawing.title.into());
    window.set_drawing_width(drawing.width);
    window.set_drawing_height(drawing.height);
    window.set_drawing_lines(path(drawing.lines));
    window.set_drawing_marks(path(drawing.marks));
    window.set_drawing_highlight(path(drawing.highlight));
    window.set_drawing_dimensions(path(drawing.dimensions));
    let labels: Vec<ui::DrawingLabel> = drawing
        .labels
        .into_iter()
        .map(|l| ui::DrawingLabel {
            x: l.x,
            y: l.y,
            text: l.text.into(),
        })
        .collect();
    window.set_drawing_labels(ModelRc::new(VecModel::from(labels)));
    window.set_placement_problems(designer.placement_problems().join("\n").into());
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// The window's text, reading, error and edit callback of each dimension's
/// field.
macro_rules! fields {
    ($($dimension:ident: $text:ident, $reading:ident, $error:ident, $edited:ident;)*) => {
        fn set_text(window: &AppWindow, dimension: Dimension, value: SharedString) {
            match dimension {
                $(Dimension::$dimension => window.$text(value),)*
            }
        }

        fn set_reading(window: &AppWindow, dimension: Dimension, value: SharedString) {
            match dimension {
                $(Dimension::$dimension => window.$reading(value),)*
            }
        }

        fn set_error(window: &AppWindow, dimension: Dimension, message: String) {
            let message = message.into();
            match dimension {
                $(Dimension::$dimension => window.$error(message),)*
            }
        }

        fn on_edited(
            window: &AppWindow,
            dimension: Dimension,
            handler: impl FnMut(SharedString) + 'static,
        ) {
            match dimension {
                $(Dimension::$dimension => window.$edited(handler),)*
            }
        }
    };
}

fields! {
    Width: set_width_text, set_width_reading, set_width_error, on_width_edited;
    Height: set_height_text, set_height_reading, set_height_error, on_height_edited;
    Depth: set_depth_text, set_depth_reading, set_depth_error, on_depth_edited;
    BlindWidth: set_blind_width_text, set_blind_width_reading, set_blind_width_error,
        on_blind_width_edited;
    SectionHeight: set_section_height_text, set_section_height_reading,
        set_section_height_error, on_section_height_edited;
    WallLength: set_wall_length_text, set_wall_length_reading, set_wall_length_error,
        on_wall_length_edited;
    WallHeight: set_wall_height_text, set_wall_height_reading, set_wall_height_error,
        on_wall_height_edited;
    OpeningOffset: set_opening_offset_text, set_opening_offset_reading,
        set_opening_offset_error, on_opening_offset_edited;
    OpeningWidth: set_opening_width_text, set_opening_width_reading, set_opening_width_error,
        on_opening_width_edited;
    OpeningHeight: set_opening_height_text, set_opening_height_reading,
        set_opening_height_error, on_opening_height_edited;
    OpeningSill: set_opening_sill_text, set_opening_sill_reading, set_opening_sill_error,
        on_opening_sill_edited;
    PlacementOffset: set_placement_offset_text, set_placement_offset_reading,
        set_placement_offset_error, on_placement_offset_edited;
    PlacementElevation: set_placement_elevation_text, set_placement_elevation_reading,
        set_placement_elevation_error, on_placement_elevation_edited;
}

/// Connects the File menu's exports to `app`.
fn connect_exports(window: &AppWindow, app: &Rc<App>) {
    let handle = Rc::clone(app);
    window.on_export_cut_list(move |semicolon| {
        let separator = if semicolon {
            Separator::Semicolon
        } else {
            Separator::Comma
        };
        handle.export_cut_list(separator);
    });
    let handle = Rc::clone(app);
    window.on_export_cut_list_pdf(move || handle.export_cut_list_pdf());
    let handle = Rc::clone(app);
    window.on_export_materials(move |semicolon| {
        let separator = if semicolon {
            Separator::Semicolon
        } else {
            Separator::Comma
        };
        handle.export_materials(separator);
    });
    let handle = Rc::clone(app);
    window.on_export_parts_dxf(move || handle.export_parts_dxf());
    let handle = Rc::clone(app);
    window.on_export_elevations_pdf(move || handle.export_elevations_pdf());
    let handle = Rc::clone(app);
    window.on_export_elevations_dxf(move || handle.export_elevations_dxf());
}

/// Connects the Walls tab and the placement section to `app`.
fn connect_room(window: &AppWindow, app: &Rc<App>) {
    /// Calls `choose` with a list's index, when it is one.
    fn chosen(choose: fn(&mut Designer, usize) -> bool) -> impl Fn(&mut Designer, i32) {
        move |designer, index| {
            if let Ok(index) = usize::try_from(index) {
                choose(designer, index);
            }
        }
    }
    let handle = Rc::clone(app);
    window.on_tab_chosen(move |tab| handle.update(|d| d.show_walls(tab == 1)));
    let (handle, choose) = (Rc::clone(app), chosen(Designer::select_wall));
    window.on_wall_chosen(move |i| handle.update(|d| choose(d, i)));
    let handle = Rc::clone(app);
    window.on_add_wall(move || handle.update(Designer::add_wall));
    let handle = Rc::clone(app);
    window.on_delete_wall(move || handle.change_room(Designer::delete_wall));
    let (handle, choose) = (Rc::clone(app), chosen(Designer::select_opening));
    window.on_opening_chosen(move |i| handle.update(|d| choose(d, i)));
    let handle = Rc::clone(app);
    window.on_add_window(move || handle.change_room(|d| d.add_opening(OpeningKind::Window)));
    let handle = Rc::clone(app);
    window.on_add_door(move || handle.change_room(|d| d.add_opening(OpeningKind::Door)));
    let handle = Rc::clone(app);
    window.on_delete_opening(move || handle.change_room(Designer::delete_opening));
    let (handle, choose) = (Rc::clone(app), chosen(Designer::select_placement));
    window.on_placement_chosen(move |i| handle.update(|d| choose(d, i)));
    let handle = Rc::clone(app);
    window.on_place_cabinet(move || handle.change_room(Designer::place));
    let handle = Rc::clone(app);
    window.on_unplace_cabinet(move || handle.change_room(Designer::unplace));
    let (handle, choose) = (Rc::clone(app), chosen(Designer::set_placement_wall));
    window.on_placement_wall_chosen(move |i| handle.update(|d| choose(d, i)));
}

/// Connects the Front and Quantity sections to `app`.
fn connect_front(window: &AppWindow, app: &Rc<App>) {
    let handle = Rc::clone(app);
    window.on_section_chosen(move |index| handle.choose_section(index));
    let handle = Rc::clone(app);
    window.on_add_section(move || handle.change_room(Designer::add_section));
    let handle = Rc::clone(app);
    window.on_delete_section(move || handle.change_room(Designer::delete_section));
    let handle = Rc::clone(app);
    window.on_move_section(move |up| handle.update(|d| d.move_section(up)));
    let handle = Rc::clone(app);
    window.on_section_kind_chosen(move |index| {
        handle.update(|d| {
            let kinds = d.section_kinds();
            if let Some(&kind) = usize::try_from(index).ok().and_then(|i| kinds.get(i)) {
                d.set_section_kind(kind);
            }
        });
    });
    let handle = Rc::clone(app);
    window.on_door_count_chosen(move |index| {
        handle.update(|d| d.set_door_count(if index == 0 { 1 } else { 2 }));
    });
    let handle = Rc::clone(app);
    window.on_hinge_chosen(move |index| {
        handle.update(|d| {
            d.set_hinge_side(if index == 0 { Side::Left } else { Side::Right });
        });
    });
    let handle = Rc::clone(app);
    window.on_shelves_edited(move |text| handle.edit_shelves(&text));
    let handle = Rc::clone(app);
    window.on_quantity_edited(move |text| handle.edit_quantity(&text));
}

/// The edited cabinet's sections and quantity; with `texts`, also rewrites
/// their count fields.
fn show_front(window: &AppWindow, designer: &Designer, texts: bool) {
    let sections: Vec<StandardListViewItem> = designer
        .sections()
        .iter()
        .map(|s| StandardListViewItem::from(s.as_str()))
        .collect();
    window.set_sections(ModelRc::new(VecModel::from(sections)));
    let position = |i: Option<usize>| i.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1);
    window.set_section_index(position(designer.selected_section()));
    window.set_front_note(designer.front_note().unwrap_or_default().into());
    let kinds = designer.section_kinds();
    let names: Vec<SharedString> = kinds.iter().map(|k| k.name().into()).collect();
    window.set_section_kinds(ModelRc::new(VecModel::from(names)));
    let kind = designer
        .section_kind()
        .and_then(|k| kinds.iter().position(|&c| c == k));
    window.set_section_kind_index(position(kind));
    let doors = designer.door_count();
    window.set_section_has_doors(doors.is_some());
    window.set_door_count_index(i32::from(doors == Some(2)));
    window.set_section_single_door(doors == Some(1));
    window.set_hinge_index(match designer.hinge_side() {
        Some(Side::Left) => 0,
        Some(Side::Right) => 1,
        None => -1,
    });
    let shelves = designer.shelves();
    window.set_section_has_shelves(shelves.is_some());
    let placed = designer.placed_count();
    window.set_cabinet_placed(placed > 0);
    window.set_placed_note(match placed {
        0 => SharedString::new(),
        1 => "1 placed on walls".into(),
        count => format!("{count} placed on walls").into(),
    });
    if texts {
        let shelves = shelves.map_or_else(String::new, |s| s.to_string());
        window.set_shelves_text(shelves.into());
        let quantity = if placed > 0 {
            placed.to_string()
        } else {
            designer.quantity().to_string()
        };
        window.set_quantity_text(quantity.into());
    }
}

/// The Walls tab's lists and the edited cabinet's placements.
fn show_room(window: &AppWindow, designer: &Designer) {
    let list = |items: Vec<String>| {
        let items: Vec<StandardListViewItem> = items
            .iter()
            .map(|i| StandardListViewItem::from(i.as_str()))
            .collect();
        ModelRc::new(VecModel::from(items))
    };
    let position = |i: Option<usize>| i.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1);
    let walls = designer.walls();
    let names: Vec<SharedString> = walls
        .iter()
        .map(|w| w.split(" · ").next().unwrap_or_default().into())
        .collect();
    window.set_wall_names(ModelRc::new(VecModel::from(names)));
    window.set_walls(list(walls));
    window.set_wall_index(position(designer.selected_wall()));
    window.set_has_wall(designer.selected_wall().is_some());
    window.set_openings(list(designer.openings()));
    window.set_opening_index(position(designer.selected_opening()));
    window.set_has_opening(designer.selected_opening().is_some());
    window.set_placements(list(designer.placements()));
    window.set_placement_index(position(designer.selected_placement()));
    window.set_placement_wall_index(position(designer.placement_wall()));
    window.set_has_placement(designer.placement_wall().is_some());
}

fn clear_errors(window: &AppWindow) {
    for dimension in Dimension::ALL {
        set_error(window, dimension, String::new());
    }
    window.set_label_error(slint::SharedString::new());
    window.set_shelves_error(SharedString::new());
    window.set_quantity_error(SharedString::new());
}
