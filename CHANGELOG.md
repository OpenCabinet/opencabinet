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

- **Light and dark themes:** **View → Theme** chooses light, dark or the
  same as the system, which is the usual choice. The program remembers it in
  the user's folder, beside the shop template, and exported PDF and DXF files
  do not change. Errors and warnings now have colors that are easy to read on
  either background.
- **Several rooms:** the Walls tab adds, renames, chooses and deletes the
  project's rooms, each with its own walls. Cabinets still belong to the
  project and can be placed in any room; deleting a room that holds walls
  or placed cabinets asks first, and keeps the cabinets.
- **Plan view:** on the Walls tab, **View** shows the chosen room from above
  instead of the wall's elevation: walls at their thickness, cut by doors
  and pass-throughs, with a line for each window; every placed cabinet's
  footprint with its label, dashed when it hangs off the floor; and each
  wall's length and name outside the room. The edited wall and the edited
  cabinet are outlined. The plan is on screen only for now.

### Changed

- Elevations are named after their room and wall, as "Kitchen - North", on
  each page of the PDF and in each DXF file's name, and placement warnings
  say which room when the project has more than one.

## [0.1.0-beta.2] - 2026-11-07

The first published test release of OpenCabinet Designer, for shops willing
to try it on a real job and tell us what is wrong. It is a beta: check every
size before cutting. Version 0.1.0-beta.1 was prepared but never published,
so everything is listed here.

### What to try

Set up your shop first: enter your materials on the Materials tab and your
rules on the Standard tab, then choose **File → Save as shop template**. Then
enter a small job you have already built, and compare the cut list with what
you really cut. To report what is wrong, use the
[bug report form](https://github.com/OpenCabinet/opencabinet/issues/new/choose).
Please do not attach anything you are not free to share.

### Added

- **Cabinets:** base, wall and tall cabinets, frameless or with a face
  frame, and blind corner base and wall cabinets, where a fixed blind panel
  takes the part of the front hidden by the adjoining cabinet. Every size
  comes from the shop's construction standard and is exact to the nanometer,
  in inches or millimeters.
- **Fronts:** sections stacked from top to bottom: slab or five-piece doors,
  drawers with their boxes, false fronts, open sections and appliance spaces,
  each a fixed height or taking the rest, with the number of doors, the hinge
  side and adjustable shelves.
- **Several cabinets per project:** add, duplicate, rename and delete them,
  and say how many the job needs of those not placed on a wall.
- **Materials:** a Materials tab edits the project's sheets (name, the
  thickness they are sold as, the real one, length, width and grain) and edge
  bands. A cabinet chooses the material of each part it has, and can give its
  choices to every other cabinet.
- **Construction standards:** a Standard tab edits every rule the evaluator
  reads, by group: toe kick, case, back, fronts, face frame, drawers, shelves,
  five-piece fronts, line boring, hinges and the bill of materials. New
  projects carry a frameless and a face frame standard, the File menu starts
  either kind, and each cabinet chooses its own. The values to start from are
  general ones to adjust, not recommendations.
- **Shop template:** the File menu saves the project's materials, edge bands,
  standards, hardware and units as the shop's template in the user's folder,
  with the materials of the selected cabinet for each part. New projects
  start from it; projects already saved do not change. Nothing of the job
  goes in it, and a template that is replaced or removed stays as a backup.
- **Cut list** on screen, as CSV (with commas, or semicolons for Excel in
  Spanish) and as PDF.
- **Bill of materials:** what to buy for the job, on screen, on the last page
  of the cut list PDF, and as CSV. Sheets are estimated from the parts' area
  plus the standard's waste; solid wood is counted by length, in boards; edge
  band by the length of every banded edge plus an allowance for trimming; and
  the hinges, drawer slides and shelf pins the cabinets use.
- **Machining:** the back's groove, 32 mm line boring for shelf pins and
  hinge plates, hinge cups, and the drawer bottom's groove.
- **Parts as DXF** for CAM programs: one file per part at its cut size, with
  layers named after the tool and depth, and one file with every part.
- **Walls:** walls at right angles with windows and doors, and cabinets
  placed on them, with warnings when a cabinet runs past a wall, covers an
  opening or collides with another, also around a corner, and when a blind
  corner is left in view or stands in front of the doors.
- **Elevations** of every wall: on screen, as PDF at a standard scale, one
  wall to a page, and as DXF at full size.
- **Windows installer** that installs for the current user, without
  administrator rights.
- **A sample kitchen,** `sample-kitchen.ocab`, to try the program with:
  base, wall and tall cabinets on two walls, with every material chosen.

### Known limitations

- **The file format can still change.** Comments on its design are open until
  2026-11-08. A project saved with this beta may not open in a later version,
  so keep what you need from it as PDF and CSV too.
- Sink, filler and vanity cabinets show as "cannot be evaluated yet".
- Drawers in blind corner cabinets are not supported yet.
- Drawer slides in face frame cabinets mount on blocking out to the frame, and
  that blocking is not in the cut list yet. A note says so.
- A front with sections side by side is kept as it is, but cannot be edited in
  the window.
- The window cannot add or duplicate a standard, and only the standard's hinge
  can be chosen from the hardware. Projects saved earlier do not gain the other
  standard.
- There is no undo, including for giving a cabinet's materials to every other
  cabinet. Save first.
- The window edits the first room of a project; walls run at right angles;
  free-standing islands are not checked for collisions.
- Slide, pull and joinery holes, face frame pocket holes and five-piece
  profiles are not machined yet.
- Windows only. The installer is not signed yet: Windows warns that it
  protected your PC; choose **More info**, then **Run anyway**, after
  checking the installer against `SHA256SUMS.txt`.

[Unreleased]: https://github.com/OpenCabinet/opencabinet/compare/v0.1.0-beta.2...HEAD
[0.1.0-beta.2]: https://github.com/OpenCabinet/opencabinet/releases/tag/v0.1.0-beta.2
