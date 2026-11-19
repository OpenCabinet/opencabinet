// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sample kitchen in 3D: what the scene holds, and, where a graphics
//! adapter or the processor can draw, pictures of it. The pictures are
//! saved in `target/view3d/` to be looked at; the test checks that they
//! show something besides the sky.

use std::path::PathBuf;

use opencabinet_model::{Library, Project};
use opencabinet_view3d::{Camera, Frame, Mode, Model, Pick, Renderer, View};
use serde::de::DeserializeOwned;

fn read<T: DeserializeOwned>(path: &str) -> T {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/kitchen.ocab")
        .join(path);
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

fn kitchen() -> Model {
    let project: Project = read("project.json");
    let library: Library = read("library.json");
    Model::build(&project, &library, Some(0))
}

#[test]
fn the_kitchen_has_its_cabinets_walls_and_floor() {
    let model = kitchen();
    assert!(model.instances.len() >= 2, "{}", model.instances.len());
    assert_ne!(model.walls.len(), 0);
    assert!(model.floor.is_some());
    let mesh = model.mesh(&View::default());
    assert!(mesh.solid.len() > 1000);
    assert_ne!(mesh.lines.len(), 0);
    assert_eq!(mesh.overlay.len(), 0);
}

#[test]
fn a_part_is_picked_from_the_camera() {
    // The ray through the middle of a view framed on the first cabinet
    // alone meets one of its parts.
    let model = kitchen();
    let view = View {
        isolated: Some(0),
        ..View::default()
    };
    let mut camera = Camera::default();
    let (low, high) = model.bounds(&view);
    camera.frame(low, high);
    let pick = model.pick(&view, camera.ray(400.0, 300.0, 800.0, 600.0));
    let Some(Pick { instance, part }) = pick else {
        panic!("nothing picked");
    };
    assert_eq!(instance, 0);
    assert_eq!(model.parts[part].product, model.instances[0].product);
    // Hidden fronts let the ray through to what is behind them.
    let open = View {
        fronts: false,
        ..view
    };
    let inside = model.pick(&open, camera.ray(400.0, 300.0, 800.0, 600.0));
    assert!(inside.is_some_and(|p| !matches!(model.parts[p.part].role, "door" | "drawerFront")));
}

fn save(name: &str, pixels: &[u8], width: u32, height: u32) {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/view3d");
    std::fs::create_dir_all(&folder).unwrap();
    let file = std::fs::File::create(folder.join(name)).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(pixels)
        .unwrap();
}

#[test]
fn the_kitchen_is_drawn() {
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(message) => {
            // CI draws on the processor and requires it; a computer
            // without any adapter only skips the pictures.
            assert!(
                std::env::var_os("OPENCABINET_REQUIRE_3D").is_none(),
                "{message}"
            );
            eprintln!("skipped: {message}");
            return;
        }
    };
    eprintln!("drawing with {}", renderer.adapter);
    let model = kitchen();
    let (width, height) = (960, 600);
    let mut pixels = vec![0; (width * height * 4) as usize];
    let views = [
        ("room.png", View::default()),
        (
            "cabinet.png",
            View {
                isolated: Some(0),
                ..View::default()
            },
        ),
        (
            "inside.png",
            View {
                isolated: Some(0),
                fronts: false,
                selected: model.instances[0]
                    .parts
                    .iter()
                    .find(|&&p| model.parts[p].key == "case/side.left")
                    .map(|&part| Pick { instance: 0, part }),
                ..View::default()
            },
        ),
        (
            "xray.png",
            View {
                isolated: Some(0),
                mode: Mode::XRay,
                ..View::default()
            },
        ),
        (
            "exploded.png",
            View {
                isolated: Some(0),
                explode: 0.6,
                ..View::default()
            },
        ),
    ];
    for (name, view) in views {
        let mut camera = Camera::default();
        let (low, high) = model.bounds(&view);
        camera.frame(low, high);
        if view.isolated.is_none() {
            camera.pitch = 0.6;
            camera.yaw = 0.5;
        }
        renderer.upload(0, &model.mesh(&view));
        #[allow(clippy::cast_precision_loss, reason = "a small image")]
        let frame = Frame {
            view_projection: camera.view_projection(width as f32 / height as f32),
            eye: camera.eye().to_array(),
            textured: view.mode == Mode::Textured,
            ghost_opacity: 0.2,
        };
        renderer
            .draw(0, &frame, width, height, &mut pixels)
            .unwrap();
        save(name, &pixels, width, height);
        let (colors, _) = pixels.as_chunks::<4>();
        let different = colors.iter().filter(|c| **c != colors[0]).count();
        assert!(different > pixels.len() / 40, "{name} shows nothing");
    }
    measure(&mut renderer, &model, (width, height), &mut pixels);
}

/// How long a frame of the room takes, for ADR-0016's budgets; printed,
/// since CI's machines vary too much to fail on it.
fn measure(renderer: &mut Renderer, model: &Model, (width, height): (u32, u32), pixels: &mut [u8]) {
    let view = View::default();
    let start = std::time::Instant::now();
    let mesh = model.mesh(&view);
    let built = start.elapsed();
    renderer.upload(0, &mesh);
    let mut camera = Camera::default();
    let (low, high) = model.bounds(&view);
    camera.frame(low, high);
    #[allow(clippy::cast_precision_loss, reason = "a small image")]
    let frame = Frame {
        view_projection: camera.view_projection(width as f32 / height as f32),
        eye: camera.eye().to_array(),
        textured: true,
        ghost_opacity: 0.2,
    };
    let start = std::time::Instant::now();
    for _ in 0..5 {
        renderer.draw(0, &frame, width, height, pixels).unwrap();
    }
    eprintln!(
        "room: scene built in {built:?}, a {width} × {height} frame drawn and read back in {:?}",
        start.elapsed() / 5
    );
}
