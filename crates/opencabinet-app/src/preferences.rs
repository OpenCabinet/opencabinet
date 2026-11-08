// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user's preferences, kept in the user's folder beside the shop template.
//! They belong to the user, not to a job, so no project file carries them.
//!
//! The file is a JSON object. What this version does not know is kept as it
//! is when the file is saved again, so that a later version's preferences
//! survive this one.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

const FILE_NAME: &str = "preferences.json";
const THEME: &str = "theme";

/// How the window looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Light or dark, as the system is, changing when it changes.
    #[default]
    System,
    /// Dark text on a light background.
    Light,
    /// Light text on a dark background.
    Dark,
}

impl Theme {
    /// Every theme, in the order of the menu.
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// The user's preferences.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preferences {
    /// How the window looks.
    pub theme: Theme,
    /// What the file holds that this version does not know.
    others: Map<String, Value>,
}

/// Where the preferences live in the user's `folder`.
#[must_use]
pub fn path_in(folder: &Path) -> PathBuf {
    folder.join(FILE_NAME)
}

impl Preferences {
    /// Reads the preferences at `path`. Without a file they are the usual
    /// ones; with a file that cannot be used they are the usual ones too, and
    /// the second value says why, so that the user can be told.
    #[must_use]
    pub fn read(path: &Path) -> (Self, Option<String>) {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return (Self::default(), None),
            Err(e) => return (Self::default(), Some(e.to_string())),
        };
        let mut others = match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(object)) => object,
            Ok(_) => return (Self::default(), Some("it is not a JSON object".into())),
            Err(e) => return (Self::default(), Some(e.to_string())),
        };
        let theme = match others.get(THEME) {
            None => Theme::default(),
            Some(value) => {
                let known = Theme::ALL
                    .into_iter()
                    .find(|t| value.as_str() == Some(t.key()));
                let Some(theme) = known else {
                    let why = format!("the theme {value} is not known");
                    // It stays in `others`, so that saving another preference
                    // does not lose a theme a later version wrote.
                    let theme = Theme::default();
                    return (Self { theme, others }, Some(why));
                };
                others.remove(THEME);
                theme
            }
        };
        (Self { theme, others }, None)
    }

    /// Saves the preferences at `path`, creating its folder. The file is
    /// written beside it first and then put in its place, so that a save
    /// that fails halfway leaves the previous file whole.
    ///
    /// # Errors
    /// When the folder or the file cannot be written.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        let mut object = self.others.clone();
        object.insert(THEME.into(), Value::from(self.theme.key()));
        let mut text =
            serde_json::to_string_pretty(&Value::Object(object)).map_err(io::Error::other)?;
        text.push('\n');
        let partial = path.with_extension("json.partial");
        fs::write(&partial, text)?;
        fs::rename(&partial, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join("opencabinet-preferences-tests")
            .join(name);
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn without_a_file_the_window_follows_the_system() {
        let path = path_in(&folder("absent"));
        assert_eq!(Preferences::read(&path), (Preferences::default(), None));
        assert_eq!(Preferences::default().theme, Theme::System);
    }

    #[test]
    fn a_saved_theme_is_read_back() {
        let path = path_in(&folder("nested").join("user folder"));
        for theme in Theme::ALL {
            let preferences = Preferences {
                theme,
                ..Preferences::default()
            };
            preferences.save(&path).unwrap();
            assert_eq!(Preferences::read(&path), (preferences, None));
        }
        assert!(!path.with_extension("json.partial").exists());
    }

    #[test]
    fn a_damaged_file_gives_the_usual_preferences_and_says_why() {
        let folder = folder("damaged");
        fs::create_dir_all(&folder).unwrap();
        let path = path_in(&folder);
        for text in ["{ not json", "[\"dark\"]"] {
            fs::write(&path, text).unwrap();
            let (preferences, why) = Preferences::read(&path);
            assert_eq!(preferences, Preferences::default());
            assert!(why.is_some(), "{text}");
        }
    }

    #[test]
    fn what_a_later_version_wrote_survives_a_save() {
        let folder = folder("later");
        fs::create_dir_all(&folder).unwrap();
        let path = path_in(&folder);
        fs::write(&path, r#"{"theme": "contrast", "zoom": 2}"#).unwrap();
        let (mut preferences, why) = Preferences::read(&path);
        assert_eq!(preferences.theme, Theme::System);
        assert!(why.unwrap().contains("contrast"));

        // Choosing a theme replaces the unknown one, and keeps the rest.
        preferences.theme = Theme::Dark;
        preferences.save(&path).unwrap();
        let saved: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved, serde_json::json!({"theme": "dark", "zoom": 2}));
    }
}
