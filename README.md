<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# OpenCabinet

> [!WARNING]
> **Pre-alpha.** There is no usable software yet. This repository currently
> holds the project's foundations: licensing, contribution rules and design
> decisions. Nothing here is ready for production work.

OpenCabinet is a free, open source and modular suite for designing,
documenting and manufacturing architectural millwork and cabinetry.

## Why another tool?

The difference is millwork knowledge, not yet another generic CAD program.
The project starts where a shop gets value first: a cabinet designer that
produces a correct cut list and bill of materials. A general 2D CAD module
comes much later, once a community exists to sustain it.

## Principles

1. **Free forever.** Distributed derivatives stay open (copyleft).
2. **Useful early.** Every phase delivers something a shop can use on real
   jobs.
3. **Your data is yours.** Open, documented file formats; no lock-in.
4. **Works offline and without AI.** Core features never depend on an
   external or paid service.
5. **Truly modular.** Install only what you use; the core runs on modest
   hardware.
6. **Secure by design.** Plugins get no file, network or data access without
   declared permissions.
7. **Honest about its size.** Process and structure grow with the community,
   not before.

## Status

**OpenCabinet Designer 0.1.0-beta.2** is the first published test release:
cabinets, your shop's materials and construction standards, cut lists, a bill
of materials, machining, walls and elevations, exported as CSV, PDF and DXF.
It is for shops willing to try it on a real job and
[report what is wrong](https://github.com/OpenCabinet/opencabinet/issues/new/choose);
download it from the [releases](https://github.com/OpenCabinet/opencabinet/releases),
and see the [changelog](CHANGELOG.md) for what it does and does not do yet.

See the [roadmap](ROADMAP.md) for what comes next. Decisions that are hard
to reverse are recorded as [architecture decision records](docs/adr/).

Imperial units (including fractional inches) are the default; metric is a
first-class option.

## Contributing

Contributions of every kind are welcome: code, documentation, translations,
testing and, above all, real-world millwork expertise. Read
[CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Every commit
must be signed off under the [Developer Certificate of Origin](https://developercertificate.org/).

Everyone taking part is expected to follow the
[Code of Conduct](CODE_OF_CONDUCT.md). For help, see [SUPPORT.md](SUPPORT.md);
to report a vulnerability, see [SECURITY.md](SECURITY.md).

## License

OpenCabinet follows the [REUSE](https://reuse.software/) specification:
every file declares its copyright and license, and the full license texts
live in [`LICENSES/`](LICENSES/).

| Component | License |
| --- | --- |
| Core and official desktop modules | [GPL-3.0-or-later](LICENSE) |
| Server services (future) | AGPL-3.0-or-later |
| File format specification (future) | CC-BY-4.0 |
| Plugin templates and examples (future) | MIT |
| Documentation and interface icons | CC-BY-SA-4.0 |
| Trivial configuration files | CC0-1.0 |

The rationale is recorded in [ADR-0002](docs/adr/0002-license-per-component.md).

The OpenCabinet name and logo are not covered by these licenses; see
[TRADEMARKS.md](TRADEMARKS.md).
