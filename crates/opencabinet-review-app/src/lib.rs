// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! OpenCabinet Review's window (RFC-0004), as a library so tests can drive
//! it without a window system (see `tests/window.rs`).
//!
//! The page is drawn into an image only when what is shown of it changes;
//! the markups are drawn over it by the window, so drawing or moving one
//! never redraws the page.

pub mod dialogs;
pub mod viewer;

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use opencabinet_model::Units;
use opencabinet_pdf::{View, render};
use slint::{
    CloseRequestResponse, Color, ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer,
    SharedString, StandardListViewItem, VecModel,
};

use crate::dialogs::{Dialogs, FileKind, SystemDialogs};
use crate::viewer::{COLORS, TOOLS, Viewer};

/// The code Slint generates from `ui/review.slint`, which follows Slint's
/// conventions rather than this workspace's lints.
#[allow(missing_docs, clippy::pedantic)]
mod ui {
    slint::include_modules!();
}

pub use ui::{Label, ReviewWindow, Shape};

/// The name and version shown to users.
pub const NAME_AND_VERSION: &str = concat!("OpenCabinet Review ", env!("CARGO_PKG_VERSION"));

/// Creates the window, asking the user through the system's dialogs, with
/// the PDF at `path` open, if any.
///
/// # Errors
/// When the platform cannot create a window.
pub fn create(path: Option<&Path>) -> Result<ReviewWindow, slint::PlatformError> {
    create_with(Rc::new(SystemDialogs), path)
}

struct App {
    viewer: RefCell<Viewer>,
    dialogs: Rc<dyn Dialogs>,
    window: slint::Weak<ReviewWindow>,
    /// What the page image shows, so it is drawn again only when that
    /// changes.
    drawn: RefCell<Option<(View, usize)>>,
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
    let window = ReviewWindow::new()?;
    let app = Rc::new(App {
        viewer: RefCell::new(Viewer::default()),
        dialogs,
        window: window.as_weak(),
        drawn: RefCell::new(None),
        generation: RefCell::new(0),
    });

    let tools: Vec<SharedString> = TOOLS.iter().map(|t| t.label().into()).collect();
    window.set_tools(ModelRc::new(VecModel::from(tools)));
    let colors: Vec<Color> = COLORS.iter().map(|&(_, rgb)| color(rgb)).collect();
    window.set_colors(ModelRc::new(VecModel::from(colors)));
    let names: Vec<SharedString> = COLORS.iter().map(|&(name, _)| name.into()).collect();
    window.set_color_names(ModelRc::new(VecModel::from(names)));
    window.set_author(app.viewer.borrow().author.as_str().into());

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
    window.on_cancel(on(|app| app.change(Viewer::cancel)));
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
        a.viewer.borrow_mut().set_text(&text);
        // Not the whole window: the field being typed in keeps its text.
        a.refresh_list();
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
        f(&mut self.viewer.borrow_mut());
        self.refresh();
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
        window.set_zoom_label(viewer.zoom_label().into());
        window.set_scale_label(viewer.scale_label().into());
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
        let selected = viewer.selected();
        window.set_has_selection(selected.is_some());
        window.set_selected_text(selected.map(|m| m.text.as_str()).unwrap_or_default().into());
        drop(viewer);
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
        let viewer = self.viewer.borrow();
        let Some(view) = viewer.view(scale) else {
            return;
        };
        let key = (view, *self.generation.borrow());
        if self.drawn.borrow().as_ref() == Some(&key) {
            return;
        }
        let Some(review) = viewer.review() else {
            return;
        };
        match render(review.document(), view) {
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
                color: color(s.color),
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
