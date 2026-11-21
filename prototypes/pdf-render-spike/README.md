<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# PDF rendering spike (RFC-0004)

Throwaway. Deleted when RFC-0004 is accepted.

Questions from RFC-0004 §11:

1. Is **hayro** fast enough for drawings on modest computers (ADR-0014)?
2. Can **lopdf** save a markup as an incremental update that leaves the
   original bytes untouched, and does hayro draw it?

```sh
cargo run --release -- [sheets] [segments per sheet]   # default 30 40000
```

It writes a sheet set of its own (`out/sheets.pdf`): 36" × 24" sheets,
each with a border, 40,000 short wall-like lines and 400 text labels in
Helvetica, uncompressed. It is denser than a typical architectural sheet
on purpose. Pictures land in `out/`.

## Results

On the founder's computer (12 threads), release build, 2026-11-21:

| Measure | Time |
| --- | --- |
| Write the 30-sheet set (43.9 MB) | 447 ms |
| Open it with hayro | 0.2 ms |
| First sheet fit to the 1366 × 768 screen | 47.6 ms, then 46.3 ms |
| A 512 × 512 tile at 400 % | median 10.5 ms, slowest 15.7 ms |
| Full sheet at 150 dpi (5400 × 3599) | 140 ms |
| 4 sheets fit to screen on 2 threads | 125 ms |
| Incremental save of one markup | 102 ms, 586 bytes appended |

- The original bytes are untouched after the save, as RFC-0004 §3 needs.
- hayro draws the saved markup from its appearance stream, and leaves it
  out when asked for the page alone (`RenderContent::PAGE`). The module
  can therefore draw the page once and keep live markups on top, without
  redrawing the page each time a markup moves.

## Findings

1. **Fast enough here; still to check on two cores.** Every measure is
   far inside a frame or two. The CI runner (2 to 4 cores) or the
   reference computer must confirm before setting budgets.
2. **One renderer per thread.** `RenderCache` is not shared between
   threads; each background thread opens its own `Pdf` over the same bytes
   (shared, not copied) and keeps its own cache.
3. **Opening is instant**: hayro reads objects on demand, so a large set
   opens as fast as a small one.
4. **Still to try**: compressed content, embedded fonts, images and
   scanned sheets, from real drawings we are allowed to use, and PDFium
   only if hayro falls short on those.
