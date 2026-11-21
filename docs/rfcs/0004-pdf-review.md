<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# RFC 0004 — PDF review and live review sessions

- **Status:** draft
- **Author(s):** Andrés (@A-newGP)
- **Created:** 2026-11-21
- **Comment period ends:** 2026-12-05
- **Discussion:** <https://github.com/OpenCabinet/opencabinet/pull/59>

## Summary

A new official module, **OpenCabinet Review**, opens PDF drawings, marks
them up, measures them to scale and stamps them, and saves the markups
into the PDF as standard annotations that any PDF viewer shows. Several
people can mark up the same drawings at once in a **review session**:
everyone sees everyone's markups as they are made, and a log records who
changed what and when. Sessions are **self-hosted**: the first person can
host one from the desktop app, or a shop can run the same session server
on a computer of its own. Nothing goes through a service we run.

## Motivation

Millwork lives on drawings. A shop receives the architect's PDF set,
marks questions and changes on it, sends shop drawings back, gets them
returned with comments, and repeats until approval. Today that loop needs
proprietary review software, usually Windows-only, sold per seat, and
whose live collaboration runs on the vendor's cloud. Small shops either
pay for it, print and scan, or email PDFs back and forth and lose track of
who said what.

No open source program covers the loop: open viewers mark up one person
at a time, and none offers live, self-hosted co-review. The suite also
needs this module for itself: the shop drawings it will generate
(phase 3) are reviewed and returned the same way.

If we do nothing, users of the suite keep a proprietary tool for the step
that follows ours, and the shop drawings module has no review loop.

## Guide-level explanation

A drafter opens `Kitchen - A-201.pdf`. Sheets show at full speed, with
thumbnails on the side and text search. They pick the **cloud** tool and
draw around the island, then add a **callout** asking "Confirm 36\"
counter height". Before measuring they **calibrate** the sheet by
clicking two ends of a dimension known to be 12'-0"; length, area and
count measurements then read in feet and inches, and the **markup list**
shows each one with its page, author, status and value, ready to export
as CSV.

They stamp the cover sheet **Approved as noted**, one of the stamps that
come with the program; its date and name fill themselves in. They make
their own stamp, with the shop's logo and a "Job no." field, in the stamp
editor.

Then they click **Start session**. The app shows an invite, a short code
and a link, which they send to the project manager and the installer.
Those two open it in their own copy of the program; it downloads the PDF
once from the host and then every markup appears for all three as it is
drawn, colored by author. The **activity** panel lists every change: who
added, moved, restyled, replied to or deleted each markup, and when. Any
of them can save a copy of the PDF with all markups at any time.

When the drafter closes the app, another participant becomes the host and
the session carries on. A shop that reviews with people outside its
office, or wants sessions that last for weeks, runs **OpenCabinet Review
Server** on an office computer or a rented server instead, with accounts
and roles.

## Reference-level explanation

### 1. Crates

| Crate | Responsibility | License |
| --- | --- | --- |
| `opencabinet-pdf` | Open PDFs, render pages and tiles, read text, read and write annotations with incremental saves; no UI | GPL-3.0-or-later |
| `opencabinet-review` | The module: markup model, tools, measurement, stamps, markup list, undo | GPL-3.0-or-later |
| `opencabinet-session` | The session document (a CRDT), its sync protocol and the activity log; no sockets | GPL-3.0-or-later |
| `opencabinet-session-server` | The network side: the host built into the app, and the standalone server binary | AGPL-3.0-or-later |
| `opencabinet-review-app` | The Review desktop application binary | GPL-3.0-or-later |

**Review is its own program.** It has its own executable and its own
installer, apart from OpenCabinet Designer: a shop that only reviews
drawings installs only Review, and Review releases on its own schedule.
Both are built from this monorepo and share its crates, its trademark
policy and its look.

They follow RFC-0002 §1: the core crates never depend on them, and
`opencabinet-pdf` stays usable by the shop drawings module. The session
protocol is specified in `docs/spec/` under CC BY 4.0, like the file
format, so other programs can join sessions.

The server crate is AGPL because it is a server service (ADR-0002). The
Review app links it to host sessions, and AGPL-3.0 and GPL-3.0 may be
combined (each license's section 13), so the Review app stays GPL-3.0 for
everything except the server code, whose network clause applies when
someone modifies it and offers it over a network.

### 2. Rendering

- **Library:** **hayro** (Apache-2.0 OR MIT), a pure Rust PDF renderer by
  the author of krilla, which we already use (ADR-0015). Pure Rust keeps
  the build simple and needs no separate binary. **PDFium** (BSD-3-Clause)
  is the fallback if the spike in §11 shows hayro too slow or missing
  features on real drawings; the choice becomes an ADR.
- **Speed on modest computers (ADR-0014):** pages are drawn in tiles on
  background threads, cached at the current zoom, and redrawn sharper
  when the user stops zooming. Each thread keeps its own renderer over
  the same bytes, since hayro's caches are not shared between threads.
  The spike sets the budgets: time to show the first page of a large
  sheet set, and time to redraw at a new zoom.
- **Page and markups apart:** the page is drawn without its annotations,
  which hayro allows, and the markups are drawn on top by the module. A
  markup being drawn or moved never redraws the page.
- **Text:** extracted per page for search, copy and snapping to words.

### 3. Markups

A markup is one record:

| Field | Meaning |
| --- | --- |
| `id` | UUIDv7, as elsewhere in the suite (RFC-0001) |
| `page` | Page index, and the hash of the page's content to detect replaced sheets |
| `kind` | One of the kinds below |
| `geometry` | Points in the page's PDF user space (1/72"), so markups stay put at any zoom |
| `style` | Stroke color, width and pattern, fill and opacity, font |
| `content` | Text, subject and label |
| `author`, `created`, `modified` | Who made it and when |
| `status`, `replies` | Review state (open, accepted, rejected, done) and a thread of comments |
| `layer` | Optional grouping, shown and hidden together |

Kinds, each saved as the standard PDF annotation (ISO 32000-2 §12.5):
line and arrow (`Line`), polyline (`PolyLine`), rectangle (`Square`),
ellipse (`Circle`), polygon and **cloud** (`Polygon` with a cloudy border
effect), freehand (`Ink`), text box and **callout** (`FreeText`), note
(`Text`), highlight, underline and strike-through (text markup), image
and **stamp** (`Stamp`), and measurements (below). Fields without a
standard key go into a private dictionary under a key of our own, which
other viewers ignore; every standard key we write stays correct on its
own, so other viewers show the markup.

Saving is an **incremental update**: the original bytes of the PDF stay
untouched and the markups are appended, so a save can always be undone
by cutting the file back, and a signature over the original bytes still
verifies them.
Flattening markups into the page content is a separate, explicit export.

### 4. Measurement

- **Calibration:** two points and a known distance give a scale per page,
  or per region of a page for sheets with several details at different
  scales. Scales are saved as standard measure dictionaries and viewports
  (ISO 32000-2 §12.9), so a calibrated sheet reopens calibrated.
- **Tools:** length, polylength, area with perimeter, count, angle, and
  rectangle and polygon areas with cutouts.
- **Units:** imperial by default and metric as an option (ADR-0008),
  shown with the suite's `Length` and fractions.
- **Snapping:** to the drawing's line ends, midpoints and intersections,
  read from the page's vector content.

### 5. Stamps

A stamp is a template of our own format: vector content plus fields
(date, user, status, custom text) filled in when placed. The program
ships its own stamp designs, licensed like the icons (CC BY-SA 4.0). The
stamp editor makes and edits templates; a placed stamp is saved as a
`Stamp` annotation with its appearance, so it looks the same everywhere.

### 6. Sessions

- **The shared document.** Each session is one Automerge document
  (MIT): markups, replies, statuses, the list of participants and a chat.
  Automerge is a CRDT: every participant keeps a full copy and edits it
  locally, and copies exchanged in any order converge to the same state.
  Two people editing the same markup at once never block each other.
- **The base PDF** never changes during a session. It is identified by its
  SHA-256, sent once to each participant, and checked on arrival.
- **The activity log.** Every change in Automerge carries the author's
  identity and a time. The activity panel reads that history, so the log
  is complete by construction and cannot drift from the markups.
- **Transport.** WebSocket over TLS (RFC 6455) between each participant
  and the host. The host relays changes; it does not need to understand
  them to keep the session going.
- **Host in the app.** Start session runs the host in the desktop app.
  The invite carries the host's address, a secret and the fingerprint of
  its certificate, so participants can trust a self-signed certificate
  without a certificate authority.
- **Host handover.** Since every participant has the whole document,
  when the host leaves, the next participant in join order becomes the
  host, and the others reconnect to it. No markup is lost while
  disconnected: offline edits merge when the connection returns.
- **Server mode.** The same host code runs as a headless binary, with
  persistent sessions, accounts and roles (owner, editor, viewer). It is
  packaged as a single executable and a container image.
- **Across the internet.** A desktop host works within one network: an
  office, or a VPN. Most routers block connections coming in from the
  internet, so for outside participants a shop uses server mode on a
  reachable machine. Connecting through routers without a server (NAT
  traversal and relays) is left for later.

### 7. Identity and security

- **Authorship.** Each change carries an actor id. The host assigns one
  to each participant it admits and refuses changes from any other, so no
  one can write as someone else. In a desktop-hosted session,
  participants type their name and the host vouches for it: authorship is as trustworthy as the
  people the invite was sent to. In server mode, authorship comes from
  accounts. Signing each change with the author's own key is left for
  later.
- **Access.** Joining needs the invite's secret. All traffic is encrypted.
  The host can remove a participant and end the session.
- **The PDF is never executed.** JavaScript, forms that submit, launch
  actions and embedded files in PDFs are ignored. Malformed PDFs must
  fail safely, which a pure Rust renderer helps with.

### 8. Clean-room and names

Commercial review programs inspired the functions above, which are
described here only by what they do. No code, icons, stamps, texts,
templates or documentation of any proprietary program is used
(CONTRIBUTING.md, clean-room policy). The interface and its terms are our
own: "review session", "markup list", "tool sets", "activity". No
competitor's product or feature names appear in the program, its
documentation or its marketing. Before building a feature particular to
one competing program, a contributor checks for patents on it, and the
pull request says what was checked.

### 9. Files

The module works on plain PDFs and needs no change to `.ocab` files.
Tool sets (saved markup styles) and stamp templates are files in the
user's library folder. Later, the shop drawings module links the sheets
it generates to their review state.

### 10. Releases

1. **Review 0.1:** viewer with thumbnails, search and sheet navigation;
   the markup kinds of §3; markup list with CSV export; stamps and the
   stamp editor; calibration and the measurement tools of §4; incremental
   save.
2. **Review 0.2:** desktop-hosted sessions with host handover, the
   activity panel and chat.
3. **Review 0.3:** server mode with accounts and roles.
4. **Later:** comparing two revisions of a sheet, overlays, OCR for
   scanned drawings (Tesseract, Apache-2.0), sheet sets with numbering
   and links, batch stamping, connecting without a server, and signed
   changes.

### 11. Spikes before acceptance

Throwaway prototypes in `prototypes/`, deleted when this RFC is
accepted. Their READMEs hold the measurements.

- **Rendering:** open a large drawing with hayro and measure the time to
  the first page and to redraw at a new zoom, on the reference computer
  of ADR-0014 or a CI runner; compare with PDFium if hayro falls short.
- **Sessions:** two participants editing markups at once through
  Automerge over a local connection, converging, with the activity log
  read from the history.

First results, on a 12-thread computer:

- **Rendering:** a dense 36" × 24" sheet (40,000 lines, 400 labels) fits
  the 1366 × 768 screen in 47 ms; a 512 px tile at 400 % takes about
  10 ms; a 30-sheet, 44 MB set opens in under 1 ms. A markup saved by
  lopdf as an incremental update leaves the original bytes untouched, and
  hayro draws it.
- **Sessions:** concurrent edits to one markup converge on every copy;
  edits to the same field keep both values, so the window can show the
  conflict. The activity log reads author, time and message from the
  history. A new host takes over without losing an offline edit. 5,000
  markups save in 31 kB and a newcomer joins in 37 ms.

Still to measure: rendering on two cores, and real drawings with
compressed content, embedded fonts, images and scans.

## Drawbacks

- **Size.** Mature review programs took many years to build. The module
  competes for time with the designer's 0.1 and the shop drawings
  module.
- **PDFs vary widely.** Real drawings from many CAD programs will expose
  rendering gaps, and each must be fixed or reported upstream.
- **Network code is security code.** Sessions expose a program to other
  computers; mistakes there cost more than elsewhere in the suite.
- **Two runtimes to support.** The desktop app and the server binary.

## Alternatives

- **A web viewer built on pdf.js** (Apache-2.0): mature rendering, but a
  second technology stack beside Rust and Slint (ADR-0011, ADR-0012).
- **MuPDF:** an excellent renderer under AGPL-3.0 or a commercial license.
  It can be combined with GPL-3.0, but would put the whole app under the
  AGPL's network terms. hayro and PDFium avoid that.
- **Only peer to peer, no server:** nothing to install, but no sessions
  across the internet and none that outlive the participants.
- **Only a server:** reliable, but too much to ask of a small shop
  reviewing among three people in one office.
- **Operational transforms with a central server** instead of a CRDT:
  less memory per participant, but offline edits and host handover
  become hard.
- **Yrs (the Rust port of Yjs, MIT)** instead of Automerge: faster on
  some workloads; Automerge's history API maps more directly to the
  activity log. The sessions spike may revisit this.

## Compatibility and migration

Nothing changes in existing project files or the plugin API. Saved PDFs
remain standard PDFs that other viewers open, with the markups showing.
Markups made in other programs, read from standard annotations, can be
viewed and edited, losing only what a program keeps in private keys.

## Unresolved questions

Before acceptance:

- hayro or PDFium: hayro so far, pending two cores and real drawings.
- Automerge or Yrs: Automerge so far; Yrs only if a larger test shows
  Automerge too slow.

During implementation:

- Budgets for large sheet sets (ADR-0014).
- The protocol's versioning, so different versions can share a session.
- Connecting across routers without a server.

## Sources

- ISO 32000-2:2020, *Document management — Portable document format —
  Part 2: PDF 2.0*: §12.5 (annotations), §12.9 (measurement and
  viewports), §7.5.6 (incremental updates).
- Automerge documentation, <https://automerge.org/docs/>.
- hayro, <https://github.com/LaurenzV/hayro>.
- PDFium, <https://pdfium.googlesource.com/pdfium/>.
- RFC 6455, *The WebSocket Protocol*.
- M. Kleppmann et al., *Local-first software* (2019).
