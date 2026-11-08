// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user's folder: where the program keeps what belongs to the user rather
//! than to a project, such as the shop template and the preferences.

use std::ffi::OsString;
use std::path::PathBuf;

/// This user's folder; `None` when the system does not say where it is.
#[must_use]
pub fn usual() -> Option<PathBuf> {
    from(|name| std::env::var_os(name))
}

/// The folder, given the environment variables. A relative path is refused,
/// because it would depend on where the program was started.
fn from(variable: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let absolute = |name: &str| {
        variable(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    if cfg!(windows) {
        Some(absolute("APPDATA")?.join("OpenCabinet"))
    } else if cfg!(target_os = "macos") {
        Some(absolute("HOME")?.join("Library/Application Support/OpenCabinet"))
    } else {
        Some(
            absolute("XDG_CONFIG_HOME")
                .or_else(|| absolute("HOME").map(|home| home.join(".config")))?
                .join("opencabinet"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_comes_from_the_system() {
        let base = std::env::temp_dir();
        let folder = from(|_| Some(base.clone().into_os_string())).unwrap();
        assert!(folder.starts_with(&base));
        assert_ne!(folder, base);
        assert_eq!(from(|_| None), None);
        assert_eq!(from(|_| Some("relative".into())), None);
    }
}
