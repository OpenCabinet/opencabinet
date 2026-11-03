<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Changelog

All notable changes to OpenCabinet are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Bill of materials:** what to buy for the job, on a Materials tab next
  to the cut list, on the last page of the cut list PDF, and as CSV.
  Sheets are estimated from the parts' area plus the standard's waste;
  solid wood is counted by length, in boards; edge band by the length of
  every banded edge plus an allowance for trimming; and the hinges, drawer
  slides and shelf pins the cabinets use. Standards gain the
  `billOfMaterials` rules for the waste and the allowance.
- **Blind corner cabinets** (`blind-corner-base@1`,
  `blind-corner-wall@1`), frameless and face frame: the part of the front
  hidden by the adjoining cabinet, `blindWidth` from its `blindSide`, takes
  a fixed blind panel made like a slab door, and nothing that opens reaches
  into it. Face frames gain a mullion beside the panel. Doors hinge away
  from the blind side unless they say otherwise. Elevations warn when the
  cabinet on the adjoining wall leaves part of the blind panel in view or
  stands in front of the doors. In the window, the Type list offers
  "Blind corner base" (42" wide, 24" blind) and "Blind corner wall" (36"
  wide, 12" blind), labeled BC and WBC, with fields for the blind side and
  width; hiding the other side rehangs a single door away from it.
- The sample kitchen's blind corner has a back, so it is evaluated.
- **Front editor:** the window edits a cabinet's front as sections stacked
  from top to bottom: doors, drawers, false fronts, open sections and
  appliance spaces, each a fixed height or taking the rest, with the number
  of doors, the hinge side and the shelves. A front with sections side by
  side is kept as it is, but cannot be edited in the window.
- **Quantity:** how many of a cabinet the job needs when it is not placed
  on a wall. A placed cabinet counts its placements.
- **Materials:** a Materials tab edits the project's sheets (name, the
  thickness they are sold as, the real one, length, width and grain) and
  edge bands. A material in use cannot be deleted, and the message says
  which cabinets use it. A cabinet chooses the material of each part it
  has, and can give its choices to every other cabinet. The tab that says
  what to buy is now called Bill of materials.
- **Construction standards:** a Standard tab edits every rule the
  evaluator reads, by group: toe kick, case, back, fronts, face frame,
  drawers, shelves, five-piece fronts, line boring, hinges and the bill of
  materials. Rules it does not show stay in the file. Values that cannot be
  read are refused in the field.
- **Frameless or face frame:** new projects carry both standards, the File
  menu starts either kind, and each cabinet chooses its own. The face frame
  values are general ones to adjust: stiles and rails of 1-1/2", a partial
  overlay of 1/2" and solid wood 3/4" × 5-1/2" × 96".
- **Shop template:** the File menu saves the project's materials, edge bands,
  standards, hardware and units as the shop's template in the user's folder,
  with the materials of the selected cabinet for each part. New projects
  start from it; projects already saved do not change. Nothing of the job
  goes in it, and a template that is replaced or removed stays as a backup.

### Known limitations

- Drawers in blind corner cabinets are not supported yet.

## [0.1.0-beta.1] - 2026-10-26

The first test release of OpenCabinet Designer, for shops willing to try
it on a real job and tell us what is wrong. It is a beta: check every
size before cutting.

### Added

- **Cabinets:** base, wall and tall cabinets, frameless or with a face
  frame. Fronts are slab or five-piece doors, drawers and false fronts,
  with drawer boxes and adjustable shelves. Every size comes from the
  shop's construction standard and is exact to the nanometer, in inches
  or millimeters.
- **Several cabinets per project:** add, duplicate, rename and delete
  them; the cut list covers the whole project.
- **Cut list** on screen, as CSV (with commas, or semicolons for Excel in
  Spanish) and as PDF.
- **Machining:** the back's groove, 32 mm line boring for shelf pins and
  hinge plates, hinge cups, and the drawer bottom's groove.
- **Parts as DXF** for CAM programs: one file per part at its cut size,
  with layers named after the tool and depth, and one file with every
  part.
- **Walls:** walls at right angles with windows and doors, and cabinets
  placed on them, with warnings when a cabinet runs past a wall, covers
  an opening or collides with another, also around a corner.
- **Elevations** of every wall: on screen, as PDF at a standard scale, one
  wall to a page, and as DXF at full size.
- **Windows installer** that installs for the current user, without
  administrator rights.

### Known limitations

- Not yet: a bill of materials (sheets, edge band length, hardware), and
  blind corner, sink, filler and vanity cabinets, which show as "cannot
  be evaluated yet".
- The window edits the first room of a project; walls run at right
  angles; free-standing islands are not checked for collisions.
- Slide, pull and joinery holes, face frame pocket holes and five-piece
  profiles are not machined yet.
- Windows only. The installer is not signed yet: Windows warns that it
  protected your PC; choose **More info**, then **Run anyway**, after
  checking the installer against `SHA256SUMS.txt`.

[Unreleased]: https://github.com/OpenCabinet/opencabinet/compare/v0.1.0-beta.1...HEAD
[0.1.0-beta.1]: https://github.com/OpenCabinet/opencabinet/releases/tag/v0.1.0-beta.1
