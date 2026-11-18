// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `.ocab` container: a ZIP file or an unpacked directory with the same
//! entries (RFC-0001 §13).
//!
//! Saving is deterministic: the same document always produces the same
//! bytes, so projects diff cleanly in version control.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::error::{Error, Result};
use opencabinet_model::{Library, Project, Unknown};

/// The media type of a project file, also the content of its `mimetype`
/// entry.
pub const MIMETYPE: &str = "application/vnd.opencabinet.project+zip";

/// The format version this crate writes.
pub const CURRENT: FormatVersion = FormatVersion { major: 0, minor: 1 };

const MIMETYPE_ENTRY: &str = "mimetype";
const MANIFEST: &str = "manifest.json";
const PROJECT: &str = "project.json";
const LIBRARY: &str = "library.json";
const JSON: &str = "application/json";
const OCTET_STREAM: &str = "application/octet-stream";

/// Limits that keep a hostile file from exhausting memory.
const MAX_ENTRIES: usize = 10_000;
const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

/// A whole project file in memory.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    /// The version the file was read with; saving never lowers it, so a
    /// file from a newer minor version is not relabeled as an older one.
    pub format_version: FormatVersion,
    /// The program that wrote the file, as `name version`. Set it to the
    /// saving program before saving.
    pub generator: String,
    /// The project.
    pub project: Project,
    /// The library snapshots the project uses.
    pub library: Library,
    /// Entries this version does not interpret (attachments, plugin data),
    /// kept byte for byte.
    pub attachments: BTreeMap<String, Attachment>,
    /// Manifest fields from a newer version, kept on save.
    pub manifest_unknown: Unknown,
    /// Problems with the container that do not prevent reading it.
    pub warnings: Vec<String>,
}

/// An entry kept as it was read: an image, a PDF or a plugin's data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    /// Its media type, from the manifest.
    pub media_type: String,
    /// Its content.
    pub bytes: Vec<u8>,
}

/// A format version, `major.minor`. A reader rejects a newer major
/// version and reads a newer minor one, keeping what it does not know.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FormatVersion {
    /// Changes that older readers cannot read.
    pub major: u32,
    /// Additions older readers can keep without understanding.
    pub minor: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format_version: FormatVersion,
    generator: String,
    entries: Vec<ManifestEntry>,
    #[serde(flatten)]
    unknown: Unknown,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestEntry {
    path: String,
    media_type: String,
}

impl Document {
    /// A new document for a project and its library.
    #[must_use]
    pub fn new(project: Project, library: Library) -> Self {
        Self {
            format_version: CURRENT,
            generator: concat!("OpenCabinet ", env!("CARGO_PKG_VERSION")).into(),
            project,
            library,
            attachments: BTreeMap::new(),
            manifest_unknown: Unknown::new(),
            warnings: Vec::new(),
        }
    }

    /// Loads a project from a `.ocab` ZIP file or directory.
    ///
    /// # Errors
    /// When the file cannot be read, is not a project file, needs a newer
    /// major format version, or could reach outside the project.
    pub fn load(path: &Path) -> Result<Self> {
        let mut warnings = Vec::new();
        let entries = if path.is_dir() {
            read_dir(path)?
        } else {
            read_zip(path, &mut warnings)?
        };
        let mut document = from_entries(entries)?;
        document.warnings.splice(0..0, warnings);
        Ok(document)
    }

    /// Saves as a ZIP file, through a temporary file so that a failed save
    /// never destroys the previous version.
    ///
    /// # Errors
    /// When the file cannot be written.
    pub fn save_zip(&self, path: &Path) -> Result<()> {
        let mut temporary = path.as_os_str().to_owned();
        temporary.push(".partial");
        let temporary = Path::new(&temporary);
        fs::write(temporary, self.to_zip_bytes()?)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    /// The ZIP file's bytes.
    ///
    /// # Errors
    /// When the project cannot be serialized.
    pub fn to_zip_bytes(&self) -> Result<Vec<u8>> {
        let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in self.entries()? {
            let method = if name == MIMETYPE_ENTRY {
                CompressionMethod::Stored
            } else {
                CompressionMethod::Deflated
            };
            let options = SimpleFileOptions::default()
                .compression_method(method)
                .last_modified_time(DateTime::default())
                .unix_permissions(0o644);
            zip.start_file(name, options)?;
            zip.write_all(&bytes)?;
        }
        Ok(zip.finish()?.into_inner())
    }

    /// Saves as an unpacked directory, which must not exist yet or be empty.
    ///
    /// Saving over an existing directory would have to decide what to do
    /// with files the user added there; that is left to the specification.
    ///
    /// # Errors
    /// When the directory is not empty or cannot be written.
    pub fn save_dir(&self, path: &Path) -> Result<()> {
        if path.exists() && fs::read_dir(path)?.next().is_some() {
            return Err(Error::InvalidContainer(format!(
                "{} is not empty",
                path.display()
            )));
        }
        for (name, bytes) in self.entries()? {
            let file = path.join(&name);
            if let Some(parent) = file.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(file, bytes)?;
        }
        Ok(())
    }

    /// The entries in the order they are written: `mimetype` first.
    ///
    /// # Errors
    /// When the project cannot be serialized.
    pub fn entries(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let mut listed = vec![
            ManifestEntry {
                path: PROJECT.into(),
                media_type: JSON.into(),
            },
            ManifestEntry {
                path: LIBRARY.into(),
                media_type: JSON.into(),
            },
        ];
        listed.extend(self.attachments.iter().map(|(path, a)| ManifestEntry {
            path: path.clone(),
            media_type: a.media_type.clone(),
        }));
        let manifest = Manifest {
            format_version: self.format_version.max(CURRENT),
            generator: self.generator.clone(),
            entries: listed,
            unknown: self.manifest_unknown.clone(),
        };

        let mut entries = vec![
            (MIMETYPE_ENTRY.into(), MIMETYPE.as_bytes().to_vec()),
            (MANIFEST.into(), to_json(&manifest)?),
            (PROJECT.into(), to_json(&self.project)?),
            (LIBRARY.into(), to_json(&self.library)?),
        ];
        entries.extend(
            self.attachments
                .iter()
                .map(|(path, a)| (path.clone(), a.bytes.clone())),
        );
        Ok(entries)
    }
}

/// Pretty JSON with a final newline: readable and stable in diffs.
fn to_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn from_entries(mut entries: BTreeMap<String, Vec<u8>>) -> Result<Document> {
    let mut take = |name: &str| {
        entries
            .remove(name)
            .ok_or_else(|| Error::InvalidContainer(format!("missing entry `{name}`")))
    };

    if take(MIMETYPE_ENTRY)? != MIMETYPE.as_bytes() {
        return Err(Error::InvalidContainer("wrong mimetype".into()));
    }
    let manifest_bytes = take(MANIFEST)?;
    let version = serde_json::from_slice::<serde_json::Value>(&manifest_bytes)?
        .get("formatVersion")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    // Check the version before the rest of the manifest, whose shape a new
    // major version may change.
    match version.parse::<FormatVersion>() {
        Ok(v) if v.major == CURRENT.major => {}
        Ok(_) => return Err(Error::UnsupportedVersion(version)),
        Err(()) => {
            return Err(Error::InvalidContainer(format!(
                "bad formatVersion `{version}`"
            )));
        }
    }
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;
    let project = serde_json::from_slice(&take(PROJECT)?)?;
    let library = serde_json::from_slice(&take(LIBRARY)?)?;

    let mut media_types: BTreeMap<String, String> = manifest
        .entries
        .into_iter()
        .filter(|e| e.path != PROJECT && e.path != LIBRARY)
        .map(|e| (e.path, e.media_type))
        .collect();
    let mut warnings = Vec::new();
    let attachments = entries
        .into_iter()
        .map(|(path, bytes)| {
            let media_type = media_types.remove(&path).unwrap_or_else(|| {
                warnings.push(format!("entry `{path}` is not in the manifest"));
                OCTET_STREAM.into()
            });
            (path, Attachment { media_type, bytes })
        })
        .collect();
    if let Some(path) = media_types.keys().next() {
        return Err(Error::InvalidContainer(format!(
            "manifest lists missing entry `{path}`"
        )));
    }

    Ok(Document {
        format_version: manifest.format_version,
        generator: manifest.generator,
        project,
        library,
        attachments,
        manifest_unknown: manifest.unknown,
        warnings,
    })
}

fn read_zip(path: &Path, warnings: &mut Vec<String>) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut archive = ZipArchive::new(File::open(path)?)?;
    if archive.len() > MAX_ENTRIES {
        return Err(Error::InvalidContainer("too many entries".into()));
    }
    let mut entries = BTreeMap::new();
    let mut total = 0;
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name()?.into_owned();
        if i == 0 && (name != MIMETYPE_ENTRY || file.compression() != CompressionMethod::Stored) {
            warnings.push("`mimetype` is not the first, uncompressed entry".into());
        }
        check_entry_name(&name)?;
        // Read one byte past the limit: sizes in the ZIP header can lie.
        let mut bytes = Vec::new();
        file.take(MAX_TOTAL_BYTES - total + 1)
            .read_to_end(&mut bytes)?;
        total += bytes.len() as u64;
        if total > MAX_TOTAL_BYTES {
            return Err(Error::InvalidContainer("contents too large".into()));
        }
        if entries.insert(name.clone(), bytes).is_some() {
            return Err(Error::InvalidContainer(format!("duplicate entry `{name}`")));
        }
    }
    Ok(entries)
}

fn read_dir(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for item in fs::read_dir(&dir)? {
            let path = item?.path();
            // Never follow links: a cloned repository could point them at
            // files outside the project.
            let kind = fs::symlink_metadata(&path)?.file_type();
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                let name = path
                    .strip_prefix(root)
                    .expect("walked from root")
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                entries.insert(name, fs::read(&path)?);
            } else {
                return Err(Error::InvalidContainer(format!(
                    "{} is a link",
                    path.display()
                )));
            }
            if entries.len() > MAX_ENTRIES {
                return Err(Error::InvalidContainer("too many entries".into()));
            }
        }
    }
    Ok(entries)
}

/// Accepts only plain relative paths, so no entry can be written outside
/// the project when it is unpacked.
fn check_entry_name(name: &str) -> Result<()> {
    let bad = name.is_empty()
        || name.starts_with('/')
        || name.contains(['\\', ':', '\0'])
        || name.split('/').any(|part| matches!(part, "" | "." | ".."));
    if bad {
        return Err(Error::InvalidContainer(format!(
            "unsafe entry name `{name}`"
        )));
    }
    Ok(())
}

impl fmt::Display for FormatVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl FromStr for FormatVersion {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, ()> {
        let (major, minor) = s.split_once('.').ok_or(())?;
        Ok(Self {
            major: major.parse().map_err(|_| ())?,
            minor: minor.parse().map_err(|_| ())?,
        })
    }
}

impl Serialize for FormatVersion {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for FormatVersion {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        text.parse()
            .map_err(|()| serde::de::Error::custom(format!("bad formatVersion `{text}`")))
    }
}
