// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! OpenCabinet Designer's window, as a library so tests can drive it
//! without a window system (see `tests/window.rs`).

pub mod designer;
pub mod dialogs;
pub mod drawing;
pub mod fonts;
pub mod perf;
pub mod preferences;
pub mod user_folder;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use opencabinet_model::{Method, Side, Units};
use slint::{
    CloseRequestResponse, ComponentHandle, Model, ModelRc, SharedString, StandardListViewItem,
    VecModel,
};

use crate::designer::{
    CabinetKind, Designer, Dimension, OpeningKind, RoleChoices, RuleKind, template,
};
use crate::dialogs::{Dialogs, FileKind, SystemDialogs};
use crate::preferences::{Preferences, Theme};
use opencabinet_reports::Separator;

/// The code Slint generates from `ui/app.slint`, which follows Slint's
/// conventions rather than this workspace's lints.
#[allow(missing_docs, clippy::pedantic)]
mod ui {
    slint::include_modules!();
}

pub use ui::{AppWindow, MaterialRole, RuleRow};

/// The name and version shown to users and written into saved files.
pub const NAME_AND_VERSION: &str = concat!("OpenCabinet Designer ", env!("CARGO_PKG_VERSION"));

/// Creates the window, asking the user through the system's dialogs.
///
/// # Errors
/// When the platform cannot create a window.
pub fn create() -> Result<AppWindow, slint::PlatformError> {
    create_with(Rc::new(SystemDialogs), user_folder::usual().as_deref())
}

/// Creates the window and connects it to a new [`Designer`], asking the
/// user through `dialogs`, with the shop template and the preferences in the
/// user's `folder`; `None` when there is no such folder.
///
/// # Errors
/// When the platform cannot create a window.
pub fn create_with(
    dialogs: Rc<dyn Dialogs>,
    folder: Option<&Path>,
) -> Result<AppWindow, slint::PlatformError> {
    let window = AppWindow::new()?;
    let app = App::starting(&window, dialogs, folder);

    for dimension in Dimension::ALL {
        let app = Rc::clone(&app);
        on_edited(&window, dimension, move |text| app.edit(dimension, &text));
    }
    connect_room(&window, &app);
    connect_front(&window, &app);
    connect_materials(&window, &app);
    connect_standard(&window, &app);
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
    window.on_new_project(move |face_frame| handle.new_project(face_frame));
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
    window.on_save_shop_template(move || handle.save_template());
    let handle = Rc::clone(&app);
    window.on_remove_shop_template(move || handle.remove_template());
    let handle = Rc::clone(&app);
    window.on_theme_chosen(move |index| handle.choose_theme(index));
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

/// What the window says when it opens: what came of the shop template, and
/// why the preferences could not be read.
fn starting_message(template: String, preferences: Option<String>) -> String {
    let Some(why) = preferences else {
        return template;
    };
    let preferences =
        format!("Your preferences cannot be read, so the window follows the system: {why}");
    if template.is_empty() {
        preferences
    } else {
        format!("{template}. {preferences}")
    }
}

/// The window's state and what it does with it.
struct App {
    window: slint::Weak<AppWindow>,
    designer: RefCell<Designer>,
    dialogs: Rc<dyn Dialogs>,
    /// Where the shop template lives; `None` when the system does not say
    /// where the user's folder is.
    template_path: Option<PathBuf>,
    /// Where the preferences live, for the same reason optional.
    preferences_path: Option<PathBuf>,
    preferences: RefCell<Preferences>,
}

impl App {
    /// The state of a window that opens, from what the user's `folder` holds.
    fn starting(window: &AppWindow, dialogs: Rc<dyn Dialogs>, folder: Option<&Path>) -> Rc<Self> {
        let template_path = folder.map(template::path_in);
        let preferences_path = folder.map(preferences::path_in);
        let (designer, told) = Designer::starting(template_path.as_deref(), Units::Imperial, None);
        let (preferences, unreadable) = preferences_path
            .as_deref()
            .map(Preferences::read)
            .unwrap_or_default();
        let app = Rc::new(Self {
            window: window.as_weak(),
            designer: RefCell::new(designer),
            dialogs,
            template_path,
            preferences_path,
            preferences: RefCell::new(preferences),
        });
        window.set_system_theme_title(
            if cfg!(windows) {
                "Same as Windows"
            } else {
                "Same as the system"
            }
            .into(),
        );
        app.show(window, true);
        app.show_template(window);
        app.show_theme(window);
        let told = starting_message(told, unreadable);
        if !told.is_empty() {
            app.status(&told);
        }
        app
    }

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

    /// Edits another sheet, unless it is the one being edited: the list
    /// also reports the sheet the window has just set.
    fn choose_sheet(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        if self.designer.borrow().selected_sheet() != Some(index) {
            self.update(|designer| {
                designer.select_sheet(index);
            });
        }
    }

    /// Edits another edge band, unless it is the one being edited, as for
    /// the sheets.
    fn choose_band(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        if self.designer.borrow().selected_band() != Some(index) {
            self.update(|designer| {
                designer.select_band(index);
            });
        }
    }

    fn edit_sheet_name(&self, text: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let result = self.designer.borrow_mut().set_sheet_name(text);
        window.set_sheet_name_error(result.err().unwrap_or_default().into());
        self.show(&window, false);
    }

    fn edit_band_name(&self, text: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let result = self.designer.borrow_mut().set_band_name(text);
        window.set_band_name_error(result.err().unwrap_or_default().into());
        self.show(&window, false);
    }

    /// Gives every other cabinet the edited cabinet's materials, and says
    /// how many changed.
    fn apply_materials_to_all(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let changed = self.designer.borrow_mut().apply_materials_to_all();
        self.show(&window, false);
        self.status(&match changed {
            0 => "The other cabinets already have these materials".to_owned(),
            1 => "1 other cabinet took these materials".to_owned(),
            n => format!("{n} other cabinets took these materials"),
        });
    }

    fn edit_standard_name(&self, text: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let result = self.designer.borrow_mut().set_standard_name(text);
        window.set_standard_name_error(result.err().unwrap_or_default().into());
        self.show(&window, false);
    }

    /// Stores what was typed into a rule; the line under it says if it could
    /// not be, and everything else keeps what is being typed.
    fn edit_rule(&self, key: &str, text: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let result = self.designer.borrow_mut().edit_rule(key, text);
        self.show(&window, false);
        let rows = window.get_rule_rows();
        for i in 0..rows.row_count() {
            if let Some(mut row) = rows.row_data(i).filter(|r| r.key == key) {
                row.error = result.err().unwrap_or_default().into();
                rows.set_row_data(i, row);
                break;
            }
        }
    }

    fn choose_style(&self, five_piece: bool) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        self.designer.borrow_mut().set_five_piece(five_piece);
        self.show(&window, false);
    }

    fn new_project(&self, face_frame: bool) {
        if !self.may_discard() {
            return;
        }
        let units = self.designer.borrow().units();
        let method = if face_frame {
            Method::FaceFrame
        } else {
            Method::Frameless
        };
        let (designer, told) =
            Designer::starting(self.template_path.as_deref(), units, Some(method));
        self.replace(designer);
        if !told.is_empty() {
            self.status(&told);
        }
    }

    fn save_template(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let Some(path) = &self.template_path else {
            self.status("Cannot save the shop template: no user folder was found");
            return;
        };
        if path.exists()
            && !self.dialogs.confirm(
                "Replace shop template",
                "Replace your shop template with this project's materials and standards? \
                 The current one is kept as a backup.",
            )
        {
            return;
        }
        let designer = self.designer.borrow();
        match designer.save_template(path) {
            Ok(()) => self.status(&format!(
                "Shop template saved, with the materials of {}",
                designer.label()
            )),
            Err(message) => self.status(&message),
        }
        self.show_template(&window);
    }

    fn remove_template(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let Some(path) = self.template_path.as_ref().filter(|p| p.exists()) else {
            return;
        };
        if !self.dialogs.confirm(
            "Remove shop template",
            "Remove your shop template? New projects will start from the general values. \
             The current one is kept as a backup.",
        ) {
            return;
        }
        match template::remove(path) {
            Ok(()) => {
                self.status("Shop template removed. New projects start from the general values");
            }
            Err(message) => self.status(&message),
        }
        self.show_template(&window);
    }

    fn choose_theme(&self, index: i32) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let Some(&theme) = usize::try_from(index).ok().and_then(|i| Theme::ALL.get(i)) else {
            return;
        };
        self.preferences.borrow_mut().theme = theme;
        self.show_theme(&window);
        let saved = match &self.preferences_path {
            None => Err("no user folder was found".to_owned()),
            Some(path) => self
                .preferences
                .borrow()
                .save(path)
                .map_err(|e| e.to_string()),
        };
        if let Err(why) = saved {
            self.status(&format!(
                "The theme lasts until the program closes, because it cannot be remembered: {why}"
            ));
        }
    }

    fn show_theme(&self, window: &AppWindow) {
        let theme = self.preferences.borrow().theme;
        let index = Theme::ALL.iter().position(|&t| t == theme).unwrap_or(0);
        window.set_theme_index(i32::try_from(index).unwrap_or(0));
    }

    fn show_template(&self, window: &AppWindow) {
        let found = self.template_path.as_ref().is_some_and(|p| p.exists());
        window.set_has_shop_template(found);
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
        show_materials(window, &designer, texts);
        show_standard(window, &designer, texts);
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
    SheetNominal: set_sheet_nominal_text, set_sheet_nominal_reading, set_sheet_nominal_error,
        on_sheet_nominal_edited;
    SheetActual: set_sheet_actual_text, set_sheet_actual_reading, set_sheet_actual_error,
        on_sheet_actual_edited;
    SheetLength: set_sheet_length_text, set_sheet_length_reading, set_sheet_length_error,
        on_sheet_length_edited;
    SheetWidth: set_sheet_width_text, set_sheet_width_reading, set_sheet_width_error,
        on_sheet_width_edited;
    BandThickness: set_band_thickness_text, set_band_thickness_reading, set_band_thickness_error,
        on_band_thickness_edited;
    BandWidth: set_band_width_text, set_band_width_reading, set_band_width_error,
        on_band_width_edited;
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

/// Connects the Materials tab and the cabinet's Materials section to `app`.
fn connect_materials(window: &AppWindow, app: &Rc<App>) {
    let handle = Rc::clone(app);
    window.on_sheet_chosen(move |index| handle.choose_sheet(index));
    let handle = Rc::clone(app);
    window.on_add_sheet(move || handle.update(Designer::add_sheet));
    let handle = Rc::clone(app);
    window.on_duplicate_sheet(move || handle.update(Designer::duplicate_sheet));
    let handle = Rc::clone(app);
    window.on_delete_sheet(move || handle.change_room(Designer::delete_sheet));
    let handle = Rc::clone(app);
    window.on_sheet_name_edited(move |text| handle.edit_sheet_name(&text));
    let handle = Rc::clone(app);
    window.on_sheet_grain_chosen(move |index| handle.update(|d| d.set_sheet_grain(index == 1)));
    let handle = Rc::clone(app);
    window.on_band_chosen(move |index| handle.choose_band(index));
    let handle = Rc::clone(app);
    window.on_add_band(move || handle.update(Designer::add_band));
    let handle = Rc::clone(app);
    window.on_duplicate_band(move || handle.update(Designer::duplicate_band));
    let handle = Rc::clone(app);
    window.on_delete_band(move || handle.change_room(Designer::delete_band));
    let handle = Rc::clone(app);
    window.on_band_name_edited(move |text| handle.edit_band_name(&text));
    let handle = Rc::clone(app);
    window.on_material_role_chosen(move |key, index| {
        if let Ok(index) = usize::try_from(index) {
            handle.update(|d| d.set_material_role(&key, index));
        }
    });
    let handle = Rc::clone(app);
    window.on_apply_materials_to_all(move || handle.apply_materials_to_all());
}

/// The project's sheets and edge bands, and what the edited cabinet is made
/// of; with `texts`, also rewrites the names.
fn show_materials(window: &AppWindow, designer: &Designer, texts: bool) {
    let list = |items: Vec<String>| {
        let items: Vec<StandardListViewItem> = items
            .iter()
            .map(|i| StandardListViewItem::from(i.as_str()))
            .collect();
        ModelRc::new(VecModel::from(items))
    };
    let names = |choices: RoleChoices| {
        let names: Vec<SharedString> = designer
            .role_choices(choices)
            .iter()
            .map(SharedString::from)
            .collect();
        ModelRc::new(VecModel::from(names))
    };
    let position = |i: Option<usize>| i.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1);
    window.set_sheets(list(designer.sheets()));
    window.set_sheet_index(position(designer.selected_sheet()));
    window.set_has_sheet(designer.selected_sheet().is_some());
    window.set_sheet_grain_index(i32::from(designer.sheet_has_grain() == Some(true)));
    window.set_sheet_note(designer.sheet_note().into());
    window.set_bands(list(designer.bands()));
    window.set_band_index(position(designer.selected_band()));
    window.set_has_band(designer.selected_band().is_some());
    window.set_role_sheets(names(RoleChoices::Sheets));
    window.set_role_sheets_or_doors(names(RoleChoices::SheetsOrDoors));
    window.set_role_sheets_or_case(names(RoleChoices::SheetsOrCase));
    window.set_role_bands(names(RoleChoices::Bands));
    if texts {
        window.set_sheet_name_text(designer.sheet_name().into());
        window.set_band_name_text(designer.band_name().into());
    }
    let rows: Vec<ui::MaterialRole> = designer
        .material_roles()
        .into_iter()
        .map(|role| ui::MaterialRole {
            key: role.key.into(),
            label: role.label.into(),
            choices: match role.choices {
                RoleChoices::Sheets => 0,
                RoleChoices::SheetsOrDoors => 1,
                RoleChoices::SheetsOrCase => 2,
                RoleChoices::Bands => 3,
            },
            index: position(role.chosen),
        })
        .collect();
    // The same rows keep their lists, so that a list being used keeps the
    // keyboard: this runs after every keystroke and every choice.
    let shown = window.get_material_roles();
    if shown.row_count() == rows.len() {
        for (i, row) in rows.into_iter().enumerate() {
            if shown.row_data(i).as_ref() != Some(&row) {
                shown.set_row_data(i, row);
            }
        }
    } else {
        window.set_material_roles(ModelRc::new(VecModel::from(rows)));
    }
}

/// Connects the Standard tab and the cabinet's standard to `app`.
fn connect_standard(window: &AppWindow, app: &Rc<App>) {
    let handle = Rc::clone(app);
    window.on_standard_chosen(move |index| {
        if let Ok(index) = usize::try_from(index) {
            handle.update(|d| d.select_standard(index));
        }
    });
    let handle = Rc::clone(app);
    window.on_cabinet_standard_chosen(move |index| {
        if let Ok(index) = usize::try_from(index) {
            handle.update(|d| d.set_cabinet_standard(index));
        }
    });
    let handle = Rc::clone(app);
    window.on_standard_name_edited(move |text| handle.edit_standard_name(&text));
    let handle = Rc::clone(app);
    window.on_rule_edited(move |key, text| handle.edit_rule(&key, &text));
    let handle = Rc::clone(app);
    window.on_rule_chosen(move |key, index| {
        if let Ok(index) = usize::try_from(index) {
            handle.update(|d| d.choose_rule(&key, index));
        }
    });
    let handle = Rc::clone(app);
    window.on_add_rule_group(move |key| handle.update(|d| d.add_rule_group(&key)));
}

/// The project's standards and the edited one's rules; with `texts`, also
/// rewrites the fields and clears their errors.
fn show_standard(window: &AppWindow, designer: &Designer, texts: bool) {
    let position = |i: Option<usize>| i.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1);
    let names: Vec<SharedString> = designer
        .standards()
        .iter()
        .map(SharedString::from)
        .collect();
    window.set_standard_names(ModelRc::new(VecModel::from(names)));
    window.set_standard_index(position(designer.selected_standard()));
    window.set_cabinet_standard_index(position(designer.cabinet_standard()));
    window.set_standard_note(designer.standard_note().into());
    if texts {
        window.set_standard_name_text(designer.standard_name().into());
    }
    let rows: Vec<ui::RuleRow> = designer
        .rule_rows()
        .into_iter()
        .map(|row| {
            let (kind, options, index) = match row.kind {
                RuleKind::Group => (0, Vec::new(), -1),
                RuleKind::MissingGroup => (1, Vec::new(), -1),
                RuleKind::Field => (2, Vec::new(), -1),
                RuleKind::Choice { options, chosen } => (3, options.to_vec(), position(chosen)),
            };
            let options: Vec<SharedString> = options.into_iter().map(SharedString::from).collect();
            ui::RuleRow {
                key: row.key.into(),
                label: row.label.into(),
                kind,
                text: row.text.into(),
                reading: row.reading.into(),
                error: SharedString::new(),
                options: ModelRc::new(VecModel::from(options)),
                index,
            }
        })
        .collect();
    // The same rows keep their fields, so that a field being typed in keeps
    // the keyboard and what is typed: this runs after every keystroke.
    let shown = window.get_rule_rows();
    let same = shown.row_count() == rows.len()
        && rows
            .iter()
            .enumerate()
            .all(|(i, row)| shown.row_data(i).is_some_and(|old| old.key == row.key));
    if !same {
        window.set_rule_rows(ModelRc::new(VecModel::from(rows)));
        return;
    }
    for (i, mut row) in rows.into_iter().enumerate() {
        let Some(old) = shown.row_data(i) else {
            continue;
        };
        if !texts {
            row.text = old.text.clone();
            row.error = old.error.clone();
        }
        let changed = row.text != old.text
            || row.reading != old.reading
            || row.error != old.error
            || row.index != old.index;
        if changed {
            shown.set_row_data(i, row);
        }
    }
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
    window.set_sheet_name_error(SharedString::new());
    window.set_band_name_error(SharedString::new());
    window.set_standard_name_error(SharedString::new());
}
