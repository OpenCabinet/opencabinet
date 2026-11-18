// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drawings as plain shapes on named layers, in millimeters with `y` up,
//! and how they are written as DXF (ADR-0015). The parts' DXF, the
//! elevations' DXF and PDF, and the window's drawing area all draw them.

use chrono::{Local, NaiveDate, TimeZone, Utc};
use dxf::entities::{Circle, Entity, EntityType, Line, LwPolyline, Text};
use dxf::enums::{AcadVersion, HorizontalTextJustification, Units, VerticalTextJustification};
use dxf::tables::Layer;
use dxf::{Color, Drawing, LwPolylineVertex, Point};

/// A DXF file, named for what it draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxfFile {
    /// A file name, such as `B30 - Left side.dxf`.
    pub name: String,
    /// The file's bytes.
    pub contents: Vec<u8>,
}

/// Something drawn, in millimeters.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A closed polyline.
    Polygon {
        /// The layer it is on.
        layer: String,
        /// Its corners, in order.
        points: Vec<(f64, f64)>,
    },
    /// A circle.
    Circle {
        /// The layer it is on.
        layer: String,
        /// Its center.
        center: (f64, f64),
        /// Its radius.
        radius: f64,
    },
    /// A straight line.
    Line {
        /// The layer it is on.
        layer: String,
        /// Its start.
        from: (f64, f64),
        /// Its end.
        to: (f64, f64),
    },
    /// One line of text.
    Text {
        /// The layer it is on.
        layer: String,
        /// The start of its baseline, or its center when `centered`.
        at: (f64, f64),
        /// Its height, in the drawing's millimeters.
        height: f64,
        /// What it says.
        text: String,
        /// Whether `at` is its center rather than its start.
        centered: bool,
    },
}

impl Shape {
    /// The same shape, moved by `(dx, dy)`.
    #[must_use]
    pub fn moved(&self, (dx, dy): (f64, f64)) -> Self {
        let shift = |(x, y): (f64, f64)| (x + dx, y + dy);
        match self {
            Self::Polygon { layer, points } => Self::Polygon {
                layer: layer.clone(),
                points: points.iter().copied().map(shift).collect(),
            },
            Self::Circle {
                layer,
                center,
                radius,
            } => Self::Circle {
                layer: layer.clone(),
                center: shift(*center),
                radius: *radius,
            },
            Self::Line { layer, from, to } => Self::Line {
                layer: layer.clone(),
                from: shift(*from),
                to: shift(*to),
            },
            Self::Text {
                layer,
                at,
                height,
                text,
                centered,
            } => Self::Text {
                layer: layer.clone(),
                at: shift(*at),
                height: *height,
                text: text.clone(),
                centered: *centered,
            },
        }
    }

    /// The layer it is on.
    #[must_use]
    pub fn layer(&self) -> &str {
        match self {
            Self::Polygon { layer, .. }
            | Self::Circle { layer, .. }
            | Self::Line { layer, .. }
            | Self::Text { layer, .. } => layer,
        }
    }
}

/// A file name without the characters Windows, macOS and Linux reject.
pub(crate) fn safe_file_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim_end_matches(['.', ' '])
        .to_owned()
}

/// The `AutoCAD` Color Index of a layer: white for outlines, walls and
/// cabinets, red for holes, blue for grooves, cyan for openings, green for
/// fronts, yellow for dimensions, gray for labels and hinge marks.
fn color(layer: &str) -> u8 {
    match layer {
        _ if layer.starts_with("HOLE_") => 1,
        _ if layer.starts_with("GROOVE_") => 5,
        "OPENINGS" => 4,
        "FRONTS" => 3,
        "DIMENSIONS" => 2,
        "LABEL" | "HINGE_MARKS" => 8,
        _ => 7,
    }
}

/// The shapes as a DXF 2000 file in millimeters.
pub(crate) fn write_dxf(shapes: &[Shape]) -> Result<Vec<u8>, String> {
    let mut drawing = Drawing::new();
    drawing.header.version = AcadVersion::R2000;
    drawing.header.default_drawing_units = Units::Millimeters;
    // The same parts always give the same bytes: the crate would stamp the
    // time and random identifiers. The dates are local wall-clock times in
    // the file, so a fixed one reads the same in every time zone.
    let epoch = NaiveDate::from_ymd_opt(2000, 1, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .expect("a valid date");
    let local = Local
        .from_local_datetime(&epoch)
        .earliest()
        .ok_or("no local time for the fixed date")?;
    drawing.header.creation_date = local;
    drawing.header.update_date = local;
    drawing.header.creation_date_universal = Utc.from_utc_datetime(&epoch);
    drawing.header.update_date_universal = Utc.from_utc_datetime(&epoch);
    drawing.header.fingerprint_guid = uuid::Uuid::nil();
    drawing.header.version_guid = uuid::Uuid::nil();

    let mut layers: Vec<&str> = Vec::new();
    for shape in shapes {
        if !layers.contains(&shape.layer()) {
            layers.push(shape.layer());
        }
    }
    for name in layers {
        drawing.add_layer(Layer {
            name: name.into(),
            color: Color::from_index(color(name)),
            ..Default::default()
        });
    }

    let point = |(x, y): (f64, f64)| Point::new(x, y, 0.0);
    for shape in shapes {
        let specific = match shape {
            Shape::Polygon { points, .. } => {
                let mut polyline = LwPolyline {
                    vertices: points
                        .iter()
                        .map(|&(x, y)| LwPolylineVertex {
                            x,
                            y,
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                };
                polyline.set_is_closed(true);
                EntityType::LwPolyline(polyline)
            }
            Shape::Circle { center, radius, .. } => EntityType::Circle(Circle {
                center: point(*center),
                radius: *radius,
                ..Default::default()
            }),
            Shape::Line { from, to, .. } => EntityType::Line(Line {
                p1: point(*from),
                p2: point(*to),
                ..Default::default()
            }),
            Shape::Text {
                at,
                height,
                text,
                centered,
                ..
            } => {
                let mut entity = Text {
                    location: point(*at),
                    text_height: *height,
                    value: text.clone(),
                    ..Default::default()
                };
                if *centered {
                    entity.horizontal_text_justification = HorizontalTextJustification::Center;
                    entity.vertical_text_justification = VerticalTextJustification::Middle;
                    entity.second_alignment_point = point(*at);
                }
                EntityType::Text(entity)
            }
        };
        let mut entity = Entity::new(specific);
        entity.common.layer = shape.layer().into();
        drawing.add_entity(entity);
    }
    drawing.normalize();

    let mut bytes = Vec::new();
    drawing
        .save(&mut bytes)
        .map_err(|e| format!("the DXF could not be written: {e}"))?;
    fix_table_headers(&bytes)
}

/// Gives each table header the handle and owner the `dxf` crate leaves
/// out, which readers otherwise have to repair (ADR-0015), and moves the
/// handle seed past the new handles.
fn fix_table_headers(bytes: &[u8]) -> Result<Vec<u8>, String> {
    const EOL: &[u8] = b"\r\n";
    fn code(line: &[u8]) -> Option<&str> {
        std::str::from_utf8(line).ok().map(str::trim)
    }
    let lines: Vec<&[u8]> = bytes
        .strip_suffix(EOL)
        .unwrap_or(bytes)
        .split(|&b| b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .collect();
    if !lines.len().is_multiple_of(2) {
        return Err("the DXF has a group code without a value".into());
    }
    let pairs: Vec<(&[u8], &[u8])> = lines.chunks(2).map(|p| (p[0], p[1])).collect();
    let seed_at = pairs
        .iter()
        .position(|&(c, v)| code(c) == Some("9") && v == b"$HANDSEED")
        .ok_or("the DXF has no handle seed")?
        + 1;
    let mut seed = std::str::from_utf8(pairs[seed_at].1)
        .ok()
        .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
        .ok_or("the DXF's handle seed is not a handle")?;

    let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(pairs.len() + 32);
    let mut i = 0;
    while i < pairs.len() {
        let (c, v) = pairs[i];
        out.push((c.to_vec(), v.to_vec()));
        let opens_table = code(c) == Some("0") && v == b"TABLE";
        if opens_table && pairs.get(i + 1).is_some_and(|&(c, _)| code(c) == Some("2")) {
            let (name_code, name) = pairs[i + 1];
            out.push((name_code.to_vec(), name.to_vec()));
            out.push((b"  5".to_vec(), format!("{seed:X}").into_bytes()));
            out.push((b"330".to_vec(), b"0".to_vec()));
            seed += 1;
            i += 2;
            continue;
        }
        i += 1;
    }
    let seed_line = out
        .iter()
        .position(|(c, v)| code(c) == Some("9") && v.as_slice() == b"$HANDSEED")
        .expect("copied from the input")
        + 1;
    out[seed_line].1 = format!("{seed:X}").into_bytes();

    let mut fixed = Vec::with_capacity(bytes.len() + 32 * 12);
    for (c, v) in out {
        fixed.extend_from_slice(&c);
        fixed.extend_from_slice(EOL);
        fixed.extend_from_slice(&v);
        fixed.extend_from_slice(EOL);
    }
    Ok(fixed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_avoid_characters_systems_reject() {
        assert_eq!(safe_file_name("B30 - Door 1"), "B30 - Door 1");
        assert_eq!(safe_file_name("W/30: \"top\"?."), "W_30_ _top__");
    }
}
