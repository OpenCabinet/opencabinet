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

- **OpenCabinet Review, first pieces** (RFC-0004): a separate program,
  `opencabinet-review`, with an installer of its own, that opens PDF
  drawings and marks them up.
  - **Viewing:** pages drawn with hayro, fit to the window or its width,
    zoomed with Ctrl and the wheel, moved by dragging; a column of page
    thumbnails to go to any page. Markups other programs made are drawn
    from their own appearance, exactly as that program drew them, until
    they are changed.
  - **Markups:** cloud, rectangle, ellipse, polygon, arrow, line,
    polyline, pen, highlight, text box, callout, note and stamp. Each is
    picked, moved, recolored, made thinner or thicker, filled, given a
    subject, a status (open, accepted, rejected, completed) and a comment,
    and deleted, from the page or the markup list. Stamps come in eight
    wordings and colors, with who stamped and when, and their words can be
    changed.
  - **Measuring:** a page is calibrated by drawing along a known
    dimension or by choosing a common scale (architects', engineers' or
    metric), for one page or all; lengths, lengths along a path, areas
    and counts then read to scale, in feet and inches (`12'-6 1/2"`) or
    millimeters. Other programs' measurements keep their own scale.
  - **Copy, paste and duplicate** markups, on the same page or another,
    and zoom to the one selected.
  - **Find** words on the drawing, page by page, each place highlighted.
  - **Compare** two drawings: another PDF laid over this one, what only
    this one has in red, what only the other has in blue.
  - **Export flattened:** a copy with every markup drawn into the page,
    to send where it must not be edited.
  - **Markup list filters** by status and by words in the subject,
    author or text.
  - Escape stops a tool and goes back to Select; the tool in use is the
    one highlighted.
  - **A window built around the drawing:** menus on top, a rail on the
    left that opens one panel at a time (pages, bookmarks, find and
    compare, measure, session), the markup list under the drawing, the tools on
    the right as icons of our own, and a status bar with pages, zoom and
    scale.
  - **The markup list under the drawing:** subject, page, comments,
    author, date, status and color, sorted by any column's header, with
    the selected markup's subject, status and text beside it.
  - **Bookmarks:** the PDF's own bookmarks, the sheet index many sets
    carry, in a panel; a click shows the sheet.
  - **Markup summary:** the list as shown, filtered and sorted, written
    as a PDF report with each markup's color, ready to send.
  - **My tools:** a markup's style (kind, color, width, fill and
    subject) saved to draw with again in any drawing.
  - **Review sessions on the local network:** start one with the PDF
    open and send the invitation; whoever joins gets the drawing and
    everyone's markups, and every markup, change and delete reaches
    everyone within a moment. The connection is encrypted with TLS
    against the host's own certificate, whose fingerprint the invitation
    carries, and only who has the invitation's secret gets in. The
    session panel lists who is in, what each did, and a chat. The
    invitation carries every address of the host's computer and whoever
    joins tries them all; the host listens on port 47100 when it is free.
    The host is told when the Windows firewall or a public network keeps
    people out, and can let Review through with Windows' own permission
    prompt; installed for everyone, the installer can add that rule. Not yet: a
    new host when the host leaves, and sessions across the internet.
  - **Replies:** each markup carries a conversation. Replies are read
    from and written to the PDF as standard replies, so other PDF viewers
    list them under the markup; answering another program's markup leaves
    it exactly as it was. The list counts them, the CSV has them, and in
    a session replies given at once by two people are both kept. Who wrote
    a reply can take it back.
  - **Grips:** the markup selected shows grips on its corners, ends and
    points; dragging one reshapes it.
  - **Several markups at once:** Ctrl+click adds or takes one out, a box
    dragged with Shift picks those inside it, and Ctrl+A picks every
    markup on the page. They move together, take a subject, status,
    color, width or fill together, and are deleted together, each change
    one step to undo.
  - **Snap points:** while drawing, measuring or calibrating, the mouse
    snaps to the drawing's own lines: their ends, middles, crossings, or
    the nearest point on a line, and to other markups' points. A mark
    shows what it snaps to, and the status bar names it. F3, the Snap
    button or the View menu turn it on and off. The lines are read once
    per page and filed in a grid, so dense sheets answer at once.
  - **Click, then click:** a line, arrow, length, rectangle, cloud or
    any other dragged markup can also be placed by a click at each end,
    and a page calibrated the same way. Shift keeps a line at a multiple
    of 45° and a box square. A polyline shows its next segment following
    the mouse.
  - **One full page:** a mode where the page fits the window and the
    wheel zooms without Ctrl (Ctrl+wheel then moves the page). Review
    remembers it, and snapping, for next time.
  - **Pages:** turn pages a quarter turn either way, move them up or
    down, put in a blank page or every page of another PDF, delete
    pages, and extract pages with their markups to a PDF of their own.
    Ctrl or Shift + click on the thumbnails chooses several. The page
    tools are in the Pages panel and a Pages menu; markups and page
    scales go with their page, and Undo takes each change back. Saving
    still appends one incremental update; pages cannot change during a
    session.
  - **Tabs:** several PDFs open at once, each in a tab above the
    drawing, with its own markups, pages and undo. Open picks several
    files at a time, a file already open goes to its tab, Ctrl+Tab goes
    to the next tab and Ctrl+W closes one, asking first if it has
    changes not saved. The tool, color and line in use follow from tab
    to tab. File > Open recent lists the last ten files.
  - **Sheet sets:** a project's drawings kept together in a small
    `.ocset` file beside them. Opening a set opens each drawing in a tab
    and lists them in the Set panel, saying which are open, shown or
    missing; drawings are added and taken out there, and File > Save
    tabs as sheet set makes one from the open tabs.
  - **Handing a session over:** the host picks who hosts next in the
    Session panel. That person's computer starts hosting the copy it
    has, everyone else moves to it with theirs, and nothing is lost on
    the way. A host who leaves hands the session to someone still in
    it.
  - **Session server:** `opencabinet-review-server` (AGPL-3.0-or-later)
    holds sessions for people on different networks. It prints a server
    address (`ocs1:…`); Review starts a session there from the Session
    panel, and the invitation it gets works like any other. The server
    keeps its certificate on disk, so its address stays the same.
  - **Turn the view:** View > Turn view turns every page a quarter turn
    on screen without changing the file; markups, snapping and
    measuring turn with it, and the status bar says how far.
  - **Page labels:** a drawing's own page names, such as A-201, are
    shown in the status bar and under the thumbnails. Moving pages drops
    them, since they name pages by their place.
  - **Stamps of your own:** a stamp's words may hold `{name}` and
    `{date}`, filled in when it is placed (the date as 11/22/2026 in
    imperial units, 2026-11-22 in metric). Type new words under the
    stamp list to add a stamp in the color in use; Review keeps them in
    stamps.json beside My tools. A new built-in stamp, REVIEWED BY
    {name} {date}.
  - **Angle and perimeter:** Angle measures the angle at a corner from
    three clicks, finished at the third; Perimeter measures the length
    around a closed shape at the page's scale.
  - **Quantities:** the Totals panel adds up the measuring markups by
    subject and kind (lengths and areas at each page's scale, counts
    counted), says which pages they are on and how many had no scale,
    and exports them for spreadsheets.
  - **Links:** clicking a link a drawing already has goes where it
    says: another page, another drawing (opened in a tab) or a web
    address (opened in the browser). The Link tool draws a box whose
    text says where it goes: a page number, a drawing such as
    A-301.pdf#2, or a web or mail address. It is saved as a standard PDF
    link, so other viewers follow it too; double-click it to follow it
    in Review. Other programs' links are followed, not listed or
    changed.
  - **Only these:** a switch on the markup list shows on the drawing only
    the markups the list shows, other programs' included; the rest can
    not be picked meanwhile. The file does not change.
  - **Find in every open drawing:** the Find panel lists the open
    drawings that have the words, with how many times and on how many
    pages; choosing one shows it at the first match.
  - **Session server package:** each pull request that touches it builds
    `opencabinet-review-server` for Linux and Windows, starts it and
    checks it from outside (a session started on it, joined, a markup
    crossing it), and keeps it as an artifact with its license, guide
    and a systemd service for Linux. `--version` says which it is.
  - **Tool sets:** My tools became named sets of saved tools, in a Tool
    sets panel on the left. Sets are added, renamed, folded, removed
    (asking first when they hold tools) and exported or imported as a
    `.octools` file to share with a team, with the stamps their tools
    place; a tool moves from one set to another. My tools as it was
    becomes the first set.
  - **Stamps of your own, designed:** New stamp or Edit opens a stamp in a
    tab of its own beside the drawings: its name, up to eight lines each
    with its size and weight, how they are aligned, a rounded, square or
    no border, its color and whether it is filled, with a preview drawn
    exactly as it will be placed. Review's own stamps stay as they are;
    Copy makes one's own from them. Each stamp is a `.ocstamp` file in a
    stamps folder, which can be a shared office folder. A placed stamp
    keeps its design in the PDF and looks the same in other viewers.
  - **Stamp fields:** `{name}`, `{date}`, `{date:dd/MM/yyyy}`, `{time}`,
    `{page}`, `{sheet}`, `{file}` are filled in when a stamp is placed;
    `{ask:Job number}` is answered under the stamp list before placing.
    A stamp clicked in place takes its own size.
  - **Faster moving around:** the page is drawn in pieces kept for
    reuse, so dragging or scrolling a drawing draws only what comes into
    view, and the pieces missing are drawn together in one pass. Each
    markup is painted only in its own box, and those out of sight not at
    all. On a real sheet with 60 markups, in a full-screen window at
    125 %, a step of dragging went from about 134 ms to 17 ms, and a step
    of zooming from about 350 ms to 34 ms.
  - **Sheet links:** the Set panel's Link sheet numbers finds every
    mention of another sheet's name in every drawing, such as A-301 or
    4/A-301, and makes it a link to that sheet, as one step to undo;
    the names come from the PDFs' page labels and the set's sheets, and
    A-301 is not found in A-3010.
  - **Every drawing at once:** from the Set panel, open all the set's
    drawings, stamp every page with the stamp in use, give every page
    this page's scale, save all, write flattened copies of all to a
    folder, or write one markup summary of all, with a Sheet column.
  - **Capture:** the Capture tool, second in the tools, takes a part of a drawing, its lines
    and words, not a picture, and puts it beside it to move where it is
    wanted. The Colors tab lists the colors found in that part and
    changes each, such as black to magenta. It is saved as a stamp
    drawn from the page's own content, so it stays sharp and looks the
    same in other viewers.
  - **Undo and redo** of every step, typing included.
  - **Saving:** one incremental update after the original bytes. Every
    markup is a standard annotation that other PDF viewers show, and
    measurements carry a standard scale. Markups other programs made are
    shown and edited; one left alone is kept exactly as it was, one moved
    keeps its own look, and keys Review does not know are never dropped.
  - **Markup list** to CSV with page, type, subject, status, author, date,
    text, measurement and replies.
  - Not yet: review sessions, comparing revisions, text search, OCR and
    sheet sets.
- **Joinery:** construction standards can join the case with dowels,
  confirmat screws or both (`joinery` rules): the sides to the bottom, the
  top and the stretchers. Dowels stand in a row on the 32 mm grid,
  centered on each joint; their holes are bored into the sides' faces and
  the other parts' edges, so they always meet. The parts' DXF draws holes
  in edges on layers of their own, the bill of materials counts dowels and
  screws, and the 3D view shows them, solid in X-ray and between the parts
  when exploded. The sample standards and new projects use 8 × 35 mm
  dowels. The standard's page edits them under **Case joinery**: what
  holds each joint, the dowel's and the screw's sizes and their holes. A
  joint set to screws in a standard without their rules gets the usual
  ones, and the library a screw if it has none.
- **A window built around the views:** the views take most of the window,
  laid out as one, two side by side, two stacked or four (**Views**). Each
  shows the project in 3D, the room's plan, an elevation or the edited
  cabinet, chosen in its own header; each 3D view has its own camera and
  display. The window opens on the plan, so starting is as quick and light
  as before; 3D starts when a view shows it.
- **One panel that follows what you do:** it opens on the project, to add a
  cabinet, choose one, or open the rooms and walls, the materials or the
  construction standard. Clicking a cabinet in a 3D view, the plan or an
  elevation opens its options; clicking a wall in the plan opens the walls;
  clicking empty space goes back to the project. **Panel** hides it.
- **Place a new cabinet with a click:** after **Add cabinet**, a click on a
  wall in the plan or the 3D view places the cabinet there, centered on the
  click.
- **Reports on demand:** the cut list and bill of materials open with
  **Reports**; the status bar says how many warnings they hold.
- **3D view:** the **3D** button shows the room in 3D: its floor and walls
  with their windows and doors, and every placed cabinet built part by part,
  with wood grain that follows each part, plywood edges, melamine colors,
  edge bands, holes, grooves, and hinges, slides and shelf pins as simple
  shapes. Drag to turn, right-drag to slide, scroll to zoom. Double-click a
  cabinet to see it alone and edit it; click a part to read its name,
  material, size, edges and machining. **Display** draws it textured,
  shaded, in X-ray or as edges; **Fronts** hides doors and drawer fronts to
  see inside; **Explode** pulls the parts apart. Cabinets that cannot be
  built yet show as see-through boxes. Without a usable graphics card the
  view is drawn on the processor, slower; everything else is unchanged
  (ADR-0016).
- **Where every part goes:** the evaluator places each part in its cabinet
  (RFC-0001 §9), and a test checks that no two parts are in the same place
  except where a joint puts one into the other.
- **Light and dark themes:** **View → Theme** chooses light, dark or the
  same as the system, which is the usual choice. The program remembers it in
  the user's folder, beside the shop template, and exported PDF and DXF files
  do not change. Errors and warnings now have colors that are easy to read on
  either background.
- **Several rooms:** the Walls tab adds, renames, chooses and deletes the
  project's rooms, each with its own walls. Cabinets still belong to the
  project and can be placed in any room; deleting a room that holds walls
  or placed cabinets asks first, and keeps the cabinets.
- **Plan view:** a view shows the chosen room from above: walls at their thickness, cut by doors
  and pass-throughs, with a line for each window; every placed cabinet's
  footprint with its label, dashed when it hangs off the floor; and each
  wall's name and length outside the room. The edited wall and the edited
  cabinet are outlined. The plan is on screen only for now.

### Changed

- Elevations are named after their room and wall, as "Kitchen - North", on
  each page of the PDF and in each DXF file's name, and placement warnings
  say which room when the project has more than one.
- **Hinges on the 32 mm grid:** in a frameless cabinet, each hinge moves
  from 3" from the door's end to the nearest place where its plate's two
  holes land on the side's 32 mm grid, so the plate uses the shelf pin
  holes instead of running into them; a hole they share is bored once.

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
