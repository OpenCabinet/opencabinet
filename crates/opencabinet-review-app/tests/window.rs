// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drives the real window with Slint's headless testing backend: no screen,
//! keyboard or mouse of the machine running the test is used, and the
//! dialogs give fixed answers instead of opening.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use opencabinet_review_app::ReviewWindow;
use opencabinet_review_app::dialogs::{Dialogs, FileKind};
use opencabinet_review_app::viewer::TOOLS;
use slint::{ComponentHandle, Model};

#[derive(Default)]
struct Answers {
    open: RefCell<Option<PathBuf>>,
    save: RefCell<Option<PathBuf>>,
    discard: Cell<bool>,
    asked_to_discard: Cell<u32>,
}

impl Dialogs for Answers {
    fn open_path(&self) -> Option<PathBuf> {
        self.open.borrow().clone()
    }

    fn save_path(&self, _suggested_name: &str, kind: FileKind) -> Option<PathBuf> {
        let path = self.save.borrow().clone()?;
        Some(match kind {
            FileKind::Pdf | FileKind::Set => path,
            FileKind::List => path.with_extension("csv"),
        })
    }

    fn discard_changes(&self) -> bool {
        self.asked_to_discard.set(self.asked_to_discard.get() + 1);
        self.discard.get()
    }
}

/// The place of the tool called `label` in the window's tool list.
fn tool(label: &str) -> i32 {
    let index = TOOLS.iter().position(|t| t.label() == label).unwrap();
    i32::try_from(index).unwrap()
}

#[test]
#[allow(clippy::too_many_lines, reason = "one scenario, step by step")]
fn a_drawing_is_opened_marked_up_and_saved() {
    i_slint_backend_testing::init_integration_test_with_system_time();
    let dir =
        std::env::temp_dir().join(format!("opencabinet-review-window-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let drawing = dir.join("A-201.pdf");
    std::fs::write(
        &drawing,
        opencabinet_pdf::sample::sheets(2, 2592.0, 1728.0, 6),
    )
    .unwrap();

    let answers = Rc::new(Answers::default());
    let window = opencabinet_review_app::create_with(answers.clone(), None).unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1280.0, 760.0));
    window.show().unwrap();
    assert!(!window.get_has_document());

    *answers.open.borrow_mut() = Some(drawing.clone());
    window.invoke_open();
    assert!(window.get_has_document());
    assert_eq!(window.get_page_label(), "Page 1 of 2");
    // A thumbnail for each page, drawn as they come.
    assert_eq!(window.get_thumbnails().row_count(), 2);
    assert_eq!(window.get_window_title(), "A-201.pdf — OpenCabinet Review");
    window.invoke_resized(800.0, 600.0);
    let image = window.get_page_image().size();
    assert_eq!((image.width, image.height), (800, 600));

    // A cloud, then a text box with its text.
    window.invoke_choose_tool(tool("Cloud"));
    window.invoke_pressed(200.0, 200.0, 0);
    window.invoke_dragged(300.0, 250.0);
    window.invoke_released(400.0, 300.0);
    assert_eq!(window.get_rows().row_count(), 1);
    // The cloud and the grips on its corners, and maybe the mark of what
    // the mouse snaps to.
    assert!((5..=6).contains(&window.get_shapes().row_count()));
    assert!(window.get_has_selection());
    assert_eq!(window.get_selected_count(), 1);
    window.invoke_reply("Is this the new opening?".into());
    assert_eq!(window.get_replies().row_count(), 1);
    assert!(window.get_replies().row_data(0).unwrap().mine);
    window.invoke_choose_tool(tool("Text"));
    window.invoke_pressed(450.0, 200.0, 0);
    window.invoke_released(450.0, 200.0);
    window.invoke_text_edited("Filler 3\" here".into());
    assert_eq!(window.get_labels().row_count(), 1);
    let row = window.get_rows().row_data(1).unwrap();
    assert_eq!(row.row_data(0).unwrap().text, "Text");
    // Undone and redone.
    assert!(window.get_can_undo());
    window.invoke_undo();
    assert_eq!(window.get_labels().row_count(), 0);
    window.invoke_redo();
    assert_eq!(window.get_labels().row_count(), 1);
    assert_eq!(row.row_data(2).unwrap().text, "Filler 3\" here");
    assert!(window.get_window_title().starts_with("* "));

    // An area, point by point, finished with Enter.
    window.invoke_choose_tool(tool("Area"));
    for (x, y) in [(500.0, 300.0), (600.0, 300.0), (600.0, 400.0)] {
        window.invoke_pressed(x, y, 0);
        window.invoke_released(x, y);
    }
    assert!(window.get_building());
    window.invoke_finish();
    assert!(!window.get_building());
    assert_eq!(window.get_rows().row_count(), 3);
    window.invoke_choose_status(2);
    assert_eq!(
        window
            .get_rows()
            .row_data(2)
            .unwrap()
            .row_data(5)
            .unwrap()
            .text,
        "Rejected"
    );
    window.invoke_delete_selected();

    // A copy of the cloud pasted beside it, then undone.
    window.invoke_row_selected(0);
    window.invoke_copy();
    window.invoke_paste();
    assert_eq!(window.get_rows().row_count(), 3);
    window.invoke_undo();
    assert_eq!(window.get_rows().row_count(), 2);

    // Escape goes back to the select tool.
    window.invoke_choose_tool(tool("Area"));
    window.invoke_escape();
    assert_eq!(window.get_tool(), tool("Select"));

    // The sheet number is found; the list is filtered.
    window.invoke_find("A-201".into());
    assert_eq!(window.get_found_label(), "1 of 1");
    window.invoke_set_filter(0, "filler".into());
    assert_eq!(window.get_rows().row_count(), 1);
    assert_eq!(window.get_list_label(), "1 of 2 markups");
    window.invoke_set_filter(0, "".into());

    // The list sorts by a column's header; a markup's style is saved in
    // My tools and drawn with again.
    window.invoke_sort_list(0, true);
    let first = |w: &ReviewWindow| w.get_rows().row_data(0).unwrap().row_data(0).unwrap().text;
    assert_eq!(first(&window), "Text");
    window.invoke_sort_list(0, false);
    assert_eq!(first(&window), "Cloud");
    window.invoke_row_selected(0);
    window.invoke_save_tool();
    assert_eq!(window.get_saved_tools().row_count(), 1);
    assert_eq!(window.get_saved_tools().row_data(0).unwrap().name, "Cloud");
    window.invoke_use_saved(0);
    assert_eq!(window.get_saved_in_use(), 0);
    assert_eq!(window.get_tool(), tool("Cloud"));
    window.invoke_remove_saved(0);
    assert_eq!(window.get_saved_tools().row_count(), 0);
    window.invoke_escape();

    // A common scale for every page.
    let quarter = (0..window.get_scales().row_count())
        .find(|&i| window.get_scales().row_data(i).unwrap() == "1/4\" = 1'-0\"")
        .unwrap();
    window.invoke_choose_scale(i32::try_from(quarter).unwrap(), true);
    assert_eq!(window.get_scale_label(), "Scale 1/4\" = 1'-0\"");

    // Opening the same drawing again stays in its tab, markups and all.
    window.invoke_open();
    assert_eq!(answers.asked_to_discard.get(), 0);
    assert_eq!(window.get_tabs().row_count(), 1);
    assert_eq!(window.get_rows().row_count(), 2);

    // Saved where it was opened, then opened again.
    window.invoke_save();
    assert_eq!(window.get_window_title(), "A-201.pdf — OpenCabinet Review");
    *answers.save.borrow_mut() = Some(dir.join("A-201 list.pdf"));
    window.invoke_export_list();
    let list = std::fs::read_to_string(dir.join("A-201 list.csv")).unwrap();
    assert_eq!(list.lines().count(), 3);
    *answers.save.borrow_mut() = Some(dir.join("A-201 summary.pdf"));
    window.invoke_export_summary();
    let summary = std::fs::read(dir.join("A-201 summary.pdf")).unwrap();
    assert!(summary.starts_with(b"%PDF"));
    window.invoke_open();
    assert_eq!(window.get_rows().row_count(), 2);

    // Deleting from the list, then the next page.
    window.invoke_row_selected(0);
    window.invoke_delete_selected();
    assert_eq!(window.get_rows().row_count(), 1);
    window.invoke_next_page();
    assert_eq!(window.get_page_label(), "Page 2 of 2");
    assert_eq!(window.get_shapes().row_count(), 0);

    // Both pages chosen in the Pages panel, turned, then the first taken
    // out; the thumbnails follow, and undo brings it back.
    assert!(window.get_can_change_pages());
    window.invoke_page_clicked(0, true, false);
    assert_eq!(window.get_pages_label(), "2 pages chosen");
    assert_eq!(window.get_pages_chosen().row_data(0), Some(true));
    window.invoke_turn_pages(1);
    assert!(window.get_status().starts_with("Turned pages 1 and 2"));
    assert!(window.get_window_title().starts_with("* "));
    window.invoke_page_clicked(0, false, false);
    assert_eq!(window.get_pages_label(), "Page 1 chosen");
    window.invoke_delete_pages();
    assert_eq!(window.get_page_label(), "Page 1 of 1");
    assert_eq!(window.get_thumbnails().row_count(), 1);
    window.invoke_undo();
    assert_eq!(window.get_page_label(), "Page 1 of 2");
    assert_eq!(window.get_thumbnails().row_count(), 2);
    *answers.save.borrow_mut() = Some(dir.join("A-201 page 1.pdf"));
    window.invoke_extract_pages();
    assert!(dir.join("A-201 page 1.pdf").exists());

    // Another drawing opens in a tab of its own, with the tool in use;
    // the first keeps its changes, and closing it asks first.
    let other = dir.join("A-301.pdf");
    std::fs::write(
        &other,
        opencabinet_pdf::sample::sheets(3, 1728.0, 1296.0, 2),
    )
    .unwrap();
    window.invoke_choose_tool(tool("Cloud"));
    *answers.open.borrow_mut() = Some(other.clone());
    window.invoke_open();
    assert_eq!(window.get_tabs().row_count(), 2);
    assert_eq!(window.get_active_tab(), 1);
    assert_eq!(window.get_page_label(), "Page 1 of 3");
    assert_eq!(window.get_window_title(), "A-301.pdf — OpenCabinet Review");
    assert_eq!(window.get_tool(), tool("Cloud"));
    let first = window.get_tabs().row_data(0).unwrap();
    assert_eq!(first.title, "A-201.pdf");
    assert!(first.modified);
    // Open recent: the newest first, each once.
    let recent = window.get_recent();
    assert_eq!(recent.row_count(), 2);
    assert!(recent.row_data(0).unwrap().ends_with("A-301.pdf"));
    assert!(recent.row_data(1).unwrap().ends_with("A-201.pdf"));
    window.invoke_next_tab();
    assert_eq!(window.get_active_tab(), 0);
    assert_eq!(window.get_page_label(), "Page 1 of 2");
    // Words looked for in every open drawing: only the second has A-203.
    window.invoke_find_everywhere("A-203".into());
    let found = window.get_found_everywhere();
    assert_eq!(found.row_count(), 1);
    assert_eq!(found.row_data(0).unwrap().text, "A-301.pdf — 1 on 1 page");
    window.invoke_show_found_in(0);
    assert_eq!(window.get_active_tab(), 1);
    assert_eq!(window.get_page_label(), "Page 3 of 3");
    window.invoke_next_tab();
    assert_eq!(window.get_active_tab(), 0);
    answers.discard.set(false);
    window.invoke_close_tab(0);
    assert_eq!(answers.asked_to_discard.get(), 1);
    assert_eq!(window.get_tabs().row_count(), 2);
    window.invoke_close_tab(1);
    assert_eq!(window.get_tabs().row_count(), 1);
    assert_eq!(
        window.get_window_title(),
        "* A-201.pdf — OpenCabinet Review"
    );
    answers.discard.set(true);
    window.invoke_close_this_tab();
    assert_eq!(window.get_tabs().row_count(), 1);
    assert!(!window.get_has_document());

    // A sheet set of both drawings: saved, then opened again, each in a
    // tab, with one drawing gone missing.
    *answers.open.borrow_mut() = Some(drawing.clone());
    window.invoke_open();
    *answers.open.borrow_mut() = Some(other.clone());
    window.invoke_open();
    let set = dir.join("Kitchen.ocset");
    *answers.save.borrow_mut() = Some(set.clone());
    window.invoke_save_tabs_as_set();
    assert!(set.exists());
    assert_eq!(window.get_set_name(), "Kitchen");
    let sheets = window.get_sheets();
    assert_eq!(sheets.row_count(), 2);
    assert_eq!(sheets.row_data(1).unwrap().state, "Shown");
    assert_eq!(sheets.row_data(0).unwrap().state, "Open");
    window.invoke_remove_from_set(1);
    assert_eq!(window.get_sheets().row_count(), 1);
    window.invoke_add_to_set();
    assert_eq!(window.get_sheets().row_count(), 2);
    window.invoke_save_set();
    window.invoke_close_this_tab();
    window.invoke_close_this_tab();
    std::fs::rename(&other, dir.join("moved.pdf")).unwrap();
    *answers.open.borrow_mut() = Some(set.clone());
    window.invoke_open();
    assert_eq!(window.get_panel(), 5);
    assert_eq!(window.get_tabs().row_count(), 1);
    let sheets = window.get_sheets();
    assert_eq!(sheets.row_data(0).unwrap().label, "A-201");
    assert_eq!(sheets.row_data(1).unwrap().state, "Missing");
    assert!(window.get_status().contains("1 not found"));
    window.invoke_open_sheet(0);
    assert_eq!(window.get_tabs().row_count(), 1);

    std::fs::remove_dir_all(&dir).ok();
}
