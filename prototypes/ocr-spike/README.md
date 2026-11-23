<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# OCR spike (throwaway)

Can OpenCabinet Review make scanned drawings searchable with
[ocrs](https://github.com/robertknight/ocrs) 0.14 (pure Rust, MIT or
Apache-2.0), and how fast on modest hardware (ADR-0014)? Measured on
2026-11-23. Delete this folder when the OCR decision is recorded.

## Run it

The models are not in git. Download the two ONNX files from
<https://huggingface.co/robertknight/ocrs> (license CC BY-SA 4.0) into
`models/`:

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `text-detection-ssfbcj81.onnx` | 2 518 158 | `c15df092060abe9417929b04eaf552b98946e15144036b3c83b09a3bf4d5d5a7` |
| `text-rec-checkpoint-s52qdbqt.onnx` | 9 713 787 | `3774cdfe8ba082a17c181d5ab0698f3ff2fd0def07cd2b9134faec9b1a191919` |

```sh
cargo run --release                      # the test sheets below
RAYON_NUM_THREADS=2 cargo run --release  # as on two cores
cargo run --release -- drawing.pdf 150   # your own scan, at 150 dpi
```

## What it measures

A sheet made for the test with 20 words drawings have (room names at
18–24 pt, cabinet labels such as B30 and W3030 at 10 pt, notes at 8 pt,
dimensions such as 36" and 2'-6" at 8 pt, small notes at 6–7 pt), three
rows of them, over a border and a light vertical grid. Each sheet is drawn
as a picture at several resolutions, as printed and as a scan would be
(turned half a degree, ±8 of 255 noise), and read by ocrs in tiles of its
detection model's 800 × 600 input with 120 px of overlap (a whole sheet
shrunk to that size loses every small word).

## Results

12-thread desktop; with `RAYON_NUM_THREADS=2` the times were the same
(ocrs reads tile after tile).

| Sheet | dpi | Words found | Seconds |
| --- | --- | --- | --- |
| Letter, printed | 100 / 150 / 200 / 300 | 4 / 7 / 7 / 1 of 20 | 1.4 / 2.6 / 4.6 / 8.6 |
| Letter, scanned | 100 / 150 / 200 / 300 | 4 / 6 / 9 / 5 of 20 | 1.0 / 2.8 / 5.2 / 8.7 |
| 36" × 24", printed | 100 / 150 / 200 / 300 | 14 / 14 / 7 / 0 of 20 | 11 / 18 / 33 / 45 |
| 36" × 24", scanned | 100 / 150 / 200 / 300 | 9 / 13 / 12 / 10 of 20 | 11 / 27 / 47 / 85 |

What it read, on a good run (36" × 24" printed, 150 dpi): most room names,
cabinet labels and long words; never a dimension (36", 2'-6") nor TYP.

## What it shows

- **Not good enough for drawings.** At best 70 % of the words, never the
  dimensions, and results that change unpredictably with resolution.
- **Linework is read as letters.** Borders and grid lines come back as
  strings of "1", "T", "L" and "C", and at higher resolutions words fall
  apart into single letters. ocrs is trained on photos and documents
  (HierText), not on sheets crossed by lines.
- **Slow.** 11–27 s for a 36" × 24" sheet at 100–150 dpi.
- Its author calls it an early preview; this agrees.

So ocrs is not recommended for Review now. ADR-0017 should compare the
other candidates (Tesseract, the OCR built into Windows) on these same
sheets before anything is chosen.
