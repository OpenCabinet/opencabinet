// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drives the real window with Slint's headless testing backend: no screen,
//! keyboard or mouse of the machine running the test is used, and the
//! dialogs give fixed answers instead of opening.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use opencabinet_review_app::dialogs::{Dialogs, FileKind};
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
            FileKind::Pdf => path,
            FileKind::List => path.with_extension("csv"),
        })
    }

    fn discard_changes(&self) -> bool {
        self.asked_to_discard.set(self.asked_to_discard.get() + 1);
        self.discard.get()
    }
}

#[test]
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
    assert_eq!(window.get_window_title(), "A-201.pdf — OpenCabinet Review");
    window.invoke_resized(800.0, 600.0);
    let image = window.get_page_image().size();
    assert_eq!((image.width, image.height), (800, 600));

    // A cloud, then a text box with its text.
    window.invoke_choose_tool(1);
    window.invoke_pressed(200.0, 200.0, 0);
    window.invoke_dragged(300.0, 250.0);
    window.invoke_released(400.0, 300.0);
    assert_eq!(window.get_rows().row_count(), 1);
    assert_eq!(window.get_shapes().row_count(), 1);
    assert!(window.get_has_selection());
    window.invoke_choose_tool(7);
    window.invoke_pressed(450.0, 200.0, 0);
    window.invoke_released(450.0, 200.0);
    window.invoke_text_edited("Filler 3\" here".into());
    assert_eq!(window.get_labels().row_count(), 1);
    let row = window.get_rows().row_data(1).unwrap();
    assert_eq!(row.row_data(1).unwrap().text, "Text");
    assert_eq!(row.row_data(3).unwrap().text, "Filler 3\" here");
    assert!(window.get_window_title().starts_with("* "));

    // Opening another drawing asks first; no keeps the markups.
    window.invoke_open();
    assert_eq!(answers.asked_to_discard.get(), 1);
    assert_eq!(window.get_rows().row_count(), 2);

    // Saved where it was opened, then opened again.
    window.invoke_save();
    assert_eq!(window.get_window_title(), "A-201.pdf — OpenCabinet Review");
    *answers.save.borrow_mut() = Some(dir.join("A-201 list.pdf"));
    window.invoke_export_list();
    let list = std::fs::read_to_string(dir.join("A-201 list.csv")).unwrap();
    assert_eq!(list.lines().count(), 3);
    window.invoke_open();
    assert_eq!(window.get_rows().row_count(), 2);

    // Deleting from the list, then the next page.
    window.invoke_row_selected(0);
    window.invoke_delete_selected();
    assert_eq!(window.get_rows().row_count(), 1);
    window.invoke_next_page();
    assert_eq!(window.get_page_label(), "Page 2 of 2");
    assert_eq!(window.get_shapes().row_count(), 0);

    std::fs::remove_dir_all(&dir).ok();
}
