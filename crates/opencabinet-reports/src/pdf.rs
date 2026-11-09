// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! PDFs to print (ADR-0015), on landscape pages: the cut list, with a
//! title, the problems found, and a table that continues across pages; and
//! the elevations, one wall to a page at a standard scale.

// Page layout works in floating point millimeters; the counts converted
// (rows, problems) are small and the results are positions on paper.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "page layout in floating point millimeters"
)]

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point};
use krilla::metadata::Metadata;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::surface::Surface;
use krilla::text::{Font, TextDirection};
use opencabinet_model::{Length, Library, Project, Units};
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

use crate::bill_of_materials::{Bom, BomRow};
use crate::cut_list::{CutList, CutRow};
use crate::elevations::{
    CABINETS, DIMENSIONS, FRONTS, HINGE_MARKS, OPENINGS, WALL, bounds, elevation_shapes,
    every_elevation,
};
use crate::sketch::Shape;

/// A sheet of paper, in millimeters, portrait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paper {
    /// US Letter, 8-1/2" × 11".
    Letter,
    /// ISO A4, 210 × 297 mm.
    A4,
}

impl Paper {
    /// The usual paper for a project's units: Letter for inches, A4 for
    /// millimeters.
    #[must_use]
    pub fn for_units(units: Units) -> Self {
        match units {
            Units::Imperial => Self::Letter,
            Units::Metric => Self::A4,
        }
    }

    /// Width and height in millimeters, landscape.
    fn landscape(self) -> (f32, f32) {
        match self {
            Self::Letter => (279.4, 215.9),
            Self::A4 => (297.0, 210.0),
        }
    }
}

/// The two faces of the bundled font the PDF uses.
pub struct Fonts {
    regular: Face,
    bold: Face,
}

impl Fonts {
    /// Reads the regular and bold faces from their TrueType files' bytes.
    ///
    /// # Errors
    /// When either is not a font.
    pub fn new(regular: Vec<u8>, bold: Vec<u8>) -> Result<Self, String> {
        Ok(Self {
            regular: Face::new(regular)?,
            bold: Face::new(bold)?,
        })
    }
}

/// A font face: for drawing, and for measuring text.
struct Face {
    bytes: Vec<u8>,
    font: Font,
}

impl Face {
    fn new(bytes: Vec<u8>) -> Result<Self, String> {
        FontRef::new(&bytes).map_err(|_| "not a font".to_owned())?;
        let font = Font::new(bytes.clone().into(), 0).ok_or("not a font")?;
        Ok(Self { bytes, font })
    }

    /// The width of `text` at `size` points, from the font's own metrics.
    fn width(&self, text: &str, size: f32) -> f32 {
        let font = FontRef::new(&self.bytes).expect("checked when loaded");
        let charmap = font.charmap();
        let metrics = font.glyph_metrics(Size::new(size), LocationRef::default());
        text.chars()
            .map(|c| {
                charmap
                    .map(c)
                    .and_then(|g| metrics.advance_width(g))
                    .unwrap_or(size / 2.0)
            })
            .sum()
    }
}

const PT_PER_MM: f32 = 72.0 / 25.4;
const MARGIN: f32 = 12.7;
const TITLE_SIZE: f32 = 14.0;
const TEXT_SIZE: f32 = 8.5;
const ROW: f32 = 6.0;
/// Each column's share of the table's width, in the order of
/// [`CutRow::TITLES`].
const COLUMNS: [f32; 10] = [0.07, 0.15, 0.17, 0.04, 0.09, 0.09, 0.07, 0.10, 0.10, 0.12];
/// The same for the bill of materials, in the order of [`BomRow::TITLES`].
const MATERIAL_COLUMNS: [f32; 4] = [0.10, 0.30, 0.12, 0.48];

/// The cut list as PDF bytes, with the bill of materials on a last page.
/// `title` heads every page and `generator`, such as "OpenCabinet Designer
/// 0.1.0", is written in its footer and metadata. The same input always
/// gives the same bytes.
///
/// # Errors
/// When the PDF cannot be written.
pub fn cut_list_pdf(
    list: &CutList,
    materials: &Bom,
    title: &str,
    generator: &str,
    paper: Paper,
    fonts: &Fonts,
) -> Result<Vec<u8>, String> {
    let (width, height) = paper.landscape();
    let table_top = MARGIN + 12.0 + list.problems.len() as f32 * 4.5;
    let rows_per_page = (((height - MARGIN - 8.0 - table_top) / ROW) as usize)
        .saturating_sub(1)
        .max(1);
    let chunks: Vec<&[CutRow]> = if list.rows.is_empty() {
        vec![&[]]
    } else {
        list.rows.chunks(rows_per_page).collect()
    };

    let mut document = Document::new();
    document.set_metadata(
        Metadata::new()
            .title(format!("Cut list — {title}"))
            .creator(generator.to_owned()),
    );
    let pages = chunks.len() + 1;
    for (index, rows) in chunks.into_iter().enumerate() {
        let settings = PageSettings::from_wh(width * PT_PER_MM, height * PT_PER_MM)
            .ok_or("the paper size is not positive")?;
        let mut page = document.start_page_with(settings);
        let mut surface = page.surface();
        let mut pen = Pen {
            surface: &mut surface,
        };

        pen.text(
            &fonts.bold,
            (MARGIN, MARGIN + 5.0),
            TITLE_SIZE,
            &format!("Cut list — {title}"),
        );
        let page_label = format!("Page {} of {pages}", index + 1);
        let label_width = fonts.regular.width(&page_label, TEXT_SIZE) / PT_PER_MM;
        pen.text(
            &fonts.regular,
            (width - MARGIN - label_width, MARGIN + 5.0),
            TEXT_SIZE,
            &page_label,
        );
        for (i, problem) in list.problems.iter().enumerate() {
            let y = MARGIN + 11.0 + i as f32 * 4.5;
            pen.text(&fonts.regular, (MARGIN, y), TEXT_SIZE, problem);
        }

        let table_width = width - 2.0 * MARGIN;
        let header = CutRow::TITLES;
        pen.row(&fonts.bold, table_top, table_width, &header, &COLUMNS);
        pen.rule(table_top + ROW, table_width, 0.4);
        for (i, row) in rows.iter().enumerate() {
            let y = table_top + ROW * (i as f32 + 1.0);
            pen.row(&fonts.regular, y, table_width, &row.cells(), &COLUMNS);
            pen.rule(y + ROW, table_width, 0.15);
        }
        pen.text(
            &fonts.regular,
            (MARGIN, height - MARGIN + 4.0),
            TEXT_SIZE - 1.5,
            generator,
        );

        surface.finish();
        page.finish();
    }
    materials_page(
        &mut document,
        materials,
        title,
        generator,
        paper,
        fonts,
        pages,
    )?;
    document
        .finish()
        .map_err(|e| format!("Cannot write the PDF: {e:?}"))
}

/// The bill of materials as the last page of the cut list: its notes,
/// then a table.
fn materials_page(
    document: &mut Document,
    materials: &Bom,
    title: &str,
    generator: &str,
    paper: Paper,
    fonts: &Fonts,
    pages: usize,
) -> Result<(), String> {
    let (width, height) = paper.landscape();
    let settings = PageSettings::from_wh(width * PT_PER_MM, height * PT_PER_MM)
        .ok_or("the paper size is not positive")?;
    let mut page = document.start_page_with(settings);
    let mut surface = page.surface();
    let mut pen = Pen {
        surface: &mut surface,
    };
    pen.text(
        &fonts.bold,
        (MARGIN, MARGIN + 5.0),
        TITLE_SIZE,
        &format!("Materials — {title}"),
    );
    let page_label = format!("Page {pages} of {pages}");
    let label_width = fonts.regular.width(&page_label, TEXT_SIZE) / PT_PER_MM;
    pen.text(
        &fonts.regular,
        (width - MARGIN - label_width, MARGIN + 5.0),
        TEXT_SIZE,
        &page_label,
    );
    for (i, note) in materials.notes.iter().enumerate() {
        let y = MARGIN + 11.0 + i as f32 * 4.5;
        pen.text(&fonts.regular, (MARGIN, y), TEXT_SIZE, note);
    }
    let table_top = MARGIN + 12.0 + materials.notes.len() as f32 * 4.5;
    let table_width = width - 2.0 * MARGIN;
    pen.row(
        &fonts.bold,
        table_top,
        table_width,
        &BomRow::TITLES,
        &MATERIAL_COLUMNS,
    );
    pen.rule(table_top + ROW, table_width, 0.4);
    for (i, row) in materials.rows.iter().enumerate() {
        let y = table_top + ROW * (i as f32 + 1.0);
        pen.row(
            &fonts.regular,
            y,
            table_width,
            &row.cells(),
            &MATERIAL_COLUMNS,
        );
        pen.rule(y + ROW, table_width, 0.15);
    }
    pen.text(
        &fonts.regular,
        (MARGIN, height - MARGIN + 4.0),
        TEXT_SIZE - 1.5,
        generator,
    );
    surface.finish();
    page.finish();
    Ok(())
}

/// The scales an elevation may be drawn at, largest first: the
/// architect's scales for inches, the metric ones for millimeters.
fn scales(units: Units) -> &'static [(f32, &'static str)] {
    match units {
        Units::Imperial => &[
            (4.0, "3\" = 1'-0\""),
            (8.0, "1-1/2\" = 1'-0\""),
            (12.0, "1\" = 1'-0\""),
            (16.0, "3/4\" = 1'-0\""),
            (24.0, "1/2\" = 1'-0\""),
            (32.0, "3/8\" = 1'-0\""),
            (48.0, "1/4\" = 1'-0\""),
            (96.0, "1/8\" = 1'-0\""),
            (192.0, "1/16\" = 1'-0\""),
        ],
        Units::Metric => &[
            (5.0, "1:5"),
            (10.0, "1:10"),
            (20.0, "1:20"),
            (25.0, "1:25"),
            (50.0, "1:50"),
            (100.0, "1:100"),
            (200.0, "1:200"),
        ],
    }
}

/// The elevations of every wall, one to a landscape page, each at the
/// largest standard scale that fits it, with lengths shown by `show` and
/// the scale in the units' own terms. `title` and `generator` are as for
/// [`cut_list_pdf`]. The same input always gives the same bytes.
///
/// # Errors
/// When the project has no walls, or the PDF cannot be written.
#[allow(clippy::too_many_arguments, reason = "what a printed set needs")]
pub fn elevations_pdf(
    project: &Project,
    library: &Library,
    units: Units,
    show: &dyn Fn(Length) -> String,
    title: &str,
    generator: &str,
    paper: Paper,
    fonts: &Fonts,
) -> Result<Vec<u8>, String> {
    let elevations = every_elevation(project, library);
    if elevations.is_empty() {
        return Err("The project has no walls to draw".into());
    }
    let (width, height) = paper.landscape();
    let area_top = MARGIN + 14.0;
    let area = (width - 2.0 * MARGIN, height - area_top - MARGIN - 6.0);

    let mut document = Document::new();
    document.set_metadata(
        Metadata::new()
            .title(format!("Elevations — {title}"))
            .creator(generator.to_owned()),
    );
    let pages = elevations.len();
    for (index, elevation) in elevations.iter().enumerate() {
        let shapes = elevation_shapes(elevation, None, show);
        let ((low_x, low_y), (high_x, high_y)) = bounds(&shapes);
        let extent = ((high_x - low_x) as f32, (high_y - low_y) as f32);
        let (scale, scale_label) = choose_scale(extent, area, units);
        // Centered in the drawing area, `y` up.
        let left = MARGIN + (area.0 - extent.0 / scale) / 2.0;
        let top = area_top + (area.1 - extent.1 / scale) / 2.0;
        let place = |(x, y): (f64, f64)| {
            (
                left + (x - low_x) as f32 / scale,
                top + (high_y - y) as f32 / scale,
            )
        };

        let settings = PageSettings::from_wh(width * PT_PER_MM, height * PT_PER_MM)
            .ok_or("the paper size is not positive")?;
        let mut page = document.start_page_with(settings);
        let mut surface = page.surface();
        let mut pen = Pen {
            surface: &mut surface,
        };
        pen.text(
            &fonts.bold,
            (MARGIN, MARGIN + 5.0),
            TITLE_SIZE,
            &format!("Elevation — {} - {}", elevation.room, elevation.name),
        );
        pen.text(&fonts.regular, (MARGIN, MARGIN + 10.0), TEXT_SIZE, title);
        let corner = format!("{scale_label} · Page {} of {pages}", index + 1);
        let corner_width = fonts.regular.width(&corner, TEXT_SIZE) / PT_PER_MM;
        pen.text(
            &fonts.regular,
            (width - MARGIN - corner_width, MARGIN + 5.0),
            TEXT_SIZE,
            &corner,
        );
        for shape in &shapes {
            pen.shape(fonts, shape, &place);
        }
        pen.text(
            &fonts.regular,
            (MARGIN, height - MARGIN + 4.0),
            TEXT_SIZE - 1.5,
            generator,
        );
        surface.finish();
        page.finish();
    }
    document
        .finish()
        .map_err(|e| format!("Cannot write the PDF: {e:?}"))
}

/// The largest standard scale at which a drawing `extent` millimeters
/// fits in `area`, as a divisor and its label; when none does, the
/// divisor that fits it exactly, "not to scale".
fn choose_scale(extent: (f32, f32), area: (f32, f32), units: Units) -> (f32, String) {
    scales(units)
        .iter()
        .find(|(s, _)| extent.0 / s <= area.0 && extent.1 / s <= area.1)
        .map_or_else(
            || {
                let fit = (extent.0 / area.0).max(extent.1 / area.1);
                (fit, "not to scale".to_owned())
            },
            |(s, label)| (*s, format!("Scale {label}")),
        )
}

/// How a layer's lines are drawn: their weight in points and gray level.
fn line_style(layer: &str) -> Option<(f32, u8)> {
    match layer {
        WALL => Some((0.7, 0)),
        OPENINGS | CABINETS => Some((0.4, 0)),
        FRONTS => Some((0.3, 0)),
        HINGE_MARKS | DIMENSIONS => Some((0.2, 120)),
        _ => None,
    }
}

/// Draws on a page in millimeters from the top-left corner.
struct Pen<'s, 'p> {
    surface: &'s mut Surface<'p>,
}

impl Pen<'_, '_> {
    /// `text` with its baseline starting at `at`.
    fn text(&mut self, face: &Face, at: (f32, f32), size: f32, text: &str) {
        self.surface.set_fill(Some(Fill::default()));
        self.surface.set_stroke(None);
        self.surface.draw_text(
            Point::from_xy(at.0 * PT_PER_MM, at.1 * PT_PER_MM),
            face.font.clone(),
            size,
            text,
            false,
            TextDirection::Auto,
        );
    }

    /// One table row whose top is at `top`, in columns with these shares
    /// of `width`; each cell is cut to its column, with an ellipsis,
    /// measured with the font.
    fn row(&mut self, face: &Face, top: f32, width: f32, cells: &[&str], columns: &[f32]) {
        let mut x = MARGIN;
        for (cell, &share) in cells.iter().zip(columns) {
            let column = share * width;
            let room = (column - 2.0) * PT_PER_MM;
            let text = fit(face, cell, TEXT_SIZE, room);
            self.text(face, (x + 1.0, top + ROW - 1.8), TEXT_SIZE, &text);
            x += column;
        }
    }

    /// One shape of a drawing, its points placed on the page by `place`.
    fn shape(&mut self, fonts: &Fonts, shape: &Shape, place: &dyn Fn((f64, f64)) -> (f32, f32)) {
        match shape {
            Shape::Polygon { layer, points } => {
                if let Some(style) = line_style(layer) {
                    let points: Vec<(f32, f32)> = points.iter().map(|p| place(*p)).collect();
                    self.lines(&points, true, style);
                }
            }
            Shape::Line { layer, from, to } => {
                if let Some(style) = line_style(layer) {
                    self.lines(&[place(*from), place(*to)], false, style);
                }
            }
            Shape::Circle { .. } => {}
            Shape::Text {
                at, text, centered, ..
            } => {
                let size = TEXT_SIZE - 1.5;
                let (mut x, mut y) = place(*at);
                if *centered {
                    x -= fonts.regular.width(text, size) / PT_PER_MM / 2.0;
                    y += size * 0.35 / PT_PER_MM;
                }
                self.text(&fonts.regular, (x, y), size, text);
            }
        }
    }

    /// Lines through `points`, back to the first when `closed`, with a
    /// weight in points and a gray level.
    fn lines(&mut self, points: &[(f32, f32)], closed: bool, (weight, gray): (f32, u8)) {
        let mut path = PathBuilder::new();
        for (i, (x, y)) in points.iter().enumerate() {
            let (x, y) = (x * PT_PER_MM, y * PT_PER_MM);
            if i == 0 {
                path.move_to(x, y);
            } else {
                path.line_to(x, y);
            }
        }
        if closed {
            path.close();
        }
        let Some(path) = path.finish() else { return };
        self.surface.set_fill(None);
        self.surface.set_stroke(Some(Stroke {
            paint: rgb::Color::new(gray, gray, gray).into(),
            width: weight,
            ..Stroke::default()
        }));
        self.surface.draw_path(&path);
    }

    /// A horizontal line under a row, `weight` points thick.
    fn rule(&mut self, y: f32, width: f32, weight: f32) {
        let mut path = PathBuilder::new();
        path.move_to(MARGIN * PT_PER_MM, y * PT_PER_MM);
        path.line_to((MARGIN + width) * PT_PER_MM, y * PT_PER_MM);
        let Some(path) = path.finish() else { return };
        self.surface.set_fill(None);
        self.surface.set_stroke(Some(Stroke {
            paint: rgb::Color::new(140, 140, 140).into(),
            width: weight,
            ..Stroke::default()
        }));
        self.surface.draw_path(&path);
    }
}

/// `text`, shortened with an ellipsis until it fits in `room` points.
fn fit(face: &Face, text: &str, size: f32, room: f32) -> String {
    if face.width(text, size) <= room {
        return text.to_owned();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>() + "…";
        if face.width(&candidate, size) <= room {
            return candidate;
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevations_are_drawn_at_the_largest_standard_scale_that_fits() {
        // 4 m by 3 m in 254 by 170 mm: 1:16 is too tall, 1:24 fits.
        let area = (254.0, 170.0);
        assert_eq!(
            choose_scale((4000.0, 3000.0), area, Units::Imperial),
            (24.0, "Scale 1/2\" = 1'-0\"".to_owned())
        );
        assert_eq!(
            choose_scale((4000.0, 3000.0), area, Units::Metric),
            (20.0, "Scale 1:20".to_owned())
        );
        // A 100 m wall fits no standard scale.
        assert_eq!(
            choose_scale((100_000.0, 3000.0), area, Units::Metric).1,
            "not to scale"
        );
    }
}
