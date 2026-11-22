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

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use opencabinet_model::Units;
use opencabinet_pdf::{View, render};
use slint::{
    CloseRequestResponse, Color, ComponentHandle, Image, Model, ModelRc, Rgba8Pixel,
    SharedPixelBuffer, SharedString, StandardListViewItem, VecModel,
};

use crate::dialogs::{Dialogs, FileKind, SystemDialogs};
use crate::viewer::{COLORS, Keys, TOOLS, Tool, Viewer, WIDTHS};
use opencabinet_review::{Kind, Status, stamps};

/// The code Slint generates from `ui/review.slint`, which follows Slint's
/// conventions rather than this workspace's lints.
#[allow(missing_docs, clippy::pedantic)]
mod ui {
    slint::include_modules!();
}

pub use ui::{BookmarkInfo, Label, ReplyInfo, ReviewWindow, SavedInfo, Shape, ToolInfo};

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
    build(Rc::new(SystemDialogs), path, chest::default_path())
}

struct App {
    viewer: RefCell<Viewer>,
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
    build(dialogs, path, None)
}

/// The window, keeping My tools at `saved` when given.
fn build(
    dialogs: Rc<dyn Dialogs>,
    path: Option<&Path>,
    saved: Option<PathBuf>,
) -> Result<ReviewWindow, slint::PlatformError> {
    let window = ReviewWindow::new()?;
    let app = Rc::new(App {
        viewer: RefCell::new(Viewer::default()),
        dialogs,
        window: window.as_weak(),
        drawn: RefCell::new(None),
        generation: RefCell::new(0),
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
    let words: Vec<SharedString> = stamps::TEMPLATES.iter().map(|t| t.words.into()).collect();
    window.set_stamps(strings(&words));
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
    match path {
        Some(path) => app.open(path),
        None => app.refresh(),
    }
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
    window.on_export_flattened(on(App::export_flattened));
    window.on_export_summary(on(App::export_summary));
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
    window.on_use_saved(move |i| {
        if let Ok(i) = usize::try_from(i) {
            a.change(|v| v.use_saved_tool(i));
        }
    });
    let a = Rc::clone(app);
    window.on_remove_saved(move |i| {
        let Ok(i) = usize::try_from(i) else {
            return;
        };
        let result = a.viewer.borrow_mut().remove_saved_tool(i);
        a.refresh();
        if let Err(message) = result {
            a.status(&message);
        }
    });
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
    window.on_released(move |x, y| a.change(|v| v.release((f64::from(x), f64::from(y)))));
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
        if a.viewer.borrow().modified() && !a.dialogs.discard_changes() {
            CloseRequestResponse::KeepWindowShown
        } else {
            CloseRequestResponse::HideWindow
        }
    });
}

impl App {
    fn change(&self, f: impl FnOnce(&mut Viewer)) {
        {
            let mut viewer = self.viewer.borrow_mut();
            f(&mut viewer);
            viewer.publish();
        }
        self.refresh();
    }

    /// Takes in others' changes in a review session.
    fn poll(&self) {
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
        window.set_hosting(viewer.invite().is_some());
        window.set_invite(
            viewer
                .invite()
                .map(ToString::to_string)
                .unwrap_or_default()
                .into(),
        );
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
        if self.viewer.borrow().modified() && !self.dialogs.discard_changes() {
            return;
        }
        if let Some(path) = self.dialogs.open_path() {
            self.open(&path);
        }
    }

    fn open(&self, path: &Path) {
        let result = self.viewer.borrow_mut().open(path);
        *self.generation.borrow_mut() += 1;
        self.draw_thumbnails();
        self.refresh();
        if let Err(message) = result {
            self.status(&message);
        }
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
        window.set_found_label(viewer.found_label().into());
        window.set_comparing(viewer.compared().is_some());
        window.set_zoom_label(viewer.zoom_label().into());
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
        window.set_stamping(viewer.tool() == Tool::Draw(Kind::Stamp));
        let saved: Vec<SavedInfo> = viewer
            .saved_tools()
            .iter()
            .map(|t| SavedInfo {
                name: t.name.as_str().into(),
                color: color(t.color),
            })
            .collect();
        window.set_saved_tools(ModelRc::new(VecModel::from(saved)));
        window.set_saved_in_use(
            viewer
                .saved_in_use()
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
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
            None => render(document, view),
        };
        match drawn {
            Ok(bitmap) => {
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
            .map(|s| Shape {
                commands: s.commands.into(),
                stroke: s.stroke.map_or(Color::from_argb_u8(0, 0, 0, 0), color),
                fill: s
                    .fill
                    .map_or(Color::from_argb_u8(0, 0, 0, 0), |(rgb, alpha)| {
                        color(rgb).with_alpha(alpha as f32)
                    }),
                width: s.width as f32,
                selected: s.selected,
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
