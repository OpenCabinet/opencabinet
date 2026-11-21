// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The questions the window asks in the system's own dialogs. Tests
//! replace them with fixed answers.

use std::path::PathBuf;

use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

/// What kind of file is being saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// A PDF with its markups.
    Pdf,
    /// The markup list for spreadsheets.
    List,
}

impl FileKind {
    const fn filter(self) -> (&'static str, &'static str) {
        match self {
            Self::Pdf => ("PDF document", "pdf"),
            Self::List => ("Markup list for spreadsheets", "csv"),
        }
    }
}

/// Asks the user through dialogs.
pub trait Dialogs {
    /// The PDF to open, or `None` when the user cancels.
    fn open_path(&self) -> Option<PathBuf>;

    /// Where to save a file of `kind`, starting from `suggested_name`, or
    /// `None` when the user cancels.
    fn save_path(&self, suggested_name: &str, kind: FileKind) -> Option<PathBuf>;

    /// Whether the user agrees to lose the changes that are not saved.
    fn discard_changes(&self) -> bool;
}

/// The operating system's own dialogs.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemDialogs;

impl Dialogs for SystemDialogs {
    fn open_path(&self) -> Option<PathBuf> {
        let (name, extension) = FileKind::Pdf.filter();
        FileDialog::new()
            .set_title("Open PDF")
            .add_filter(name, &[extension])
            .pick_file()
    }

    fn save_path(&self, suggested_name: &str, kind: FileKind) -> Option<PathBuf> {
        let (name, extension) = kind.filter();
        let title = match kind {
            FileKind::Pdf => "Save PDF as",
            FileKind::List => "Export markup list",
        };
        let path = FileDialog::new()
            .set_title(title)
            .add_filter(name, &[extension])
            .set_file_name(suggested_name)
            .save_file()?;
        // Some systems do not add the extension the filter shows.
        Some(if path.extension().is_some_and(|e| e == extension) {
            path
        } else {
            path.with_extension(extension)
        })
    }

    fn discard_changes(&self) -> bool {
        let answer = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("Unsaved markups")
            .set_description("The PDF has markups that are not saved. Discard them?")
            .set_buttons(MessageButtons::YesNo)
            .show();
        answer == MessageDialogResult::Yes
    }
}
