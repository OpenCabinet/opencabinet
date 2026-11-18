<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# 0011 — Rust for the core

- **Status:** accepted
- **Date:** 2026-10-25
- **Deciders:** Andrés (@A-newGP)

## Context and problem statement

The core of OpenCabinet (data model, evaluator, file format, reports) will
outlive every user interface and plugin built on it. Which language and
runtime should it be written in?

This ADR decides the **core** only. The desktop user interface is a separate
decision that follows from this one.

## Decision drivers

- **Exactness and determinism:** native 64-bit integers for nanometer
  lengths ([ADR-0009](0009-length-representation.md)); identical results on
  every platform.
- **Modest hardware:** small executables, low memory, fast start-up
  (principle 5 of the plan).
- **Plugins and the web:** the core and plugins must be able to run in a
  WebAssembly (WASM) sandbox, which RFC-0002 may choose for plugin isolation,
  and which keeps a browser version possible.
- **Long-term quality:** strong typing, safe memory, good testing tools.
- **Ecosystem:** DXF, PDF, 2D geometry and a desktop UI, all with
  GPLv3-compatible licenses.
- **Contributors:** who can realistically help.
- The founder prioritizes quality over ease of learning.

## Considered options

1. **Rust**
2. **C# / .NET** with native ahead-of-time (AOT) compilation
3. TypeScript (Electron or browser)
4. C++ with Qt and Open CASCADE, or Python on FreeCAD

Options 3 and 4 were discarded without prototypes: TypeScript has no native
64-bit integer (`BigInt` is slow and awkward for every length), and Electron
contradicts the modest hardware principle; C++ adds memory-safety risk and a
complex build for newcomers, and FreeCAD brings a heavy runtime and its own
user experience.

## Evidence: the stack spike

Both remaining options implemented the same slice of RFC-0001 in
[`prototypes/stack-spike/`](https://github.com/OpenCabinet/opencabinet/tree/48a01194cbe4099d0359d54f1f5d564fac15cd80/prototypes/stack-spike)
(removed after this decision; the link points to the commit that added it): load a `.ocab`
project, evaluate four frameless cabinets into parts and hinge holes, write
a cut list and an operations list, and round-trip the project through a ZIP
`.ocab`. An independent Python reference generated the golden outputs.

**Both prototypes reproduced the golden outputs byte for byte on Linux and
Windows**, passed their tests, linters and formatters in CI, and
round-tripped losslessly.

Measurements from CI (GitHub-hosted runners, run 40205216392; single runs,
so differences under ~20% are noise):

| | Rust | C# native AOT |
| --- | --- | --- |
| Executable, Linux | 907 KB | 3,433 KB |
| Executable, Windows | 723 KB | 3,310 KB |
| Start-up + evaluate, Linux | 1 ms | 6 ms |
| Start-up + evaluate, Windows | 24 ms | 37 ms |
| Evaluation only, Linux | 2.2 µs | 3.9 µs |
| Evaluation only, Windows | 5.2 µs | 5.0 µs |
| Peak memory, Linux | 2.9 MB | 6.5 MB |
| Release build, Linux | 13 s | 9 s |
| Lines of code (excluding tests) | ~660 | ~440 |
| Third-party dependencies | 3 direct (40 in lock file) | 0 (standard library) |

What the spike shows:

- **Performance is not a deciding factor.** Both evaluate a small kitchen in
  microseconds and start in milliseconds.
- **Footprint favors Rust:** executables 3.5 to 4.5 times smaller and about
  half the memory.
- **Productivity favors C#:** about a third less code for the same
  behavior, no third-party dependencies, faster builds.
- **Pitfalls met:** in C#, native AOT forced compile-time JSON
  serialization and a stricter subset of .NET, and the .NET 10 test
  platform change broke the first CI run; in Rust, the ZIP library's API
  changed between major versions, and there is more ceremony (explicit
  error types, lifetimes).

## Decision outcome

Chosen option: **Rust for the core**, because it is the only option that
satisfies all the hard drivers at once:

- **WASM is first-class.** The same core compiles to small WASM modules, and
  Rust hosts WASM plugins natively (for example with Wasmtime). If RFC-0002
  chooses isolated plugins, Rust is the natural host; .NET in WASM ships a
  multi-megabyte runtime and is a poor fit for sandboxed plugins.
- **Best footprint** for the modest hardware principle.
- **Determinism and safety by construction:** integer types, no garbage
  collector, no runtime reflection, memory safety without a runtime.

### Consequences

- Good, because one core serves the desktop app, a future web version and
  plugin hosting.
- Good, because Rust's compiler, `clippy` and `rustfmt` enforce a high,
  uniform quality bar that suits a project with many future contributors.
- Bad, because Rust is harder to learn; the founder accepts this, but it
  narrows the pool of casual contributors.
- Bad, because the desktop UI ecosystem in Rust is younger than .NET's
  (Avalonia). The UI ADR must choose carefully, for example a web front end
  in Tauri or a native toolkit such as Slint or iced.
- Bad, because DXF and PDF libraries in Rust are less mature than in .NET
  (where netDxf and PDFsharp exist); phase 2 must evaluate them early.
- Neutral, with a legal effect: Rust has no stable in-process plugin ABI,
  so plugins will in practice be isolated (WASM or separate processes).
  Isolation improves security but weakens the argument that plugins must be
  GPL (see the plan, section A). RFC-0002 must decide this with that trade
  in mind.

## Pros and cons of the options

### Rust

- Good, because of WASM, footprint, determinism and safety (above).
- Good, because Cargo makes builds and dependency audits reproducible.
- Bad, because of the learning curve and a younger GUI ecosystem.

### C# / .NET (native AOT)

- Good, because it is productive: less code, a rich standard library, and
  the mature, MIT-licensed Avalonia UI toolkit.
- Good, because many millwork and AEC developers already know C# from
  AutoCAD and Revit plugin work.
- Good, because in-process plugins are natural (assembly loading), which
  strengthens the GPL argument for plugins.
- Bad, because executables are 3.5 to 4.5 times larger and memory is about
  double.
- Bad, because WASM support carries the .NET runtime, which makes sandboxed
  plugins and a web version heavy.
- Bad, because native AOT restricts reflection-based libraries.

## Sources

- Stack spike and CI workflow, as of commit `48a0119`:
  `prototypes/stack-spike/` and `.github/workflows/stack-spike.yml`.
- The Rust Reference and Rust Edition Guide (2024 edition).
- .NET documentation: Native AOT deployment; System.Text.Json source
  generation; Microsoft Testing Platform.
- WebAssembly Core Specification 2.0 (W3C).
- Bytecode Alliance, Wasmtime documentation.
