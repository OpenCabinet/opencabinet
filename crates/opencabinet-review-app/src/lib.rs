// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! OpenCabinet Review's window (RFC-0004), as a library so tests can drive
//! it without a window system (see `tests/window.rs`).
//!
//! The page is drawn into an image only when what is shown of it changes;
//! the markups are drawn over it by the window, so drawing or moving one
//! never redraws the page.

pub mod chest;
pub mod dialogs;
pub mod icons;
pub mod viewer;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use opencabinet_model::Units;
use opencabinet_pdf::{View, render};
use slint::{
    CloseRequestResponse, Color, ComponentHandle, Image, Model, ModelRc, Rgba8Pixel,
    SharedPixelBuffer, SharedString, StandardListViewItem, VecModel,
};

use crate::dialogs::{Dialogs, FileKind, SystemDialogs};
use crate::viewer::{COLORS, COLUMNS, Follow, Keys, SheetName, TOOLS, Tool, Viewer, WIDTHS};
use opencabinet_review::sets::{self, SheetSet};
use opencabinet_review::stamps::{self, Design, Fields};
use opencabinet_review::{Kind, Status};

/// Shows the tool sets, the stamps' folder and the questions the stamp
/// placed next asks.
fn refresh_tool_sets(window: &ReviewWindow, viewer: &Viewer) {
    let current = viewer.current_set();
    let sets: Vec<ToolSetInfo> = viewer
        .tool_sets()
        .iter()
        .enumerate()
        .map(|(s, set)| {
            let tools: Vec<SavedInfo> = set
                .tools
                .iter()
                .map(|t| SavedInfo {
                    name: t.name.as_str().into(),
                    color: color(t.color),
                })
                .collect();
            ToolSetInfo {
                name: set.name.as_str().into(),
                collapsed: set.collapsed,
                current: s == current,
                tools: ModelRc::new(VecModel::from(tools)),
            }
        })
        .collect();
    window.set_tool_sets(ModelRc::new(VecModel::from(sets)));
    let (set, tool) = viewer.saved_in_use().map_or((-1, -1), |(s, t)| {
        (
            i32::try_from(s).unwrap_or(-1),
            i32::try_from(t).unwrap_or(-1),
        )
    });
    window.set_saved_set(set);
    window.set_saved_in_use(tool);
    window.set_stamps_folder(
        viewer
            .stamps_folder()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default()
            .into(),
    );
    let asks: Vec<AskInfo> = viewer
        .stamp_asks()
        .into_iter()
        .map(|(question, answer)| AskInfo {
            question: question.into(),
            answer: answer.into(),
        })
        .collect();
    let model = window.get_stamp_asks();
    // Answers being typed are not set again under the cursor.
    let same = model.row_count() == asks.len()
        && asks
            .iter()
            .enumerate()
            .all(|(i, a)| model.row_data(i).is_some_and(|r| r.question == a.question));
    if !same {
        window.set_stamp_asks(ModelRc::new(VecModel::from(asks)));
    }
}

fn connect_tool_sets(window: &ReviewWindow, app: &Rc<App>) {
    let a = Rc::clone(app);
    window.on_choose_set(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.choose_set(i));
        }
    });
    let a = Rc::clone(app);
    window.on_toggle_set(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.toggle_set(i));
        }
    });
    let a = Rc::clone(app);
    window.on_add_set(move |name| a.try_change(|v| v.add_set(&name)));
    let a = Rc::clone(app);
    window.on_rename_set(move |i, name| {
        if let Ok(i) = usize::try_from(i) {
            a.try_change(|v| v.rename_set(i, &name));
        }
    });
    let a = Rc::clone(app);
    window.on_remove_set(move |i| {
        let Ok(i) = usize::try_from(i) else {
            return;
        };
        let (name, count) = {
            let viewer = a.viewer.borrow();
            let set = viewer.tool_sets().get(i);
            (
                set.map(|s| s.name.clone()).unwrap_or_default(),
                set.map_or(0, |s| s.tools.len()),
            )
        };
        if count > 0
            && !a.dialogs.confirm(
                "Remove tool set",
                &format!("Remove the tool set {name} and its {count} tools?"),
            )
        {
            return;
        }
        a.try_change(|v| v.remove_set(i));
    });
    let a = Rc::clone(app);
    window.on_move_tool(move |set, i| {
        if let (Ok(set), Ok(i)) = (usize::try_from(set), usize::try_from(i)) {
            a.try_change(|v| {
                let to = (set + 1) % v.tool_sets().len().max(1);
                v.move_tool(set, i, to)
            });
        }
    });
    let a = Rc::clone(app);
    window.on_export_set(move |i| {
        let Ok(i) = usize::try_from(i) else {
            return;
        };
        let name = a
            .viewer
            .borrow()
            .tool_sets()
            .get(i)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        if let Some(path) = a
            .dialogs
            .save_path(&format!("{name}.octools"), FileKind::Tools)
        {
            a.try_change(|v| v.export_set(i, &path));
        }
    });
    window.on_import_sets(on_app(app, |app| {
        if let Some(path) = app.dialogs.open_kind(FileKind::Tools) {
            app.try_change(|v| v.import_sets(&path));
        }
    }));
    window.on_choose_stamps_folder(on_app(app, |app| {
        if let Some(folder) = app.dialogs.folder_path() {
            app.try_change(|v| v.set_stamps_folder(&folder));
        }
    }));
    let a = Rc::clone(app);
    window.on_set_ask(move |question, answer| {
        a.viewer.borrow_mut().set_ask(&question, &answer);
    });
}

fn connect_batch(window: &ReviewWindow, app: &Rc<App>) {
    window.on_set_open_all(on_app(app, |app| {
        app.open_set_sheets();
        app.refresh();
    }));
    window.on_set_link_sheets(on_app(app, |app| {
        app.open_set_sheets();
        let names = app.sheet_names();
        let made = app.every_drawing(|v| v.link_sheets(&names));
        app.refresh();
        app.status(&format!(
            "Linked {made} mention{} of {} sheets, in every drawing. Double-click a link to follow it.",
            if made == 1 { "" } else { "s" },
            names.len()
        ));
    }));
    window.on_set_stamp_all(on_app(app, |app| {
        app.open_set_sheets();
        let stamp = app.viewer.borrow().current_design().name;
        let drawings = app.tabs.borrow().len();
        if !app.dialogs.confirm(
            "Stamp every page",
            &format!(
                "Place {stamp} on every page of {drawings} drawings? Undo takes it back in each."
            ),
        ) {
            return;
        }
        let placed = app.every_drawing(Viewer::stamp_every_page);
        app.refresh();
        app.status(&format!(
            "Placed {stamp} on {placed} pages. Save all keeps them."
        ));
    }));
    window.on_set_scale_all(on_app(app, |app| {
        let Some(scale) = app.viewer.borrow().shown_scale() else {
            app.status("Give this page a scale first, then press this again.");
            return;
        };
        app.open_set_sheets();
        app.every_drawing(|v| {
            v.scale_everywhere(scale);
            1
        });
        app.refresh();
        app.status("Every page of every drawing has this page's scale now.");
    }));
    window.on_set_save_all(on_app(app, |app| {
        let mut saved = 0;
        let mut failed = Vec::new();
        app.every_drawing(|v| {
            if v.modified() && v.path().is_some() {
                match v.save(None) {
                    Ok(()) => saved += 1,
                    Err(e) => failed.push(e),
                }
            }
            0
        });
        app.refresh();
        app.status(&match failed.first() {
            None => format!("Saved {saved} drawings."),
            Some(why) => format!("Saved {saved}; {} could not be: {why}", failed.len()),
        });
    }));
    connect_batch_files(window, app);
}

/// Every drawing written out: flattened copies, one summary.
fn connect_batch_files(window: &ReviewWindow, app: &Rc<App>) {
    window.on_set_flatten_all(on_app(app, |app| {
        let Some(folder) = app.dialogs.folder_path() else {
            return;
        };
        app.open_set_sheets();
        let mut written = 0;
        let mut failed = Vec::new();
        app.every_drawing(|v| {
            let stem = v.path().and_then(Path::file_stem).map_or_else(
                || "drawing".to_owned(),
                |s| s.to_string_lossy().into_owned(),
            );
            match v.export_flattened(&folder.join(format!("{stem} (flattened).pdf"))) {
                Ok(()) => written += 1,
                Err(e) => failed.push(e),
            }
            0
        });
        app.refresh();
        app.status(&match failed.first() {
            None => format!("Wrote {written} flattened copies to {}.", folder.display()),
            Some(why) => format!("Wrote {written}; {} could not be: {why}", failed.len()),
        });
    }));
    window.on_set_summary(on_app(app, |app| {
        app.open_set_sheets();
        let name = app.set.borrow().name.clone();
        let name = if name.is_empty() {
            "Drawings".to_owned()
        } else {
            name
        };
        let Some(path) = app
            .dialogs
            .save_path(&format!("{name} summary.pdf"), FileKind::Pdf)
        else {
            return;
        };
        let mut rows = Vec::new();
        app.every_drawing(|v| {
            let sheet = v.tab_title();
            for mut row in v.summary_rows() {
                row.cells.insert(0, sheet.clone());
                rows.push(row);
            }
            0
        });
        let count = rows.len();
        let columns: Vec<(String, f64)> = std::iter::once(("Sheet".to_owned(), 1.4))
            .chain(
                COLUMNS
                    .iter()
                    .take(6)
                    .zip([1.4, 0.5, 3.6, 1.1, 1.3, 0.9])
                    .map(|(c, share)| ((*c).to_owned(), share)),
            )
            .collect();
        let table = opencabinet_pdf::report::Table {
            title: format!("Markup summary · {name}"),
            subtitle: format!(
                "{} · {count} markups",
                chrono::Local::now().format("%Y-%m-%d %H:%M")
            ),
            columns,
            rows,
        };
        let result = opencabinet_pdf::report::table(&table)
            .map_err(|e| e.to_string())
            .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
        app.status(&match result {
            Ok(()) => format!(
                "Wrote one summary of {count} markups to {}.",
                path.display()
            ),
            Err(why) => format!("Cannot write the summary: {why}"),
        });
    }));
}

/// A callback that calls `f` with the window's app.
fn on_app(app: &Rc<App>, f: fn(&App)) -> impl Fn() + 'static {
    let app = Rc::clone(app);
    move || f(&app)
}

fn connect_stamp_editor(window: &ReviewWindow, app: &Rc<App>) {
    window.on_new_stamp(on_app(app, |app| {
        let taken = app.viewer.borrow().stamp_names();
        let mut design = Design::default();
        let mut n = 1;
        while taken.contains(&design.name) {
            n += 1;
            design.name = format!("New stamp {n}");
        }
        app.open_editor(design, None);
    }));
    let a = Rc::clone(app);
    window.on_edit_stamp(move |i| {
        let Ok(i) = usize::try_from(i) else {
            return;
        };
        let Some(mut design) = a.viewer.borrow().designs().get(i).cloned() else {
            return;
        };
        if i < Viewer::built_in_stamps() {
            // Review's own stay as they are; this makes one's own from it.
            design.name = format!("{} (mine)", design.name);
            a.open_editor(design, None);
        } else {
            let was = design.name.clone();
            a.open_editor(design, Some(was));
        }
    });
    window.on_show_editor(on_app(app, |app| {
        app.editor_shown.set(true);
        app.refresh();
    }));
    window.on_editor_close(on_app(app, |app| {
        *app.editor.borrow_mut() = None;
        app.editor_shown.set(false);
        app.refresh();
    }));
    window.on_editor_save(on_app(app, |app| {
        let Some((design, was)) = app
            .editor
            .borrow()
            .as_ref()
            .map(|e| (e.design.clone(), e.was.clone()))
        else {
            return;
        };
        let result = app
            .viewer
            .borrow_mut()
            .save_design(design.clone(), was.as_deref());
        *app.editor_message.borrow_mut() = match result {
            Ok(()) => {
                if let Some(editor) = app.editor.borrow_mut().as_mut() {
                    editor.was = Some(design.name.trim().to_owned());
                }
                format!("Saved. {} is in the stamp list.", design.name.trim())
            }
            Err(message) => message,
        };
        app.refresh();
    }));
    connect_editor_fields(window, app);
}

/// The stamp editor's fields: name, lines, alignment, border, color, fill.
fn connect_editor_fields(window: &ReviewWindow, app: &Rc<App>) {
    let a = Rc::clone(app);
    window.on_editor_rename(move |name| a.edit_design(false, |d| d.name = name.to_string()));
    let a = Rc::clone(app);
    window.on_editor_line(move |i, text, size, bold| {
        let Ok(i) = usize::try_from(i) else {
            return;
        };
        a.edit_design(true, |d| {
            if let Some(line) = d.lines.get_mut(i) {
                line.text = text.chars().take(stamps::MAX_WORDS).collect();
                line.size = f64::from(size).clamp(4.0, 96.0);
                line.bold = bold;
            }
        });
    });
    window.on_editor_add_line(on_app(app, |app| {
        app.edit_design(true, |d| {
            if d.lines.len() < stamps::MAX_LINES {
                d.lines.push(stamps::Line {
                    text: String::new(),
                    size: 10.0,
                    bold: false,
                });
            }
        });
    }));
    let a = Rc::clone(app);
    window.on_editor_remove_line(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.edit_design(true, |d| {
                if d.lines.len() > 1 && i < d.lines.len() {
                    d.lines.remove(i);
                }
            });
        }
    });
    let a = Rc::clone(app);
    window.on_editor_align_to(move |i| {
        if let Some(&align) = usize::try_from(i)
            .ok()
            .and_then(|i| stamps::Align::ALL.get(i))
        {
            a.edit_design(true, |d| d.align = align);
        }
    });
    let a = Rc::clone(app);
    window.on_editor_border_to(move |i| {
        if let Some(&border) = usize::try_from(i)
            .ok()
            .and_then(|i| stamps::Border::ALL.get(i))
        {
            a.edit_design(true, |d| d.border = border);
        }
    });
    let a = Rc::clone(app);
    window.on_editor_color_to(move |i| {
        if let Some(&(_, rgb)) = usize::try_from(i).ok().and_then(|i| COLORS.get(i)) {
            a.edit_design(true, |d| d.color = rgb);
        }
    });
    let a = Rc::clone(app);
    window.on_editor_fill_to(move |fill| a.edit_design(true, |d| d.fill = fill));
}

/// A small picture of `design`, filled in with `fields`, drawn as Review
/// draws a placed stamp.
fn stamp_preview(design: &Design, fields: &Fields) -> Option<Image> {
    let text = fields.fill(&design.text());
    let (w, h) = design.size_with(&text);
    let margin = 6.0;
    let blank = opencabinet_pdf::sample::blank(w + margin * 2.0, h + margin * 2.0);
    let mut review = opencabinet_review::Review::open(blank).ok()?;
    review.add(
        opencabinet_review::Markup {
            kind: Kind::Stamp,
            points: vec![
                opencabinet_pdf::Point::new(margin, margin),
                opencabinet_pdf::Point::new(margin + w, margin + h),
            ],
            color: design.color,
            width: 1.5,
            fill: design.fill,
            text,
            look: design.look(),
            ..opencabinet_review::Markup::default()
        },
        &fields.name,
    );
    let document = opencabinet_pdf::Document::open(review.save(Units::Imperial).ok()?).ok()?;
    let page = *document.pages().first()?;
    let zoom = (900.0 / page.width).min(440.0 / page.height).min(4.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small picture's pixels"
    )]
    let (width, height) = (
        (page.width * zoom).ceil().max(1.0) as u32,
        (page.height * zoom).ceil().max(1.0) as u32,
    );
    let bitmap = render(
        &document,
        View {
            page: 0,
            zoom,
            origin: opencabinet_pdf::Point::new(0.0, 0.0),
            width,
            height,
            turn: 0,
        },
    )
    .ok()?;
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &bitmap.pixels,
        bitmap.width,
        bitmap.height,
    );
    Some(Image::from_rgba8(buffer))
}

/// Shows what the measuring markups add up to.
fn refresh_quantities(window: &ReviewWindow, viewer: &Viewer) {
    let quantities: Vec<QuantityInfo> = viewer
        .quantities()
        .into_iter()
        .map(|[what, total, detail]| QuantityInfo {
            what: what.into(),
            total: total.into(),
            detail: detail.into(),
        })
        .collect();
    window.set_quantities(ModelRc::new(VecModel::from(quantities)));
}

/// The code Slint generates from `ui/review.slint`, which follows Slint's
/// conventions rather than this workspace's lints.
#[allow(missing_docs, clippy::pedantic)]
mod ui {
    slint::include_modules!();
}

pub use ui::{
    AskInfo, BookmarkInfo, CaptureColor, EditorLine, Label, QuantityInfo, ReplyInfo, ReviewWindow,
    SavedInfo, Shape, SheetInfo, TabInfo, ToolInfo, ToolSetInfo,
};

/// Opens `address`, a web or mail address a link goes to, in the
/// system's own browser or mail program.
///
/// # Errors
/// A message for the user when it cannot.
fn open_web(address: &str) -> Result<(), String> {
    // Only addresses a link may go to; never a program or a file.
    let lower = address.to_lowercase();
    if !["http://", "https://", "mailto:"]
        .iter()
        .any(|p| lower.starts_with(p))
        || address
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '"')
    {
        return Err("Links open only web and mail addresses.".into());
    }
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(address)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot open {address}: {e}"))
}

/// The most pages drawn as thumbnails.
const MAX_THUMBNAILS: usize = 500;
/// A thumbnail's largest size, in pixels.
const THUMBNAIL: (f64, f64) = (120.0, 80.0);

/// A small picture of page `index`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "thumbnail sizes are small positive numbers"
)]
fn thumbnail(
    document: &opencabinet_pdf::Document,
    index: usize,
) -> Option<opencabinet_pdf::Bitmap> {
    let page = document.pages().get(index)?;
    let zoom = (THUMBNAIL.0 / page.width).min(THUMBNAIL.1 / page.height);
    let view = View {
        page: index,
        zoom,
        origin: opencabinet_pdf::Point::new(0.0, 0.0),
        width: (page.width * zoom).ceil().max(1.0) as u32,
        height: (page.height * zoom).ceil().max(1.0) as u32,
        turn: 0,
    };
    render(document, view).ok()
}

/// The name and version shown to users.
pub const NAME_AND_VERSION: &str = concat!("OpenCabinet Review ", env!("CARGO_PKG_VERSION"));

/// Creates the window, asking the user through the system's dialogs, with
/// the PDF at `path` open, if any.
///
/// # Errors
/// When the platform cannot create a window.
pub fn create(path: Option<&Path>) -> Result<ReviewWindow, slint::PlatformError> {
    let paths: Vec<PathBuf> = path.into_iter().map(Path::to_owned).collect();
    create_all(&paths)
}

/// Creates the window, asking the user through the system's dialogs, with
/// the PDFs at `paths` open, each in a tab.
///
/// # Errors
/// When the platform cannot create a window.
pub fn create_all(paths: &[PathBuf]) -> Result<ReviewWindow, slint::PlatformError> {
    build(Rc::new(SystemDialogs), paths, chest::default_path())
}

/// As [`create_all`], but without the person's My tools, settings and
/// recent files, which it neither reads nor changes: for pictures of the
/// window made by tools.
///
/// # Errors
/// When the platform cannot create a window.
pub fn create_apart(paths: &[PathBuf]) -> Result<ReviewWindow, slint::PlatformError> {
    build(Rc::new(SystemDialogs), paths, None)
}

/// The stamp being designed in the editor's tab, and the name it is kept
/// under, if it was kept already.
struct StampEditor {
    design: Design,
    was: Option<String>,
}

struct App {
    /// The stamp editor, when open, and whether its tab is shown.
    editor: RefCell<Option<StampEditor>>,
    editor_shown: Cell<bool>,
    editor_message: RefCell<String>,
    /// The tab shown.
    viewer: RefCell<Viewer>,
    /// Every tab, in order; the one shown is `None` here, as it is in
    /// `viewer`.
    tabs: RefCell<Vec<Option<Viewer>>>,
    /// Which of `tabs` is shown.
    active: Cell<usize>,
    /// The files opened last, newest first, and where they are kept.
    recent: RefCell<Vec<PathBuf>>,
    chest: Option<PathBuf>,
    /// The words last looked for in every drawing, and the tabs they are
    /// in.
    found_in: RefCell<(String, Vec<usize>)>,
    /// The sheet set, and where it is kept once saved or opened.
    set: RefCell<SheetSet>,
    set_path: RefCell<Option<PathBuf>>,
    dialogs: Rc<dyn Dialogs>,
    window: slint::Weak<ReviewWindow>,
    /// What the page image shows, so it is drawn again only when that
    /// changes.
    drawn: RefCell<Option<(View, usize, u64)>>,
    /// Takes in others' changes in a review session, every few tenths of
    /// a second.
    timer: slint::Timer,
    /// Bumped whenever another document opens, which may share a view.
    generation: RefCell<usize>,
    /// Pieces of the page already drawn, so moving it draws only what
    /// comes into view.
    tiles: RefCell<Tiles>,
    /// Looks for pieces drawn apart while some are being drawn.
    tile_timer: slint::Timer,
}

/// The side of a piece of the page drawn at once, in pixels.
const TILE: u32 = 256;
/// The most pieces kept: about 64 MB.
const MAX_TILES: usize = 256;

/// Which drawing, version, page, zoom and turn pieces belong to.
type TileKey = (usize, u64, usize, u64, u8);

/// Pieces drawn, by column and row.
type Drawn = Vec<((i64, i64), opencabinet_pdf::Bitmap)>;

/// Where pieces go to be drawn, and where they come back.
type Worker = (
    std::sync::mpsc::Sender<Job>,
    std::sync::mpsc::Receiver<(TileKey, Drawn)>,
);

/// Pieces to draw: whose, of what, in which view, and which.
struct Job {
    key: TileKey,
    document: opencabinet_pdf::Document,
    view: View,
    pieces: Vec<(i64, i64)>,
}

/// Pieces of a page drawn at one zoom, kept to be used again. Those not
/// drawn yet are drawn apart from the window, so moving never waits:
/// meanwhile the view drawn last shows in their place, moved and sized.
#[derive(Default)]
struct Tiles {
    key: Option<TileKey>,
    /// By column and row, with when each was last used.
    pieces: std::collections::HashMap<(i64, i64), (opencabinet_pdf::Bitmap, u64)>,
    clock: u64,
    /// Where pieces are sent to be drawn and come back, once started.
    worker: Option<Worker>,
    /// The drawing shown, for the worker to skip what is no longer
    /// wanted.
    current: std::sync::Arc<std::sync::Mutex<Option<TileKey>>>,
    /// How many sendings are being drawn.
    pending: usize,
    /// The pieces sent and not back yet.
    asked: std::collections::HashSet<(i64, i64)>,
    /// The view put together last, and of which drawing.
    last: Option<(View, TileKey, opencabinet_pdf::Bitmap)>,
    /// Whether pieces are being drawn.
    waiting: bool,
}

/// The `pieces` (columns and rows) of `view`, drawn in one pass, since drawing goes through the whole page however little
/// is drawn; then cut.
fn draw_pieces(
    document: &opencabinet_pdf::Document,
    view: View,
    pieces: &[(i64, i64)],
) -> Result<Drawn, opencabinet_pdf::Error> {
    let Some(&(c0, r0)) = pieces.first() else {
        return Ok(Vec::new());
    };
    let (c0, r0) = pieces
        .iter()
        .fold((c0, r0), |(c, r), p| (c.min(p.0), r.min(p.1)));
    let (c1, r1) = pieces
        .iter()
        .fold((c0, r0), |(c, r), p| (c.max(p.0), r.max(p.1)));
    let (across, down) = (
        u32::try_from(c1 - c0 + 1).unwrap_or(1),
        u32::try_from(r1 - r0 + 1).unwrap_or(1),
    );
    let side = f64::from(TILE);
    #[expect(clippy::cast_precision_loss, reason = "tile numbers")]
    let whole = opencabinet_pdf::render(
        document,
        View {
            origin: opencabinet_pdf::Point::new(c0 as f64 * side, r0 as f64 * side),
            width: TILE * across,
            height: TILE * down,
            ..view
        },
    )?;
    let stride = (TILE * across) as usize * 4;
    let row = TILE as usize * 4;
    Ok(pieces
        .iter()
        .map(|&at| {
            let (left, top) = (
                usize::try_from(at.0 - c0).unwrap_or(0) * row,
                usize::try_from(at.1 - r0).unwrap_or(0) * TILE as usize,
            );
            let mut pixels = Vec::with_capacity(row * TILE as usize);
            for y in top..top + TILE as usize {
                let from = y * stride + left;
                pixels.extend_from_slice(&whole.pixels[from..from + row]);
            }
            (
                at,
                opencabinet_pdf::Bitmap {
                    width: TILE,
                    height: TILE,
                    pixels,
                },
            )
        })
        .collect())
}

/// The pieces, by column and row, that cover `view`, and `around` more on
/// every side.
fn pieces_of(view: View, around: i64) -> Vec<(i64, i64)> {
    let side = f64::from(TILE);
    #[expect(clippy::cast_possible_truncation, reason = "tile numbers")]
    let range = |from: f64, length: u32| {
        let first = (from / side).floor() as i64;
        let last = ((from + f64::from(length) - 1.0) / side).floor() as i64;
        first - around..=last + around
    };
    let (columns, rows) = (
        range(view.origin.x, view.width),
        range(view.origin.y, view.height),
    );
    rows.flat_map(|r| columns.clone().map(move |c| (c, r)))
        .collect()
}

/// `last`, a view of a page, as it would show in `view` of the same page
/// at another zoom or place: each pixel taken from the nearest one.
fn moved_and_sized(last: &(View, opencabinet_pdf::Bitmap), view: View) -> Vec<u8> {
    let (old, bitmap) = last;
    let (w, h) = (view.width as usize, view.height as usize);
    let mut pixels = vec![0u8; w * h * 4];
    let ratio = old.zoom / view.zoom;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "pixels, checked to be in the bitmap"
    )]
    let source = |at: usize, origin: f64, old_origin: f64, size: u32| -> Option<usize> {
        let v = ((origin + at as f64 + 0.5) * ratio - old_origin).floor();
        (v >= 0.0 && v < f64::from(size)).then_some(v as usize)
    };
    let columns: Vec<Option<usize>> = (0..w)
        .map(|x| source(x, view.origin.x, old.origin.x, old.width))
        .collect();
    let stride = old.width as usize * 4;
    for y in 0..h {
        let Some(sy) = source(y, view.origin.y, old.origin.y, old.height) else {
            continue;
        };
        let row = &bitmap.pixels[sy * stride..(sy + 1) * stride];
        let out = &mut pixels[y * w * 4..(y + 1) * w * 4];
        for (x, sx) in columns.iter().enumerate() {
            if let Some(sx) = sx {
                out[x * 4..x * 4 + 4].copy_from_slice(&row[sx * 4..sx * 4 + 4]);
            }
        }
    }
    pixels
}

impl Tiles {
    /// Takes in pieces drawn apart; whether any came for the drawing shown.
    fn take_drawn(&mut self) -> bool {
        let Some((_, done)) = &self.worker else {
            return false;
        };
        let mut came = false;
        while let Ok((key, drawn)) = done.try_recv() {
            self.pending = self.pending.saturating_sub(1);
            if Some(key) == self.key {
                for (at, bitmap) in drawn {
                    self.asked.remove(&at);
                    self.pieces.insert(at, (bitmap, self.clock));
                }
                came = true;
            }
        }
        self.waiting = self.pending > 0;
        came
    }

    /// Sends pieces to be drawn apart, starting the worker the first time.
    fn send(&mut self, job: Job) {
        let current = std::sync::Arc::clone(&self.current);
        let (jobs, _) = self.worker.get_or_insert_with(|| {
            let (jobs, inbox) = std::sync::mpsc::channel::<Job>();
            let (outbox, done) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                for job in inbox {
                    // No longer shown: skipped, but answered.
                    let wanted = current.lock().is_ok_and(|c| *c == Some(job.key));
                    let drawn = if wanted {
                        draw_pieces(&job.document, job.view, &job.pieces).unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    if outbox.send((job.key, drawn)).is_err() {
                        break;
                    }
                }
            });
            (jobs, done)
        });
        let pieces = job.pieces.clone();
        if jobs.send(job).is_ok() {
            self.asked.extend(pieces);
            self.pending += 1;
            self.waiting = true;
        }
    }

    /// `view` of `document`, made of pieces drawn now or before; `which`
    /// says which drawing and version it is. Pieces missing are drawn
    /// apart, then a ring around the view, and meanwhile the view shown
    /// last stands in for them. Only a page not shown before is drawn at
    /// once.
    fn view(
        &mut self,
        document: &opencabinet_pdf::Document,
        view: View,
        (generation, version): (usize, u64),
    ) -> Result<opencabinet_pdf::Bitmap, opencabinet_pdf::Error> {
        let key = (
            generation,
            version,
            view.page,
            view.zoom.to_bits(),
            view.turn,
        );
        if self.key != Some(key) {
            self.pieces.clear();
            self.asked.clear();
            self.key = Some(key);
            if let Ok(mut current) = self.current.lock() {
                *current = Some(key);
            }
        }
        self.clock += 1;
        self.take_drawn();
        let wanted = pieces_of(view, 0);
        let missing: Vec<(i64, i64)> = wanted
            .iter()
            .copied()
            .filter(|at| !self.pieces.contains_key(at))
            .collect();
        // The view shown last stands in while the same page is drawn.
        let stand_in = self
            .last
            .as_ref()
            .filter(|(_, k, _)| k.0 == key.0 && k.2 == key.2 && k.4 == key.4)
            .map(|(v, _, b)| (*v, b.clone()));
        if !missing.is_empty() && stand_in.is_none() {
            for (at, bitmap) in draw_pieces(document, view, &missing)? {
                self.pieces.insert(at, (bitmap, self.clock));
            }
        } else {
            let ask: Vec<(i64, i64)> = missing
                .iter()
                .copied()
                .filter(|at| !self.asked.contains(at))
                .collect();
            if !ask.is_empty() {
                self.send(Job {
                    key,
                    document: document.clone(),
                    view,
                    pieces: ask,
                });
            }
        }
        if self.pending == 0 {
            // A ring around the view too, so moving finds it drawn.
            let around: Vec<(i64, i64)> = pieces_of(view, 1)
                .into_iter()
                .filter(|at| !self.pieces.contains_key(at) && !self.asked.contains(at))
                .collect();
            if !around.is_empty() {
                self.send(Job {
                    key,
                    document: document.clone(),
                    view,
                    pieces: around,
                });
            }
        }
        let still_missing = wanted.iter().any(|at| !self.pieces.contains_key(at));
        let start = match stand_in {
            Some(last) if still_missing => moved_and_sized(&last, view),
            _ => vec![0u8; view.width as usize * view.height as usize * 4],
        };
        let bitmap = self.put_together(view, &wanted, start);
        self.last = Some((view, key, bitmap.clone()));
        Ok(bitmap)
    }

    /// The pieces `wanted` of `view` put together over `pixels`.
    fn put_together(
        &mut self,
        view: View,
        wanted: &[(i64, i64)],
        mut pixels: Vec<u8>,
    ) -> opencabinet_pdf::Bitmap {
        #[expect(clippy::cast_possible_truncation, reason = "pixels")]
        let (ox, oy) = (view.origin.x.round() as i64, view.origin.y.round() as i64);
        let tile = i64::from(TILE);
        for at in wanted {
            let Some((bitmap, used)) = self.pieces.get_mut(at) else {
                continue;
            };
            *used = self.clock;
            let (tx, ty) = (at.0 * tile, at.1 * tile);
            let x0 = tx.max(ox);
            let x1 = (tx + tile).min(ox + i64::from(view.width));
            if x1 <= x0 {
                continue;
            }
            let bytes = usize::try_from(x1 - x0).unwrap_or(0) * 4;
            for y in ty.max(oy)..(ty + tile).min(oy + i64::from(view.height)) {
                let from = usize::try_from(((y - ty) * tile + (x0 - tx)) * 4).unwrap_or(0);
                let to = usize::try_from(((y - oy) * i64::from(view.width) + (x0 - ox)) * 4)
                    .unwrap_or(0);
                pixels[to..to + bytes].copy_from_slice(&bitmap.pixels[from..from + bytes]);
            }
        }
        // The least used go first.
        while self.pieces.len() > MAX_TILES.max(wanted.len() * 3) {
            let Some(oldest) = self
                .pieces
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(k, _)| *k)
            else {
                break;
            };
            self.pieces.remove(&oldest);
        }
        opencabinet_pdf::Bitmap {
            width: view.width,
            height: view.height,
            pixels,
        }
    }
}

/// Creates the window, asking the user through `dialogs`, with the PDF at
/// `path` open, if any.
///
/// # Errors
/// When the platform cannot create a window.
pub fn create_with(
    dialogs: Rc<dyn Dialogs>,
    path: Option<&Path>,
) -> Result<ReviewWindow, slint::PlatformError> {
    let paths: Vec<PathBuf> = path.into_iter().map(Path::to_owned).collect();
    build(dialogs, &paths, None)
}

/// The window, keeping My tools at `saved` when given.
fn build(
    dialogs: Rc<dyn Dialogs>,
    paths: &[PathBuf],
    saved: Option<PathBuf>,
) -> Result<ReviewWindow, slint::PlatformError> {
    let window = ReviewWindow::new()?;
    let recent = saved.as_deref().map(chest::load_recent).unwrap_or_default();
    let app = Rc::new(App {
        editor: RefCell::new(None),
        editor_shown: Cell::new(false),
        editor_message: RefCell::new(String::new()),
        viewer: RefCell::new(Viewer::default()),
        tabs: RefCell::new(vec![None]),
        active: Cell::new(0),
        recent: RefCell::new(recent),
        chest: saved.clone(),
        found_in: RefCell::new((String::new(), Vec::new())),
        set: RefCell::new(SheetSet::default()),
        set_path: RefCell::new(None),
        dialogs,
        window: window.as_weak(),
        drawn: RefCell::new(None),
        generation: RefCell::new(0),
        tiles: RefCell::new(Tiles::default()),
        tile_timer: slint::Timer::default(),
        timer: slint::Timer::default(),
    });
    let polling = Rc::downgrade(&app);
    app.timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(250),
        move || {
            if let Some(app) = polling.upgrade() {
                app.poll();
            }
        },
    );
    // Pieces of the page drawn apart come in here, while some are drawn.
    let looking = Rc::downgrade(&app);
    app.tile_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(15),
        move || {
            let Some(app) = looking.upgrade() else {
                return;
            };
            let came = app.tiles.borrow_mut().take_drawn();
            if came {
                *app.drawn.borrow_mut() = None;
                if let Some(window) = app.window.upgrade() {
                    app.refresh_page(&window);
                }
            }
            if !app.tiles.borrow().waiting {
                app.tile_timer.stop();
            }
        },
    );
    app.tile_timer.stop();

    let tools: Vec<ToolInfo> = TOOLS
        .iter()
        .map(|&t| {
            let (icon, filled) = icons::icon(t);
            ToolInfo {
                name: t.label().into(),
                icon: icon.into(),
                filled,
            }
        })
        .collect();
    window.set_tools(ModelRc::new(VecModel::from(tools)));
    let widths: Vec<SharedString> = WIDTHS.iter().map(|&(name, _)| name.into()).collect();
    window.set_widths(strings(&widths));
    let statuses: Vec<SharedString> = Status::ALL.iter().map(|s| s.label().into()).collect();
    window.set_statuses(strings(&statuses));
    let scales: Vec<SharedString> = Viewer::scale_names().into_iter().map(Into::into).collect();
    window.set_scales(strings(&scales));
    let colors: Vec<Color> = COLORS.iter().map(|&(_, rgb)| color(rgb)).collect();
    window.set_colors(ModelRc::new(VecModel::from(colors)));
    let names: Vec<SharedString> = COLORS.iter().map(|&(name, _)| name.into()).collect();
    window.set_color_names(ModelRc::new(VecModel::from(names)));
    window.set_author(app.viewer.borrow().author.as_str().into());
    if let Some(saved) = saved {
        app.viewer.borrow_mut().keep_saved_tools_at(saved);
    }

    connect(&window, &app);
    for path in paths {
        app.open(path);
    }
    app.refresh();
    Ok(window)
}

#[allow(clippy::too_many_lines, reason = "one line per callback")]
fn connect(window: &ReviewWindow, app: &Rc<App>) {
    let on = |f: fn(&App)| {
        let app = Rc::clone(app);
        move || f(&app)
    };
    window.on_open(on(App::choose_and_open));
    window.on_save(on(|app| app.save(false)));
    window.on_save_as(on(|app| app.save(true)));
    window.on_export_list(on(App::export_list));
    window.on_previous_page(on(|app| {
        app.change(|v| {
            let page = v.page();
            if page > 0 {
                v.go_to(page - 1);
            }
        });
    }));
    window.on_next_page(on(|app| app.change(|v| v.go_to(v.page() + 1))));
    window.on_zoom_in(on(|app| app.change(|v| v.zoom_center(1.25))));
    window.on_zoom_out(on(|app| app.change(|v| v.zoom_center(0.8))));
    window.on_fit(on(|app| app.change(Viewer::fit)));
    window.on_fit_width(on(|app| app.change(Viewer::fit_width)));
    window.on_undo(on(|app| app.change(Viewer::undo)));
    window.on_redo(on(|app| app.change(Viewer::redo)));
    window.on_finish(on(|app| app.change(Viewer::finish)));
    window.on_copy(on(|app| app.change(Viewer::copy)));
    window.on_paste(on(|app| app.change(Viewer::paste)));
    window.on_duplicate(on(|app| app.change(Viewer::duplicate)));
    window.on_zoom_to_selected(on(|app| app.change(Viewer::zoom_to_selected)));
    window.on_scale_every_page(on(|app| app.change(Viewer::scale_every_page)));
    window.on_double_clicked(on(|app| app.change(Viewer::double_click)));
    window.on_cancel(on(|app| app.change(Viewer::cancel)));
    window.on_escape(on(|app| app.change(Viewer::escape)));
    let weak = window.as_weak();
    window.on_quit(move || {
        if let Some(window) = weak.upgrade() {
            window
                .window()
                .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        }
    });
    window.on_stop_compare(on(|app| {
        app.change(Viewer::stop_compare);
        *app.generation.borrow_mut() += 1;
        app.refresh();
    }));
    window.on_compare(on(App::compare));
    window.on_start_session(on(|app| {
        let result = app.viewer.borrow_mut().start_session();
        app.refresh();
        if let Err(message) = result {
            app.status(&message);
        }
    }));
    window.on_leave_session(on(|app| app.change(Viewer::leave_session)));
    window.on_allow_firewall(on(|app| app.change(Viewer::allow_firewall)));
    let a = Rc::clone(app);
    window.on_join_session(move |invite| {
        let result = a.viewer.borrow_mut().join_session(&invite);
        *a.generation.borrow_mut() += 1;
        a.draw_thumbnails();
        a.refresh();
        if let Err(message) = result {
            a.status(&message);
        }
    });
    let a = Rc::clone(app);
    window.on_start_on_server(move |server| {
        let result = a.viewer.borrow_mut().start_session_on_server(&server);
        a.refresh();
        if let Err(message) = result {
            a.status(&message);
        }
    });
    let a = Rc::clone(app);
    window.on_hand_over(move |name| a.try_change(|v| v.hand_over(&name)));
    window.on_export_flattened(on(App::export_flattened));
    window.on_export_summary(on(App::export_summary));
    let a = Rc::clone(app);
    window.on_page_clicked(move |i, control, shift| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.click_page(i, Keys { control, shift }));
        }
    });
    let a = Rc::clone(app);
    window.on_turn_pages(move |turn| a.try_change(|v| v.turn_pages(turn)));
    let a = Rc::clone(app);
    window.on_move_pages(move |by| a.try_change(|v| v.move_pages(by)));
    window.on_delete_pages(on(|app| app.try_change(Viewer::delete_pages)));
    window.on_insert_blank(on(|app| app.try_change(Viewer::insert_blank)));
    window.on_insert_pages(on(|app| {
        if let Some(path) = app.dialogs.open_path() {
            app.try_change(|v| v.insert_file(&path));
        }
    }));
    window.on_extract_pages(on(App::extract_pages));
    let a = Rc::clone(app);
    window.on_find_everywhere(move |text| a.find_everywhere(&text));
    let a = Rc::clone(app);
    window.on_show_found_in(move |row| {
        if let Ok(row) = usize::try_from(row) {
            a.show_found_in(row);
        }
    });
    let a = Rc::clone(app);
    window.on_capture_color_to(move |i, c| {
        let Ok(i) = usize::try_from(i) else {
            return;
        };
        let to = usize::try_from(c)
            .ok()
            .and_then(|c| COLORS.get(c))
            .map(|&(_, rgb)| rgb);
        a.change(|v| v.set_capture_color(i, to));
    });
    let a = Rc::clone(app);
    window.on_set_only_listed(move |on| a.change(|v| v.set_only_listed(on)));
    window.on_export_quantities(on(|app| {
        let stem = app
            .viewer
            .borrow()
            .path()
            .and_then(Path::file_stem)
            .map_or_else(
                || "drawing".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
        if let Some(path) = app
            .dialogs
            .save_path(&format!("{stem} quantities.csv"), FileKind::List)
        {
            app.try_change(|v| v.export_quantities(&path));
        }
    }));
    let a = Rc::clone(app);
    window.on_add_stamp(move |words| a.try_change(|v| v.add_stamp(&words)));
    let a = Rc::clone(app);
    window.on_remove_stamp(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.try_change(|v| v.remove_stamp(i));
        }
    });
    let a = Rc::clone(app);
    window.on_turn_view(move |turn| a.change(|v| v.turn_view(turn)));
    let a = Rc::clone(app);
    window.on_choose_tab(move |i| {
        if let Ok(i) = usize::try_from(i) {
            // A drawing's tab puts the stamp editor in the background.
            a.editor_shown.set(false);
            a.switch_to(i);
            a.refresh();
        }
    });
    let a = Rc::clone(app);
    window.on_close_tab(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.close_tab(i);
        }
    });
    window.on_next_tab(on(|app| {
        let count = app.tabs.borrow().len();
        app.switch_to((app.active.get() + 1) % count.max(1));
    }));
    window.on_close_this_tab(on(|app| app.close_tab(app.active.get())));
    let a = Rc::clone(app);
    window.on_open_sheet(move |i| {
        let path = usize::try_from(i)
            .ok()
            .and_then(|i| a.set.borrow().sheets.get(i).map(|s| s.path.clone()));
        if let Some(path) = path {
            a.open(&path);
        }
    });
    window.on_add_to_set(on(|app| {
        let path = app.viewer.borrow().path().map(Path::to_owned);
        if let Some(path) = path {
            if app.set.borrow().sheets.is_empty() && app.set.borrow().name.is_empty() {
                app.set.borrow_mut().name = "Sheet set".into();
            }
            let added = app.set.borrow_mut().add(&path);
            app.refresh();
            app.status(&if added {
                format!(
                    "Added {} to the set. Save the set to keep it.",
                    sets::label_for(&path)
                )
            } else {
                "The set has this drawing already.".to_owned()
            });
        } else {
            app.status("Save the drawing first, so the set can find it.");
        }
    }));
    let a = Rc::clone(app);
    window.on_remove_from_set(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.set.borrow_mut().remove(i);
            a.refresh();
        }
    });
    window.on_save_set(on(|app| app.save_set(false)));
    window.on_save_tabs_as_set(on(|app| {
        let mut set = SheetSet {
            name: app.set.borrow().name.clone(),
            sheets: Vec::new(),
        };
        let paths: Vec<PathBuf> = app.tab_paths();
        for path in &paths {
            set.add(path);
        }
        if set.name.is_empty() {
            set.name = "Sheet set".into();
        }
        *app.set.borrow_mut() = set;
        app.save_set(true);
    }));
    let a = Rc::clone(app);
    window.on_open_recent(move |i| {
        let path = usize::try_from(i)
            .ok()
            .and_then(|i| a.recent.borrow().get(i).cloned());
        if let Some(path) = path {
            a.open(&path);
        }
    });
    let a = Rc::clone(app);
    window.on_go_to_bookmark(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.go_to_bookmark(i));
        }
    });
    window.on_save_tool(on(|app| {
        let result = app.viewer.borrow_mut().save_selected_tool();
        app.refresh();
        if let Err(message) = result {
            app.status(&message);
        }
    }));
    let a = Rc::clone(app);
    window.on_use_saved(move |set, i| {
        if let (Ok(set), Ok(i)) = (usize::try_from(set), usize::try_from(i)) {
            a.change(|v| v.use_saved_tool(set, i));
        }
    });
    let a = Rc::clone(app);
    window.on_remove_saved(move |set, i| {
        if let (Ok(set), Ok(i)) = (usize::try_from(set), usize::try_from(i)) {
            a.try_change(|v| v.remove_saved_tool(set, i));
        }
    });
    connect_tool_sets(window, app);
    connect_batch(window, app);
    connect_stamp_editor(window, app);
    let a = Rc::clone(app);
    window.on_sort_list(move |column, down| {
        if let Ok(column) = usize::try_from(column) {
            a.change(|v| v.sort_by(Some((column, down))));
        }
    });
    let a = Rc::clone(app);
    window.on_say(move |text| {
        if a.viewer.borrow_mut().say(&text) {
            a.refresh_session();
        }
    });
    window.on_delete_selected(on(|app| app.change(Viewer::delete_selected)));

    let a = Rc::clone(app);
    window.on_choose_tool(move |i| {
        if let Some(&tool) = usize::try_from(i).ok().and_then(|i| TOOLS.get(i)) {
            a.change(|v| v.choose_tool(tool));
        }
    });
    let a = Rc::clone(app);
    window.on_choose_color(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.choose_color(i));
        }
    });
    let a = Rc::clone(app);
    window.on_find(move |text| a.change(|v| v.find(&text)));
    let a = Rc::clone(app);
    window.on_find_next(move |back| a.change(|v| v.find_next(back)));
    let a = Rc::clone(app);
    window.on_set_filter(move |status, text| {
        let status = usize::try_from(status)
            .ok()
            .and_then(|i| i.checked_sub(1))
            .and_then(|i| Status::ALL.get(i).copied());
        a.change(|v| v.set_filter(status, &text));
    });
    let a = Rc::clone(app);
    window.on_choose_scale(move |i, every| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.choose_scale(i, every));
        }
    });
    let a = Rc::clone(app);
    window.on_go_to_page(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.go_to(i));
        }
    });
    let a = Rc::clone(app);
    window.on_choose_width(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.choose_width(i));
        }
    });
    let a = Rc::clone(app);
    window.on_set_fill(move |fill| a.change(|v| v.set_fill(fill)));
    let a = Rc::clone(app);
    window.on_choose_stamp(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.choose_stamp(i));
        }
    });
    let a = Rc::clone(app);
    window.on_choose_status(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.set_status(i));
        }
    });
    let a = Rc::clone(app);
    window.on_subject_edited(move |text| {
        {
            let mut viewer = a.viewer.borrow_mut();
            viewer.set_subject(&text);
            viewer.publish();
        }
        a.refresh_list();
        if let Some(window) = a.window.upgrade() {
            a.refresh_page(&window);
        }
        a.refresh_overlay();
    });
    let a = Rc::clone(app);
    window.on_author_edited(move |text| text.trim().clone_into(&mut a.viewer.borrow_mut().author));
    let a = Rc::clone(app);
    window.on_choose_units(move |i| {
        a.change(|v| {
            v.units = if i == 1 {
                Units::Metric
            } else {
                Units::Imperial
            };
        });
    });
    let a = Rc::clone(app);
    window.on_pressed_with(move |x, y, button, control, shift| {
        let button = u8::try_from(button).unwrap_or(0);
        a.change(|v| {
            v.press_with(
                (f64::from(x), f64::from(y)),
                button,
                Keys { control, shift },
            );
        });
    });
    let a = Rc::clone(app);
    window.on_reply(move |text| {
        a.change(|v| {
            v.reply(&text);
        });
    });
    let a = Rc::clone(app);
    window.on_remove_reply(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.remove_reply(i));
        }
    });
    window.on_select_all(on(App::select_all));
    let a = Rc::clone(app);
    window.on_moved_to(move |x, y, shift| {
        let at = (f64::from(x), f64::from(y));
        let keys = Keys {
            shift,
            ..Keys::default()
        };
        if a.viewer.borrow().dragging() {
            a.change(|v| v.pointer_moved(at, keys));
            return;
        }
        // Only hovering: no markup changes, so only what is drawn over
        // the page and the snap's name are brought up to date.
        let tracked = {
            let mut viewer = a.viewer.borrow_mut();
            let tracked = viewer.tracks_hover();
            if tracked {
                viewer.pointer_moved(at, keys);
            }
            tracked
        };
        if tracked {
            a.refresh_overlay();
            if let Some(window) = a.window.upgrade() {
                window.set_snap_label(a.viewer.borrow().snap_label().into());
            }
        }
    });
    let a = Rc::clone(app);
    window.on_set_snap(move |on| a.change(|v| v.set_snap(on)));
    let a = Rc::clone(app);
    window.on_set_wheel_zooms(move |on| a.change(|v| v.set_wheel_zooms(on)));
    let a = Rc::clone(app);
    window.on_pressed(move |x, y, button| {
        let button = u8::try_from(button).unwrap_or(1);
        a.change(|v| v.press((f64::from(x), f64::from(y)), button));
    });
    let a = Rc::clone(app);
    window.on_dragged(move |x, y| a.change(|v| v.drag((f64::from(x), f64::from(y)))));
    let a = Rc::clone(app);
    let w = window.as_weak();
    window.on_released(move |x, y| {
        a.change(|v| v.release((f64::from(x), f64::from(y))));
        // A capture just made: its colors, to change.
        if a.viewer.borrow_mut().take_capture_made()
            && let Some(window) = w.upgrade()
        {
            window.set_list_open(true);
            window.set_editor_tab(2);
        }
    });
    let a = Rc::clone(app);
    window.on_wheel(move |dx, dy, zoom, x, y| {
        a.change(|v| {
            v.wheel(
                (f64::from(dx), f64::from(dy)),
                zoom,
                (f64::from(x), f64::from(y)),
            );
        });
    });
    let a = Rc::clone(app);
    window.on_resized(move |w, h| a.change(|v| v.resize(f64::from(w), f64::from(h))));
    let a = Rc::clone(app);
    window.on_row_selected(move |row| {
        if let Ok(row) = usize::try_from(row) {
            a.change(|v| v.select_row(row));
        }
    });
    let a = Rc::clone(app);
    window.on_text_edited(move |text| {
        {
            let mut viewer = a.viewer.borrow_mut();
            viewer.set_text(&text);
            viewer.publish();
        }
        // Not the whole window: the field being typed in keeps its text.
        a.refresh_list();
        if let Some(window) = a.window.upgrade() {
            a.refresh_page(&window);
        }
        a.refresh_overlay();
    });
    let a = Rc::clone(app);
    window.on_calibrate(move |text| {
        let result = a.viewer.borrow_mut().calibrate(&text);
        if let Err(message) = result {
            a.status(&message);
        } else {
            a.refresh();
        }
    });
    let a = Rc::clone(app);
    window.window().on_close_requested(move || {
        if a.any_modified() && !a.dialogs.discard_changes() {
            CloseRequestResponse::KeepWindowShown
        } else {
            CloseRequestResponse::HideWindow
        }
    });
}

impl App {
    fn change(&self, f: impl FnOnce(&mut Viewer)) {
        let (changed, follow) = {
            let mut viewer = self.viewer.borrow_mut();
            let before = viewer.document_key();
            f(&mut viewer);
            viewer.publish();
            (viewer.document_key() != before, viewer.take_follow())
        };
        match follow {
            Some(Follow::File(path, page)) => {
                if path.is_file() {
                    self.open(&path);
                    self.viewer.borrow_mut().go_to(page);
                } else {
                    self.viewer.borrow_mut().say_status(&format!(
                        "The link goes to {}, which is not there.",
                        path.display()
                    ));
                }
            }
            Some(Follow::Web(address)) => {
                if let Err(message) = open_web(&address) {
                    self.viewer.borrow_mut().say_status(&message);
                }
            }
            None => {}
        }
        if changed {
            // Other pages: other thumbnails, and the page drawn again.
            *self.generation.borrow_mut() += 1;
            self.draw_thumbnails();
        }
        self.refresh();
    }

    /// As [`App::change`], showing what went wrong, if anything.
    fn try_change(&self, f: impl FnOnce(&mut Viewer) -> Result<(), String>) {
        let mut result = Ok(());
        self.change(|v| result = f(v));
        if let Err(message) = result {
            self.status(&message);
        }
    }

    fn extract_pages(&self) {
        let (stem, chosen) = {
            let viewer = self.viewer.borrow();
            let stem = viewer.path().and_then(Path::file_stem).map_or_else(
                || "drawing".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
            (stem, viewer.pages_chosen())
        };
        let pages = match chosen.as_slice() {
            [one] => format!("page {}", one + 1),
            [first, .., last] => format!("pages {}-{}", first + 1, last + 1),
            [] => return,
        };
        let Some(path) = self
            .dialogs
            .save_path(&format!("{stem} ({pages}).pdf"), FileKind::Pdf)
        else {
            return;
        };
        self.try_change(|v| v.extract_pages(&path));
    }

    /// Takes in others' changes in a review session.
    fn poll(&self) {
        // Tabs not shown keep up with their sessions too.
        for tab in self.tabs.borrow_mut().iter_mut().flatten() {
            if tab.in_session() {
                tab.pull();
            }
        }
        let pulled = self.viewer.borrow_mut().pull();
        if pulled {
            self.refresh();
        } else if self.viewer.borrow().in_session() {
            self.refresh_session();
        }
    }

    /// Shows the session's state, people and activity.
    fn refresh_session(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let viewer = self.viewer.borrow();
        window.set_in_session(viewer.in_session());
        window.set_hosting(viewer.hosting());
        window.set_invite(viewer.invite().unwrap_or_default().into());
        let others: Vec<SharedString> = viewer.others().into_iter().map(Into::into).collect();
        window.set_others(strings(&others));
        window.set_session_state(viewer.session_state().into());
        let items = |lines: Vec<String>| {
            let items: Vec<StandardListViewItem> = lines
                .iter()
                .map(|l| StandardListViewItem::from(l.as_str()))
                .collect();
            ModelRc::new(VecModel::from(items))
        };
        window.set_people(items(viewer.people()));
        window.set_activity(items(viewer.activity()));
        window.set_chat(items(viewer.chat()));
        window.set_firewall_advice(viewer.firewall_advice().into());
        window.set_firewall_blocks(viewer.firewall_blocks());
    }

    fn choose_and_open(&self) {
        for path in self.dialogs.open_paths() {
            self.open(&path);
        }
    }

    /// Opens the PDF at `path`: in its tab if it is open already, in the
    /// tab shown if that has nothing open, else in a new tab.
    fn open(&self, path: &Path) {
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ocset"))
        {
            self.open_set(path);
            return;
        }
        if let Some(at) = self.tab_of(path) {
            self.switch_to(at);
            return;
        }
        let has_document = self.viewer.borrow().review().is_some();
        if has_document {
            let tab = self.viewer.borrow().new_tab();
            let at = {
                let mut tabs = self.tabs.borrow_mut();
                tabs.insert(self.active.get() + 1, Some(tab));
                self.active.get() + 1
            };
            self.switch_to(at);
        }
        let result = self.viewer.borrow_mut().open(path);
        if result.is_ok() {
            self.remember(path);
        } else if has_document {
            // Nothing to show in the new tab: it goes again.
            self.close_tab(self.active.get());
        }
        *self.generation.borrow_mut() += 1;
        self.draw_thumbnails();
        self.refresh();
        if let Err(message) = result {
            self.status(&message);
        }
    }

    /// Opens every drawing of the sheet set not open yet, each in a tab.
    fn open_set_sheets(&self) {
        let paths: Vec<PathBuf> = self
            .set
            .borrow()
            .sheets
            .iter()
            .filter(|s| s.path.is_file())
            .map(|s| s.path.clone())
            .collect();
        let shown = self.active.get();
        for path in paths {
            if self.tab_of(&path).is_none() {
                self.open(&path);
            }
        }
        self.switch_to(shown.min(self.tabs.borrow().len().saturating_sub(1)));
    }

    /// Does `f` to every open drawing; what the calls return, added up.
    fn every_drawing(&self, mut f: impl FnMut(&mut Viewer) -> usize) -> usize {
        let mut total = f(&mut self.viewer.borrow_mut());
        for tab in self.tabs.borrow_mut().iter_mut().flatten() {
            total += f(tab);
        }
        total
    }

    /// Every sheet's name the open drawings and the sheet set give, such
    /// as `A-301`: the PDFs' page labels and the sheets' labels, those
    /// with a digit in them.
    fn sheet_names(&self) -> Vec<SheetName> {
        let mut names: Vec<SheetName> = Vec::new();
        let mut add = |name: &str, path: &Path, page: usize| {
            let name = name.trim();
            if name.chars().count() >= 3
                && name.chars().any(|c| c.is_ascii_digit())
                && name.chars().any(char::is_alphabetic)
                && !names.iter().any(|n| n.name.eq_ignore_ascii_case(name))
            {
                names.push(SheetName {
                    name: name.to_owned(),
                    path: path.to_owned(),
                    page,
                });
            }
        };
        {
            let viewer = self.viewer.borrow();
            let tabs = self.tabs.borrow();
            for v in tabs.iter().map(|t| t.as_ref().unwrap_or(&viewer)) {
                let Some(path) = v.path() else {
                    continue;
                };
                for (page, label) in v.labels().iter().enumerate() {
                    add(label, path, page);
                }
            }
        }
        for sheet in &self.set.borrow().sheets {
            add(&sheet.label, &sheet.path, 0);
        }
        names
    }

    /// Opens the sheet set at `path`: each drawing in a tab, and the set
    /// in its panel.
    fn open_set(&self, path: &Path) {
        let set = match sets::read(path) {
            Ok(set) => set,
            Err(message) => {
                self.status(&message);
                return;
            }
        };
        let missing = set.sheets.iter().filter(|s| !s.path.is_file()).count();
        let to_open: Vec<PathBuf> = set
            .sheets
            .iter()
            .filter(|s| s.path.is_file())
            .map(|s| s.path.clone())
            .collect();
        let name = set.name.clone();
        let count = set.sheets.len();
        *self.set.borrow_mut() = set;
        *self.set_path.borrow_mut() = Some(path.to_owned());
        for sheet in &to_open {
            self.open(sheet);
        }
        self.remember(path);
        if let Some(window) = self.window.upgrade() {
            window.set_panel(5);
        }
        self.refresh();
        self.status(&if missing == 0 {
            format!("Opened the sheet set {name}: {count} drawings.")
        } else {
            format!("Opened the sheet set {name}: {count} drawings, {missing} not found where the set says.")
        });
    }

    /// Writes the sheet set where it is kept, or where the user says when
    /// `ask` or it was never kept.
    fn save_set(&self, ask: bool) {
        let known = self.set_path.borrow().clone();
        let path = if let Some(path) = known.filter(|_| !ask) {
            path
        } else {
            let name = self.set.borrow().name.clone();
            let Some(path) = self
                .dialogs
                .save_path(&format!("{name}.ocset"), FileKind::Set)
            else {
                return;
            };
            path
        };
        if self.set_path.borrow().as_ref() != Some(&path) {
            self.set.borrow_mut().name = sets::label_for(&path);
        }
        let result = sets::write(&path, &self.set.borrow());
        match result {
            Ok(()) => {
                *self.set_path.borrow_mut() = Some(path.clone());
                self.remember(&path);
                self.refresh();
                self.status(&format!("Saved the sheet set {}.", sets::label_for(&path)));
            }
            Err(message) => self.status(&message),
        }
    }

    /// Where each tab's drawing is, in tab order.
    fn tab_paths(&self) -> Vec<PathBuf> {
        let viewer = self.viewer.borrow();
        self.tabs
            .borrow()
            .iter()
            .filter_map(|t| t.as_ref().unwrap_or(&viewer).path().map(Path::to_owned))
            .collect()
    }

    /// The tab where `path` is open, if any.
    fn tab_of(&self, path: &Path) -> Option<usize> {
        let same = |v: &Viewer| {
            v.path().is_some_and(|p| {
                p == path
                    || std::fs::canonicalize(p)
                        .is_ok_and(|p| std::fs::canonicalize(path).is_ok_and(|q| p == q))
            })
        };
        if same(&self.viewer.borrow()) {
            return Some(self.active.get());
        }
        self.tabs
            .borrow()
            .iter()
            .position(|t| t.as_ref().is_some_and(same))
    }

    /// Puts `path` first in Open recent.
    fn remember(&self, path: &Path) {
        // Whole, but without the long-path prefix canonicalizing adds on
        // Windows, so Open recent reads as the person wrote it.
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        let list = chest::remembered(&self.recent.borrow(), &path);
        if let Some(chest) = &self.chest {
            // Not remembering a file is no reason to stop.
            let _ = chest::store_recent(chest, &list);
        }
        *self.recent.borrow_mut() = list;
    }

    /// Shows tab `at`, keeping the tool, color and line in use.
    fn switch_to(&self, at: usize) {
        let active = self.active.get();
        if at == active {
            return;
        }
        {
            let mut tabs = self.tabs.borrow_mut();
            let Some(mut next) = tabs.get_mut(at).and_then(Option::take) else {
                return;
            };
            let mut viewer = self.viewer.borrow_mut();
            viewer.share_preferences(&mut next);
            let old = std::mem::replace(&mut *viewer, next);
            tabs[active] = Some(old);
            self.active.set(at);
        }
        *self.generation.borrow_mut() += 1;
        self.draw_thumbnails();
        self.refresh();
    }

    /// Closes tab `at`, asking first if it has changes not saved. The
    /// last tab is emptied instead.
    fn close_tab(&self, at: usize) {
        let active = self.active.get();
        let count = self.tabs.borrow().len();
        if at >= count {
            return;
        }
        let modified = if at == active {
            self.viewer.borrow().modified()
        } else {
            self.tabs.borrow()[at]
                .as_ref()
                .is_some_and(Viewer::modified)
        };
        if modified && !self.dialogs.discard_changes() {
            return;
        }
        if count == 1 {
            let empty = self.viewer.borrow().new_tab();
            *self.viewer.borrow_mut() = empty;
        } else if at == active {
            // The tab after it, or before it when it was the last.
            let next = if at + 1 < count { at + 1 } else { at - 1 };
            self.switch_to(next);
            let mut tabs = self.tabs.borrow_mut();
            tabs.remove(at);
            if next > at {
                self.active.set(next - 1);
            }
        } else {
            self.tabs.borrow_mut().remove(at);
            if at < active {
                self.active.set(active - 1);
            }
        }
        *self.generation.borrow_mut() += 1;
        self.draw_thumbnails();
        self.refresh();
    }

    /// Whether any tab has changes not saved.
    fn any_modified(&self) -> bool {
        self.viewer.borrow().modified() || self.tabs.borrow().iter().flatten().any(Viewer::modified)
    }

    fn save(&self, ask: bool) {
        let suggested = self
            .viewer
            .borrow()
            .path()
            .and_then(Path::file_name)
            .map_or_else(
                || "drawing.pdf".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
        let path = if ask {
            match self.dialogs.save_path(&suggested, FileKind::Pdf) {
                Some(path) => Some(path),
                None => return,
            }
        } else {
            None
        };
        let result = self.viewer.borrow_mut().save(path.as_deref());
        self.refresh();
        if let Err(message) = result {
            self.status(&message);
        }
    }

    fn compare(&self) {
        let Some(path) = self.dialogs.open_path() else {
            return;
        };
        let result = self.viewer.borrow_mut().compare_with(&path);
        *self.generation.borrow_mut() += 1;
        self.refresh();
        if let Err(message) = result {
            self.status(&message);
        }
    }

    fn select_all(&self) {
        self.change(Viewer::select_all);
    }

    fn export_summary(&self) {
        let suggested = self
            .viewer
            .borrow()
            .path()
            .and_then(Path::file_stem)
            .map_or_else(
                || "drawing".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
        let Some(path) = self
            .dialogs
            .save_path(&format!("{suggested} (markup summary).pdf"), FileKind::Pdf)
        else {
            return;
        };
        let result = self.viewer.borrow_mut().export_summary(&path);
        self.refresh();
        if let Err(message) = result {
            self.status(&message);
        }
    }

    fn export_flattened(&self) {
        let suggested = self
            .viewer
            .borrow()
            .path()
            .and_then(Path::file_stem)
            .map_or_else(
                || "drawing".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
        let Some(path) = self
            .dialogs
            .save_path(&format!("{suggested} (flattened).pdf"), FileKind::Pdf)
        else {
            return;
        };
        let result = self.viewer.borrow_mut().export_flattened(&path);
        self.refresh();
        if let Err(message) = result {
            self.status(&message);
        }
    }

    fn export_list(&self) {
        let suggested = self
            .viewer
            .borrow()
            .path()
            .and_then(Path::file_stem)
            .map_or_else(
                || "markups".to_owned(),
                |n| format!("{} markups", n.to_string_lossy()),
            );
        let Some(path) = self
            .dialogs
            .save_path(&format!("{suggested}.csv"), FileKind::List)
        else {
            return;
        };
        let result = self.viewer.borrow_mut().export_list(&path);
        self.refresh();
        if let Err(message) = result {
            self.status(&message);
        }
    }

    /// Draws a small picture of every page, on a thread of its own, and
    /// shows each as it is ready; another file opened meanwhile wins.
    fn draw_thumbnails(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        // The bookmarks change with the document too.
        let marks: Vec<BookmarkInfo> = self
            .viewer
            .borrow()
            .bookmarks()
            .iter()
            .map(|b| BookmarkInfo {
                title: b.title.as_str().into(),
                level: i32::try_from(b.level).unwrap_or(0),
                has_page: b.page.is_some(),
            })
            .collect();
        window.set_bookmarks(ModelRc::new(VecModel::from(marks)));
        let Some(document) = self.viewer.borrow().review().map(|r| r.document().clone()) else {
            window.set_thumbnails(ModelRc::default());
            return;
        };
        let count = document.pages().len().min(MAX_THUMBNAILS);
        let blank = vec![Image::default(); count];
        window.set_thumbnails(ModelRc::new(VecModel::from(blank)));
        let generation = *self.generation.borrow();
        let weak = window.as_weak();
        std::thread::spawn(move || {
            for index in 0..count {
                let Some(bitmap) = thumbnail(&document, index) else {
                    continue;
                };
                let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                    &bitmap.pixels,
                    bitmap.width,
                    bitmap.height,
                );
                let delivered = weak.upgrade_in_event_loop(move |window| {
                    if window.get_document_number() != i32::try_from(generation).unwrap_or(-1) {
                        return;
                    }
                    let thumbnails = window.get_thumbnails();
                    if index < thumbnails.row_count() {
                        thumbnails.set_row_data(index, Image::from_rgba8(buffer));
                    }
                });
                if delivered.is_err() {
                    return;
                }
            }
        });
        window.set_document_number(i32::try_from(generation).unwrap_or(-1));
    }

    fn status(&self, message: &str) {
        if let Some(window) = self.window.upgrade() {
            window.set_status(message.into());
        }
    }

    /// Looks for `query` in every open drawing; the window lists where it
    /// is, drawing by drawing.
    fn find_everywhere(&self, query: &str) {
        let query = query.trim();
        let mut lines = Vec::new();
        let mut where_found = Vec::new();
        {
            let viewer = self.viewer.borrow();
            let tabs = self.tabs.borrow();
            for (at, tab) in tabs.iter().enumerate() {
                let v = tab.as_ref().unwrap_or(&viewer);
                let (pages, times) = v.count_found(query);
                if times > 0 {
                    lines.push(format!(
                        "{} — {times} on {pages} page{}",
                        v.tab_title(),
                        if pages == 1 { "" } else { "s" }
                    ));
                    where_found.push(at);
                }
            }
        }
        if lines.is_empty() && !query.is_empty() {
            lines.push(format!("\"{query}\" is in none of the open drawings."));
        }
        *self.found_in.borrow_mut() = (query.to_owned(), where_found);
        if let Some(window) = self.window.upgrade() {
            let rows: Vec<StandardListViewItem> = lines
                .iter()
                .map(|l| StandardListViewItem::from(l.as_str()))
                .collect();
            window.set_found_everywhere(ModelRc::new(VecModel::from(rows)));
        }
    }

    /// Shows the drawing found in, at its first match.
    fn show_found_in(&self, row: usize) {
        let (query, tab) = {
            let found = self.found_in.borrow();
            (found.0.clone(), found.1.get(row).copied())
        };
        if let Some(tab) = tab {
            self.switch_to(tab);
            self.change(|v| v.find(&query));
        }
    }

    /// Opens `design` in the stamp editor's tab, kept under `was` when it
    /// was kept already.
    fn open_editor(&self, design: Design, was: Option<String>) {
        *self.editor.borrow_mut() = Some(StampEditor { design, was });
        self.editor_message.borrow_mut().clear();
        self.editor_shown.set(true);
        self.refresh();
        self.refresh_editor_preview();
    }

    /// Changes the stamp in the editor through `change`; draws its preview
    /// again when `looks` says how it looks changed.
    fn edit_design(&self, looks: bool, change: impl FnOnce(&mut Design)) {
        if let Some(editor) = self.editor.borrow_mut().as_mut() {
            change(&mut editor.design);
        }
        self.editor_message.borrow_mut().clear();
        self.refresh_editor();
        if looks {
            self.refresh_editor_preview();
        }
    }

    /// Shows the stamp editor's state.
    fn refresh_editor(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let editor = self.editor.borrow();
        window.set_editor_open(editor.is_some());
        window.set_editor_shown(editor.is_some() && self.editor_shown.get());
        window.set_editor_message(self.editor_message.borrow().as_str().into());
        let Some(editor) = editor.as_ref() else {
            return;
        };
        let design = &editor.design;
        window.set_editor_name(design.name.as_str().into());
        let lines: Vec<EditorLine> = design
            .lines
            .iter()
            .map(|l| EditorLine {
                text: l.text.as_str().into(),
                #[expect(clippy::cast_possible_truncation, reason = "sizes from 4 to 96")]
                size: l.size.round() as i32,
                bold: l.bold,
            })
            .collect();
        let model = window.get_editor_lines();
        if model.row_count() == lines.len() {
            // Row by row, so the line being typed in keeps its cursor.
            for (i, line) in lines.into_iter().enumerate() {
                if model.row_data(i).as_ref() != Some(&line) {
                    model.set_row_data(i, line);
                }
            }
        } else {
            window.set_editor_lines(ModelRc::new(VecModel::from(lines)));
        }
        let align = stamps::Align::ALL.iter().position(|a| *a == design.align);
        let border = stamps::Border::ALL.iter().position(|b| *b == design.border);
        window.set_editor_align(align.and_then(|i| i32::try_from(i).ok()).unwrap_or(1));
        window.set_editor_border(border.and_then(|i| i32::try_from(i).ok()).unwrap_or(2));
        window.set_editor_color(
            COLORS
                .iter()
                .position(|&(_, c)| {
                    c.iter()
                        .zip(design.color)
                        .all(|(a, b)| (a - b).abs() < 1e-6)
                })
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
        window.set_editor_fill(design.fill);
    }

    /// Draws the stamp in the editor again, filled in as if placed now.
    fn refresh_editor_preview(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let Some(design) = self.editor.borrow().as_ref().map(|e| e.design.clone()) else {
            return;
        };
        let fields = self.viewer.borrow().stamp_fields();
        window.set_editor_preview(stamp_preview(&design, &fields).unwrap_or_default());
    }

    /// Shows the sheet set.
    fn refresh_set(&self, window: &ReviewWindow, viewer: &Viewer) {
        let open = {
            let tabs = self.tabs.borrow();
            let mut open: Vec<PathBuf> = tabs
                .iter()
                .flatten()
                .filter_map(|t| t.path().map(Path::to_owned))
                .collect();
            open.extend(viewer.path().map(Path::to_owned));
            open
        };
        let set = self.set.borrow();
        let sheets: Vec<SheetInfo> = set
            .sheets
            .iter()
            .map(|s| {
                let shown = viewer.path() == Some(s.path.as_path());
                let state = if shown {
                    "Shown"
                } else if open.contains(&s.path) {
                    "Open"
                } else if s.path.is_file() {
                    "Not open"
                } else {
                    "Missing"
                };
                SheetInfo {
                    label: s.label.as_str().into(),
                    file: s
                        .path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                        .into(),
                    state: state.into(),
                    shown,
                }
            })
            .collect();
        window.set_sheets(ModelRc::new(VecModel::from(sheets)));
        window.set_set_name(set.name.as_str().into());
    }

    /// Shows the tabs and Open recent.
    fn refresh_tabs(&self, window: &ReviewWindow, viewer: &Viewer) {
        self.refresh_set(window, viewer);
        let tabs: Vec<TabInfo> = self
            .tabs
            .borrow()
            .iter()
            .map(|t| {
                let v = t.as_ref().unwrap_or(viewer);
                TabInfo {
                    title: v.tab_title().into(),
                    modified: v.modified(),
                }
            })
            .collect();
        window.set_tabs(ModelRc::new(VecModel::from(tabs)));
        window.set_active_tab(i32::try_from(self.active.get()).unwrap_or(0));
        let recent: Vec<SharedString> = self
            .recent
            .borrow()
            .iter()
            .map(|p| p.to_string_lossy().as_ref().into())
            .collect();
        window.set_recent(strings(&recent));
    }

    /// Shows the viewer's state in the window.
    fn refresh(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let viewer = self.viewer.borrow();
        window.set_window_title(viewer.title().into());
        window.set_has_document(viewer.review().is_some());
        window.set_page_label(viewer.page_label().into());
        window.set_page_index(i32::try_from(viewer.page()).unwrap_or(0));
        drop(viewer);
        self.refresh_editor();
        let viewer = self.viewer.borrow();
        self.refresh_tabs(&window, &viewer);
        let count = viewer.review().map_or(0, |r| r.document().pages().len());
        let chosen = viewer.pages_chosen();
        let marks: Vec<bool> = (0..count.min(MAX_THUMBNAILS))
            .map(|p| chosen.len() > 1 && chosen.contains(&p))
            .collect();
        window.set_pages_chosen(ModelRc::new(VecModel::from(marks)));
        window.set_pages_label(viewer.pages_label().into());
        let names: Vec<SharedString> = viewer.page_names().into_iter().map(Into::into).collect();
        window.set_page_names(strings(&names));
        window.set_can_change_pages(viewer.can_change_pages());
        window.set_found_label(viewer.found_label().into());
        window.set_comparing(viewer.compared().is_some());
        window.set_zoom_label(viewer.zoom_label().into());
        window.set_turn_label(viewer.turn_label().into());
        window.set_only_listed(viewer.only_listed());
        let colors: Vec<CaptureColor> = viewer
            .capture_colors()
            .into_iter()
            .map(|(from, to)| CaptureColor {
                from: color(from),
                to: color(to.unwrap_or(from)),
                changed: to.is_some(),
            })
            .collect();
        window.set_capture_colors(ModelRc::new(VecModel::from(colors)));
        window.set_capture_selected(viewer.capture_selected());
        refresh_quantities(&window, &viewer);
        window.set_scale_label(viewer.scale_label().into());
        window.set_snap_label(viewer.snap_label().into());
        window.set_snap_on(viewer.snapping());
        window.set_wheel_zooms(viewer.wheel_zooms());
        window.set_status(viewer.status().into());
        window.set_tool(
            TOOLS
                .iter()
                .position(|&t| t == viewer.tool())
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(0),
        );
        window.set_chosen_color(i32::try_from(viewer.color()).unwrap_or(0));
        window.set_units(i32::from(viewer.units == Units::Metric));
        window.set_calibrating(viewer.calibrating());
        window.set_building(viewer.building_by_points());
        let (undo, redo) = viewer.history();
        window.set_can_undo(undo);
        window.set_can_redo(redo);
        window.set_width_index(i32::try_from(viewer.width()).unwrap_or(1));
        window.set_fill_shapes(viewer.fill());
        window.set_stamp(i32::try_from(viewer.stamp()).unwrap_or(0));
        let stamps: Vec<SharedString> = viewer.stamp_names().into_iter().map(Into::into).collect();
        window.set_stamps(strings(&stamps));
        window.set_built_in_stamps(i32::try_from(Viewer::built_in_stamps()).unwrap_or(0));
        window.set_stamping(viewer.tool() == Tool::Draw(Kind::Stamp));
        refresh_tool_sets(&window, &viewer);
        let selected = viewer.selected();
        window.set_has_selection(selected.is_some());
        window.set_selected_count(i32::try_from(viewer.selected_count()).unwrap_or(i32::MAX));
        let replies: Vec<ReplyInfo> = viewer
            .replies()
            .into_iter()
            .map(|(header, text, mine)| ReplyInfo {
                header: header.into(),
                text: text.into(),
                mine,
            })
            .collect();
        window.set_replies(ModelRc::new(VecModel::from(replies)));
        window.set_selected_text(selected.map(|m| m.text.as_str()).unwrap_or_default().into());
        window.set_selected_subject(
            selected
                .map(opencabinet_review::Markup::subject)
                .unwrap_or_default()
                .into(),
        );
        window.set_selected_status(
            selected
                .and_then(|m| Status::ALL.iter().position(|&s| s == m.status))
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(0),
        );
        drop(viewer);
        self.refresh_session();
        self.refresh_list();
        self.refresh_page(&window);
        self.refresh_overlay();
    }

    /// Shows the markup list.
    fn refresh_list(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let viewer = self.viewer.borrow();
        let rows: Vec<ModelRc<StandardListViewItem>> = viewer
            .rows()
            .into_iter()
            .map(|row| {
                let cells: Vec<StandardListViewItem> = row
                    .iter()
                    .map(|cell| StandardListViewItem::from(cell.as_str()))
                    .collect();
                ModelRc::new(VecModel::from(cells))
            })
            .collect();
        window.set_rows(ModelRc::new(VecModel::from(rows)));
        window.set_list_label(viewer.list_label().into());
        window.set_selected_row(
            viewer
                .selected_row()
                .and_then(|r| i32::try_from(r).ok())
                .unwrap_or(-1),
        );
    }

    /// Draws the page again when what is shown of it changed.
    fn refresh_page(&self, window: &ReviewWindow) {
        let scale = f64::from(window.window().scale_factor());
        let mut viewer = self.viewer.borrow_mut();
        viewer.keep_sight();
        let Some(view) = viewer.view(scale) else {
            return;
        };
        let compared = viewer.compared().cloned();
        let Some(review) = viewer.review_mut() else {
            return;
        };
        let (document, version) = match review.page_document() {
            Ok(shown) => shown,
            Err(e) => {
                window.set_status(format!("Cannot draw the page: {e}").into());
                return;
            }
        };
        let key = (view, *self.generation.borrow(), version);
        if self.drawn.borrow().as_ref() == Some(&key) {
            return;
        }
        let drawn = match compared {
            Some(other) => render(document, view).and_then(|mine| {
                let page = view.page.min(other.pages().len().saturating_sub(1));
                let theirs = render(&other, View { page, ..view })?;
                opencabinet_pdf::overlay(&mine, &theirs)
            }),
            None => {
                self.tiles
                    .borrow_mut()
                    .view(document, view, (*self.generation.borrow(), version))
            }
        };
        match drawn {
            Ok(bitmap) => {
                if self.tiles.borrow().waiting && !self.tile_timer.running() {
                    self.tile_timer.restart();
                }
                let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                    &bitmap.pixels,
                    bitmap.width,
                    bitmap.height,
                );
                window.set_page_image(Image::from_rgba8(buffer));
                *self.drawn.borrow_mut() = Some(key);
            }
            Err(e) => window.set_status(format!("Cannot draw the page: {e}").into()),
        }
    }

    /// Draws the markups again.
    #[expect(clippy::cast_possible_truncation, reason = "screen pixels")]
    fn refresh_overlay(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let (shapes, labels) = self.viewer.borrow().overlay();
        let shapes: Vec<Shape> = shapes
            .into_iter()
            .map(|s| {
                let [x, y, w, h] = s.bounds().unwrap_or_default();
                Shape {
                    x: x as f32,
                    y: y as f32,
                    w: w as f32,
                    h: h as f32,
                    commands: s.commands.into(),
                    stroke: s.stroke.map_or(Color::from_argb_u8(0, 0, 0, 0), color),
                    fill: s
                        .fill
                        .map_or(Color::from_argb_u8(0, 0, 0, 0), |(rgb, alpha)| {
                            color(rgb).with_alpha(alpha as f32)
                        }),
                    width: s.width as f32,
                    selected: s.selected,
                }
            })
            .collect();
        let labels: Vec<Label> = labels
            .into_iter()
            .map(|l| Label {
                x: l.x as f32,
                y: l.y as f32,
                text: l.text.into(),
                color: color(l.color),
                size: l.size as f32,
                bold: l.bold,
            })
            .collect();
        window.set_shapes(ModelRc::new(VecModel::from(shapes)));
        window.set_labels(ModelRc::new(VecModel::from(labels)));
        window.set_window_title(self.viewer.borrow().title().into());
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "color channels from 0 to 1"
)]
fn color([r, g, b]: [f64; 3]) -> Color {
    let channel = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::from_rgb_u8(channel(r), channel(g), channel(b))
}

fn strings(items: &[SharedString]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(items.to_vec()))
}
