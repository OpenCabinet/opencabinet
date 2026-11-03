// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The questions the window asks the user in the system's own dialogs:
//! which file to open, where to save, which folder to export into, whether
//! to discard changes, and whether to go on with something that replaces
//! what the user had.
//! Tests replace them with fixed answers.

use std::path::PathBuf;

use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

/// Asks the user through dialogs.
pub trait Dialogs {
    /// The project file to open, or `None` when the user cancels.
    fn open_path(&self) -> Option<PathBuf>;

    /// Where to save a file of `kind`, starting from `suggested_name`, or
    /// `None` when the user cancels.
    fn save_path(&self, suggested_name: &str, kind: FileKind) -> Option<PathBuf>;

    /// The folder to export files into, asked with `title`, or `None` when
    /// the user cancels.
    fn folder(&self, title: &str) -> Option<PathBuf>;

    /// Whether the user agrees to lose the changes that are not saved.
    fn discard_changes(&self) -> bool;

    /// Whether the user agrees to `question`, shown under `title`.
    fn confirm(&self, title: &str, question: &str) -> bool;
}

/// What kind of file is being saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// An OpenCabinet project, `.ocab`.
    Project,
    /// A cut list for spreadsheets, `.csv`.
    CutList,
    /// A cut list to print, `.pdf`.
    CutListPdf,
    /// The walls' elevations to print, `.pdf`.
    ElevationsPdf,
    /// The bill of materials for spreadsheets, `.csv`.
    Materials,
}

impl FileKind {
    fn filter(self) -> (&'static str, &'static str) {
        match self {
            Self::Project => ("OpenCabinet project", "ocab"),
            Self::CutList => ("Cut list for spreadsheets", "csv"),
            Self::Materials => ("Materials for spreadsheets", "csv"),
            Self::CutListPdf | Self::ElevationsPdf => ("PDF document", "pdf"),
        }
    }
}

/// The operating system's own dialogs.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemDialogs;

impl Dialogs for SystemDialogs {
    fn open_path(&self) -> Option<PathBuf> {
        let (name, extension) = FileKind::Project.filter();
        FileDialog::new()
            .set_title("Open project")
            .add_filter(name, &[extension])
            .pick_file()
    }

    fn save_path(&self, suggested_name: &str, kind: FileKind) -> Option<PathBuf> {
        let (name, extension) = kind.filter();
        let title = match kind {
            FileKind::Project => "Save project",
            FileKind::CutList | FileKind::CutListPdf => "Export cut list",
            FileKind::ElevationsPdf => "Export elevations",
            FileKind::Materials => "Export materials",
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

    fn folder(&self, title: &str) -> Option<PathBuf> {
        FileDialog::new().set_title(title).pick_folder()
    }

    fn discard_changes(&self) -> bool {
        let answer = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("Unsaved changes")
            .set_description("The project has changes that are not saved. Discard them?")
            .set_buttons(MessageButtons::YesNo)
            .show();
        answer == MessageDialogResult::Yes
    }

    fn confirm(&self, title: &str, question: &str) -> bool {
        let answer = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title(title)
            .set_description(question)
            .set_buttons(MessageButtons::YesNo)
            .show();
        answer == MessageDialogResult::Yes
    }
}
