// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! My tools: markup styles saved to use again, such as a red cloud with
//! the subject "RFI" or a thick blue length.
//!
//! They are kept in `tools.json` in the user's settings folder, so they
//! are there in every drawing. Beside them, `settings.json` keeps how the
//! person likes to work: snapping, and whether the wheel zooms.

use std::path::{Path, PathBuf};

use opencabinet_review::{Color, Kind};
use serde_json::{Value as Json, json};

/// A saved markup style.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedTool {
    /// What the tool is called: its subject, or its kind.
    pub name: String,
    /// What it draws.
    pub kind: Kind,
    /// Its color.
    pub color: Color,
    /// Its line width, in points.
    pub width: f64,
    /// Whether closed shapes are filled.
    pub fill: bool,
    /// The subject its markups take; empty for the kind's name.
    pub subject: String,
}

/// How the person likes to work, kept between runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Whether points snap to the drawing.
    pub snap: bool,
    /// Whether the wheel zooms without Ctrl, the whole page in view.
    pub wheel_zooms: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            snap: true,
            wheel_zooms: false,
        }
    }
}

/// The settings kept beside the tools at `tools`; the defaults when there
/// are none.
#[must_use]
pub fn load_settings(tools: &Path) -> Settings {
    let defaults = Settings::default();
    let Ok(text) = std::fs::read_to_string(tools.with_file_name("settings.json")) else {
        return defaults;
    };
    let Ok(json) = serde_json::from_str::<Json>(&text) else {
        return defaults;
    };
    let flag = |key: &str, default: bool| json.get(key).and_then(Json::as_bool).unwrap_or(default);
    Settings {
        snap: flag("snap", defaults.snap),
        wheel_zooms: flag("wheelZooms", defaults.wheel_zooms),
    }
}

/// Keeps `settings` beside the tools at `tools`.
///
/// # Errors
/// When the file cannot be written.
pub fn store_settings(tools: &Path, settings: Settings) -> std::io::Result<()> {
    if let Some(folder) = tools.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let text = serde_json::to_string_pretty(&json!({
        "snap": settings.snap,
        "wheelZooms": settings.wheel_zooms,
    }))
    .map_err(std::io::Error::other)?;
    std::fs::write(tools.with_file_name("settings.json"), text)
}

/// How many files Open recent remembers.
pub const RECENT: usize = 10;

/// The files opened last, newest first, kept beside the tools at `tools`;
/// none when the list cannot be read.
#[must_use]
pub fn load_recent(tools: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(tools.with_file_name("recent.json")) else {
        return Vec::new();
    };
    let Ok(Json::Array(items)) = serde_json::from_str::<Json>(&text) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Json::as_str)
        .map(PathBuf::from)
        .take(RECENT)
        .collect()
}

/// Keeps `recent` beside the tools at `tools`.
///
/// # Errors
/// When the file cannot be written.
pub fn store_recent(tools: &Path, recent: &[PathBuf]) -> std::io::Result<()> {
    if let Some(folder) = tools.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let items: Vec<String> = recent
        .iter()
        .take(RECENT)
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let text = serde_json::to_string_pretty(&items).map_err(std::io::Error::other)?;
    std::fs::write(tools.with_file_name("recent.json"), text)
}

/// `recent` with `opened` first, once, and no more than [`RECENT`].
#[must_use]
pub fn remembered(recent: &[PathBuf], opened: &Path) -> Vec<PathBuf> {
    let mut list = vec![opened.to_owned()];
    list.extend(recent.iter().filter(|p| p.as_path() != opened).cloned());
    list.truncate(RECENT);
    list
}

/// Where the saved tools are kept: `%APPDATA%\OpenCabinet\Review` on
/// Windows, `~/.config/opencabinet-review` elsewhere.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return Some(
            PathBuf::from(appdata)
                .join("OpenCabinet")
                .join("Review")
                .join("tools.json"),
        );
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("opencabinet-review").join("tools.json"))
}

/// The tools saved at `path`; none when there is no file or it cannot be
/// read.
#[must_use]
pub fn load(path: &Path) -> Vec<SavedTool> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(Json::Array(items)) = serde_json::from_str(&text) else {
        return Vec::new();
    };
    items.iter().filter_map(read).collect()
}

fn read(item: &Json) -> Option<SavedTool> {
    let color = item.get("color")?.as_array()?;
    let channel = |i: usize| -> Option<f64> { Some(color.get(i)?.as_f64()?.clamp(0.0, 1.0)) };
    Some(SavedTool {
        name: item.get("name")?.as_str()?.to_owned(),
        kind: Kind::from_key(item.get("kind")?.as_str()?)?,
        color: [channel(0)?, channel(1)?, channel(2)?],
        width: item.get("width")?.as_f64()?.clamp(0.0, 20.0),
        fill: item.get("fill").and_then(Json::as_bool).unwrap_or(false),
        subject: item
            .get("subject")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// Writes `tools` to `path`, making its folder when needed.
///
/// # Errors
/// When the file cannot be written.
pub fn store(path: &Path, tools: &[SavedTool]) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let items: Vec<Json> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "kind": t.kind.key(),
                "color": t.color,
                "width": t.width,
                "fill": t.fill,
                "subject": t.subject,
            })
        })
        .collect();
    let text = serde_json::to_string_pretty(&Json::Array(items)).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_files_come_newest_first_once_and_are_kept() {
        let a = PathBuf::from("A-201.pdf");
        let b = PathBuf::from("A-301.pdf");
        let list = remembered(&remembered(&remembered(&[], &a), &b), &a);
        assert_eq!(list, [a.clone(), b.clone()]);
        let many: Vec<PathBuf> = (0..20).map(|n| PathBuf::from(format!("{n}.pdf"))).collect();
        assert_eq!(remembered(&many, &a).len(), RECENT);

        let folder = std::env::temp_dir().join(format!("oc-recent-{}", std::process::id()));
        let tools = folder.join("tools.json");
        assert_eq!(load_recent(&tools).len(), 0);
        store_recent(&tools, &list).unwrap();
        assert_eq!(load_recent(&tools), list);
        std::fs::write(folder.join("recent.json"), "not json").unwrap();
        assert_eq!(load_recent(&tools).len(), 0);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn saved_tools_are_written_and_read_back() {
        let folder = std::env::temp_dir().join(format!("oc-chest-{}", std::process::id()));
        let path = folder.join("deeper").join("tools.json");
        let tools = vec![SavedTool {
            name: "RFI".into(),
            kind: Kind::Cloud,
            color: [0.85, 0.1, 0.1],
            width: 2.0,
            fill: false,
            subject: "RFI".into(),
        }];
        store(&path, &tools).unwrap();
        assert_eq!(load(&path), tools);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load(&path), []);
        assert_eq!(load(&folder.join("missing.json")), []);
        assert_eq!(load_settings(&path), Settings::default());
        let settings = Settings {
            snap: false,
            wheel_zooms: true,
        };
        store_settings(&path, settings).unwrap();
        assert_eq!(load_settings(&path), settings);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
