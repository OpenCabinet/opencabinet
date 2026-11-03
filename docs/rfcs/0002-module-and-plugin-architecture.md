<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# RFC 0002 — Module and plugin architecture

- **Status:** draft
- **Author(s):** Andrés (@A-newGP)
- **Created:** 2026-10-25
- **Comment period ends:** 2026-11-08
- **Discussion:** pull request introducing this RFC; announced in [Discussions #11](https://github.com/OpenCabinet/opencabinet/discussions/11)

## Summary

OpenCabinet is built as layered Rust crates in one workspace. **Official
modules** are crates compiled into the application. **Third-party plugins**
are WebAssembly components that run in a sandbox, see only what their
manifest declares and, when they take part in evaluation, produce the same
results on every machine. Both reach the core through the same small,
versioned API, which stays at 0.x until three real modules use it.

## Motivation

The project plan asks for a suite that is "modular for real": install only
what you use, keep the core small, and let anyone extend it without
endangering users. RFC-0001 already relies on this: product types (base,
wall, blind corner…) are "generators shipped by modules", and CNC output
needs one post-processor per machine, which no core team can write alone.

Three principles constrain the design:

- **Secure by design (principle 6):** a plugin gets no file, network or data
  access without declared permissions.
- **Determinism (RFC-0001, invariant 2):** the same project produces the
  same cut list on every machine, including the parts that plugins create.
- **Modest hardware (principle 5):** extension must not require a heavy
  runtime.

The founder decided on 2026-10-25 that third-party plugins run **isolated in
a WebAssembly sandbox**, choosing security and determinism over the stronger
copyleft argument that in-process plugins would give (see
[§8](#8-licensing-and-policy)).

## Guide-level explanation

Lucía's shop runs a CNC router from a brand OpenCabinet does not support.
She writes a post-processor plugin, in Rust or any language that compiles to
a WebAssembly component, implementing one interface: receive the evaluated
parts and their operations, return the program files.

Her manifest declares the plugin's identity, the API version it targets and
the extension point it implements. It requests **no permissions**: it never
touches the disk or the network itself. When a user exports, OpenCabinet
asks where to save, calls the plugin, and writes the files it returns.

A user installs her `.ocplugin` package. OpenCabinet shows what it is, who
made it, its license and its permissions (none), and asks for confirmation.
If the plugin crashes or loops, OpenCabinet stops it, reports the error and
keeps working.

Later, Tomás publishes a plugin that adds a diagonal corner cabinet type.
Product types take part in evaluation, so they run under the strictest
profile: no clock, no randomness, no files, no network; pure computation.
Every user who opens a project with Tomás's corner gets identical parts. The
project records which plugin, version and binary produced them; a user
without the plugin still sees the last computed parts, read-only, with a
warning.

## Reference-level explanation

### 1. Workspace layout and dependency rules

The monorepo holds one Cargo workspace. Proposed crates, lowest layer
first:

| Crate | Responsibility | May depend on |
| --- | --- | --- |
| `opencabinet-model` | Data model types, `Length`, IDs, validation; no I/O | — |
| `opencabinet-format` | `.ocab` reading and writing, format migrations | model |
| `opencabinet-eval` | Evaluator, construction standards, product type registry | model |
| `opencabinet-reports` | Cut list, bill of materials, operations lists | model, eval |
| `opencabinet-plugin-api` | WIT interface definitions and a Rust guest SDK for plugin authors | — |
| `opencabinet-plugin-host` | Wasmtime host, manifests, permissions, resource limits | model, eval, plugin-api |
| Module crates (`opencabinet-designer`, `opencabinet-cnc`, …) | Official functionality | any crate above |
| `opencabinet-app` | The desktop application binary | everything |

Rules, enforced in CI:

1. Dependencies point only downward in the table; the core crates never
   depend on modules, and `opencabinet-model` has no I/O.
2. The plan's rule for the core holds: code enters a core crate only when at
   least two modules need it and it cannot live outside without being
   duplicated.
3. Every third-party crate must have a GPLv3-compatible license, checked by
   `cargo-deny` against an allowlist (plan, section A).

### 2. Official modules and plugins

| | Official module | Third-party plugin |
| --- | --- | --- |
| Form | Rust crate compiled into the app | WebAssembly component in a `.ocplugin` package |
| Trust | Reviewed by maintainers | Untrusted by default |
| Process | Same process | Same process, inside a sandbox |
| API | The same extension traits | The same extension points, through WIT |

Official modules implement the same extension points as plugins (as Rust
traits that mirror the WIT interfaces). This keeps the plugin API honest:
anything a module can do through an extension point, a plugin can do too.
CI builds at least one example plugin per extension point and runs it
against every change, as the plan requires.

### 3. Plugin runtime

- **Format:** a WebAssembly component (the WebAssembly Component Model),
  with interfaces described in WIT. Any language with component tooling can
  produce plugins; the project ships a Rust SDK first.
- **Host:** Wasmtime, embedded in `opencabinet-plugin-host`.
- **System interface:** WASI 0.2, or its successor at implementation time,
  with **no ambient capabilities**: a plugin receives only the handles its
  granted permissions allow.
- **Resource limits:** a memory cap and CPU limits (fuel metering or epoch
  interruption) per call, with defaults the host can tighten. Exceeding a
  limit aborts the call, never the application.
- **Data exchange:** coarse-grained calls. A call receives what it needs
  (for example a product, its standard and the library snapshot) and returns
  complete results, so crossing the sandbox boundary stays cheap relative to
  the work done.

#### Deterministic profile

Plugins that take part in evaluation run with a profile that guarantees
reproducible results:

- no access to clocks, randomness, files, network or environment
  variables, whatever the manifest says;
- floating-point NaN canonicalization enabled, and no threads;
- lengths exchanged as 64-bit integers in nanometers (ADR-0009).

The derived-data cache of RFC-0001 records, for each product produced by a
plugin, the plugin ID, its version and the SHA-256 hash of its component
binary, so a different binary is detected even with the same version
number.

### 4. Extension points (API 0.x)

| Extension point | Purpose | Profile |
| --- | --- | --- |
| `product-type` | Declare parameters; turn a product, its standard and the library into assemblies, parts and operations (RFC-0001 §6, §9) | Deterministic |
| `validator` | Return errors and warnings for a product or the project | Deterministic |
| `exporter` | Turn a project and its evaluation into files (CSV, DXF, CNC programs); the host writes them where the user chooses | Standard |
| `report` | Produce tabular data for a report the host renders | Standard |

Contributions to the user interface (panels, dialogs, tools) are out of
scope until the user interface ADR is decided.

### 5. Manifest

Every plugin package contains a `plugin.toml`:

```toml
id = "org.example.cnc-acme"           # reverse-domain, globally unique
name = "ACME router post-processor"
version = "1.2.0"                      # SemVer
api = "0.1"                            # plugin API version targeted
license = "GPL-3.0-or-later"           # SPDX expression
authors = ["Lucía <lucia@example.org>"]
source = "https://example.org/cnc-acme"

[[provides]]
extension = "exporter"
name = "ACME router program"

[permissions]                          # all closed by default
# filesystem = ["read:user-selected"]
# network = ["https://api.example.org"]
```

- The host refuses a manifest whose `api` major version it does not
  support.
- Plugins that provide a deterministic extension point may not request any
  permission.
- Installation shows the identity, license, source and permissions, and
  asks for confirmation; an update that adds permissions asks again.

### 6. Packaging and installation

- A `.ocplugin` file is a ZIP archive containing `plugin.toml`, the
  component (`plugin.wasm`) and optional assets (icons, translations).
- Plugins are installed per user, from a file (works offline) or later from
  the registry. Package signing and the registry are a future RFC.
- Several versions of a plugin may be installed side by side, so an old
  project can be re-evaluated with the version it was designed with.

### 7. API versioning and stability

As the plan requires:

- The plugin API has its own SemVer, independent of the application's.
- It stays at **0.x** until three real modules use it; only then is 1.0
  declared.
- After 1.0, anything deprecated keeps working, with a warning, for one full
  major version before removal.
- CI runs the example plugins against every change to detect breakage.

### 8. Licensing and policy

Plugins isolated in a sandbox and talking to the core through a narrow
interface are most likely separate works, so the GPL of the core probably
does not extend to them. This was a deliberate trade:

- **Security and determinism** come from isolation and are guaranteed by
  the architecture.
- **Openness of the official catalog** comes from the registry policy
  (plan, section B): official and verified plugins must carry an
  OSI-approved, GPLv3-compatible license and publish their source.
  Plugins outside the registry may use any license, which the plan already
  accepts the project cannot prevent.

The WIT definitions and the Rust guest SDK are plugin templates under
ADR-0002, so they are licensed **MIT**: writing a plugin imposes nothing
beyond the plugin's own license.

### 9. Failure handling

A plugin that traps, exceeds a limit or returns invalid data produces an
error attributed to that plugin and that product; the rest of the project
still evaluates. Invalid data includes references that do not resolve,
non-positive dimensions and anything that fails the model's validation
(RFC-0001 §12).

## Drawbacks

- **Tooling maturity.** Component Model tooling is mature for Rust and
  improving elsewhere; authors in some languages will meet rough edges.
- **No native libraries in plugins.** A plugin cannot link a vendor's
  native SDK; such integrations need a separate helper program the user
  installs, outside the plugin system.
- **Copying across the boundary.** Every call serializes its inputs and
  outputs. Coarse-grained calls keep this small, but it is not free.
- **Weaker copyleft over plugins**, accepted as described in §8.
- **Deferred UI extensibility** until the UI ADR.

## Alternatives

- **Separate processes** speaking JSON-RPC: any language and simple to
  write, but the sandbox is only as fine as the operating system's, and
  determinism cannot be guaranteed.
- **In-process native libraries:** the strongest argument that plugins must
  be GPL, but no security boundary, and Rust has no stable binary interface,
  so plugins would break with every compiler or dependency change.
- **An embedded scripting language** (such as Lua or Rhai): simple and
  sandboxable, but it locks authors into one language, is slower for
  geometry-heavy work, and has a much smaller ecosystem.

## Compatibility and migration

This is the first plugin architecture, so there is nothing to migrate. The
API starts at 0.1, and breaking changes are allowed in 0.x minor versions,
announced in release notes.

## Unresolved questions

1. **User interface contributions:** can plugins add panels, tools or
   dialogs, and how? Depends on the user interface ADR.
2. **Default resource limits** (memory and CPU per call): set during
   implementation, from measurements on modest hardware.
3. **Signing and distribution:** left to the registry RFC.
4. **Official modules as plugins:** should some official modules (for
   example CNC post-processors) ship as WASM plugins themselves, to prove
   the API and allow independent updates?

## Sources

- Project plan, sections A, B and C (Spanish): [docs/es/plan.md](../es/plan.md).
- WebAssembly Component Model and WIT, <https://component-model.bytecodealliance.org/>.
- WASI 0.2, <https://wasi.dev/>.
- Wasmtime documentation, <https://docs.wasmtime.dev/>.
- `cargo-deny` documentation, <https://embarkstudios.github.io/cargo-deny/>.
- [ADR-0011](../adr/0011-core-technical-stack.md), Rust for the core.
