// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saving and reopening `.ocab` projects, against fixtures written by hand.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use opencabinet_format::{Document, Error, FormatVersion, MIMETYPE};
use opencabinet_model::validate::Severity;
use opencabinet_model::{InsertKind, Length, validate};
use serde_json::json;
use zip::write::SimpleFileOptions;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

/// A fresh, empty directory for one test.
fn scratch(test: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("opencabinet-format-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn errors(document: &Document) -> Vec<String> {
    validate(&document.project, &document.library)
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.to_string())
        .collect()
}

/// Writes a ZIP with the given entries, in order.
fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

/// The kitchen's entries with one replaced.
fn kitchen_entries_with(name: &str, bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let document = Document::load(&fixture("kitchen.ocab")).unwrap();
    let mut entries = document.entries().unwrap();
    entries.iter_mut().find(|(n, _)| n == name).unwrap().1 = bytes.to_vec();
    entries
}

fn as_refs(entries: &[(String, Vec<u8>)]) -> Vec<(&str, &[u8])> {
    entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect()
}

#[test]
fn the_sample_kitchen_is_valid() {
    let document = Document::load(&fixture("kitchen.ocab")).unwrap();
    assert_eq!(validate(&document.project, &document.library), []);
    assert_eq!(document.warnings, Vec::<String>::new());
    assert_eq!(document.project.products.len(), 7);
}

#[test]
fn saving_reproduces_the_fixtures_byte_for_byte() {
    for name in ["kitchen.ocab", "sample-kitchen.ocab", "future-minor.ocab"] {
        let document = Document::load(&fixture(name)).unwrap();
        let saved = scratch(&format!("canonical-{name}"));
        let saved = saved.join(name);
        document.save_dir(&saved).unwrap();
        for (entry, bytes) in document.entries().unwrap() {
            let original = fs::read(fixture(name).join(&entry)).unwrap();
            assert_eq!(bytes, original, "{name}: {entry} changed");
            assert_eq!(fs::read(saved.join(&entry)).unwrap(), original);
        }
    }
}

#[test]
fn zip_and_directory_forms_hold_the_same_project() {
    let from_dir = Document::load(&fixture("kitchen.ocab")).unwrap();
    let zip = scratch("forms").join("kitchen.ocab");
    from_dir.save_zip(&zip).unwrap();
    let from_zip = Document::load(&zip).unwrap();
    assert_eq!(from_zip, from_dir);
}

#[test]
fn saving_is_deterministic_and_puts_the_mimetype_first() {
    let document = Document::load(&fixture("kitchen.ocab")).unwrap();
    let bytes = document.to_zip_bytes().unwrap();
    assert_eq!(bytes, document.to_zip_bytes().unwrap());

    // The ZIP local header is 30 bytes; then the name, then the stored data.
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    assert_eq!(&bytes[8..10], [0, 0], "mimetype must be stored");
    assert_eq!(&bytes[30..38], b"mimetype");
    assert_eq!(&bytes[38..38 + MIMETYPE.len()], MIMETYPE.as_bytes());
}

#[test]
fn a_newer_minor_version_survives_a_save() {
    let document = Document::load(&fixture("future-minor.ocab")).unwrap();
    assert_eq!(
        document.format_version,
        FormatVersion { major: 0, minor: 2 }
    );
    assert_eq!(document.project.unknown["jobPhase"], json!("install"));
    assert!(document.library.unknown.contains_key("countertops"));
    assert!(
        document
            .attachments
            .contains_key("org.example.cnc-nesting/nest.json")
    );

    let insert = document.project.products[0]
        .front
        .as_ref()
        .and_then(|f| f.insert.as_ref())
        .unwrap();
    assert_eq!(insert.kind, InsertKind::Other("tambour".into()));
    assert_eq!(
        errors(&document),
        ["error: product T24: unknown insert `tambour`; open with a newer OpenCabinet"]
    );
    // The byte-for-byte test above shows the save keeps "0.2" and every
    // unknown field.
}

#[test]
fn a_newer_major_version_is_rejected() {
    let manifest = br#"{"formatVersion": "1.0", "layout": "changed"}"#;
    let zip = scratch("major").join("next.ocab");
    write_zip(
        &zip,
        &as_refs(&kitchen_entries_with("manifest.json", manifest)),
    );
    assert!(matches!(
        Document::load(&zip),
        Err(Error::UnsupportedVersion(v)) if v == "1.0"
    ));
}

#[test]
fn entries_that_could_escape_the_project_are_rejected() {
    let dir = scratch("unsafe");
    for name in [
        "../evil.txt",
        "/etc/evil",
        "a/../../evil",
        "C:/evil",
        "a\\evil",
    ] {
        let mut entries = kitchen_entries_with("mimetype", MIMETYPE.as_bytes());
        entries.push((name.into(), b"x".to_vec()));
        let zip = dir.join("unsafe.ocab");
        write_zip(&zip, &as_refs(&entries));
        let result = Document::load(&zip);
        assert!(
            matches!(&result, Err(Error::InvalidContainer(m)) if m.contains("unsafe")),
            "{name}: {result:?}"
        );
    }
}

#[test]
fn a_mimetype_out_of_place_and_unlisted_entries_are_warnings() {
    let mut entries = kitchen_entries_with("mimetype", MIMETYPE.as_bytes());
    entries.rotate_left(1);
    entries.push(("notes.txt".into(), b"call the client".to_vec()));
    let zip = scratch("lenient").join("rezipped.ocab");
    write_zip(&zip, &as_refs(&entries));

    let document = Document::load(&zip).unwrap();
    assert_eq!(
        document.warnings,
        [
            "`mimetype` is not the first, uncompressed entry",
            "entry `notes.txt` is not in the manifest",
        ]
    );
    assert_eq!(
        document.attachments["notes.txt"].media_type,
        "application/octet-stream"
    );
}

#[test]
fn a_listed_entry_that_is_missing_is_an_error() {
    let mut entries = kitchen_entries_with("mimetype", MIMETYPE.as_bytes());
    entries.retain(|(n, _)| n != "attachments/sketch.svg");
    let zip = scratch("missing").join("missing.ocab");
    write_zip(&zip, &as_refs(&entries));
    assert!(matches!(
        Document::load(&zip),
        Err(Error::InvalidContainer(m)) if m.contains("sketch.svg")
    ));
}

#[test]
fn an_edited_project_saves_and_reopens() {
    let mut document = Document::load(&fixture("kitchen.ocab")).unwrap();
    document.project.products[0].width = Length::from_inches(33);
    document.generator = "OpenCabinet test".into();
    let zip = scratch("edited").join("kitchen.ocab");
    document.save_zip(&zip).unwrap();
    // Saving again over the file replaces it.
    document.save_zip(&zip).unwrap();

    let reopened = Document::load(&zip).unwrap();
    assert_eq!(reopened.project.products[0].width, Length::from_inches(33));
    assert_eq!(reopened.generator, "OpenCabinet test");
    assert_eq!(reopened, document);
}
