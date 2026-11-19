<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# 0015 — Libraries for PDF and DXF export, and a bundled font

- **Status:** accepted
- **Date:** 2026-10-26
- **Deciders:** Andrés (@A-newGP)

## Context and problem statement

Version 0.1 promises CSV, PDF and DXF exports. CSV needs no library and
already ships. [ADR-0011](0011-core-technical-stack.md) named PDF and DXF
the weakest part of the Rust ecosystem, so a throwaway prototype
([#13](https://github.com/OpenCabinet/opencabinet/pull/13)) compared the
candidates on the same shop drawings and checked every file with
independent open source readers. Which libraries should the program use,
and which font should its drawings use?

## Decision drivers

- **Correct files:** readers such as CAD, CAM and PDF programs must open
  them without repairs; a DXF must not leave its units to a guess, because a
  part cut in inches instead of millimeters is scrap.
- **The same drawing everywhere:** a shop drawing must look the same on
  every computer and printer, and show every character a user may type.
- **Archival and accessibility:** drawings are kept for years, and some
  users read with screen readers.
- **Licenses** compatible with GPL-3.0-or-later (ADR-0002, ADR-0013).
- **Size and speed** within the budgets of
  [ADR-0014](0014-modest-hardware-budgets.md).

## Considered options

For PDF: **krilla** (MIT or Apache-2.0) or **printpdf** (MIT). For DXF: the
**`dxf` crate** (MIT) or **writing DXF R12 by hand**. For the font: **a
font bundled with the program**, or **fonts already on the computer**.

## Evidence: the export prototype

From CI on GitHub-hosted runners (run 40235929328), every file checked with
ezdxf, pypdf and qpdf:

| | krilla | printpdf | `dxf` crate | DXF R12 by hand |
| --- | --- | --- | --- | --- |
| Lines of code | 154 | 129 | 83 | 136 (with CSV) |
| Crates in the tree | 87 | 104 | 46 | 15 (the core's) |
| Executable, Windows | 3.2 MB | 2.0 MB | 1.4 MB | 0.4 MB |
| Output | PDF 1.7, font subset embedded | PDF 1.3, built-in Helvetica | DXF 2018, millimeters declared | DXF R12, no units |
| Checks | no errors | no errors | 0 errors, 7 repairs | 0 errors, 0 repairs |

Every run took well under a tenth of a second.

## Decision outcome

Chosen:

1. **PDF with krilla.** It embeds the subset of the font a document uses,
   so a drawing looks the same everywhere and any character can appear; it
   supports archival PDF/A and tagged, accessible PDF. printpdf's built-in
   Helvetica is limited to Western European characters, and each reader
   substitutes its own; printpdf also pulls in more crates.
2. **DXF with the `dxf` crate**, writing DXF 2000 or later with
   millimeters declared. Its two defects found by the prototype are worked
   around in our code and proposed upstream: an invalid default
   transparency on every entity, and table headers without handles, which
   ezdxf repaired. The ezdxf audit runs in CI on every DXF the tests
   write. Hand-written R12 stays out: it cannot declare its units.
3. **Bundle one open font: Noto Sans**, under the SIL Open Font License
   1.1. It covers the Latin, Greek and Cyrillic scripts, has clear figures
   and the fraction, degree and inch characters drawings need, and is
   maintained by a large open project; other scripts can add their Noto
   family later. It is installed as a
   separate file next to the program and read at run time, not compiled
   into the executable, so the GPL program and the OFL font remain separate
   works distributed together, as both licenses allow; fonts embedded in
   exported PDFs are expressly permitted by the OFL. The official license
   text goes into `LICENSES/OFL-1.1.txt`.
4. **Text is measured** with the font's own metrics, through the text
   layout support that krilla is designed to work with (parley), so labels
   center exactly instead of the prototype's estimate.
5. **Paper size follows the units:** US Letter for imperial projects, A4 for
   metric ones, changeable per export.

### Consequences

- Good, because PDFs look identical everywhere, can be archived as PDF/A
  and read by screen readers.
- Good, because every DXF states its units, and CI proves each one opens
  cleanly.
- Good, because all three libraries are permissively licensed and checked
  by cargo-deny like every other dependency.
- Bad, because krilla adds about 3 MB to the executable and Noto Sans about
  0.6 MB to the installer: well within the 50 MB budget, which the
  installer check keeps watching.
- Bad, because we depend on the `dxf` crate's maintainer accepting fixes;
  until then, our workaround stays and the audit guards it.
- Neutral, because a new license, the OFL, enters the repository, for the
  font only.

## Pros and cons of the options

### krilla

- Good, because of embedded fonts, PDF/A and tagged PDF (above).
- Bad, because it is larger, and it needs a font to be shipped.

### printpdf

- Good, because it is smaller and needs no font file.
- Bad, because the built-in fonts cover few characters and render
  differently in each reader.

### `dxf` crate

- Good, because it writes modern DXF with units in the fewest lines.
- Bad, because of the two defects, which need a workaround and a fix
  upstream.

### DXF R12 by hand

- Good, because it needs no dependency and passed the audit cleanly.
- Bad, because R12 cannot declare units, and modern features would have to
  be written by hand.

### Fonts on the computer

- Good, because nothing is shipped.
- Bad, because drawings would differ between computers, and some
  computers lack the characters a project uses.

## Sources

- The export prototype, its README and CI run 40235929328:
  [#13](https://github.com/OpenCabinet/opencabinet/pull/13).
- SIL Open Font License 1.1 and its FAQ (<https://openfontlicense.org>).
- Free Software Foundation, "Various Licenses and Comments about Them": the
  SIL Open Font License.
- ISO 19005 (PDF/A) and ISO 14289 (PDF/UA), as summarized in the krilla
  documentation.
- The Noto fonts project (<https://notofonts.github.io>).
