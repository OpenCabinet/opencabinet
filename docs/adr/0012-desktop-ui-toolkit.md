<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# 0012 — Desktop user interface toolkit

- **Status:** accepted
- **Date:** 2026-10-25
- **Deciders:** Andrés (@A-newGP)

## Context and problem statement

The core is written in Rust ([ADR-0011](0011-core-technical-stack.md)).
OpenCabinet Designer needs a desktop user interface on top of it: forms for
cabinet parameters, tables such as the cut list, and a drawing area for
elevations that later grows into the 2D CAD of phase 3. Which toolkit should
the desktop interface use?

## Decision drivers

- **Modest hardware:** fast start-up and low memory (principle 5 of the
  plan).
- **Testable:** the interface must be driven by automated tests in CI,
  without a person clicking.
- **Accessible:** screen readers and keyboard navigation from the first
  release.
- **Drawing:** elevations now; a 2D drawing area with thousands of entities,
  zoom, pan and snapping in phase 3.
- **Licensing:** compatible with GPL-3.0-or-later, and with the still open
  choice between "or later" and "only".
- **Sustainability:** an active project that will still exist in ten years.
- **Contributors:** how easy it is for newcomers to change a screen.

## Considered options

1. **Slint** 1.18: native, declarative `.slint` markup compiled to Rust.
2. **egui** / eframe 0.37: native, immediate mode, written in Rust only.
3. **Tauri** 2: the system web view (WebView2 on Windows, WebKitGTK on
   Linux) with HTML, CSS and SVG, and Rust behind it.

Other options were discarded without prototypes: **iced** did not yet
support screen readers when this ADR was written; **GTK 4** and **Qt** bindings make Windows
distribution and the build harder for contributors; **Electron** ships a
whole browser and contradicts the modest hardware principle.

## Evidence: the UI spike

Each option implemented the same screen in
[`prototypes/ui-spike/`](https://github.com/OpenCabinet/opencabinet/tree/831d1eaf81c77c5bc8fb0a4a4ec1cce390daabc1/prototypes/ui-spike)
(removed after this decision; the link points to the commit that added it)
on the same Rust core:
width, height and depth typed in shop notation (`30-1/2`, `30 1/2"`), the
number of doors, a scaled elevation with doors, hinge cups and dimensions,
and a cut list that updates on every keystroke. Invalid input shows an error
and keeps the last valid result.

Measurements from CI (GitHub-hosted runners without a GPU, run
40217151798; cold release builds; launch is the median of five runs):

| | Slint | egui | Tauri |
| --- | --- | --- | --- |
| Executable, Windows | 12.6 MB | 12.9 MB | 8.0 MB |
| Executable, Linux | 18.1 MB | 16.6 MB | 5.7 MB |
| Launch to first frame, Windows | 166 ms | 249 ms | 603 ms |
| Launch to first frame, Linux | 234 ms | 83 ms | 769 ms |
| Memory, Windows (process tree) | 28 MB | 57 MB | 317 MB |
| Memory, Linux (process tree) | 149 MB | 133 MB | 385 MB |
| Release build, Windows | 249 s | 233 s | 211 s |
| User interface code (excluding tests) | 134 Rust + 111 `.slint` lines | 256 Rust lines | 61 Rust + 191 HTML, CSS and JS lines + 33 of configuration |
| Crates in the dependency tree | 319 | 235 | 262 |
| Headless UI test in CI | Yes, through the accessibility tree | Yes, with an accessibility check | No; needs WebDriver |

How to read the numbers:

- Linux memory is inflated for every option by the virtual display and
  software rendering of the CI runner; Windows is closer to a real desk.
- Launch times move between runs: egui on Windows took 605 ms in an earlier
  run and 249 ms in this one. Differences under about 30% are noise.
- Tauri's executable is the smallest because the browser engine is not in
  it; it is in the operating system and in the memory column.

What the spike shows:

- **Slint and egui are both fit for the job;** Tauri is not, under the
  modest hardware principle: five to eleven times the memory of the native
  options on Windows and the slowest start, and its interface could not be
  tested in CI without extra tooling.
- **Slint has the lightest footprint on Windows,** the main platform of
  millwork shops: 28 MB and the fastest start.
- **egui is the simplest to write:** one language, the interface is plain
  Rust code, and drawing is a direct list of painter calls.
- **Slint separates the screen from the logic:** the `.slint` file describes
  the layout declaratively, which is easier to review and to hand to a
  designer, and comes with a live preview in the editor.
- **Testing works for both native options.** The Slint test drives the
  interface through the accessibility tree, so a passing test also proves
  that screen readers see the fields. egui_kittest found a missing label in
  the prototype, which was fixed.
- **Pitfalls met:** Slint's renderer presented no frame on a machine
  without a GPU until the prototype added a software fallback, and its
  testing backend can be started only once per test process; egui 0.37
  changed its application and panel API from earlier versions; the Tauri
  grid layout needed care to keep the drawing from growing the page.

## Decision outcome

Chosen option: **Slint**, because it best meets the hard drivers together:

- **Footprint:** the lowest memory and the fastest start on Windows.
- **Accessibility and testing by construction:** tests go through the same
  accessibility tree that screen readers use.
- **Maintainable screens:** declarative markup keeps layout out of the
  Rust logic, which suits many future contributors editing forms.

egui is a close second and the recommended fallback. The elevation is
computed by the core as resolution-independent shapes
(`core/src/drawing.rs` in the spike), so each front end only maps shapes to
the screen; changing toolkits later would rewrite the screens but not the
model or the drawings.

### Consequences

- Good, because the desktop app stays light on the modest computers found
  in many shops.
- Good, because accessibility is checked in CI, not promised.
- Bad, with a legal effect: Slint is licensed as **GPL-3.0-only** or under
  its vendor's royalty-free or commercial licenses. OpenCabinet's own code
  can stay GPL-3.0-or-later, but the distributed program, which includes
  Slint, is in practice under GPL-3.0 only. A future GPL version could not
  be used, nor the project relicensed, without replacing Slint or buying its
  commercial license. The pending "or later" or "only" decision should be
  taken with this in mind.
- Bad, because Slint is developed mainly by one company (SixtyFPS GmbH),
  which asks its own contributors to sign a CLA. If it changed course, the
  last GPL version would remain usable, and egui is the fallback.
- Bad, because Slint has no general drawing canvas. Phase 3's 2D CAD must
  render its drawing with a Rust 2D library into an image that Slint shows,
  or through Slint's still unstable integration with wgpu (the GPU graphics
  library). A small prototype of a drawing with thousands of entities should
  come early in phase 3.
- Neutral, because the `.slint` language is one more thing to learn, with
  good documentation and editor support.

## Pros and cons of the options

### Slint

- Good, because of footprint, accessibility tree testing and declarative
  screens (above).
- Good, because the same interface can also target the web through WASM.
- Bad, because of the GPL-3.0-only license, the single vendor and the
  missing drawing canvas (above).

### egui

- Good, because it is Rust only, with the least ceremony, and drawing
  custom shapes is direct and fast.
- Good, because it is permissively licensed (MIT or Apache-2.0) and backed
  by a company that builds a large product on it (Rerun).
- Good, because egui_kittest tests the interface and checks accessibility.
- Bad, because about twice Slint's memory on Windows.
- Bad, because immediate mode mixes layout with logic, and complex forms
  and tables take more code; the interface looks like a developer tool
  unless styled with care.
- Bad, because its API still changes between minor versions.

### Tauri

- Good, because HTML, CSS and SVG are known by many developers and make
  rich drawings easy.
- Good, because it has the smallest executable and permissive licenses (MIT
  or Apache-2.0).
- Bad, because of memory and start-up time (above).
- Bad, because the interface is in a second language (JavaScript) and talks
  to Rust through messages.
- Bad, because the web view differs between platforms (WebKitGTK on Linux),
  and automated interface tests need a WebDriver setup.

## Sources

- UI spike and CI workflow, as of commit `831d1ea`:
  `prototypes/ui-spike/` and `.github/workflows/ui-spike.yml`, run
  40217151798.
- Slint 1.18 documentation: language reference, testing backend, licensing
  (<https://slint.dev/terms-and-conditions>).
- egui and eframe 0.37 documentation; egui_kittest 0.37.
- Tauri 2 documentation: architecture, WebDriver testing.
- AccessKit documentation.
