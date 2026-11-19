<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Test projects

Projects written by hand for the tests, from scratch: no data from any
shop, client or other program. Each is an `.ocab` project in its unpacked
directory form (RFC-0001 §13).

| Project | What it covers |
| --- | --- |
| `kitchen.ocab` | A small kitchen: walls with a window, cabinets on a wall and an island, a blind corner, a filler, an unplaced vanity with a quantity, five-piece doors, a hardware adjustment, an override, plugin data and an attachment |
| `sample-kitchen.ocab` | The kitchen users get with a release, to try the program with: only cabinets it can evaluate, with every material chosen, placed on two walls without a warning |
| `future-minor.ocab` | A file from a newer minor version: unknown fields at every level, a new library section, a new insert kind and a plugin's entry, all of which must survive a save |

The files are written exactly as OpenCabinet writes them, so tests can
check that saving reproduces them byte for byte.
