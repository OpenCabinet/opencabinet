// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Measures how long moving around a drawing takes, without a screen:
//! `cargo run --release -p opencabinet-review-app --example pan_probe`.
//! It opens a dense 36" × 24" sheet with markups and times dragging the
//! page, turning the wheel and zooming, frame by frame.

use std::rc::Rc;
use std::time::Instant;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
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
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen {
        window: window.clone(),
    }))
    .expect("a platform was already set");
    let dir = std::env::temp_dir().join(format!("oc-pan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pdf = dir.join("dense.pdf");
    std::fs::write(
        &pdf,
        opencabinet_pdf::sample::sheets(3, 2592.0, 1728.0, 3000),
    )
    .unwrap();
    let review = opencabinet_review_app::create_apart(&[pdf]).unwrap();
    review.show().unwrap();
    window.set_size(PhysicalSize::new(1366, 768));
    review.invoke_resized(820.0, 420.0);
    review.invoke_fit();
    // Some markups, as a real review has.
    for n in 0..60 {
        let x = 100.0 + f32::from(u8::try_from(n % 10).unwrap()) * 60.0;
        let y = 100.0 + f32::from(u8::try_from(n / 10).unwrap()) * 40.0;
        review.invoke_choose_tool(2);
        review.invoke_pressed(x, y, 0);
        review.invoke_dragged(x + 30.0, y + 20.0);
        review.invoke_released(x + 30.0, y + 20.0);
    }
    review.invoke_choose_tool(0);
    // Close enough to read the drawing, as when reviewing it.
    for _ in 0..6 {
        review.invoke_zoom_in();
    }
    {
        let document = opencabinet_pdf::Document::open(opencabinet_pdf::sample::sheets(
            1, 2592.0, 1728.0, 3000,
        ))
        .unwrap();
        let start = Instant::now();
        for i in 0..10 {
            let view = opencabinet_pdf::View {
                page: 0,
                zoom: 1.0,
                origin: opencabinet_pdf::Point::new(f64::from(i) * 20.0, 300.0),
                width: 820,
                height: 420,
                turn: 0,
            };
            let _ = opencabinet_pdf::render(&document, view).unwrap();
        }
        let each = start.elapsed().as_secs_f64() * 100.0;
        println!("draw the view alone: {each:.1} ms");
    }
    let time = |name: &str, frames: u32, mut step: Box<dyn FnMut(u32)>| {
        let start = Instant::now();
        for i in 0..frames {
            step(i);
        }
        let each = start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
        println!("{name}: {each:.1} ms a frame");
    };
    let r = review.clone_strong();
    time(
        "drag the page",
        40,
        Box::new(move |i| {
            if i == 0 {
                r.invoke_pressed(700.0, 300.0, 2);
            }
            let x = 700.0 - f32::from(u16::try_from(i).unwrap()) * 5.0;
            r.invoke_dragged(x, 300.0);
        }),
    );
    review.invoke_released(500.0, 300.0);
    let r = review.clone_strong();
    time(
        "scroll with the wheel",
        40,
        Box::new(move |_| r.invoke_wheel(0.0, -30.0, false, 400.0, 200.0)),
    );
    let r = review.clone_strong();
    time(
        "zoom with the wheel",
        20,
        Box::new(move |i| {
            let dy = if i % 2 == 0 { 60.0 } else { -60.0 };
            r.invoke_wheel(0.0, dy, true, 400.0, 200.0);
        }),
    );
    std::fs::remove_dir_all(&dir).ok();
}
