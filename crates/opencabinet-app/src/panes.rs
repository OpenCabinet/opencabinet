// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window's views: one, two side by side or stacked, or four, each
//! showing the project in 3D, the room's plan, an elevation or the edited
//! cabinet. 3D views have their own camera and share one renderer, made
//! the first time one is shown.

use opencabinet_model::Id;
use opencabinet_view3d::Renderer;
use slint::{ModelRc, VecModel};

use crate::designer::Designer;
use crate::drawing::Drawing;
use crate::ui;
use crate::viewer::{MODES, Viewer};

/// How many views each layout shows: one, two side by side, two stacked,
/// four.
pub const LAYOUTS: [usize; 4] = [1, 2, 2, 4];

/// What a view shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// The project in 3D.
    #[default]
    ThreeD,
    /// The edited room's plan.
    Plan,
    /// An elevation: of the edited wall, or the edited cabinet's.
    Elevation,
    /// The edited cabinet alone, from the front.
    Cabinet,
}

impl Kind {
    /// Every kind, in the order the window lists them.
    pub const ALL: [Self; 4] = [Self::ThreeD, Self::Plan, Self::Elevation, Self::Cabinet];

    /// The kind at `index` of [`Kind::ALL`].
    #[must_use]
    pub fn from_index(index: i32) -> Self {
        usize::try_from(index)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
            .unwrap_or_default()
    }

    fn index(self) -> i32 {
        match self {
            Self::ThreeD => 0,
            Self::Plan => 1,
            Self::Elevation => 2,
            Self::Cabinet => 3,
        }
    }
}

/// One view.
#[derive(Debug, Default)]
pub struct Pane {
    /// What it shows.
    pub kind: Kind,
    /// Its 3D view, kept while it shows something else.
    pub viewer: Viewer,
    /// Its size in logical pixels.
    pub size: (f32, f32),
    /// The drawing it shows, to find what a click is on.
    pub drawing: Option<Drawing>,
    /// Where a button went down on a drawing, and how far it moved since.
    pub press: Option<((f32, f32), f32)>,
}

impl Pane {
    /// The point of the drawing under `(x, y)`, in the shapes' millimeters
    /// (`y` up), as the window scales and centers it.
    #[must_use]
    pub fn drawing_point(&self, x: f32, y: f32) -> Option<(f64, f64)> {
        let drawing = self.drawing.as_ref()?;
        let (w, h) = self.size;
        let scale = ((w - 32.0) / drawing.width).min((h - 32.0) / drawing.height);
        if scale <= 0.0 {
            return None;
        }
        let left = (w - drawing.width * scale) / 2.0;
        let top = (h - drawing.height * scale) / 2.0;
        let (dx, dy) = (f64::from((x - left) / scale), f64::from((y - top) / scale));
        Some((drawing.origin.0 + dx, drawing.origin.1 - dy))
    }
}

/// The window's views and the renderer their 3D views share.
#[derive(Debug)]
pub struct Panes {
    /// Four views, of which the layout shows the first ones.
    pub panes: Vec<Pane>,
    /// The layout, in [`LAYOUTS`].
    pub layout: usize,
    renderer: Option<Result<Renderer, String>>,
}

impl Default for Panes {
    /// One view of the plan; the others, when shown, start as 3D, an
    /// elevation and the cabinet. The window does not open on 3D, so that
    /// starting costs no more than it did (ADR-0014): the renderer is made
    /// when a view first shows 3D.
    fn default() -> Self {
        Self {
            panes: [Kind::Plan, Kind::ThreeD, Kind::Elevation, Kind::Cabinet]
                .into_iter()
                .map(|kind| Pane {
                    kind,
                    ..Pane::default()
                })
                .collect(),
            layout: 0,
            renderer: None,
        }
    }
}

impl Panes {
    /// How many views the layout shows.
    #[must_use]
    pub fn count(&self) -> usize {
        LAYOUTS[self.layout.min(LAYOUTS.len() - 1)]
    }

    /// The project changed: every 3D view builds its scene again.
    pub fn invalidate(&mut self) {
        for pane in &mut self.panes {
            pane.viewer.invalidate();
        }
    }

    /// What view `i` shows, drawn now; `highlight` is the cabinet being
    /// edited, outlined in 3D.
    pub fn data(&mut self, i: usize, designer: &Designer, highlight: Option<Id>) -> ui::PaneData {
        let pane = &mut self.panes[i];
        let mut data = ui::PaneData {
            kind: pane.kind.index(),
            fronts: pane.viewer.view.fronts,
            explode: pane.viewer.view.explode,
            isolated: pane.viewer.view.isolated.is_some(),
            mode: MODES
                .iter()
                .position(|m| m.1 == pane.viewer.view.mode)
                .and_then(|m| i32::try_from(m).ok())
                .unwrap_or(0),
            ..ui::PaneData::default()
        };
        let drawing = match pane.kind {
            Kind::ThreeD => {
                pane.drawing = None;
                // Not drawn before the window gives the view its size.
                if pane.size.0 < 1.0 || pane.size.1 < 1.0 {
                    return data;
                }
                pane.viewer.highlight(highlight);
                let renderer = self.renderer.get_or_insert_with(Renderer::new);
                let pane = &mut self.panes[i];
                match renderer {
                    Ok(renderer) => {
                        match pane.viewer.draw(designer, renderer, i) {
                            Ok(image) => data.image = image,
                            Err(message) => data.note = message.into(),
                        }
                        if renderer.software {
                            data.note = "Drawn without a graphics card, which is slower".into();
                        }
                    }
                    Err(message) => data.note = message.clone().into(),
                }
                data.info = pane.viewer.describe(designer).into();
                return data;
            }
            Kind::Plan => designer.plan_view().unwrap_or_else(|| {
                data.note = "Add a wall to see the room's plan".into();
                designer.cabinet_view()
            }),
            Kind::Elevation => designer.elevation_view(),
            Kind::Cabinet => designer.cabinet_view(),
        };
        data.drawing = crate::drawing_data(drawing.clone());
        self.panes[i].drawing = Some(drawing);
        data
    }

    /// Every view the layout shows, drawn now.
    pub fn all(&mut self, designer: &Designer, highlight: Option<Id>) -> Vec<ui::PaneData> {
        (0..self.count())
            .map(|i| self.data(i, designer, highlight))
            .collect()
    }
}

/// Puts `rows` in the window's model, replacing only what changed in
/// length so the views keep their state.
pub fn update(model: &VecModel<ui::PaneData>, rows: Vec<ui::PaneData>) {
    use slint::Model;
    while model.row_count() > rows.len() {
        model.remove(model.row_count() - 1);
    }
    for (i, row) in rows.into_iter().enumerate() {
        if i < model.row_count() {
            model.set_row_data(i, row);
        } else {
            model.push(row);
        }
    }
}

/// The model the window's views read.
#[must_use]
pub fn model(rows: &std::rc::Rc<VecModel<ui::PaneData>>) -> ModelRc<ui::PaneData> {
    ModelRc::from(rows.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_lands_on_the_drawing_as_the_window_scales_it() {
        // A drawing 1000 by 500 mm in a view 1032 by 532 pixels: one pixel
        // a millimeter, 16 pixels of margin all round.
        let pane = Pane {
            size: (1032.0, 532.0),
            drawing: Some(Drawing {
                width: 1000.0,
                height: 500.0,
                origin: (-100.0, 600.0),
                ..Drawing::default()
            }),
            ..Pane::default()
        };
        assert_eq!(pane.drawing_point(116.0, 66.0), Some((0.0, 550.0)));
        assert_eq!(Pane::default().drawing_point(1.0, 1.0), None);
    }
}
