// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Measures how long moving around a drawing takes, without a screen:
//! `cargo run --release -p opencabinet-review-app --example pan_probe`.
//! It opens the drawing given, or a dense 36" × 24" sheet, adds markups
//! (`OC_MARKUPS`, 60) and times dragging the page, turning the wheel and
//! zooming, frame by frame, painting the window each time. `OC_PAGE` says
//! how big the page's part of the window is: `1400x800x1.25` is 1400 by
//! 800 logical pixels on a screen at 125 %.

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

#[expect(clippy::too_many_lines, reason = "one measurement, step by step")]
fn main() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen {
        window: window.clone(),
    }))
    .expect("a platform was already set");
    let dir = std::env::temp_dir().join(format!("oc-pan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A drawing given on the command line, or a dense sample sheet.
    let pdf = std::env::args_os().nth(1).map_or_else(
        || {
            let pdf = dir.join("dense.pdf");
            std::fs::write(
                &pdf,
                opencabinet_pdf::sample::sheets(
                    3,
                    2592.0,
                    1728.0,
                    std::env::var("OC_LINES")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(3000),
                ),
            )
            .unwrap();
            pdf
        },
        std::path::PathBuf::from,
    );
    let review = opencabinet_review_app::create_apart(&[pdf]).unwrap();
    review.show().unwrap();
    // The page's part of the window, in logical pixels, and the screen's
    // scale: `OC_PAGE=1400x800x1.25`, or a small screen.
    let page: Vec<f32> = std::env::var("OC_PAGE")
        .unwrap_or_else(|_| "820x420x1".into())
        .split('x')
        .filter_map(|v| v.parse().ok())
        .collect();
    let [w, h, scale] = page[..] else {
        panic!("OC_PAGE is width x height x scale");
    };
    window.dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "pixels"
    )]
    let (pw, ph) = (((w + 540.0) * scale) as u32, ((h + 340.0) * scale) as u32);
    window.set_size(PhysicalSize::new(pw, ph));
    review.invoke_resized(w, h);
    review.invoke_fit();
    // Some markups, as a real review has.
    for n in 0..std::env::var("OC_MARKUPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60)
    {
        let x = 100.0 + f32::from(u8::try_from(n % 10).unwrap()) * 60.0;
        let y = 100.0 + f32::from(u8::try_from(n / 10).unwrap()) * 40.0;
        review.invoke_choose_tool(3);
        review.invoke_pressed(x, y, 0);
        review.invoke_dragged(x + 30.0, y + 20.0);
        review.invoke_released(x + 30.0, y + 20.0);
    }
    review.invoke_choose_tool(0);
    // Close enough to read the drawing, as when reviewing it.
    for _ in 0..6 {
        review.invoke_zoom_in();
    }
    let mut screen = vec![
        slint::platform::software_renderer::PremultipliedRgbaColor::default();
        (pw * ph) as usize
    ];
    let mut time = |name: &str, frames: u32, mut step: Box<dyn FnMut(u32)>| {
        let mut painting = std::time::Duration::ZERO;
        let mut worst = std::time::Duration::ZERO;
        for i in 0..frames {
            let frame = Instant::now();
            // What the window does between frames: pieces drawn apart
            // come in.
            slint::platform::update_timers_and_animations();
            step(i);
            // And put on the screen, as the window does.
            let painted = Instant::now();
            window.draw_if_needed(|renderer| {
                renderer.render(&mut screen, pw as usize);
            });
            painting += painted.elapsed();
            worst = worst.max(frame.elapsed());
            // The screen shows about 60 frames a second.
            std::thread::sleep(
                std::time::Duration::from_millis(16).saturating_sub(frame.elapsed()),
            );
        }
        let paint = painting.as_secs_f64() * 1000.0 / f64::from(frames);
        let worst = worst.as_secs_f64() * 1000.0;
        println!("{name}: {paint:.1} ms painting a frame, the slowest frame {worst:.1} ms");
    };
    let r = review.clone_strong();
    time(
        "drag the page",
        40,
        Box::new(move |i| {
            if i == 0 {
                r.invoke_pressed(1200.0, 300.0, 2);
            }
            // Fast, as when throwing the page across.
            let x = 1200.0 - f32::from(u16::try_from(i).unwrap()) * 25.0;
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
