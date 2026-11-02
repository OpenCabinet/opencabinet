// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drives the real window through its accessibility tree, as a screen
//! reader would, with Slint's headless testing backend: no screen, keyboard
//! or mouse of the machine running the test is used, and the dialogs give
//! fixed answers instead of opening.
//!
//! The backend can start once per process, so the scenarios run in order in
//! a single test.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use i_slint_backend_testing::ElementHandle;
use opencabinet_app::AppWindow;
use opencabinet_app::dialogs::{Dialogs, FileKind};
use slint::Model;

/// Answers the dialogs from the test, and counts the questions.
#[derive(Default)]
struct Answers {
    path: RefCell<Option<PathBuf>>,
    discard: Cell<bool>,
    asked_to_discard: Cell<u32>,
}

impl Dialogs for Answers {
    fn open_path(&self) -> Option<PathBuf> {
        self.path.borrow().clone()
    }

    fn save_path(&self, _suggested_name: &str, kind: FileKind) -> Option<PathBuf> {
        let extension = match kind {
            FileKind::Project => "ocab",
            FileKind::CutList | FileKind::Materials => "csv",
            FileKind::CutListPdf | FileKind::ElevationsPdf => "pdf",
        };
        self.path
            .borrow()
            .as_ref()
            .map(|p| p.with_extension(extension))
    }

    fn folder(&self, _title: &str) -> Option<PathBuf> {
        self.path.borrow().as_ref().map(|p| p.with_extension("dxf"))
    }

    fn discard_changes(&self) -> bool {
        self.asked_to_discard.set(self.asked_to_discard.get() + 1);
        self.discard.get()
    }
}

fn field(window: &AppWindow, label: &str) -> ElementHandle {
    ElementHandle::find_by_accessible_label(window, label)
        .find(|e| e.accessible_value().is_some())
        .unwrap_or_else(|| panic!("no editable field labelled {label}"))
}

#[test]
fn the_window_edits_saves_and_reopens_a_project() {
    i_slint_backend_testing::init_integration_test_with_system_time();
    let answers = Rc::new(Answers::default());
    let window = opencabinet_app::create_with(answers.clone()).unwrap();
    assert_eq!(window.get_width_text(), "30\"");
    assert_eq!(window.get_window_title(), "Untitled — OpenCabinet Designer");
    // A cabinet that is not placed on a wall is drawn alone.
    assert_eq!(
        ElementHandle::find_by_accessible_label(&window, "B1, not placed on a wall").count(),
        1
    );

    // The cut list shows the starter cabinet's fourteen parts, drawer box
    // included, with nothing left out.
    let cell = |row: usize, column: usize| {
        let row = window.get_cut_rows().row_data(row).unwrap();
        row.row_data(column).unwrap().text.to_string()
    };
    assert_eq!(window.get_cut_rows().row_count(), 14);
    assert_eq!(
        (cell(2, 1), cell(2, 4)),
        ("Bottom".into(), "28-19/32\"".into())
    );
    assert_eq!(window.get_cut_problems(), "");

    // Shop notation is read exactly and shown back in lowest terms.
    field(&window, "Width").set_accessible_value("30 1/2");
    assert_eq!(window.get_width_reading(), "= 30-1/2\"");
    assert_eq!(window.get_width_error(), "");
    // The cut list follows the new width at once.
    assert_eq!(cell(2, 4), "29-3/32\"");
    assert_eq!(
        window.get_window_title(),
        "Untitled* — OpenCabinet Designer"
    );

    // Something that is not a length explains why and keeps the last value.
    field(&window, "Height").set_accessible_value("tall");
    assert_eq!(window.get_height_error(), "Not a length");
    assert_eq!(window.get_height_reading(), "= 34-1/2\"");

    // Units change what is shown, never what is stored.
    window.invoke_units_chosen(true);
    assert!(window.get_metric());
    assert_eq!(window.get_width_text(), "774.7 mm");
    assert_eq!(window.get_height_error(), "");
    field(&window, "Depth").set_accessible_value("600");
    window.invoke_units_chosen(false);
    assert_eq!(window.get_depth_reading(), "≈ 23-5/8\"");

    // Refusing to discard changes keeps the project.
    window.invoke_new_project();
    assert_eq!(answers.asked_to_discard.get(), 1);
    assert_eq!(window.get_width_text(), "30-1/2\"");

    // Saving writes the file the dialog chose and clears the change mark.
    let dir = std::env::temp_dir().join(format!("opencabinet-window-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test-cabinet.ocab");
    *answers.path.borrow_mut() = Some(path.clone());
    window.invoke_save_project();
    assert!(path.is_file());
    assert_eq!(
        window.get_window_title(),
        "test-cabinet.ocab — OpenCabinet Designer"
    );
    assert_eq!(window.get_status(), "Saved test-cabinet.ocab");

    // A saved project starts over without asking, and reopens as saved.
    window.invoke_new_project();
    assert_eq!(answers.asked_to_discard.get(), 1);
    assert_eq!(window.get_width_text(), "30\"");
    window.invoke_open_project();
    assert_eq!(window.get_width_text(), "30-1/2\"");
    assert_eq!(window.get_depth_reading(), "≈ 23-5/8\"");
    assert_eq!(window.get_status(), "Opened test-cabinet.ocab");

    exports(&window, &path);

    // Five-piece fronts replace the slab ones in the cut list.
    window.invoke_style_chosen(true);
    assert_eq!(window.get_style_index(), 1);
    let parts: Vec<String> = (0..window.get_cut_rows().row_count())
        .map(|r| cell(r, 1))
        .collect();
    assert!(parts.contains(&"Door 1, panel".to_owned()), "{parts:?}");
    window.invoke_style_chosen(false);

    // Choosing another kind of cabinet gives its usual sizes and parts.
    window.invoke_kind_chosen(1);
    assert_eq!(window.get_cabinet_label(), "W1");
    assert_eq!(window.get_height_text(), "30\"");
    assert_eq!(window.get_kind_index(), 1);

    cabinet_list(&window);
    walls_and_placements(&window);
    blind_corner(&window);
    front_and_quantity(&window);

    window.invoke_about();
    assert!(window.get_status().contains("GNU GPL"));
}

/// The Front and Quantity sections of a new base cabinet: a drawer over a
/// pair of doors. They are below the visible part of the panel, so they
/// are edited through their callbacks.
fn front_and_quantity(window: &AppWindow) {
    let sections = |window: &AppWindow| -> Vec<String> {
        let model = window.get_sections();
        (0..model.row_count())
            .map(|i| model.row_data(i).unwrap().text.to_string())
            .collect()
    };
    window.invoke_add_cabinet();
    assert_eq!(window.get_front_note(), "");
    assert_eq!(
        sections(window),
        ["Drawer · 6\"", "2 doors · rest, 24-1/2\""]
    );
    assert_eq!(window.get_section_index(), 0);
    assert_eq!(window.get_section_height_text(), "6\"");
    assert!(!window.get_section_has_doors());

    // The other section's fields are shown, and reporting the same section
    // again changes nothing.
    window.invoke_section_chosen(1);
    assert_eq!(window.get_section_index(), 1);
    assert_eq!(window.get_section_height_text(), "");
    assert_eq!(
        window.get_section_height_reading(),
        "takes the rest: 24-1/2\""
    );
    assert!(window.get_section_has_doors());
    assert!(!window.get_section_single_door());
    window.invoke_shelves_edited("2".into());
    window.set_shelves_text("2 ".into());
    window.invoke_section_chosen(1);
    assert_eq!(window.get_shelves_text(), "2 ");

    window.invoke_door_count_chosen(0);
    assert!(window.get_section_single_door());
    assert_eq!(window.get_hinge_index(), 0);
    window.invoke_hinge_chosen(1);
    assert_eq!(window.get_hinge_index(), 1);
    assert_eq!(
        sections(window)[1],
        "1 door, hinged right, 2 shelves · rest, 24-1/2\""
    );
    window.invoke_shelves_edited("many".into());
    assert_eq!(
        window.get_shelves_error(),
        "Must be a whole number from 0 to 20"
    );

    // A new section goes below the edited one, and is edited.
    window.invoke_add_section();
    assert_eq!(window.get_section_index(), 2);
    assert_eq!(window.get_shelves_error(), "");
    window.invoke_section_kind_chosen(3);
    assert_eq!(window.get_section_kind_index(), 3);
    assert_eq!(sections(window)[2], "Open · 6\"");
    window.invoke_section_height_edited("40".into());
    assert_eq!(
        window.get_section_height_error(),
        "The front's fixed sizes are larger than the opening"
    );
    window.invoke_section_height_edited("8".into());
    assert_eq!(window.get_section_height_error(), "");
    assert_eq!(sections(window)[2], "Open · 8\"");

    window.invoke_move_section(true);
    assert_eq!(window.get_section_index(), 1);
    assert_eq!(sections(window)[1], "Open · 8\"");
    window.invoke_delete_section();
    assert_eq!(sections(window).len(), 2);

    // The quantity is the job's until the cabinet is placed.
    assert_eq!(window.get_quantity_text(), "1");
    assert!(!window.get_cabinet_placed());
    window.invoke_quantity_edited("3".into());
    assert_eq!(window.get_quantity_error(), "");
    window.invoke_quantity_edited("0".into());
    assert_eq!(
        window.get_quantity_error(),
        "Must be a whole number from 1 to 999"
    );
    window.invoke_place_cabinet();
    assert!(window.get_cabinet_placed());
    assert_eq!(window.get_quantity_text(), "1");
    assert_eq!(window.get_placed_note(), "1 placed on walls");
    assert_eq!(window.get_quantity_error(), "");
}

/// A blind corner's own fields, shown only for blind corners.
fn blind_corner(window: &AppWindow) {
    window.set_tab_index(0);
    assert!(!window.get_blind());
    window.invoke_kind_chosen(3);
    assert_eq!(window.get_kind_index(), 3);
    assert!(window.get_blind());
    assert_eq!(window.get_width_text(), "42\"");
    assert_eq!(window.get_blind_side_index(), 1);
    assert_eq!(window.get_blind_width_text(), "24\"");

    // Below the visible part of the panel: edited through its callback.
    window.invoke_blind_width_edited("27".into());
    assert_eq!(window.get_blind_width_reading(), "= 27\"");
    window.invoke_blind_width_edited("50".into());
    assert_eq!(
        window.get_blind_width_error(),
        "Must leave room for the doors"
    );
    assert_eq!(window.get_blind_width_reading(), "= 27\"");

    window.invoke_blind_side_chosen(0);
    assert_eq!(window.get_blind_side_index(), 0);
    assert!(window.get_cabinet_label().starts_with("BC"));

    window.invoke_kind_chosen(0);
    assert!(!window.get_blind());
    assert_eq!(window.get_blind_width_error(), "");
}

/// The list of the project's cabinets, from a project whose only cabinet
/// is W1, 30-1/2" wide.
fn cabinet_list(window: &AppWindow) {
    // The project's cabinets are listed; adding one edits it, and the cut
    // list covers both.
    let cabinets = |window: &AppWindow| -> Vec<String> {
        let model = window.get_cabinets();
        (0..model.row_count())
            .map(|i| model.row_data(i).unwrap().text.to_string())
            .collect()
    };
    assert_eq!(cabinets(window), ["W1 · Wall · 30-1/2\""]);
    window.invoke_add_cabinet();
    assert_eq!(
        cabinets(window),
        ["W1 · Wall · 30-1/2\"", "B1 · Base · 30\""]
    );
    assert_eq!(window.get_cabinet_index(), 1);
    assert_eq!(window.get_label_text(), "B1");
    assert_eq!(window.get_width_text(), "30\"");
    let rows = window.get_cut_rows();
    let products: Vec<String> = (0..rows.row_count())
        .map(|r| {
            rows.row_data(r)
                .unwrap()
                .row_data(0)
                .unwrap()
                .text
                .to_string()
        })
        .collect();
    assert!(products.contains(&"W1".to_owned()) && products.contains(&"B1".to_owned()));

    // A label another cabinet has is refused; a new one renames it.
    field(window, "Label").set_accessible_value("W1");
    assert_eq!(
        window.get_label_error(),
        "Another cabinet is already labeled W1"
    );
    field(window, "Label").set_accessible_value("B30");
    assert_eq!(window.get_label_error(), "");
    assert_eq!(window.get_cabinet_label(), "B30");

    // Choosing a cabinet in the list edits it.
    window.invoke_cabinet_chosen(0);
    assert_eq!(window.get_cabinet_label(), "W1");
    assert_eq!(window.get_width_text(), "30-1/2\"");

    // Duplicating copies it; deleting removes it, but never the last one.
    window.invoke_duplicate_cabinet();
    assert_eq!(window.get_cabinet_label(), "W2");
    // Deleting W2 edits the cabinet that takes its place, B30.
    window.invoke_delete_cabinet();
    assert_eq!(window.get_cabinet_label(), "B30");
    window.invoke_delete_cabinet();
    assert_eq!(cabinets(window), ["W1 · Wall · 30-1/2\""]);
    window.invoke_delete_cabinet();
    assert_eq!(window.get_status(), "A project keeps at least one cabinet");
}

/// The Walls tab and the placement of the only cabinet, W1, 30-1/2" wide.
fn walls_and_placements(window: &AppWindow) {
    let items = |model: slint::ModelRc<slint::StandardListViewItem>| -> Vec<String> {
        (0..model.row_count())
            .map(|i| model.row_data(i).unwrap().text.to_string())
            .collect()
    };
    // Switch to the Walls tab, as a click on it does.
    window.set_tab_index(1);
    window.invoke_tab_chosen(1);
    window.invoke_add_wall();
    assert_eq!(items(window.get_walls()), ["Wall A · 120\""]);
    assert!(window.get_has_wall());
    field(window, "Wall length").set_accessible_value("144");
    assert_eq!(window.get_wall_length_reading(), "= 144\"");
    window.invoke_add_window();
    assert_eq!(items(window.get_openings()), ["Window · 36\" at 54\""]);
    assert_eq!(window.get_drawing_title(), "Elevation of wall Wall A");

    // A wall cabinet goes 54" up.
    window.set_tab_index(0);
    window.invoke_tab_chosen(0);
    window.invoke_place_cabinet();
    assert_eq!(items(window.get_placements()), ["Wall A at 0\", 54\" up"]);
    assert!(window.get_has_placement());
    assert_eq!(window.get_placement_elevation_text(), "54\"");
    assert_eq!(window.get_placement_problems(), "");

    // Hung at 72", its top is above the 96" wall.
    // The field is scrolled out of the test window's view, so it is
    // edited through its callback, as typing into it does.
    window.invoke_placement_elevation_edited("72".into());
    assert_eq!(
        window.get_placement_problems(),
        "W1 is taller than wall Wall A"
    );
    window.invoke_unplace_cabinet();
    assert!(!window.get_has_placement());

    // The elevations export as a PDF and as one DXF per wall.
    window.invoke_export_elevations_pdf();
    assert_eq!(
        window.get_status(),
        "Exported the elevations to test-cabinet.pdf"
    );
    window.invoke_export_elevations_dxf();
    assert_eq!(
        window.get_status(),
        "Exported the elevations of 1 wall as DXF to test-cabinet.dxf"
    );
}

/// The exports of the project saved at `path`, each written next to it.
fn exports(window: &AppWindow, path: &std::path::Path) {
    // The cut list exports for Excel in Spanish: UTF-8 with a byte order
    // mark, semicolons between fields.
    window.invoke_export_cut_list(true);
    let csv = std::fs::read_to_string(path.with_extension("csv")).unwrap();
    assert!(csv.starts_with("\u{feff}Product;Part;Material;"), "{csv}");
    assert_eq!(csv.lines().count(), 15);
    assert_eq!(
        window.get_status(),
        "Exported the cut list to test-cabinet.csv"
    );

    // The materials export the same way.
    window.invoke_export_materials(false);
    let materials = std::fs::read_to_string(path.with_extension("csv")).unwrap();
    assert!(
        materials.starts_with("\u{feff}Section,Item,Quantity,Detail\r\n"),
        "{materials}"
    );
    assert_eq!(
        window.get_status(),
        "Exported the materials to test-cabinet.csv"
    );

    // And as a PDF, with the bundled font.
    window.invoke_export_cut_list_pdf();
    let pdf = std::fs::read(path.with_extension("pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert_eq!(
        window.get_status(),
        "Exported the cut list to test-cabinet.pdf"
    );

    // And every part as DXF, into a folder.
    window.invoke_export_parts_dxf();
    let folder = path.with_extension("dxf");
    assert!(folder.join("B1 - Left side.dxf").is_file());
    assert!(folder.join("all-parts.dxf").is_file());
    assert_eq!(
        window.get_status(),
        "Exported 14 parts as DXF to test-cabinet.dxf"
    );
}
