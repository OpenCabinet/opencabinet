// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shop template: how a shop works, kept in the user's folder so that new
//! projects start from it. It is an ordinary project file holding the library,
//! the units and one usual cabinet that records which material each part
//! takes, and nothing of a job: no other cabinets, walls, notes or name.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use opencabinet_format::Document;
use opencabinet_model::{Id, Library, Method, Project, Settings, Units};

use super::{Designer, new_standard, standard, usual_cabinet};
use crate::NAME_AND_VERSION;

const FILE_NAME: &str = "shop-template.ocab";

/// The name of a standard of `method` added to a template that lacks one.
fn general_name(method: Method) -> &'static str {
    match method {
        Method::Frameless => "Frameless",
        Method::FaceFrame => "Face frame",
    }
}

/// Where the template lives in the user's `folder`.
#[must_use]
pub fn path_in(folder: &Path) -> PathBuf {
    folder.join(FILE_NAME)
}

/// Where the template a save or a removal replaces is kept.
#[must_use]
pub fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("bak.ocab")
}

/// What the user's folder holds.
#[derive(Debug)]
pub enum Template {
    /// No template: new projects take the general values.
    Absent,
    /// A template to start from, with at least one cabinet.
    Found(Box<Document>),
    /// A file that cannot be used, and why. It is left as it is.
    Unreadable(String),
}

impl Template {
    /// Reads the template at `path`.
    #[must_use]
    pub fn read(path: &Path) -> Self {
        if !path.exists() {
            return Self::Absent;
        }
        match Document::load(path) {
            Ok(document) if document.project.products.is_empty() => {
                Self::Unreadable("it has no cabinet".into())
            }
            Ok(document) => Self::Found(Box::new(document)),
            Err(error) => Self::Unreadable(error.to_string()),
        }
    }
}

/// Sets the versions of the items of `library` that `previous` also has: the
/// same when the content is the same, the next one when it changed.
fn renumber(library: &mut Library, previous: &Library) {
    renumber_items(
        &mut library.materials,
        &previous.materials,
        |m| m.id,
        |m| &mut m.version,
    );
    renumber_items(
        &mut library.edge_bands,
        &previous.edge_bands,
        |b| b.id,
        |b| &mut b.version,
    );
    renumber_items(
        &mut library.hardware,
        &previous.hardware,
        |h| h.id,
        |h| &mut h.version,
    );
    renumber_items(
        &mut library.standards,
        &previous.standards,
        |s| s.id,
        |s| &mut s.version,
    );
}

fn renumber_items<T: Clone + PartialEq>(
    items: &mut [T],
    previous: &[T],
    id: fn(&T) -> Id,
    version: fn(&mut T) -> &mut u32,
) {
    for item in items {
        let Some(earlier) = previous.iter().find(|p| id(p) == id(item)) else {
            continue;
        };
        let mut earlier = earlier.clone();
        let latest = (*version(&mut earlier)).max(*version(item));
        // Only the content counts: a project may hold an older version of
        // what the template has.
        *version(&mut earlier) = *version(item);
        let changed = earlier != *item;
        *version(item) = if changed { latest + 1 } else { latest };
    }
}

/// The standard of `method` a new project's cabinet takes: the template's
/// default when it is built that way, else the first that is. A template
/// without one gains the general standard, so both kinds of project start.
fn standard_for(library: &mut Library, method: Method, default: Id) -> Id {
    let of_method = |id: Option<Id>| {
        library
            .standards
            .iter()
            .find(|s| s.method == method && id.is_none_or(|id| s.id == id))
            .map(|s| s.id)
    };
    if let Some(id) = of_method(Some(default)).or_else(|| of_method(None)) {
        return id;
    }
    let existing = library
        .hardware
        .iter()
        .find(|h| h.category == "hinge")
        .map(|h| h.id);
    let hinge = existing.unwrap_or_else(|| {
        let hinge = standard::new_hinge();
        let id = hinge.id;
        library.hardware.push(hinge);
        id
    });
    let added = new_standard(general_name(method), method, hinge);
    let id = added.id;
    library.standards.push(added);
    id
}

/// How the template's cabinet is built.
fn usual_method(template: &Document) -> Option<Method> {
    let default = template.project.settings.default_standard;
    let standards = &template.library.standards;
    let standard = standards.iter().find(|s| s.id == default)?;
    Some(standard.method)
}

impl Designer {
    /// A new, unsaved project from the template at `path`, or with the general
    /// values when there is none, and what the user should be told about it.
    /// Its cabinet is built by `method`; when the user did not choose one, as
    /// when the program starts, the way the template's cabinet is built.
    #[must_use]
    pub fn starting(path: Option<&Path>, units: Units, method: Option<Method>) -> (Self, String) {
        let general = method.unwrap_or(Method::Frameless);
        match path.map_or(Template::Absent, Template::read) {
            Template::Absent => (Self::with_method(units, general), String::new()),
            Template::Found(template) => {
                let method = method
                    .or_else(|| usual_method(&template))
                    .unwrap_or(general);
                (
                    Self::from_template(&template, method),
                    "New project from your shop template".into(),
                )
            }
            Template::Unreadable(why) => (
                Self::with_method(units, general),
                format!(
                    "Your shop template cannot be read, so this project has the general values: {why}"
                ),
            ),
        }
    }

    /// A new, unsaved project with the template's library and units, whose
    /// one cabinet is built by `method`. What it takes from the library
    /// keeps its identity, so that a later version can be recognized.
    #[must_use]
    pub fn from_template(template: &Document, method: Method) -> Self {
        let mut library = template.library.clone();
        let settings = &template.project.settings;
        let standard = standard_for(&mut library, method, settings.default_standard);
        let materials = template
            .project
            .products
            .first()
            .map_or_else(BTreeMap::new, |p| p.materials.clone());
        let project = Project {
            id: Id::new(),
            name: "Untitled".into(),
            notes: String::new(),
            settings: Settings {
                default_standard: standard,
                ..settings.clone()
            },
            rooms: Vec::new(),
            products: vec![usual_cabinet(standard, materials)],
            revisions: Vec::new(),
            extensions: BTreeMap::default(),
            unknown: BTreeMap::default(),
        };
        let mut document = Document::new(project, library);
        document.generator = NAME_AND_VERSION.into();
        Self::around(document)
    }

    /// Saves this project's library and units, and the materials of the edited
    /// cabinet, as the template at `path`. A template already there is kept as
    /// a backup, and what changed in the library since then gets a new version.
    ///
    /// # Errors
    /// A message for the user when the file cannot be written; the template
    /// already there is then left as it was.
    pub fn save_template(&self, path: &Path) -> Result<(), String> {
        let mut library = self.document.library.clone();
        if let Template::Found(previous) = Template::read(path) {
            renumber(&mut library, &previous.library);
        }
        let edited = self.product();
        let project = Project {
            id: Id::new(),
            name: "Shop template".into(),
            notes: String::new(),
            settings: Settings {
                default_standard: edited.standard,
                ..self.document.project.settings.clone()
            },
            rooms: Vec::new(),
            products: vec![usual_cabinet(edited.standard, edited.materials.clone())],
            revisions: Vec::new(),
            extensions: BTreeMap::default(),
            unknown: BTreeMap::default(),
        };
        let mut document = Document::new(project, library);
        document.generator = NAME_AND_VERSION.into();
        let failed = |e: String| format!("Cannot save the shop template: {e}");
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder).map_err(|e| failed(e.to_string()))?;
        }
        if path.exists() {
            fs::copy(path, backup_path(path)).map_err(|e| failed(e.to_string()))?;
        }
        document.save_zip(path).map_err(|e| failed(e.to_string()))
    }
}

/// Stops using the template at `path`, keeping it as the backup.
///
/// # Errors
/// A message for the user when the file cannot be moved.
pub fn remove(path: &Path) -> Result<(), String> {
    fs::rename(path, backup_path(path)).map_err(|e| format!("Cannot remove the shop template: {e}"))
}

#[cfg(test)]
mod tests {
    use opencabinet_eval::evaluate;
    use opencabinet_format::FormatVersion;
    use opencabinet_model::Length;

    use super::*;

    /// A file in a new, empty folder of its own.
    fn template_path(test: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "opencabinet-template-{}-{test}",
            std::process::id()
        ));
        if folder.exists() {
            fs::remove_dir_all(&folder).unwrap();
        }
        // Saving makes the folder when the user's has none yet.
        folder.join("nested").join(FILE_NAME)
    }

    fn found(path: &Path) -> Document {
        match Template::read(path) {
            Template::Found(template) => *template,
            other => panic!("expected a template, got {other:?}"),
        }
    }

    fn new_project(path: &Path, method: Method) -> Designer {
        Designer::starting(Some(path), Units::Imperial, Some(method)).0
    }

    /// How the edited cabinet is built.
    fn method_of(designer: &Designer) -> Option<Method> {
        let library = &designer.document.library;
        let id = designer.product().standard;
        library
            .standards
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.method)
    }

    #[test]
    fn a_template_keeps_how_the_shop_works_and_nothing_of_the_job() {
        let path = template_path("job");
        let mut designer = Designer::new(Units::Metric);
        designer.document.project.name = "Smith kitchen".into();
        designer.document.project.notes = "Call before delivery".into();
        designer.document.library.materials[0].name = "Our plywood".into();
        designer.add();
        designer.save_template(&path).unwrap();

        let template = found(&path);
        assert_eq!(template.project.notes, "");
        assert_eq!(template.project.products.len(), 1);
        assert_eq!(template.project.rooms.len(), 0);

        let (started, told) =
            Designer::starting(Some(&path), Units::Imperial, Some(Method::Frameless));
        assert_eq!(told, "New project from your shop template");
        let project = &started.document.project;
        assert_eq!(project.name, "Untitled");
        assert_eq!(project.notes, "");
        assert_eq!(project.products.len(), 1);
        assert_eq!(project.products[0].label, "B1");
        assert_eq!(project.products[0].width, Length::from_inches(30));
        assert_ne!(project.id, designer.document.project.id);
        assert_eq!(project.settings.units, Units::Metric);
        assert!(!started.modified());
        // The library keeps its identity, for the update offers to come.
        assert_eq!(
            started.document.library.materials[0].id,
            designer.document.library.materials[0].id
        );
        assert_eq!(started.document.library.materials[0].name, "Our plywood");
    }

    #[test]
    fn a_template_takes_the_materials_of_the_edited_cabinet() {
        let path = template_path("materials");
        let mut designer = Designer::new(Units::Imperial);
        designer.add();
        let thin = designer.document.library.materials[1].id;
        designer.document.project.products[1]
            .materials
            .insert("case".into(), thin);
        designer.selected = 1;
        designer.save_template(&path).unwrap();
        let started = new_project(&path, Method::Frameless);
        assert_eq!(started.product().materials["case"], thin);

        // Another cabinet is not what was asked for.
        designer.selected = 0;
        designer.save_template(&path).unwrap();
        let started = new_project(&path, Method::Frameless);
        assert_ne!(started.product().materials["case"], thin);
    }

    #[test]
    fn a_new_project_is_built_the_way_it_was_asked() {
        let path = template_path("method");
        let designer = Designer::with_method(Units::Imperial, Method::Frameless);
        designer.save_template(&path).unwrap();
        for method in [Method::Frameless, Method::FaceFrame] {
            let started = new_project(&path, method);
            assert_eq!(method_of(&started), Some(method));
        }
    }

    #[test]
    fn the_program_starts_the_way_the_template_builds() {
        let path = template_path("startup");
        let designer = Designer::with_method(Units::Imperial, Method::FaceFrame);
        designer.save_template(&path).unwrap();
        let (started, _) = Designer::starting(Some(&path), Units::Imperial, None);
        assert_eq!(method_of(&started), Some(Method::FaceFrame));
        let (started, _) = Designer::starting(None, Units::Imperial, None);
        assert_eq!(method_of(&started), Some(Method::Frameless));
    }

    #[test]
    fn a_template_without_a_method_gains_its_general_standard() {
        let path = template_path("gains");
        let mut designer = Designer::new(Units::Imperial);
        let library = &mut designer.document.library;
        library.standards.retain(|s| s.method == Method::Frameless);
        designer.save_template(&path).unwrap();

        let started = new_project(&path, Method::FaceFrame);
        assert_eq!(method_of(&started), Some(Method::FaceFrame));
        let library = &started.document.library;
        assert_eq!(library.standards.len(), 2);
        assert_eq!(library.hardware.len(), 1);
        let blocking = evaluate(&started.document.project, library)
            .problems
            .into_iter()
            .filter(|p| p.blocking)
            .count();
        assert_eq!(blocking, 0);

        // Nor does it lack a hinge to name.
        let mut bare = Library::default();
        let id = standard_for(&mut bare, Method::FaceFrame, Id::new());
        assert_eq!(bare.standards[0].id, id);
        assert_eq!(bare.hardware[0].category, "hinge");
    }

    #[test]
    fn a_new_project_prefers_the_templates_default_standard() {
        let mut library = Designer::new(Units::Imperial).document.library;
        let mut second = library.standards[0].clone();
        second.id = Id::new();
        library.standards.push(second.clone());
        assert_eq!(
            standard_for(&mut library, Method::Frameless, second.id),
            second.id
        );
        assert_eq!(
            standard_for(&mut library, Method::Frameless, Id::new()),
            library.standards[0].id
        );
        assert_eq!(library.standards.len(), 3);
    }

    #[test]
    fn only_what_changed_gets_a_new_version() {
        let path = template_path("versions");
        let mut designer = Designer::new(Units::Imperial);
        designer.save_template(&path).unwrap();
        let library = |path: &Path| found(path).library;
        assert_eq!(library(&path).materials[0].version, 1);

        // Saving the same again changes nothing.
        designer.save_template(&path).unwrap();
        assert_eq!(library(&path).materials[0].version, 1);

        designer.document.library.materials[0].name = "Our plywood".into();
        designer.document.library.standards[0].name = "Ours".into();
        designer.save_template(&path).unwrap();
        let saved = library(&path);
        assert_eq!(saved.materials[0].version, 2);
        assert_eq!(saved.materials[1].version, 1);
        assert_eq!(saved.standards[0].version, 2);
        assert_eq!(saved.standards[1].version, 1);
        assert_eq!(saved.edge_bands[0].version, 1);
        assert_eq!(saved.hardware[0].version, 1);

        // A project started before keeps version 1 of what it holds; what it
        // holds still differs, so the template's version moves on.
        designer.document.library.materials[0].name = "Better plywood".into();
        designer.save_template(&path).unwrap();
        assert_eq!(library(&path).materials[0].version, 3);

        // A project started from version 3 that changes nothing stays on it.
        let started = new_project(&path, Method::Frameless);
        assert_eq!(started.document.library.materials[0].version, 3);
        started.save_template(&path).unwrap();
        assert_eq!(library(&path).materials[0].version, 3);
    }

    #[test]
    fn an_unreadable_template_is_told_and_left_alone() {
        let path = template_path("damaged");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"not a project").unwrap();

        let (started, told) =
            Designer::starting(Some(&path), Units::Metric, Some(Method::FaceFrame));
        assert!(told.contains("cannot be read"), "{told}");
        assert_eq!(method_of(&started), Some(Method::FaceFrame));
        assert_eq!(started.document.project.settings.units, Units::Metric);
        assert_eq!(fs::read(&path).unwrap(), b"not a project");

        // Only saving another replaces it, and the backup still holds it.
        started.save_template(&path).unwrap();
        found(&path);
        assert_eq!(fs::read(backup_path(&path)).unwrap(), b"not a project");
    }

    #[test]
    fn a_template_from_a_newer_version_is_not_read() {
        let path = template_path("newer");
        let designer = Designer::new(Units::Imperial);
        designer.save_template(&path).unwrap();
        let mut template = found(&path);
        template.format_version = FormatVersion {
            major: opencabinet_format::CURRENT.major + 1,
            minor: 0,
        };
        template.save_zip(&path).unwrap();
        let before = fs::read(&path).unwrap();
        assert!(matches!(Template::read(&path), Template::Unreadable(_)));
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn a_template_without_a_cabinet_is_not_read() {
        let path = template_path("empty");
        let designer = Designer::new(Units::Imperial);
        designer.save_template(&path).unwrap();
        let mut template = found(&path);
        template.project.products.clear();
        template.save_zip(&path).unwrap();
        assert!(matches!(Template::read(&path), Template::Unreadable(_)));
    }

    #[test]
    fn no_template_gives_the_general_values_and_says_nothing() {
        let path = template_path("absent");
        let (started, told) =
            Designer::starting(Some(&path), Units::Metric, Some(Method::Frameless));
        assert_eq!(told, "");
        assert_eq!(started.document.library.standards.len(), 2);
        assert_eq!(started.document.project.settings.units, Units::Metric);
        let (_, told) = Designer::starting(None, Units::Metric, None);
        assert_eq!(told, "");
    }

    #[test]
    fn saving_and_removing_keep_the_previous_template() {
        let path = template_path("backup");
        let mut designer = Designer::new(Units::Imperial);
        designer.document.library.materials[0].name = "First".into();
        designer.save_template(&path).unwrap();
        assert!(!backup_path(&path).exists());

        designer.document.library.materials[0].name = "Second".into();
        designer.save_template(&path).unwrap();
        assert_eq!(
            found(&backup_path(&path)).library.materials[0].name,
            "First"
        );

        remove(&path).unwrap();
        assert!(matches!(Template::read(&path), Template::Absent));
        assert_eq!(
            found(&backup_path(&path)).library.materials[0].name,
            "Second"
        );
        // Nothing left to remove.
        assert!(remove(&path).is_err());
    }
}
