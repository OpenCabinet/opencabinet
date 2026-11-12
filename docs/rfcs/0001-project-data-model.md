<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# RFC 0001 — Project data model

- **Status:** draft
- **Author(s):** Andrés (@A-newGP)
- **Created:** 2026-10-25
- **Comment period ends:** 2026-11-08
- **Discussion:** pull request introducing this RFC; announced in [Discussions #11](https://github.com/OpenCabinet/opencabinet/discussions/11)

## Summary

This RFC defines the conceptual data model shared by every module of
OpenCabinet: what a project contains, how a cabinet is described, how parts
and machining are derived from that description, and the rules that keep
the result exact, reproducible and open. It deliberately does not define the
byte-level file format, which becomes a separate specification (CC-BY-4.0)
once this RFC is accepted.

The central idea: **the file stores design intent; parts, cut lists and
machining are derived from it by a deterministic evaluation.**

## Motivation

The project plan names the data model as the real core of the suite. The
designer, cut list, bill of materials, shop drawings, cut optimization and
CNC output are all views of the same model; if the model is wrong, every
module inherits the error.

Concretely, the model must let a shop:

- change a cabinet's width and get every affected part, edge band and hole
  updated, without editing parts one by one;
- trust that a job already delivered does not change when the shop later
  edits its construction standards;
- build both frameless and face frame cabinets;
- produce a correct cut list for a table saw **and** machining data for a
  CNC router from the same project;
- place cabinets on walls and produce elevations;
- send the project file to someone else and have it open with everything it
  needs, in any conforming software.

If we do nothing, each module invents its own structures, and the open
format promised to users never exists.

### Scope for version 0.1

Agreed with the founder for the first usable release (phase 2):

| In scope for 0.1 | Modeled now, implemented later | Out of scope (future RFCs) |
| --- | --- | --- |
| Frameless and face frame construction | Moldings, countertops | User-defined formulas |
| Base, wall and tall cabinets with doors, drawers, shelves and open sections | Estimating and labor | Free-form 3D geometry |
| Blind corner cabinets | Cut optimization results | Diagonal and L-shaped corner cabinets (non-orthogonal parts) |
| Slab and five-piece doors and drawer fronts | CNC post-processing | Collaboration and merging |
| Walls with openings; cabinets placed on walls; elevations | | Drawing annotations and layouts |
| Cut list, bill of materials, edge banding, hardware | | Pricing catalogs from manufacturers |
| Machining operations (holes, line boring, grooves) | | |

The breadth of this scope is intentional; the implementation should still
deliver it in milestones (see [Drawbacks](#drawbacks)).

## Guide-level explanation

Maria designs a kitchen. Her shop's **construction standard**, "Shop
frameless 2026", says how the shop builds: 3/4" melamine boxes whose sides
run full height and capture the bottom, 1/4" backs set into a groove 3/4"
from the rear edge, 4" toe kick, full-overlay doors with a 1/8" gap, and
32 mm line boring.

1. She creates a **project** and a **room**, "Kitchen", and draws a 144"
   **wall** with a window.
2. She places a **product** of type *base cabinet* on the wall, 0" from its
   left end: 30" wide, 34-1/2" high, 24" deep. The product's **front layout**
   is one opening split into a drawer on top and a pair of doors below.
3. She assigns **materials** by role: *case* is white melamine, *doors* are
   maple veneer plywood, *edge band* is 0.5 mm maple.

That is all the file stores for that cabinet: the type, the size, the front
layout, the material choices and a reference to the standard.

When she opens the cut list, the **evaluator** reads that intent together
with the standard and the libraries, and **derives** the assemblies (case,
drawer, doors), the **parts** (two sides, bottom, back, stretchers, drawer
box parts, two doors…), the edge banding of each edge and the **operations**
on each part (hinge cup holes, line boring, the back groove).

She widens the cabinet to 33". Nothing else needs editing: evaluation runs
again and every part, edge band and hole follows.

On one cabinet she needs a part the standard does not produce, a hole for a
plumbing pipe in the back. She adds it as an **override** attached to that
part. If she later deletes the doors so that part no longer exists, the
override is reported as orphaned, never silently dropped.

Next year the shop moves its back groove to 1/2". Maria updates the shop's
master standard. Her 2026 kitchen does not change, because the project keeps
its own **snapshot** of the standard; the software offers to update it and
shows what would change.

## Reference-level explanation

### 1. Invariants

These rules hold for every implementation and every module:

1. **Intent is the source of truth.** Authored data (projects, products,
   standards, libraries, overrides) is stored. Derived data (assemblies,
   parts, operations, cut lists) is a pure function of authored data plus the
   evaluator version.
2. **Determinism.** The same authored data and evaluator version produce
   identical derived data on every platform.
3. **Exact lengths.** Every length is an integer number of nanometers
   ([ADR-0009](../adr/0009-length-representation.md)). The model never stores
   a display string or a floating-point length.
4. **Stable identity.** Every authored entity has an immutable UUID (version
   7). References always use IDs, never names or positions in a list.
5. **Self-contained projects.** A project embeds a snapshot of every
   standard, material, edge band and hardware item it uses.
6. **No executable content.** Project files contain data only: no scripts,
   macros or code of any kind. Opening a file can never run anything.
7. **Lossless round trip.** Software must preserve data it does not
   understand (unknown fields and foreign extensions) when saving.
8. **Units are a display preference.** The project stores the user's
   preferred unit system and display precision; it never changes stored
   values.

### 2. Coordinate systems

- **Plan (room) coordinates:** right-handed, Z up, origin chosen by the user.
  Walls are drawn in the X-Y plane.
- **Product coordinates:** origin at the **left, back, bottom** corner of
  the product's bounding box as seen from the front. +X runs to the right
  along the width, +Y toward the front along the depth, +Z up along the
  height. The back plane (Y = 0) is the plane that rests against a wall.
- **Part coordinates:** origin at a corner of face A. +X runs along the
  part's **length**, which is the grain direction for grained materials, +Y
  along its **width**, +Z through its **thickness** from face A. Edges are
  named by side: `x0`, `x1`, `y0`, `y1`.

In v0.1 every part is axis-aligned in product coordinates; the only rotation
is a product's rotation about Z when placed in plan.

**Angles** are integers too: microdegrees, counterclockwise seen from above,
so 90° is `90000000`. Every angle a shop uses (90°, 45°, 22.5°, 15°) is
exact, and rotations are as deterministic as lengths (ADR-0009).

### 3. Entity overview

```text
Project
├── settings (units, display precision, default standard)
├── library (snapshots): materials, edge bands, hardware, standards
├── rooms
│   ├── walls (with openings)
│   └── placements ──► products
├── products (cabinets)
│   ├── type + version, dimensions, parameters
│   ├── material assignments by role
│   ├── front layout (split tree of openings with inserts)
│   └── overrides
├── drawings (reserved for the shop drawings RFC)
└── revisions (log)

Derived (never authored):
product ─► assemblies ─► parts ─► operations, edge banding
```

### 4. Project

| Field | Description |
| --- | --- |
| `id` | UUIDv7 |
| `name` | Job name |
| `settings.units` | `imperial` (default) or `metric` |
| `settings.precision` | Display precision, e.g. 1/32" or 0.1 mm |
| `settings.defaultStandard` | Standard applied to new products |
| `library` | Snapshots of everything the project uses (see §8) |
| `rooms`, `products` | See §5 and §6 |
| `revisions` | Ordered log: label (e.g. "A"), date, author, description |
| `extensions` | Namespaced plugin data (see §11) |

The project revision log is all v0.1 needs. Issuing shop drawings needs a
revision per sheet and a record of each issue (what was sent, when, to whom
and why); the shop drawings RFC adds both without changing this log.

Client details are kept to a name and free-text notes; the model does not
define fields for personal data (addresses, phone numbers) in v0.1.

### 5. Rooms, walls and placements

- **Room:** `id`, `name`, `walls`, `placements`.
- **Wall:** `id`, `name`, `start` and `end` points in plan, `height`,
  `thickness`. The wall's front side is to the left when walking from start
  to end. A wall has **openings** (windows, doors, pass-throughs): `id`,
  `kind`, `offset` from the wall's start, `width`, `height`, `sillHeight`.
- **Placement:** puts a product in a room, in one of two ways:
  - **on a wall:** `wall`, `offset` from the wall's start to the product's
    left side, `elevation` of the product's bottom above the floor; the
    product's back plane lies on the wall's front face;
  - **free:** `position` (x, y, z) of the product origin and `rotation`
    about Z in [0°, 360°), for islands and peninsulas.

  Fillers, scribes and end panels are products too, so they appear in the
  cut list and in elevations like any other.

  Several placements may reference the same product: identical cabinets on
  different walls share one definition, and changing it changes all of
  them.
- **Elevation:** a view of one wall and the products placed on it. In v0.1
  elevations are derived, one per wall; their drawing settings belong to the
  future shop drawings RFC.

### 6. Products

A product is one cabinet (or filler, end panel…) as the user designed it.

| Field | Description |
| --- | --- |
| `id` | UUIDv7 |
| `label` | The shop's item number, e.g. "B30" or "12" |
| `type` | Product type ID and version, e.g. `base-cabinet@1` |
| `standard` | Construction standard ID (snapshot in the project library) |
| `width`, `height`, `depth` | Overall dimensions, in nanometers |
| `quantity` | Number of units for a product with no placements (default 1) |
| `parameters` | Typed values the type declares (see below) |
| `materials` | Map from material **role** to library item |
| `front` | Front layout (see below) |
| `overrides` | Changes to derived data (see §10) |
| `extensions` | Namespaced plugin data |

**Quantity.** A product's count in the cut list and bill of materials is the
number of its placements when it has any, and its `quantity` otherwise. The
second form serves repeated or commercial orders that are not laid out on
walls. A product with placements and a `quantity` other than 1 is a
validation error, so the two forms can never disagree.

**Product types** (base, wall, tall, sink base, blind corner, filler, end
panel…) are generators shipped by modules: they declare their parameters and
turn a product into assemblies and parts according to the standard. Types
are versioned; a project records the type version it was designed with, and
a new major version never applies to an existing product without the user's
consent.

A **blind corner** cabinet is an orthogonal box whose front is partly hidden
behind the adjoining cabinet. Its type declares the `blindSide` (left or
right) and the `blindWidth`; the hidden part of the front becomes a fixed
panel or filler and the remaining front is an ordinary split tree. Diagonal
and L-shaped corners need non-orthogonal parts and are left to a future RFC.

**Parameters** are typed: `length`, `count`, `boolean`, `enum`, `material`,
`hardware`. Each has a default from the standard or the type. v0.1 has no
user-defined formulas: a product's behavior comes from its type and its
standard, which keeps evaluation safe and deterministic. A formula language
can be proposed in a future RFC.

The file stores each parameter as a plain JSON value under its name; the
**product type's declaration gives its type**, because a `length` and a
`count` are both JSON integers. Validation checks every value against the
declaration. A parameter the declaration does not know (from a newer type
version, for example) is kept and reported as a warning.

**Material roles** decouple "what part" from "what material": `case`,
`back`, `shelves`, `doors`, `drawerFronts`, `drawerBox`, `drawerBottom`,
`faceFrame`, `edgeBandCase`, `edgeBandFronts`… The standard provides
defaults; the product overrides them.

#### Front layout

The front of a product is a **split tree**. The root is the product's front
opening. Every node has its own UUIDv7 `id`, which never changes when the
tree is edited around it. Each node is either:

- a **split** into children, `horizontal` (stacked top to bottom) or
  `vertical` (side by side), where each child's size is a fixed length or a
  share of the remaining space; or
- an **opening**, a leaf with one **insert**: `doors` (single, pair, with
  hinge side), `drawer`, `falseFront`, `open` (with shelves), `appliance`
  (an empty space with a declared size) or `none`.

The same tree serves both construction methods; only its physical meaning
changes:

| Tree element | Frameless | Face frame |
| --- | --- | --- |
| Root opening | Inside of the case | Inside of the face frame |
| Split between nodes | Gap or fixed shelf / stretcher | Rail or mullion |
| Opening size | Space the insert overlays | Frame opening the insert fits or overlays |

The standard defines how each insert relates to its opening (overlay,
inset, reveals, gaps), so a door's size is always derived, never typed in.

**Door and drawer front styles.** Doors, drawer fronts and false fronts have
a `style`:

- `slab`: a single part;
- `fivePiece`: a sub-assembly of two stiles, two rails and a panel. Frame
  widths, joinery (cope-and-stick or mortise-and-tenon, which changes the
  rails' cut length), panel type (flat or raised), panel thickness role and
  groove depth come from the standard; the insert may override the frame
  widths. Each frame member and the panel are parts of their own, so they
  appear in the cut list with their grain running along their length.

**Hardware placement.** The standard positions hardware automatically:
number of hinges by door height and their distance from the ends, slide
type and length by cabinet depth, pull location and orientation by insert.
An insert may **adjust** that result, for example a third hinge, a hinge
moved to clear a shelf, or a pull centered instead of at the top. An
adjustment is part of the authored front layout, so it survives
re-evaluation; the derived machining (hinge cups, slide holes, pull holes)
follows the adjusted positions.

### 7. Construction standards

A standard is a structured, versioned set of construction rules. It is data,
not code. v0.1 groups its rules as follows:

| Group | Examples |
| --- | --- |
| Method | `frameless` or `faceFrame` |
| Case | Which panels capture which (sides capture bottom, or bottom under sides); top: full top or stretchers, their widths |
| Back | Applied, rabbeted or grooved; thickness role; groove depth and setback |
| Toe kick | Integral or separate base; height; recess |
| Face frame | Stile and rail widths, overhang beyond case, scribe allowance, joinery |
| Fronts | Overlay type (full, partial, inset), overlay amount, gaps and reveals; five-piece frame widths, joinery, panel type and groove depth |
| Hardware | Hinge count by door height and end distances; slide type and length by depth; pull location by insert |
| Drawers | Box construction, side thickness role, clearances per slide type, bottom capture |
| Shelves | Fixed or adjustable, front setback, clearance |
| System holes | Line boring: first hole, spacing (e.g. 32 mm), setback from front and back |
| Edge banding | Which edges of which part role receive which band role |
| Cutting | Oversize for parts banded later or trimmed after glue-up |
| Machining | Hinge cup diameter, depth and setback; connector and dowel patterns |

Every standard has an `id`, a `name`, a `version` and a `derivedFrom` link
when it is a copy of another. The shop keeps master standards outside any
project; a project keeps snapshots (§8).

### 8. Libraries and snapshots

Library items are reusable definitions:

- **Sheet material:** `name`, `nominalThickness`, `actualThickness`, sheet
  `length` and `width`, `grain` (`none` or `length`), whether each face is a
  show face, optional `cost` per sheet. Nominal and actual thickness are
  distinct fields because "3/4"" plywood is about 0.709" (18 mm); **all
  derived dimensions use the actual thickness**, and reports may show the
  nominal one.
- **Solid stock:** species, nominal and actual thickness, how it is sold
  (board foot, linear foot), optional cost.
- **Edge band:** `thickness`, `width`, material and color, whether it is
  pre-milled, optional cost per length.
- **Hardware:** `category` (hinge, slide, pull, leveler, shelf pin,
  connector), manufacturer and model as free text entered by the user, the
  dimensions the evaluator needs (slide length and required clearance, hinge
  cup diameter and boring distance) and an optional machining pattern.
- **Standards** (§7).

The project embeds a snapshot of each item it uses. Snapshots carry the
master's `id` and `version`, so the software can detect that the shop's
master changed and **offer** an update, showing the resulting differences.
An update is always explicit. The project ships no third-party catalog data;
libraries contain what users create or import with permission (see the
legal section of the project plan).

### 9. Derived data

Evaluation is a pure function:

```text
derive(product, standard, library, typeVersion, evaluatorVersion)
    → assemblies, parts, operations
```

- **Assembly:** a named group (case, drawer, door, face frame) with a key
  and child parts.
- **Part:**
  - `key`, a deterministic path that identifies the part across
    re-evaluations, built from assembly roles and front node IDs, e.g.
    `case/side.left` or `front/<node id>/drawer/front`. Because node IDs
    are stable, restructuring the front elsewhere does not change the keys
    of untouched parts;
  - `role` (left side, bottom, back, stretcher, door, drawer front,
    face frame stile…), used for material and edge band rules;
  - `material` and `grain` orientation;
  - **finished size** (length × width × thickness) and **cut size**
    (finished size plus the standard's oversize allowances, minus banding
    thickness where the band is applied after cutting);
  - **edge banding** for each edge `x0`, `x1`, `y0`, `y1`;
  - its **transform** in product coordinates (for elevations, 3D and
    collision checks);
  - **operations**.
- **Anchors:** every face, edge and operation of a part has a stable key
  derived from the part key, for example `case/side.left#face.A`,
  `#edge.y1` or `#hole/<id>`. Drawing annotations and overrides attach to
  anchors, so a dimension stays on the same feature when the model is
  re-evaluated. An annotation whose anchor no longer exists is kept and
  reported as orphaned, like an override (§10). The exact syntax belongs to
  the format specification.
- **Operation:** a machine-independent description in part coordinates,
  on a face (`A`, `B`) or an edge:
  - `hole`: position, diameter, depth;
  - `holePattern`: a repeated hole (line boring) with start, spacing and
    count;
  - `groove`: start, end, width, depth (dados and back grooves);
  - `pocket` and `profile`: reserved for later versions.

  Operations never contain G-code; CNC post-processors translate them for a
  given machine, which keeps the model useful for manual shops too (a hole
  list is a drilling guide).

The **cut list** and **bill of materials** are aggregations of parts,
edge banding and hardware; they are reports, not stored entities.

Files may include a **cache** of derived data, tagged with the evaluator
version and a hash of the authored inputs. The cache lets simple software
(for example a cut list viewer) read results without implementing the
evaluator. Conforming editors recompute on open; if the result differs from
the cache, they warn the user, because it means an evaluator change would
alter a job.

### 10. Overrides

An override changes derived data for one product without changing its type
or standard:

| Field | Description |
| --- | --- |
| `id` | UUIDv7 |
| `target` | Part or assembly `key` |
| `action` | `set` a property (material, edge band, size), `addOperation`, `removeOperation`, `exclude` the part, or `addPart` |
| `value` | The new value or the added element |
| `note` | Why the change was made, shown in reports |

Overrides apply after evaluation, in order. If an override's target no
longer exists after re-evaluation, the override is kept and reported as
**orphaned**; it is never discarded silently.

### 11. Extensions

Any entity may carry an `extensions` object keyed by a reverse-domain
namespace owned by a plugin, e.g. `org.example.cnc-nesting`. Software
preserves extensions it does not understand (invariant 7). Extensions cannot
change how core data is evaluated; a plugin that needs to change derived data
does so through the plugin API (RFC-0002), not through hidden file data.

### 12. Validation

Validation produces **errors** (the product cannot be evaluated: a reference
does not resolve, a dimension is zero or negative, the front layout does not
fit) and **warnings** (an override is orphaned, a drawer is deeper than its
slide allows, a part exceeds its sheet size). A project may be saved with
errors, as work in progress, but reports and machining output are blocked
for products with errors.

### 13. Serialization (informative)

The normative format comes in a separate specification. The intended shape:

- file extension **`.ocab`** (OpenCabinet project) and media type
  **`application/vnd.opencabinet.project+zip`**, to be registered with IANA
  in the vendor tree once the specification is published;
- a ZIP container, with a first, uncompressed `mimetype` entry holding that
  media type, as in OpenDocument and EPUB, so the file type is identifiable
  without trusting the extension;
- `manifest.json` with the `formatVersion` (major.minor), the generator and
  the list of entries;
- JSON documents for the project, the library snapshots and the optional
  derived cache; attachments (images, PDFs) as separate entries;
- lengths as JSON integers in nanometers;
- an equivalent **unpacked directory** form with the same layout and the
  same `.ocab` suffix (for example `kitchen.ocab/`), so projects can be kept
  in version control and diffed;
- a reserved top-level `drawings` collection, so the shop drawings RFC can
  add sheets, views and templates in a minor version.

**Versions.** A reader rejects a newer major `formatVersion`, reads newer
minor versions preserving unknown fields, and migrates older versions
forward with documented, tested migrations. When it saves a file read from
a newer minor version, it **keeps that newer version number**; otherwise a
newer reader would take the preserved fields for something an older writer
produced.

**Open and closed enumerations.** The specification marks each enumeration
as open or closed. Open ones may gain values in a minor version: insert
kinds, front styles, override actions, wall opening kinds, hardware
categories and operation kinds. Closed ones change only in a major version:
units, split direction, grain, construction method and sides. A reader
keeps an unknown value of an open enumeration and reports the product that
uses it as an error, so it is neither dropped nor misread, and reports are
blocked for it (§12).

**Strict writers, lenient readers.** Writers always put `mimetype` first and
uncompressed and list every entry in the manifest. Readers accept a file
that breaks those rules, as happens when an operating system tool re-zips a
project, and warn. The manifest's entry list gives each attachment its media
type; an entry missing from it is kept as `application/octet-stream`, with
a warning.

**Safe to open.** Readers reject entry names that could point outside the
project (absolute paths, `..`, drive letters, backslashes), links in the
directory form, and contents beyond published size limits.

**The directory form under version control.** Saving over a directory
rewrites the entries the software owns and removes the ones it listed
before and no longer writes; it never deletes files the manifest does not
list. JSON entries use LF line endings, and repositories should mark project
directories as binary for Git (`*.ocab/** -text`) so no tool rewrites them.

Illustrative fragment (not normative):

```json
{
  "id": "0192f3a4-7c1e-7b2a-9f00-3d5e8a1b2c4d",
  "label": "B30",
  "type": "base-cabinet@1",
  "standard": "0192f3a4-0000-7000-8000-000000000001",
  "width": 762000000,
  "height": 876300000,
  "depth": 609600000,
  "materials": { "case": "0192f3a4-0000-7000-8000-0000000000a1" },
  "front": {
    "id": "0192f3a4-7c1e-7b2a-9f00-3d5e8a1b2c50",
    "split": "horizontal",
    "children": [
      {
        "id": "0192f3a4-7c1e-7b2a-9f00-3d5e8a1b2c51",
        "size": { "length": 152400000 },
        "insert": { "kind": "drawer", "style": "slab" }
      },
      {
        "id": "0192f3a4-7c1e-7b2a-9f00-3d5e8a1b2c52",
        "size": { "share": 1 },
        "insert": { "kind": "doors", "count": 2, "style": "fivePiece" }
      }
    ]
  }
}
```

## Drawbacks

- **Scope.** Two construction methods, machining and room layout in 0.1 make
  the first release much larger than a cut list tool. Mitigation: the model
  covers everything now, and the implementation ships in milestones (single
  frameless cabinet with cut list → face frame → machining → walls and
  elevations), each tested against reference projects.
- **Snapshots duplicate data** in every project. Library items are small; the
  safety of unchanged jobs is worth more than the bytes.
- **Derived part keys disappear** when the user deletes the front node or
  insert they belong to, which orphans overrides on those parts. Stable node
  IDs limit this to genuine deletions, and orphan reporting makes it visible
  rather than silent; key stability gets tests of its own.
- **No user formulas in 0.1.** Shops with unusual products depend on product
  types shipped by modules or plugins until a formula RFC lands.

## Alternatives

- **Store only parts (a flat cut list).** Simple and common in cut list
  tools, but every design change means editing parts by hand, and nothing
  connects parts to the cabinet they came from.
- **Store full solid geometry (B-rep) as the model.** This is the general
  CAD approach. It is heavy, knows nothing about millwork (roles, banding,
  grain, standards) and makes the open format hard to implement.
- **Scripts in the file** (each product as a small program). Maximally
  flexible, but it violates the security principle: opening a file must
  never execute code.
- **Live references to the shop's standards** instead of snapshots. Less
  data, but delivered jobs change silently when standards change, which is
  exactly the failure this model exists to prevent.

## Compatibility and migration

This is the first version of the model, so there is nothing to migrate.
From the first published format version on, every change follows the rules
in §13, and changes to this model require a new RFC.

## Unresolved questions

All questions raised while drafting were settled by the founder on
2026-10-25:

| Question | Decision | Section |
| --- | --- | --- |
| Door styles in 0.1 | Slab and five-piece, as sub-assemblies | §6 |
| Part key scheme | Front nodes carry their own UUIDs; keys use them | §6, §9 |
| Hardware placement | Positioned by the standard, adjustable per insert | §6, §7 |
| Corner cabinets in 0.1 | Blind corners only | §6 |
| Quantities | Placements count units; unplaced products carry a quantity | §6 |
| File extension and media type | `.ocab`, `application/vnd.opencabinet.project+zip` | §13 |

Proposed during the comment period, after the format prototype
([#10](https://github.com/OpenCabinet/opencabinet/pull/10)) and the review
for shop drawings, and open for comment until the period ends:

| Question | Proposal | Section |
| --- | --- | --- |
| How angles are stored | Integer microdegrees, counterclockwise | §2, §5 |
| How parameter values are typed | By the product type's declaration | §6 |
| How annotations stay attached | Stable anchor keys inside parts, with orphan reporting | §9 |
| Where drawings will live | A reserved `drawings` collection | §3, §13 |
| Revisions for issued drawings | Per sheet, added by the shop drawings RFC | §4 |
| Saving a file from a newer minor version | Keep the newer version number | §13 |
| Unknown enumeration values | Open and closed enumerations; keep unknown values, block the product | §13 |
| Files re-zipped by other tools | Strict writers, lenient readers | §13 |
| Hostile files | Safe entry names, no links, size limits | §13 |
| Saving over the directory form | Owned and listed entries only; LF; binary for Git | §13 |

To settle during implementation:

1. The exact rounding and remainder distribution rules for splits (bounded
   by ADR-0009).
2. The full list of material and edge band roles.

## Sources

- Project plan, sections C and G (Spanish): [docs/es/plan.md](../es/plan.md).
- OASIS Open Document Format for Office Applications (OpenDocument) v1.3,
  Part 2: Packages (ZIP container with `mimetype` entry).
- RFC 9562, "Universally Unique IDentifiers (UUIDs)" (UUID version 7).
- RFC 8259, "The JavaScript Object Notation (JSON) Data Interchange Format".
- The 32 mm system for frameless cabinet construction, as described in
  general woodworking literature.
