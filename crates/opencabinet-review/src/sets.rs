// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sheet sets: the drawings of a project kept together, so they open
//! together and are seen as one list.
//!
//! A set is a small JSON file, `.ocset`, beside the drawings:
//!
//! ```json
//! { "format": "opencabinet-review-set", "version": 1, "name": "Kitchen",
//!   "sheets": [ { "file": "A-201.pdf", "label": "A-201" } ] }
//! ```
//!
//! Files are written relative to the set when they are in its folder or
//! under it, so the folder can be moved or shared whole.

use std::path::{Component, Path, PathBuf};

use serde_json::{Value as Json, json};

/// What the set file says it is.
const FORMAT: &str = "opencabinet-review-set";
/// Windows' folder separator, written as `/` so a set opens anywhere.
const BACKSLASH: char = '\\';
/// The most sheets a set holds.
pub const MAX_SHEETS: usize = 2000;

/// One drawing of a set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sheet {
    /// Where the PDF is, whole.
    pub path: PathBuf,
    /// What the sheet is called, such as `A-201`.
    pub label: String,
}

impl Sheet {
    /// The sheet at `path`, labelled after the file's name.
    #[must_use]
    pub fn at(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            label: label_for(path),
        }
    }
}

/// A project's drawings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetSet {
    /// The set's name.
    pub name: String,
    /// Its sheets, in order.
    pub sheets: Vec<Sheet>,
}

impl SheetSet {
    /// Adds the PDF at `path`, unless the set has it; whether it was
    /// added.
    pub fn add(&mut self, path: &Path) -> bool {
        if self.sheets.iter().any(|s| s.path == path) || self.sheets.len() >= MAX_SHEETS {
            return false;
        }
        self.sheets.push(Sheet::at(path));
        true
    }

    /// Takes sheet `index` out of the set; the PDF stays where it is.
    pub fn remove(&mut self, index: usize) {
        if index < self.sheets.len() {
            self.sheets.remove(index);
        }
    }

    /// The sheet whose PDF is at `path`, if any.
    #[must_use]
    pub fn position(&self, path: &Path) -> Option<usize> {
        self.sheets.iter().position(|s| s.path == path)
    }
}

/// A label for the drawing at `path`: its file name without `.pdf`.
#[must_use]
pub fn label_for(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

/// Reads the set at `path`.
///
/// # Errors
/// A message for the user when it cannot be read or is not a set.
pub fn read(path: &Path) -> Result<SheetSet, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Cannot read the set: {e}"))?;
    let json: Json = serde_json::from_str(&text).map_err(|_| "This is not a sheet set")?;
    if json.get("format").and_then(Json::as_str) != Some(FORMAT) {
        return Err("This is not a sheet set".into());
    }
    if json.get("version").and_then(Json::as_u64) != Some(1) {
        return Err(
            "This sheet set was written by a newer Review; update Review to open it".into(),
        );
    }
    let folder = path.parent().unwrap_or(Path::new("."));
    let sheets = json
        .get("sheets")
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .take(MAX_SHEETS)
                .filter_map(|item| {
                    let file = item.get("file")?.as_str()?;
                    let path = folder.join(file);
                    let label = item
                        .get("label")
                        .and_then(Json::as_str)
                        .map_or_else(|| label_for(&path), str::to_owned);
                    Some(Sheet { path, label })
                })
                .collect()
        })
        .unwrap_or_default();
    let name = json
        .get("name")
        .and_then(Json::as_str)
        .map_or_else(|| label_for(path), str::to_owned);
    Ok(SheetSet { name, sheets })
}

/// Writes `set` to `path`.
///
/// # Errors
/// A message for the user when it cannot be written.
pub fn write(path: &Path, set: &SheetSet) -> Result<(), String> {
    let folder = path.parent().unwrap_or(Path::new("."));
    let sheets: Vec<Json> = set
        .sheets
        .iter()
        .map(|s| {
            json!({
                "file": relative(&s.path, folder).to_string_lossy().replace(BACKSLASH, "/"),
                "label": s.label,
            })
        })
        .collect();
    let text = serde_json::to_string_pretty(&json!({
        "format": FORMAT,
        "version": 1,
        "name": set.name,
        "sheets": sheets,
    }))
    .map_err(|e| e.to_string())?;
    std::fs::write(path, text + "\n").map_err(|e| format!("Cannot save the set: {e}"))
}

/// `path` relative to `folder` when it is in it or under it; whole
/// otherwise.
fn relative(path: &Path, folder: &Path) -> PathBuf {
    let whole = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_owned());
    let (path_whole, folder_whole) = (whole(path), whole(folder));
    match path_whole.strip_prefix(&folder_whole) {
        Ok(inside)
            if inside
                .components()
                .all(|c| matches!(c, Component::Normal(_))) =>
        {
            inside.to_owned()
        }
        _ => path_whole,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_is_written_beside_its_drawings_and_read_back() {
        let folder = std::env::temp_dir().join(format!("oc-set-{}", std::process::id()));
        std::fs::create_dir_all(folder.join("elevations")).unwrap();
        let mut set = SheetSet {
            name: "Kitchen".into(),
            sheets: Vec::new(),
        };
        assert!(set.add(&folder.join("A-201.pdf")));
        assert!(set.add(&folder.join("elevations").join("A-301.pdf")));
        assert!(!set.add(&folder.join("A-201.pdf")), "once only");
        set.sheets[1].label = "A-301 Elevations".into();
        let path = folder.join("Kitchen.ocset");
        write(&path, &set).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("\"file\": \"elevations/A-301.pdf\""),
            "{text}"
        );
        let back = read(&path).unwrap();
        assert_eq!(back.name, "Kitchen");
        assert_eq!(back.sheets.len(), 2);
        assert_eq!(back.sheets[0].label, "A-201");
        assert_eq!(back.sheets[1].label, "A-301 Elevations");
        assert_eq!(
            std::path::absolute(&back.sheets[1].path).unwrap(),
            std::path::absolute(folder.join("elevations").join("A-301.pdf")).unwrap()
        );
        assert_eq!(back.position(&back.sheets[1].path.clone()), Some(1));

        std::fs::write(&path, "{\"format\": \"something else\"}").unwrap();
        assert!(read(&path).is_err());
        std::fs::write(
            &path,
            "{\"format\": \"opencabinet-review-set\", \"version\": 9}",
        )
        .unwrap();
        assert!(read(&path).unwrap_err().contains("newer"));
        std::fs::remove_dir_all(&folder).ok();
    }
}
