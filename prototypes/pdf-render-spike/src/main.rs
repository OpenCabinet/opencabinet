// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! RFC-0004 §11, rendering spike. Writes a heavy sheet set of our own, then
//! measures hayro on it: opening, a page fit to the reference screen, tiles
//! at high zoom, a full page at print resolution, and several threads at
//! once. Then saves a markup with lopdf as an incremental update and checks
//! that the original bytes are untouched and that hayro draws the markup.

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hayro::hayro_interpret::{InterpreterSettings, RenderContent};
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::Affine;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{PixmapSettings, RenderCache, RenderSettings, render, render_into};

/// A 36" × 24" sheet, in points.
const SHEET: (f64, f64) = (2592.0, 1728.0);
/// The reference screen of ADR-0014.
const SCREEN: (f64, f64) = (1366.0, 768.0);
const TILE: u16 = 512;

fn main() {
    let pages: usize = arg(1).unwrap_or(30);
    let segments: usize = arg(2).unwrap_or(40_000);
    std::fs::create_dir_all("out").unwrap();

    let started = Instant::now();
    let bytes = sheet_set(pages, segments);
    std::fs::write("out/sheets.pdf", &bytes).unwrap();
    println!(
        "sheet set: {pages} sheets × {segments} segments, {:.1} MB, written in {}",
        bytes.len() as f64 / 1e6,
        ms(started.elapsed())
    );

    let data: Arc<Vec<u8>> = Arc::new(bytes.clone());
    let started = Instant::now();
    let pdf = Pdf::new(data.clone()).expect("hayro opens the sheet set");
    println!("open: {} ({} pages)", ms(started.elapsed()), pdf.pages().len());

    let settings = InterpreterSettings::default();
    let render_settings = RenderSettings::default();
    let cache = RenderCache::new();
    let fit = (SCREEN.0 / SHEET.0).min(SCREEN.1 / SHEET.1) as f32;
    let page = &pdf.pages()[0];
    for label in ["fit to screen, first", "fit to screen, again"] {
        let started = Instant::now();
        let pixmap = render(page, &cache, &settings, &render_settings, &scale(fit));
        println!(
            "{label}: {} ({}×{})",
            ms(started.elapsed()),
            pixmap.width(),
            pixmap.height()
        );
        std::fs::write("out/fit.png", pixmap.into_png().unwrap()).unwrap();
    }

    // Tiles at 400 %, around the middle of the sheet.
    let zoom = 4.0;
    let mut times = Vec::new();
    let mut ctx = RenderContext::new(TILE, TILE);
    let mut pixmap = Pixmap::new(TILE, TILE);
    let mut resources = Resources::default();
    for row in 0..3 {
        for column in 0..3 {
            let origin = (
                SHEET.0 * zoom / 2.0 + f64::from(column) * f64::from(TILE),
                SHEET.1 * zoom / 2.0 + f64::from(row) * f64::from(TILE),
            );
            let started = Instant::now();
            tile(
                page,
                &cache,
                &settings,
                (&mut ctx, &mut pixmap, &mut resources),
                origin,
                zoom,
            );
            times.push(started.elapsed());
        }
    }
    std::fs::write("out/tile.png", pixmap.clone().into_png().unwrap()).unwrap();
    times.sort();
    println!(
        "tile {TILE}×{TILE} at 400 %: median {}, slowest {}",
        ms(times[times.len() / 2]),
        ms(times[times.len() - 1])
    );

    let started = Instant::now();
    let print = render(page, &cache, &settings, &render_settings, &scale(150.0 / 72.0));
    println!(
        "full sheet at 150 dpi: {} ({}×{})",
        ms(started.elapsed()),
        print.width(),
        print.height()
    );

    // Four sheets fit to screen on two threads: each thread opens its own
    // `Pdf` and keeps its own cache, which are not shared between threads.
    let started = Instant::now();
    let threads: Vec<_> = (0..2)
        .map(|t| {
            let data = data.clone();
            std::thread::spawn(move || {
                let pdf = Pdf::new(data).unwrap();
                let cache = RenderCache::new();
                let settings = InterpreterSettings::default();
                for index in [t, t + 2] {
                    let page = &pdf.pages()[index];
                    render(page, &cache, &settings, &RenderSettings::default(), &scale(fit));
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    println!("4 sheets fit to screen on 2 threads: {}", ms(started.elapsed()));

    markup(&bytes, fit);
}

/// Saves a red rectangle markup on the first sheet as an incremental update,
/// and checks what it should.
fn markup(original: &[u8], fit: f32) {
    use lopdf::{Dictionary, IncrementalDocument, Object, Stream, dictionary};

    let mut document = IncrementalDocument::load("out/sheets.pdf").unwrap();
    let page = *document.get_prev_documents().get_pages().get(&1).unwrap();
    document.opt_clone_object_to_new_document(page).unwrap();
    let new = &mut document.new_document;
    // Around the middle of the sheet, 300 × 200 pt.
    let rect = [1146.0, 764.0, 1446.0, 964.0];
    let appearance = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 300.into(), 200.into()],
        },
        b"1 0 0 RG 1 0 0 rg 6 w 3 3 294 194 re S".to_vec(),
    );
    let appearance = new.add_object(appearance);
    let annotation: Dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "Rect" => rect.iter().map(|&v| Object::Real(v)).collect::<Vec<_>>(),
        "C" => vec![Object::Real(1.0), Object::Real(0.0), Object::Real(0.0)],
        "T" => Object::string_literal("Andrés"),
        "Contents" => Object::string_literal("Confirm 36\" counter height"),
        "AP" => dictionary! { "N" => appearance },
    };
    let annotation = new.add_object(annotation);
    let page_dict = new.get_object_mut(page).unwrap().as_dict_mut().unwrap();
    page_dict.set("Annots", vec![Object::Reference(annotation)]);
    let started = Instant::now();
    document.save("out/marked.pdf").unwrap();
    let marked = std::fs::read("out/marked.pdf").unwrap();
    println!(
        "incremental save: {}, {} bytes appended, original bytes untouched: {}",
        ms(started.elapsed()),
        marked.len() - original.len(),
        marked.starts_with(original)
    );

    let pdf = Pdf::new(Arc::new(marked)).expect("hayro opens the marked sheet set");
    let cache = RenderCache::new();
    let page = &pdf.pages()[0];
    // Middle of the rectangle's top edge (PDF y up, pixmap y down).
    let at = (
        (1296.0 * f64::from(fit)) as u16,
        ((SHEET.1 - 961.0) * f64::from(fit)) as u16,
    );
    let red = |content| {
        let settings = InterpreterSettings {
            render_content: content,
            ..InterpreterSettings::default()
        };
        let pixmap = render(page, &cache, &settings, &RenderSettings::default(), &scale(fit));
        let pixel = pixmap.data()[usize::from(at.1) * usize::from(pixmap.width()) + usize::from(at.0)];
        if content == RenderContent::all() {
            std::fs::write("out/marked.png", pixmap.into_png().unwrap()).unwrap();
        }
        pixel.r > 200 && pixel.g < 80 && pixel.b < 80
    };
    println!("hayro draws the markup: {}", red(RenderContent::all()));
    println!(
        "and leaves it out when asked for the page alone: {}",
        !red(RenderContent::PAGE)
    );
}

/// One tile of `page` at `zoom`, its top left corner at `origin` pixels.
fn tile<'a>(
    page: &'a hayro::hayro_syntax::page::Page<'a>,
    cache: &RenderCache<'a>,
    settings: &InterpreterSettings,
    (ctx, pixmap, resources): (&mut RenderContext, &mut Pixmap, &mut Resources),
    origin: (f64, f64),
    zoom: f64,
) {
    ctx.reset_and_resize(TILE, TILE);
    let transform = Affine::translate((-origin.0, -origin.1))
        * Affine::scale(zoom)
        * page.initial_transform(true).to_kurbo();
    render_into(page, cache, settings, &RenderSettings::default(), ctx, transform);
    ctx.flush();
    ctx.render_with(
        pixmap,
        resources,
        RasterizerSettings {
            target_init: TargetInit::Clear(WHITE),
            ..Default::default()
        },
    );
}

/// A sheet set of our own: walls, dimension-like ticks and labels, made
/// from a fixed pseudo-random sequence so every run draws the same.
fn sheet_set(pages: usize, segments: usize) -> Vec<u8> {
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut objects: Vec<Vec<u8>> = Vec::new();
    // 1 catalog, 2 page tree, 3 font; then a page and its content each.
    let kids: String = (0..pages).map(|i| format!("{} 0 R ", 4 + 2 * i)).collect();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(format!("<< /Type /Pages /Kids [{kids}] /Count {pages} >>").into_bytes());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    for i in 0..pages {
        let mut content = String::from("0 0 0 RG 0.4 w 1 J\n");
        // A border and title block.
        content.push_str("2 w 36 36 2520 1656 re S 0.4 w\n");
        for _ in 0..segments {
            let (x, y) = (72.0 + next() * 2448.0, 72.0 + next() * 1584.0);
            let length = 6.0 + next() * 90.0;
            let (dx, dy) = if next() < 0.5 { (length, 0.0) } else { (0.0, length) };
            let _ = writeln!(content, "{x:.2} {y:.2} m {:.2} {:.2} l S", x + dx, y + dy);
        }
        content.push_str("BT /F1 7 Tf\n");
        for n in 0..400 {
            let (x, y) = (72.0 + next() * 2400.0, 72.0 + next() * 1560.0);
            let _ = writeln!(content, "1 0 0 1 {x:.2} {y:.2} Tm (W{n} 34-1/2\" A-{}) Tj", 201 + i);
        }
        content.push_str("ET\n");
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] \
                 /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                SHEET.0,
                SHEET.1,
                5 + 2 * i
            )
            .into_bytes(),
        );
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend_from_slice(content.as_bytes());
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }
    let mut pdf = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = write!(table, "{offset:010} 00000 n \n");
    }
    let _ = write!(
        table,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    pdf.extend_from_slice(table.as_bytes());
    pdf
}

fn scale(factor: f32) -> PixmapSettings {
    PixmapSettings {
        x_scale: factor,
        y_scale: factor,
        bg_color: WHITE,
    }
}

fn arg(n: usize) -> Option<usize> {
    std::env::args().nth(n)?.parse().ok()
}

fn ms(duration: Duration) -> String {
    format!("{:.1} ms", duration.as_secs_f64() * 1000.0)
}
