// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Throwaway spike for ADR-0017: does ocrs read the words of a drawing
//! that has no text in it (a scan), and how fast?
//!
//! `cargo run --release` makes a sheet with the kinds of words drawings
//! have (cabinet labels, dimensions, notes, room names) at several sizes,
//! draws it as a picture at several resolutions, as printed and as a
//! scan would be (a little noise, turned half a degree), and prints, for
//! each, how many of the words ocrs found and how long it took.
//!
//! `cargo run --release -- drawing.pdf [dpi]` reads the words of each page
//! of a PDF of one's own instead, and prints them with the time taken.
//!
//! Set `RAYON_NUM_THREADS=2` to measure as on the reference computer
//! (ADR-0014: two cores).

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::time::Instant;

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use opencabinet_pdf::{Document, Point, View, render};
use rten::Model;

/// Words drawings have, and the size, in points, each is written at.
const WORDS: &[(&str, f64)] = &[
    ("KITCHEN", 24.0),
    ("PANTRY", 18.0),
    ("ELEVATION", 14.0),
    ("B30", 10.0),
    ("W3030", 10.0),
    ("SB36", 10.0),
    ("DB18", 10.0),
    ("FILLER", 8.0),
    ("CONFIRM", 8.0),
    ("COUNTER", 8.0),
    ("HEIGHT", 8.0),
    ("36\"", 8.0),
    ("2'-6\"", 8.0),
    ("TYP.", 8.0),
    ("SCRIBE", 7.0),
    ("REFRIGERATOR", 7.0),
    ("DISHWASHER", 7.0),
    ("APPLIANCE", 6.0),
    ("PANEL", 6.0),
    ("VERIFY", 6.0),
];

/// A sheet `width × height` points, a light grid, and `WORDS` in rows.
fn sheet(width: f64, height: f64) -> Vec<u8> {
    let mut content = String::from("0 0 0 RG 2 w 18 18 m ");
    let _ = writeln!(
        content,
        "{:.1} 18 l {:.1} {:.1} l 18 {:.1} l h S 0.3 w",
        width - 18.0,
        width - 18.0,
        height - 18.0,
        height - 18.0
    );
    for n in 1..12 {
        let x = width * f64::from(n) / 12.0;
        let _ = writeln!(content, "{x:.1} 18 m {x:.1} {:.1} l S", height - 18.0);
    }
    let mut y = height - 80.0;
    let mut x = 60.0;
    for round in 0..3 {
        for &(word, size) in WORDS {
            let w = word.len() as f64 * size * 0.62;
            if x + w > width - 60.0 {
                x = 60.0;
                y -= 70.0;
            }
            let escaped = word.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
            let _ = writeln!(content, "BT /F1 {size:.1} Tf {x:.1} {y:.1} Td ({escaped}) Tj ET");
            x += w + 50.0 + f64::from(round) * 7.0;
        }
    }
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [4 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width:.1} {height:.1}] \
             /Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R >>"
        ),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = writeln!(table, "{offset:010} 00000 n ");
    }
    let _ = write!(
        table,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    pdf.extend_from_slice(table.as_bytes());
    pdf
}

/// Page `page` of `document` as a gray picture at `dpi`.
fn picture(document: &Document, page: usize, dpi: f64) -> (Vec<u8>, u32, u32) {
    let size = document.pages()[page];
    let zoom = dpi / 72.0;
    let (width, height) = ((size.width * zoom) as u32, (size.height * zoom) as u32);
    let bitmap = render(
        document,
        View {
            page,
            zoom,
            origin: Point::new(0.0, 0.0),
            width,
            height,
            turn: 0,
        },
    )
    .expect("the page draws");
    let gray = bitmap
        .pixels
        .chunks(4)
        .map(|p| ((u32::from(p[0]) * 3 + u32::from(p[1]) * 6 + u32::from(p[2])) / 10) as u8)
        .collect();
    (gray, width, height)
}

/// `gray` as a scanner would give it: turned half a degree, with a little
/// noise (±8 of 255).
fn scanned(gray: &[u8], width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width as i64, height as i64);
    let (sin, cos) = 0.5f64.to_radians().sin_cos();
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let mut seed: u32 = 0x2026_1123;
    let mut out = vec![255u8; gray.len()];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f64 - cx, y as f64 - cy);
            let (sx, sy) = (
                (cos * dx + sin * dy + cx).round() as i64,
                (-sin * dx + cos * dy + cy).round() as i64,
            );
            let value = if (0..w).contains(&sx) && (0..h).contains(&sy) {
                gray[(sy * w + sx) as usize]
            } else {
                255
            };
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = i32::from((seed >> 24) as u8 % 17) - 8;
            out[(y * w + x) as usize] = (i32::from(value) + noise).clamp(0, 255) as u8;
        }
    }
    out
}

/// The size of the pictures ocrs's detection model looks at; a larger
/// picture is shrunk to it, so small words on a large sheet would be lost.
const TILE: (u32, u32) = (800, 600);
/// How much tiles overlap, so a word on an edge is whole in one of them.
const OVERLAP: u32 = 120;

/// The words ocrs reads in `gray`, tile by tile, and the seconds it took.
fn read(engine: &OcrEngine, gray: &[u8], width: u32, height: u32) -> (Vec<String>, f64) {
    let start = Instant::now();
    let mut found = Vec::new();
    let step = |size: u32| (size - OVERLAP) as usize;
    for top in (0..height).step_by(step(TILE.1)) {
        for left in (0..width).step_by(step(TILE.0)) {
            let (w, h) = (TILE.0.min(width - left), TILE.1.min(height - top));
            let mut tile = Vec::with_capacity((w * h) as usize);
            for y in top..top + h {
                let row = (y * width + left) as usize;
                tile.extend_from_slice(&gray[row..row + w as usize]);
            }
            // Nothing but paper: nothing to read.
            if tile.iter().all(|&p| p > 200) {
                continue;
            }
            let source = ImageSource::from_bytes(&tile, (w, h)).expect("a gray picture");
            let input = engine.prepare_input(source).expect("prepared");
            let words = engine.detect_words(&input).expect("words found");
            let lines = engine.find_text_lines(&input, &words);
            let texts = engine.recognize_text(&input, &lines).expect("words read");
            found.extend(texts.iter().flatten().flat_map(|line| {
                line.to_string()
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            }));
        }
    }
    (found, start.elapsed().as_secs_f64())
}

fn main() {
    let models = concat!(env!("CARGO_MANIFEST_DIR"), "/models/");
    let engine = OcrEngine::new(OcrEngineParams {
        detection_model: Some(
            Model::load_file(format!("{models}text-detection-ssfbcj81.onnx"))
                .expect("see README.md for the models"),
        ),
        recognition_model: Some(
            Model::load_file(format!("{models}text-rec-checkpoint-s52qdbqt.onnx"))
                .expect("see README.md for the models"),
        ),
        ..Default::default()
    })
    .expect("the engine starts");
    let threads = std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "all".into());

    let mut args = std::env::args().skip(1);
    if let Some(path) = args.next() {
        let dpi: f64 = args.next().and_then(|d| d.parse().ok()).unwrap_or(200.0);
        let document = Document::open(std::fs::read(&path).expect("the PDF reads"))
            .expect("a PDF");
        for page in 0..document.pages().len() {
            let (gray, w, h) = picture(&document, page, dpi);
            let (found, seconds) = read(&engine, &gray, w, h);
            println!("## Page {} ({w} × {h} px, {seconds:.1} s, threads {threads})", page + 1);
            println!("{}", found.join(" "));
        }
        return;
    }

    println!("| Sheet | dpi | Picture | Found | Seconds (threads {threads}) |");
    println!("| --- | --- | --- | --- | --- |");
    for (name, (width, height)) in [("Letter", (792.0, 612.0)), ("36\" × 24\"", (2592.0, 1728.0))] {
        let document = Document::open(sheet(width, height)).expect("our sheet opens");
        // DPI=100,150 measures only those.
        let dpis: Vec<f64> = std::env::var("DPI").map_or_else(
            |_| vec![100.0, 150.0, 200.0, 300.0],
            |d| d.split(',').filter_map(|v| v.parse().ok()).collect(),
        );
        for dpi in dpis {
            let (gray, w, h) = picture(&document, 0, dpi);
            for (kind, pixels) in [("printed", gray.clone()), ("scanned", scanned(&gray, w, h))] {
                let (found, seconds) = read(&engine, &pixels, w, h);
                if std::env::var("SHOW").is_ok() {
                    eprintln!("{name} {kind} {dpi}: {}", found.join(" "));
                }
                let seen: BTreeSet<String> = found.iter().map(|w| w.to_uppercase()).collect();
                let on_sheet = WORDS
                    .iter()
                    .filter(|(word, size)| {
                        // Letter sheets fit fewer rows; count what was drawn.
                        let _ = size;
                        !word.is_empty()
                    })
                    .count();
                let hits = WORDS.iter().filter(|(word, _)| seen.contains(*word)).count();
                let missed: Vec<&str> = WORDS
                    .iter()
                    .filter(|(word, _)| !seen.contains(*word))
                    .map(|(word, _)| *word)
                    .collect();
                println!(
                    "| {name} {kind} | {dpi:.0} | {w} × {h} | {hits}/{on_sheet}{} | {seconds:.1} |",
                    if missed.is_empty() {
                        String::new()
                    } else {
                        format!(" (missed {})", missed.join(", "))
                    }
                );
            }
        }
    }
}
