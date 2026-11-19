<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Roadmap

The project advances in phases. Each phase ends at a verifiable **gate**,
not a date: the pace depends on volunteer time and on contributors joining.

| Phase | Goal | Gate to the next phase |
| --- | --- | --- |
| 0 — Foundations | Close the risks that block publishing | ✅ Done |
| 1a — Repository | A professional repository and the decisions already taken, recorded | ✅ Done |
| **1b — Design** (current) | Decide what is hard to reverse: data model, units and precision, stack, plugin architecture | RFCs accepted; a prototype saves and reopens a project in the project's own format |
| **2 — First useful module** (started in parallel) | Basic cabinet designer, cut list, bill of materials; CSV, PDF and DXF export; release 0.1 | Real users produce a correct cut list for a real job |
| 3 — Early community | Stop depending on one person: tested contribution guide, plugin API 0.x, first external plugin | A second active maintainer and a working external plugin |
| 4 — Expansion | Shop drawings, cut optimization, PDF review and markup, verified plugin registry | Plugin API 1.0 stable, used by three modules |
| 5 — Platform | Own 2D CAD, CNC output, server collaboration, optional local AI | The project runs without the founder being indispensable |

Phase 2 started before the phase 1 gate, on 2026-10-25, with work that does
not depend on the open points of the RFCs; the RFCs are still accepted only
after their public comment period.

Machining in 0.1 covers back and drawer-bottom grooves, 32 mm line boring
for shelf pins and hinge plates, and hinge cups. Still to come: case joinery
(dowels, confirmat screws), slide and pull holes, angled pocket holes for
face frames, and five-piece profiles, which are cut with shaper tooling.

Blind corner cabinets in 0.1 take doors and shelves. Drawers in them need
a partition or blocking next to the blind part for their slides, and come
later.

AI features are cross-cutting and optional: they arrive where they add value
and never become a requirement.
