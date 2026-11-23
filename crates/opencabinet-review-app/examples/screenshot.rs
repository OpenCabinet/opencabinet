// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Draws Review's window into a PNG without a screen, to look at the
//! layout: `cargo run -p opencabinet-review-app --example screenshot --
//! out.png [width height]`, with `PANEL` naming the left panel to open
//! and `REPLIES`, `SNAP`, `PAGES`, `TABS` or `TURN` showing those features.
//! It opens a drawing made for the test, with bookmarks, a few markups,
//! and the markup list.

use std::path::PathBuf;
use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize};

struct Offscreen {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().unwrap_or_else(|| "review.png".into()));
    let width: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1366);
    let height: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(768);

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen {
        window: window.clone(),
    }))
    .expect("a platform was already set");

    let dir = std::env::temp_dir().join(format!("oc-screenshot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pdf = dir.join("A-201.pdf");
    std::fs::write(&pdf, opencabinet_pdf::sample::bookmarked()).unwrap();
    // TABS=1 opens a second drawing, in a tab of its own, puts both in a
    // sheet set and shows the first.
    let mut paths = vec![pdf.clone()];
    if std::env::var("TABS").is_ok() {
        let other = dir.join("A-301 Kitchen elevations.pdf");
        std::fs::write(
            &other,
            opencabinet_pdf::sample::sheets(3, 1728.0, 1296.0, 4),
        )
        .unwrap();
        paths.push(other);
    }
    // The person's own tools, settings and recent files stay untouched.
    let review = opencabinet_review_app::create_apart(&paths).unwrap();
    if paths.len() > 1 {
        // Both drawings in a sheet set, and the first one shown.
        review.invoke_add_to_set();
        review.invoke_choose_tab(0);
        review.invoke_add_to_set();
    }
    review.show().unwrap();
    window.set_size(PhysicalSize::new(width, height));
    let mut pixels = vec![Rgb565Pixel::default(); (width * height) as usize];
    let draw = |pixels: &mut [Rgb565Pixel]| {
        slint::platform::update_timers_and_animations();
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, width as usize);
        });
    };
    // A first frame gives the drawing its size; then the page fits it.
    draw(&mut pixels);
    review.invoke_fit();
    // A few markups, so the list and the drawing have something.
    for (tool, from, to) in [
        (1, (300.0, 200.0), (420.0, 280.0)),
        (2, (500.0, 220.0), (640.0, 300.0)),
        (5, (300.0, 360.0), (480.0, 420.0)),
    ] {
        review.invoke_choose_tool(tool);
        review.invoke_pressed(from.0, from.1, 0);
        review.invoke_dragged(to.0, to.1);
        review.invoke_released(to.0, to.1);
    }
    review.invoke_choose_tool(0);
    // REPLIES=1 shows the last markup's replies in the editor.
    if std::env::var("REPLIES").is_ok() {
        review.invoke_reply("Is the opening 36\" or 42\"?".into());
        review.invoke_reply("42\", per the revised elevation.".into());
        review.set_editor_tab(1);
    }
    // SNAP=1 shows a line placed by a first click, the mouse snapped to
    // where two grid lines cross.
    if std::env::var("SNAP").is_ok() {
        review.invoke_choose_tool(6);
        review.invoke_pressed(560.0, 140.0, 0);
        review.invoke_released(560.0, 140.0);
        for (x, y) in [(580.0, 180.0), (590.0, 221.0)] {
            review.invoke_moved_to(x, y, false);
        }
    }
    // PAGES=1 turns the first page a quarter clockwise and chooses the
    // first two pages in the Pages panel.
    if std::env::var("PAGES").is_ok() {
        review.invoke_turn_pages(1);
        review.invoke_fit();
        review.invoke_page_clicked(1, true, false);
    }
    // TURN=1 turns the view a quarter clockwise and shows the stamps, with
    // one of the shop's own.
    if std::env::var("TURN").is_ok() {
        review.invoke_turn_view(1);
        review.invoke_choose_tool(13);
        review.invoke_add_stamp("SHOP CHECKED {date}".into());
    }
    // The left panel to show: 0 pages, 1 bookmarks, 2 find, 3 measure, 4
    // session, 5 sheet set.
    let panel = std::env::var("PANEL")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1);
    review.set_panel(panel);

    for _ in 0..3 {
        draw(&mut pixels);
    }
    let mut rgb = Vec::with_capacity(pixels.len() * 3);
    for p in &pixels {
        let v = p.0;
        let r = ((v >> 11) & 0x1f) as u8;
        let g = ((v >> 5) & 0x3f) as u8;
        let b = (v & 0x1f) as u8;
        rgb.extend_from_slice(&[r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2]);
    }
    let file = std::fs::File::create(&out).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&rgb)
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    println!("{}", out.display());
}
