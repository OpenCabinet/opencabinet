// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tool sets: markup styles saved to use again, such as a red cloud with
//! the subject "RFI", a thick blue length or a stamp, kept in named sets
//! ("Contractor review", "Estimating") that can be shared as files.
//!
//! They are kept in `toolsets.json` in the user's settings folder, so they
//! are there in every drawing; the first time, the old `tools.json` (My
//! tools) becomes the first set. Beside them, `settings.json` keeps how
//! the person likes to work, and the person's own stamps are files in a
//! stamps folder, `Stamps` there unless another is chosen (a shared office
//! folder, say).

use std::path::{Path, PathBuf};

use opencabinet_review::stamps::{self, Design};
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
    /// The stamp it places, by name; empty for other tools.
    pub stamp: String,
}

/// A named set of saved tools.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSet {
    /// Its name.
    pub name: String,
    /// Whether it is folded away in the window.
    pub collapsed: bool,
    /// Its tools, in order.
    pub tools: Vec<SavedTool>,
}

/// What a file of shared tool sets says it is.
const SETS_FORMAT: &str = "opencabinet-review-tools";

fn set_json(set: &ToolSet) -> Json {
    json!({
        "name": set.name,
        "collapsed": set.collapsed,
        "tools": set.tools.iter().map(tool_json).collect::<Vec<_>>(),
    })
}

fn read_set(item: &Json) -> Option<ToolSet> {
    Some(ToolSet {
        name: item.get("name")?.as_str()?.trim().to_owned(),
        collapsed: item
            .get("collapsed")
            .and_then(Json::as_bool)
            .unwrap_or(false),
        tools: item
            .get("tools")?
            .as_array()?
            .iter()
            .filter_map(read)
            .collect(),
    })
}

/// The tool sets kept beside the tools at `tools`; the first time, My
/// tools as one set.
#[must_use]
pub fn load_sets(tools: &Path) -> Vec<ToolSet> {
    let read_all = || -> Option<Vec<ToolSet>> {
        let text = std::fs::read_to_string(tools.with_file_name("toolsets.json")).ok()?;
        let json: Json = serde_json::from_str(&text).ok()?;
        Some(json.as_array()?.iter().filter_map(read_set).collect())
    };
    match read_all() {
        Some(sets) if !sets.is_empty() => sets,
        _ => vec![ToolSet {
            name: "My tools".into(),
            collapsed: false,
            tools: load(tools),
        }],
    }
}

/// Keeps `sets` beside the tools at `tools`.
///
/// # Errors
/// When the file cannot be written.
pub fn store_sets(tools: &Path, sets: &[ToolSet]) -> std::io::Result<()> {
    if let Some(folder) = tools.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let items: Vec<Json> = sets.iter().map(set_json).collect();
    let text = serde_json::to_string_pretty(&items).map_err(std::io::Error::other)?;
    std::fs::write(tools.with_file_name("toolsets.json"), text)
}

/// Writes `set`, with the stamps its tools place, to `path`, to share.
///
/// # Errors
/// When the file cannot be written.
pub fn export_set(path: &Path, set: &ToolSet, designs: &[Design]) -> std::io::Result<()> {
    let used: Vec<Json> = designs
        .iter()
        .filter(|d| set.tools.iter().any(|t| t.stamp == d.name))
        .map(Design::to_json)
        .collect();
    let text = serde_json::to_string_pretty(&json!({
        "format": SETS_FORMAT,
        "version": 1,
        "sets": [set_json(set)],
        "stamps": used,
    }))
    .map_err(std::io::Error::other)?;
    std::fs::write(path, text + "\n")
}

/// The tool sets and stamps a shared file at `path` holds.
///
/// # Errors
/// A message for the user when it cannot be read or is not one.
pub fn import_sets(path: &Path) -> Result<(Vec<ToolSet>, Vec<Design>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    let json: Json = serde_json::from_str(&text).map_err(|_| "This is not a tool set file")?;
    if json.get("format").and_then(Json::as_str) != Some(SETS_FORMAT) {
        return Err("This is not a tool set file".into());
    }
    if json.get("version").and_then(Json::as_u64) != Some(1) {
        return Err("This tool set file was written by a newer Review".into());
    }
    let sets: Vec<ToolSet> = json
        .get("sets")
        .and_then(Json::as_array)
        .map(|s| s.iter().filter_map(read_set).collect())
        .unwrap_or_default();
    let designs = json
        .get("stamps")
        .and_then(Json::as_array)
        .map(|s| s.iter().filter_map(Design::from_json).collect())
        .unwrap_or_default();
    if sets.is_empty() {
        return Err("The file has no tool sets".into());
    }
    Ok((sets, designs))
}

/// Where the person's own stamps are kept: the folder chosen in the
/// settings beside the tools at `tools`, or `Stamps` beside them.
#[must_use]
pub fn stamps_folder(tools: &Path) -> PathBuf {
    std::fs::read_to_string(tools.with_file_name("settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Json>(&t).ok())
        .and_then(|j| {
            j.get("stampsFolder")
                .and_then(Json::as_str)
                .map(PathBuf::from)
        })
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| tools.with_file_name("Stamps"))
}

/// Keeps `folder` as where the stamps are, in the settings beside the
/// tools at `tools`.
///
/// # Errors
/// When the settings cannot be written.
pub fn store_stamps_folder(tools: &Path, folder: &Path) -> std::io::Result<()> {
    update_settings(tools, "stampsFolder", json!(folder.to_string_lossy()))
}

/// The stamps in `folder`, by name; none when it cannot be read.
#[must_use]
pub fn load_designs(folder: &Path) -> Vec<Design> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut designs: Vec<Design> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ocstamp"))
        })
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .filter_map(|t| serde_json::from_str::<Json>(&t).ok())
        .filter_map(|j| Design::from_json(&j))
        .filter(|d| !d.name.is_empty())
        .collect();
    designs.sort_by_key(|d| d.name.to_lowercase());
    designs.dedup_by(|a, b| a.name == b.name);
    designs
}

/// The file in `folder` a stamp called `name` is kept in.
#[must_use]
pub fn design_path(folder: &Path, name: &str) -> PathBuf {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim().trim_matches('.');
    folder.join(format!(
        "{}.ocstamp",
        if safe.is_empty() { "stamp" } else { safe }
    ))
}

/// Keeps `design` in `folder`.
///
/// # Errors
/// When the file cannot be written.
pub fn store_design(folder: &Path, design: &Design) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let text = serde_json::to_string_pretty(&design.to_json()).map_err(std::io::Error::other)?;
    std::fs::write(design_path(folder, &design.name), text + "\n")
}

/// Takes the stamp called `name` out of `folder`.
///
/// # Errors
/// When the file is there but cannot be removed.
pub fn remove_design(folder: &Path, name: &str) -> std::io::Result<()> {
    match std::fs::remove_file(design_path(folder, name)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Changes one setting beside the tools at `tools`, keeping the others.
fn update_settings(tools: &Path, key: &str, value: Json) -> std::io::Result<()> {
    if let Some(folder) = tools.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let path = tools.with_file_name("settings.json");
    let mut json = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<Json>(&t).ok())
        .filter(Json::is_object)
        .unwrap_or_else(|| json!({}));
    json[key] = value;
    let text = serde_json::to_string_pretty(&json).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
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
    update_settings(tools, "snap", json!(settings.snap))?;
    update_settings(tools, "wheelZooms", json!(settings.wheel_zooms))
}

/// The stamps of the person's own, kept beside the tools at `tools`: their
/// words and color; none when there are none or they cannot be read.
#[must_use]
pub fn load_stamps(tools: &Path) -> Vec<(String, [f64; 3])> {
    let Ok(text) = std::fs::read_to_string(tools.with_file_name("stamps.json")) else {
        return Vec::new();
    };
    let Ok(Json::Array(items)) = serde_json::from_str::<Json>(&text) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let words = item.get("words")?.as_str()?.trim();
            let color = item.get("color")?.as_array()?;
            let channel =
                |i: usize| -> Option<f64> { Some(color.get(i)?.as_f64()?.clamp(0.0, 1.0)) };
            (!words.is_empty()).then(|| {
                Some((
                    words.chars().take(stamps::MAX_WORDS).collect(),
                    [channel(0)?, channel(1)?, channel(2)?],
                ))
            })?
        })
        .collect()
}

/// Keeps `own` stamps beside the tools at `tools`.
///
/// # Errors
/// When the file cannot be written.
pub fn store_stamps(tools: &Path, own: &[(String, [f64; 3])]) -> std::io::Result<()> {
    if let Some(folder) = tools.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let items: Vec<Json> = own
        .iter()
        .map(|(words, color)| json!({ "words": words, "color": color }))
        .collect();
    let text = serde_json::to_string_pretty(&items).map_err(std::io::Error::other)?;
    std::fs::write(tools.with_file_name("stamps.json"), text)
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
        stamp: item
            .get("stamp")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

fn tool_json(t: &SavedTool) -> Json {
    json!({
        "name": t.name,
        "kind": t.kind.key(),
        "color": t.color,
        "width": t.width,
        "fill": t.fill,
        "subject": t.subject,
        "stamp": t.stamp,
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
    let items: Vec<Json> = tools.iter().map(tool_json).collect();
    let text = serde_json::to_string_pretty(&Json::Array(items)).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_stamps_are_kept_and_read_back() {
        let folder = std::env::temp_dir().join(format!("oc-stamps-{}", std::process::id()));
        let tools = folder.join("tools.json");
        assert_eq!(load_stamps(&tools).len(), 0);
        let own = vec![("CHECKED BY {name}".to_owned(), [0.1, 0.2, 0.3])];
        store_stamps(&tools, &own).unwrap();
        assert_eq!(load_stamps(&tools), own);
        std::fs::write(
            folder.join("stamps.json"),
            "[{\"words\": \"  \", \"color\": [0,0,0]}]",
        )
        .unwrap();
        assert_eq!(load_stamps(&tools).len(), 0);
        std::fs::remove_dir_all(&folder).ok();
    }

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
            stamp: String::new(),
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
