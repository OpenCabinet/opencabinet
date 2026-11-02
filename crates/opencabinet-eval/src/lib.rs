// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The evaluator: turns products into parts by the rules of their
//! construction standard (RFC-0001 §9).
//!
//! Evaluation is a pure function of the authored data: the same project
//! always gives the same parts, to the nanometer, on every computer.
//!
//! # What this version evaluates
//!
//! Frameless and face frame base, wall and tall cabinets (`base-cabinet@1`,
//! `wall-cabinet@1`, `tall-cabinet@1`) with slab or five-piece doors,
//! drawer fronts and false fronts, drawer boxes and adjustable shelves:
//! full overlay when frameless, partial overlay or inset with a face frame.
//! Notation:
//! `W`, `H`, `D` the product's width, height and depth; `T` the actual
//! thickness of the `case` material; `K` the toe kick height, none for a
//! wall cabinet; `S` the stretcher width; `G` the front gap; `g` the back
//! groove depth. The box height is `Hb = H − K`.
//!
//! | Part | Length × width | Banded edges |
//! | --- | --- | --- |
//! | `case/side.left`, `case/side.right` | `Hb` × `D` | front (`y1`) |
//! | `case/bottom` | `W − 2T` × `D` | front (`y1`) |
//! | `case/stretcher.front`, base only | `W − 2T` × `S` | front (`y1`) |
//! | `case/stretcher.back`, base only | `W − 2T` × `S` | none |
//! | `case/top`, wall and tall | `W − 2T` × `D` | front (`y1`) |
//! | `case/back`, grooved, base | `Hb − T + g` × `W − 2T + 2g` | none |
//! | `case/back`, grooved, wall and tall | `Hb − 2T + 2g` × `W − 2T + 2g` | none |
//! | `case/back`, applied | `Hb` × `W` | none |
//!
//! Fronts cover the box face, `W` × `Hb`, divided by the front tree. Each
//! front is its opening less `G`: half on each side, so neighbors are `G`
//! apart. A pair of doors splits its opening in two first. Fronts are
//! banded on all four edges; doors run their grain up, drawer and false
//! fronts across.
//!
//! A **five-piece** front (the standard's `fivePiece` rules; the
//! `frontFrames` and `frontPanels` materials, or the doors') is two stiles
//! `s` wide the full height of the front, two rails `r` wide between them
//! that reach `j` into each stile, and a panel in grooves `p` deep, `c`
//! smaller than its grooves so it can move. For a front `F` wide and `E`
//! high: rails are `F − 2s + 2j` long, the panel `E − 2r + 2p − c` by
//! `F − 2s + 2p − c`. Frames are not banded.
//!
//! A **drawer box** (from the standard's `drawers` rules, the `drawerBox`
//! and `drawerBottom` materials, `t` their thickness) is as deep as the
//! longest stocked slide `L` that fits in `D` less the rear clearance; as
//! high as its opening less the vertical clearance `v`; and as wide as the
//! inside less the slide clearance `c` on each side. The sides run the full
//! depth and hold the front and back; the bottom sits in grooves `b` deep:
//!
//! | Part | Length × width |
//! | --- | --- |
//! | `…/drawer/box.side.left`, `.right` | `L` × `opening − v` |
//! | `…/drawer/box.front`, `.back` | `W − 2T − 2c − 2t` × `opening − v` |
//! | `…/drawer/box.bottom` | `W − 2T − 2c − 2t + 2b` × `L − 2t + 2b` |
//!
//! **Shelves** of an open section or behind doors (the `shelves` rules,
//! the `shelves` material, or the case's) span the inside: `W − 2T` less
//! the side clearance, by `D` less the front setback and the back
//! allowance, banded in front like the case.
//!
//! A **face frame** (the standard's `faceFrame` rules, the `faceFrame`
//! material, `F` its thickness) covers the box face, `W` × `Hb`, with
//! butt joints: two stiles `s` wide the full height, top and bottom rails
//! `rt` and `rb` wide between them, and for each split of the front a rail
//! (`r` wide) or a mullion (`m` wide) between its openings, as long as the
//! opening it divides. The frame is part of the depth and stands out past
//! each side of the case by the overhang `o`, so the case is `W − 2o` wide
//! and `D − F` deep, and its front edges are not banded. The top of the
//! bottom is flush with the top of the bottom rail: the bottom is raised
//! `rb − T`, which shortens a grooved back by as much. Every frameless
//! formula above holds with these widths and depth.
//!
//! The front tree divides the frame's opening, `W − 2s` × `Hb − rt − rb`,
//! less its rails and mullions; a fixed size in the tree is the size of an
//! opening. A partial overlay front is its opening plus the overlay amount
//! on each edge; an inset front is its opening less `G` on each edge. The
//! doors of a pair are `G` apart. Overlays must stay on the frame and
//! clear each other by `G` over a rail or mullion. A drawer box is as wide
//! as its frame opening less the slide clearances, rather than the inside
//! of the case.
//!
//! A **blind corner** (`blind-corner-base@1`, `blind-corner-wall@1`) is
//! a base or wall box whose front is hidden on its `blindSide` by the
//! cabinet on the adjoining wall, `blindWidth` `B` from that side. Nothing
//! that opens reaches into `B`: frameless, the doors' zone is the rest of
//! the face; with a face frame, a mullion `m` wide stands inside `B` so
//! that a door overlaying it by `o` ends at `B` exactly (for an inset door,
//! `o` is `−G`). The front tree divides the doors' zone, whose single doors
//! hinge away from the blind side unless they say otherwise. The blind
//! part's opening takes `blindPanel`, a fixed slab of the `blindPanel`
//! material, or the doors', sized and banded like a door. Shelves span the
//! whole inside; drawers are an error, with no side next to the blind
//! part to mount their slides on.
//!
//! The cut size of a part is its finished size less the thickness of the
//! bands on its ends (length) and sides (width).
//!
//! # Machining
//!
//! Each part carries its [`Operation`]s in its own coordinates: `x` along
//! its length from the `x0` end, `y` across its width from the `y0` edge,
//! on face `A` (toward the inside of the cabinet) or `B`. Sides run `x` up
//! from their bottom end and `y` forward from their rear edge; bottoms,
//! tops and shelves run `x` left to right and `y` forward; doors run `x` up
//! and `y` from their left edge as seen from the front; drawer box parts
//! run `y` up from their bottom edge.
//!
//! - **Back groove** (the `back` rules' `setback` `e`): as wide as the
//!   back is thick, `b`, centered `e + b/2` from the rear edge, the full
//!   length of the sides, the bottom and any top.
//! - **Line boring** (the `systemHoles` rules): in both sides, for each
//!   opening with shelves, a front column `f` from the case's front edge
//!   and a rear one `r` in front of the back, on a grid every `p` from the
//!   top of the bottom, clear of the opening's ends by `c`. Shelves must
//!   stop short of the back.
//! - **Hinges** (the `hinges` rules and the hinge's `cupDiameter` and
//!   `cupDepth`): as many as the table gives for the door's height, the
//!   first and last `e` from its ends and the rest evenly between, their
//!   cups' edge `k` from the hinge edge, in the door or its hinge stile. A
//!   pair hinges on its outer edges; a single door on its `hingeSide`. In
//!   a frameless cabinet, two holes on the front column of the side, `p`
//!   apart around each cup, take the hinge's plate; face frame hinges screw
//!   to the frame.
//! - **Drawer bottom groove** (the `drawers` rules'
//!   `bottomGrooveSetback`): as wide as the bottom is thick, the full
//!   length of the box's sides, front and back.
//!
//! Without a rule that only places machining, the parts are still given
//! and the evaluator notes what it could not machine.
//!
//! Anything else, such as other product types, other face frame joinery
//! and overrides, makes the product an error rather than giving parts that
//! would be wrong.

pub mod elevation;
pub mod front;
pub mod machining;
pub mod rules;
pub mod view;

use std::cell::RefCell;
use std::collections::HashMap;

use opencabinet_model::{
    EdgeBand, FrontStyle, Hardware, Id, Insert, InsertKind, Length, Library, Method, Product,
    Project, SheetMaterial, Side, SplitDirection,
};

pub use crate::elevation::{Dimension, Elevation, PlacedCabinet, elevation, placement_problems};
use crate::front::{Divider, Dividers, Layout, Opening, Rect, lay_out};
pub use crate::machining::{Face, Operation, Point};
use crate::machining::{groove_along, hinge_positions, holes_between};
use crate::rules::{Back, FaceFrame, Fronts, Overlay, Rules};
pub use crate::view::{FrontKind, FrontShape, FrontView, front_openings, front_view};

/// The four edges of a part: `x0` and `x1` are the ends of its length,
/// `y0` and `y1` the sides of its width.
pub type Edges = [Option<Id>; 4];

const Y1: usize = 3;

/// A part of a product, with everything a cut list needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// The product it belongs to.
    pub product: Id,
    /// A key that identifies the part across evaluations, such as
    /// `case/side.left` or `front/<node id>/door.1`.
    pub key: String,
    /// What the part is, such as `side` or `door`.
    pub role: &'static str,
    /// The sheet material it is cut from.
    pub material: Id,
    /// Along the grain.
    pub length: Length,
    /// Across the grain.
    pub width: Length,
    /// The material's actual thickness.
    pub thickness: Length,
    /// The length to cut, before banding.
    pub cut_length: Length,
    /// The width to cut, before banding.
    pub cut_width: Length,
    /// The edge band on each edge, if any.
    pub edges: Edges,
    /// How many of this part the job needs.
    pub quantity: u32,
    /// What is machined into it, in its own coordinates.
    pub operations: Vec<Operation>,
}

/// Something about a product that the user should know.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The product, by its label.
    pub product: String,
    /// What is wrong.
    pub message: String,
    /// Whether the product could not be evaluated at all.
    pub blocking: bool,
}

/// Hardware the evaluator places, and how many the job needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HardwareUse {
    /// The product it goes in.
    pub product: Id,
    /// What it is.
    pub kind: HardwareKind,
    /// How many, for every copy of the product the job needs.
    pub quantity: u32,
}

/// A kind of hardware the evaluator places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HardwareKind {
    /// A hinge from the library, with its mounting plate.
    Hinge(Id),
    /// A pair of drawer slides this long.
    SlidePair(Length),
    /// A pin an adjustable shelf rests on.
    ShelfPin,
}

/// The result of evaluating a project.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Evaluation {
    /// Every part of every product that could be evaluated, in product
    /// order.
    pub parts: Vec<Part>,
    /// The hardware placed in them, in product order.
    pub hardware: Vec<HardwareUse>,
    /// Products that could not be evaluated, and things worth knowing.
    pub problems: Vec<Problem>,
}

/// Evaluates every product of a project.
#[must_use]
pub fn evaluate(project: &Project, library: &Library) -> Evaluation {
    let mut placements: HashMap<Id, u32> = HashMap::new();
    for placement in project.rooms.iter().flat_map(|r| &r.placements) {
        *placements.entry(placement.product).or_default() += 1;
    }
    let mut evaluation = Evaluation::default();
    for product in &project.products {
        // Placed products count their placements; others their quantity.
        let quantity = placements
            .get(&product.id)
            .copied()
            .unwrap_or_else(|| product.quantity.unwrap_or(1));
        let problem = |message, blocking| Problem {
            product: product.label.clone(),
            message,
            blocking,
        };
        match evaluate_product(product, library, quantity) {
            Ok(evaluated) => {
                evaluation.parts.extend(evaluated.parts);
                evaluation.hardware.extend(evaluated.hardware);
                let notes = evaluated.notes.into_iter().map(|n| problem(n, false));
                evaluation.problems.extend(notes);
            }
            Err(message) => evaluation.problems.push(problem(message, true)),
        }
    }
    evaluation
}

/// The parts of one product, and what is worth knowing about them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Evaluated {
    /// The product's parts.
    pub parts: Vec<Part>,
    /// The hardware placed in it, counted for every copy.
    pub hardware: Vec<HardwareUse>,
    /// Things the user should know that did not stop the evaluation, such
    /// as edges left bare.
    pub notes: Vec<String>,
}

/// Evaluates one product, of which the job needs `quantity`.
///
/// # Errors
/// A message for the user when the product cannot be evaluated.
pub fn evaluate_product(
    product: &Product,
    library: &Library,
    quantity: u32,
) -> Result<Evaluated, String> {
    let (kind, _) = Kind::of(&product.product_type).ok_or_else(|| {
        format!(
            "product type `{}` cannot be evaluated yet",
            product.product_type
        )
    })?;
    let blind = Blind::of(product)?;
    if !product.overrides.is_empty() {
        return Err("overrides cannot be applied yet".into());
    }
    let standard = library
        .standards
        .iter()
        .find(|s| s.id == product.standard)
        .ok_or("the standard is not in the library")?;
    let rules = Rules::of(standard)?;
    let face_frame = face_frame(standard.method, &rules)?;

    let lookup = Lookup {
        product,
        library,
        notes: RefCell::new(Vec::new()),
    };
    let case = lookup.material("case")?;
    let back = lookup.material("back")?;
    // A face frame covers the front edges of the case.
    let (frame_material, case_band) = match face_frame {
        Some(_) => (Some(lookup.material("faceFrame")?), None),
        None => (None, lookup.band("edgeBandCase")),
    };

    let (w, h, d) = (product.width, product.height, product.depth);
    let t = case.actual_thickness;
    // Wall cabinets hang; base and tall ones stand on a toe kick.
    let toe_kick = match kind {
        Kind::Wall => Length::ZERO,
        Kind::Base | Kind::Tall => rules.toe_kick.height,
    };
    let box_height = h - toe_kick;
    // The frame stands out past the case's sides and is part of its depth.
    let overhang = face_frame.map_or(Length::ZERO, |f| f.overhang);
    let frame_thickness = frame_material.map_or(Length::ZERO, |m| m.actual_thickness);
    let case_width = w - overhang * 2;
    let case_depth = d - frame_thickness;
    let inner_width = case_width - t * 2;
    if box_height <= t * 2 || inner_width <= Length::ZERO || case_depth <= Length::ZERO {
        return Err("the cabinet is too small for its toe kick and sides".into());
    }
    // The top of the bottom is flush with the top of the frame's bottom rail.
    let bottom_raise = face_frame.map_or(Length::ZERO, |f| f.bottom_rail() - t);
    if bottom_raise < Length::ZERO {
        return Err(
            "the face frame's bottom rail is narrower than the case bottom is thick".into(),
        );
    }

    let maker = PartMaker { product, quantity };
    let front_edge = banded(case_band, &[Y1]);
    let size = CaseSize {
        box_height,
        inner_width,
        width: case_width,
        depth: case_depth,
        bottom_raise,
    };
    let mut parts = case_parts(kind, size, &rules, &maker, (case, back), front_edge);
    let back_front = back_front(&rules.back, back, &lookup);

    let face = Rect {
        x: Length::ZERO,
        y: toe_kick,
        width: w,
        height: box_height,
    };
    let plan = lay_out_front(product, face_frame, face, &rules.fronts, blind)?;
    let (layout, fit) = (plan.layout, plan.fit);
    if let (Some(frame), Some(material)) = (face_frame, frame_material) {
        fit.check_frame(frame, &layout.dividers)?;
        parts.extend(frame_parts(frame, material, face, &layout.dividers, &maker));
    }
    if let Some(blind) = plan.blind {
        parts.extend(blind_parts(blind, frame_material, &lookup, &maker)?);
    }
    let mut cabinet = Cabinet {
        lookup: &lookup,
        maker: &maker,
        rules: &rules,
        fit,
        blind: blind.map(|b| b.side),
        face,
        inner_width,
        depth: case_depth,
        // From the top of the bottom to the underside of the top or the
        // stretchers, up the sides.
        inside: (t + bottom_raise, box_height - t),
        back_front,
        sides: [Vec::new(), Vec::new()],
        hardware: Vec::new(),
    };
    for opening in &layout.openings {
        cabinet.insert(opening, &mut parts)?;
    }
    let hardware = tally(product.id, cabinet.hardware, quantity);
    machine_sides(&mut parts, cabinet.sides);
    Ok(Evaluated {
        parts,
        hardware,
        notes: lookup.notes.into_inner(),
    })
}

/// A blind corner's fixed panel, made like a slab door, and with a face
/// frame the mullion beside it.
fn blind_parts(
    (panel, mullion): (Rect, Option<Rect>),
    frame_material: Option<&SheetMaterial>,
    lookup: &Lookup<'_>,
    maker: &PartMaker<'_>,
) -> Result<Vec<Part>, String> {
    let mut parts = Vec::new();
    if let (Some(mullion), Some(material)) = (mullion, frame_material) {
        parts.push(maker.part(
            "frame/mullion.blind",
            "frameMullion",
            material,
            mullion.height,
            mullion.width,
            BARE,
        ));
    }
    let material = lookup.material_or("blindPanel", "doors")?;
    let bands = banded(lookup.band("edgeBandFronts"), &[0, 1, 2, 3]);
    parts.push(maker.part(
        "front/blindPanel",
        "blindPanel",
        material,
        panel.height,
        panel.width,
        bands,
    ));
    Ok(parts)
}

/// The side door `index` of `count` in `insert` hinges on: a pair on its
/// outer edges, a single door on its `hingeSide`, or in a blind corner
/// away from the blind part.
fn door_hinge(insert: &Insert, count: u32, index: usize, blind: Option<Side>) -> Option<Side> {
    match (count, index) {
        (2, 0) => Some(Side::Left),
        (2, _) => Some(Side::Right),
        _ => insert.hinge_side.or(blind.map(|side| match side {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        })),
    }
}

/// Adds what the inserts machine into the left and right sides.
fn machine_sides(parts: &mut [Part], [left, right]: [Vec<Operation>; 2]) {
    for (key, operations) in [("case/side.left", left), ("case/side.right", right)] {
        if let Some(side) = parts.iter_mut().find(|p| p.key == key) {
            side.operations.extend(operations);
        }
    }
}

/// The hardware an evaluation placed, each kind once, counted for every
/// copy of the product.
fn tally(product: Id, placed: Vec<(HardwareKind, u32)>, quantity: u32) -> Vec<HardwareUse> {
    let mut hardware: Vec<HardwareUse> = Vec::new();
    for (kind, count) in placed {
        match hardware.iter_mut().find(|h| h.kind == kind) {
            Some(used) => used.quantity += count * quantity,
            None => hardware.push(HardwareUse {
                product,
                kind,
                quantity: count * quantity,
            }),
        }
    }
    hardware
}

/// How far the back reaches into the case from its rear edge, when the
/// standard says.
fn back_front(back: &Back, material: &SheetMaterial, lookup: &Lookup<'_>) -> Option<Length> {
    match *back {
        Back::Grooved {
            setback: Some(setback),
            ..
        } => Some(setback + material.actual_thickness),
        Back::Grooved { setback: None, .. } => {
            lookup.note(
                "the back's groove is not machined: the standard's `back` rules have no \
                 `setback`",
            );
            None
        }
        Back::Applied => Some(Length::ZERO),
    }
}

/// The face frame rules of a standard built by `method`, which has none
/// when it is frameless.
fn face_frame(method: Method, rules: &Rules) -> Result<Option<&FaceFrame>, String> {
    match method {
        Method::Frameless => {
            if rules.fronts.overlay != Overlay::Full {
                return Err("frameless cabinets take full overlay fronts".into());
            }
            Ok(None)
        }
        Method::FaceFrame => rules
            .face_frame
            .as_ref()
            .map(Some)
            .ok_or_else(|| "the standard has no `faceFrame` rules".into()),
    }
}

/// A product's front laid out.
struct FrontPlan<'p> {
    /// The openings of its front tree, and the frame members between them.
    layout: Layout<'p>,
    /// How its fronts fit.
    fit: Fit,
    /// A blind corner's blind panel, and with a face frame the mullion
    /// beside it.
    blind: Option<(Rect, Option<Rect>)>,
}

/// The product's front laid over `face`, inside the face frame if there is
/// one; a blind corner's front tree takes only the doors' zone.
fn lay_out_front<'p>(
    product: &'p Product,
    face_frame: Option<&FaceFrame>,
    face: Rect,
    fronts: &Fronts,
    blind: Option<Blind>,
) -> Result<FrontPlan<'p>, String> {
    let (mut opening, dividers, fit) = match face_frame {
        None => (
            face,
            Dividers::default(),
            Fit::Frameless { gap: fronts.gap },
        ),
        Some(frame) => (frame.opening(face)?, frame.dividers(), Fit::framed(fronts)?),
    };
    let blind = match blind {
        Some(blind) => {
            let (doors, panel, mullion) = blind.divide(face, opening, fit, dividers.mullion)?;
            opening = doors;
            Some((fit.fronts(panel, 1)?[0], mullion))
        }
        None => None,
    };
    let layout = match &product.front {
        Some(root) => lay_out(root, opening, dividers)?,
        None => Layout::default(),
    };
    Ok(FrontPlan { layout, fit, blind })
}

/// The members of a face frame the size of `face`: stiles the full height,
/// the top and bottom rails between them, and the rails and mullions
/// between openings. None of them is banded.
fn frame_parts(
    frame: &FaceFrame,
    material: &SheetMaterial,
    face: Rect,
    dividers: &[Divider<'_>],
    maker: &PartMaker<'_>,
) -> Vec<Part> {
    let stile = frame.stile_width;
    let between = face.width - stile * 2;
    let part =
        |key: &str, role, length, width| maker.part(key, role, material, length, width, BARE);
    // Every member runs its grain along its length.
    let mut parts = vec![
        part("frame/stile.left", "frameStile", face.height, stile),
        part("frame/stile.right", "frameStile", face.height, stile),
        part("frame/rail.top", "frameRail", between, frame.top_rail()),
        part(
            "frame/rail.bottom",
            "frameRail",
            between,
            frame.bottom_rail(),
        ),
    ];
    for divider in dividers {
        let r = divider.rect;
        let (name, role, length, width) = match divider.direction {
            SplitDirection::Horizontal => ("rail", "frameRail", r.width, r.height),
            SplitDirection::Vertical => ("mullion", "frameMullion", r.height, r.width),
        };
        let key = format!("frame/{}/{name}.{}", divider.split.id, divider.index);
        parts.push(part(&key, role, length, width));
    }
    parts
}

/// What the evaluator works out from a standard's face frame rules.
trait FrameSizes {
    fn top_rail(&self) -> Length;
    fn bottom_rail(&self) -> Length;
    /// The widths of the rails and mullions between openings.
    fn dividers(&self) -> Dividers;
    /// The opening inside the stiles and the top and bottom rails of a
    /// frame the size of `face`.
    fn opening(&self, face: Rect) -> Result<Rect, String>;
}

impl FrameSizes for FaceFrame {
    fn top_rail(&self) -> Length {
        self.top_rail_width.unwrap_or(self.rail_width)
    }

    fn bottom_rail(&self) -> Length {
        self.bottom_rail_width.unwrap_or(self.rail_width)
    }

    fn dividers(&self) -> Dividers {
        Dividers {
            rail: self.rail_width,
            mullion: self.mullion_width.unwrap_or(self.stile_width),
        }
    }

    fn opening(&self, face: Rect) -> Result<Rect, String> {
        let opening = Rect {
            x: face.x + self.stile_width,
            y: face.y + self.bottom_rail(),
            width: face.width - self.stile_width * 2,
            height: face.height - self.top_rail() - self.bottom_rail(),
        };
        if opening.width <= Length::ZERO || opening.height <= Length::ZERO {
            return Err("the cabinet is too small for its face frame".into());
        }
        Ok(opening)
    }
}

/// How fronts sit on their openings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fit {
    /// Frameless: fronts share the box face, `gap` apart.
    Frameless { gap: Length },
    /// Face frame: fronts cover the frame by `by` around their opening, or
    /// stand back from it when `by` is less than zero (inset); the doors of
    /// a pair are `gap` apart.
    Framed { by: Length, gap: Length },
}

impl Fit {
    /// How the fronts of a face frame cabinet fit.
    fn framed(fronts: &Fronts) -> Result<Self, String> {
        let by = match fronts.overlay {
            Overlay::Partial => fronts
                .overlay_amount
                .ok_or("partial overlay fronts need an `overlayAmount`")?,
            Overlay::Inset => -fronts.gap,
            Overlay::Full => {
                return Err("face frame cabinets take partial overlay or inset fronts".into());
            }
        };
        Ok(Self::Framed {
            by,
            gap: fronts.gap,
        })
    }

    /// That overlaid fronts stay on the frame, and clear each other by at
    /// least the gap over the rails and mullions between them.
    fn check_frame(self, frame: &FaceFrame, dividers: &[Divider<'_>]) -> Result<(), String> {
        let Self::Framed { by, gap } = self else {
            return Ok(());
        };
        if by <= Length::ZERO {
            return Ok(());
        }
        let edge = frame
            .stile_width
            .min(frame.top_rail())
            .min(frame.bottom_rail());
        if by > edge {
            return Err("the fronts overlay more than the face frame around them is wide".into());
        }
        for divider in dividers {
            let width = match divider.direction {
                SplitDirection::Horizontal => divider.rect.height,
                SplitDirection::Vertical => divider.rect.width,
            };
            if width - by * 2 < gap {
                return Err(
                    "a rail or mullion is too narrow for the fronts on each side \
                    of it to clear each other by the gap"
                        .into(),
                );
            }
        }
        Ok(())
    }

    /// `count` fronts side by side for `opening`, left to right.
    fn fronts(self, opening: Rect, count: u32) -> Result<Vec<Rect>, String> {
        let mut x = opening.x;
        let fronts: Vec<Rect> = match self {
            // Each front is its share of the face less the gap.
            Self::Frameless { gap } => opening
                .width
                .split(count)
                .into_iter()
                .map(|width| {
                    let front = Rect {
                        x,
                        width,
                        ..opening
                    }
                    .inset(gap);
                    x += width;
                    front
                })
                .collect(),
            // The fronts share what covers the opening, less the gaps
            // between them.
            Self::Framed { by, gap } => {
                let whole = Rect {
                    x: opening.x - by,
                    y: opening.y - by,
                    width: opening.width + by * 2,
                    height: opening.height + by * 2,
                };
                let room = whole.width - gap * (i64::from(count) - 1);
                if room <= Length::ZERO {
                    return Err("the opening is too small for its fronts".into());
                }
                x = whole.x;
                room.split(count)
                    .into_iter()
                    .map(|width| {
                        let front = Rect { x, width, ..whole };
                        x += width + gap;
                        front
                    })
                    .collect()
            }
        };
        if fronts
            .iter()
            .any(|f| f.width <= Length::ZERO || f.height <= Length::ZERO)
        {
            return Err("the opening is too small for its fronts".into());
        }
        Ok(fronts)
    }
}

/// The overall sizes a case is built from.
#[derive(Clone, Copy)]
struct CaseSize {
    /// Above the toe kick.
    box_height: Length,
    /// Between the sides.
    inner_width: Length,
    width: Length,
    depth: Length,
    /// How far the bottom sits above the bottom of the sides.
    bottom_raise: Length,
}

/// The case's parts: sides, bottom, top or stretchers, and back.
fn case_parts(
    kind: Kind,
    size: CaseSize,
    rules: &Rules,
    maker: &PartMaker<'_>,
    (case, back): (&SheetMaterial, &SheetMaterial),
    front_edge: Bands<'_>,
) -> Vec<Part> {
    let CaseSize {
        box_height,
        inner_width,
        width: w,
        depth: d,
        bottom_raise,
    } = size;
    let t = case.actual_thickness;
    let mut parts = vec![
        maker.part("case/side.left", "side", case, box_height, d, front_edge),
        maker.part("case/side.right", "side", case, box_height, d, front_edge),
        maker.part("case/bottom", "bottom", case, inner_width, d, front_edge),
    ];
    // A base cabinet is tied at the top by two stretchers, so its back is
    // grooved into the bottom only; wall and tall cabinets have a full top,
    // grooved like the bottom.
    let grooved_ends = match kind {
        Kind::Base => {
            let stretcher = rules.case.stretcher_width;
            parts.extend([
                maker.part(
                    "case/stretcher.front",
                    "stretcher",
                    case,
                    inner_width,
                    stretcher,
                    front_edge,
                ),
                maker.part(
                    "case/stretcher.back",
                    "stretcher",
                    case,
                    inner_width,
                    stretcher,
                    BARE,
                ),
            ]);
            1
        }
        Kind::Wall | Kind::Tall => {
            parts.push(maker.part("case/top", "top", case, inner_width, d, front_edge));
            2
        }
    };
    let (back_length, back_width) = match rules.back {
        Back::Grooved { groove_depth, .. } => (
            box_height - (t - groove_depth) * grooved_ends - bottom_raise,
            inner_width + groove_depth * 2,
        ),
        Back::Applied => (box_height, w),
    };
    parts.push(maker.part("case/back", "back", back, back_length, back_width, BARE));

    // The back's groove runs the full length of the sides, the bottom and
    // any top.
    if let Back::Grooved {
        groove_depth,
        setback: Some(setback),
    } = rules.back
    {
        let width = back.actual_thickness;
        let y = setback + width.div_round(2);
        for part in &mut parts {
            if matches!(part.role, "side" | "bottom" | "top") {
                let groove = groove_along(part.length, y, width, groove_depth);
                part.operations.push(groove);
            }
        }
    }
    parts
}

/// The kinds of cabinet the evaluator builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Stands on a toe kick, tied at the top by stretchers.
    Base,
    /// Hangs on the wall, with a full top and no toe kick.
    Wall,
    /// Stands on a toe kick, with a full top.
    Tall,
}

impl Kind {
    /// The kind of box a product type builds, and whether it is a blind
    /// corner.
    fn of(product_type: &str) -> Option<(Self, bool)> {
        match product_type {
            "base-cabinet@1" => Some((Self::Base, false)),
            "wall-cabinet@1" => Some((Self::Wall, false)),
            "tall-cabinet@1" => Some((Self::Tall, false)),
            "blind-corner-base@1" => Some((Self::Base, true)),
            "blind-corner-wall@1" => Some((Self::Wall, true)),
            _ => None,
        }
    }
}

/// The part of a blind corner cabinet's front that the cabinet on the
/// adjoining wall hides, from its parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Blind {
    /// The side it is on, seen from the front.
    pub side: Side,
    /// How much of the front it takes, from that side.
    pub width: Length,
}

impl Blind {
    /// The blind part of `product`, if its type is a blind corner.
    pub fn of(product: &Product) -> Result<Option<Self>, String> {
        if !Kind::of(&product.product_type).is_some_and(|(_, blind)| blind) {
            return Ok(None);
        }
        let side = product
            .parameters
            .get("blindSide")
            .and_then(|v| serde_json::from_value::<Side>(v.clone()).ok())
            .ok_or("a blind corner cabinet needs a `blindSide`, left or right")?;
        let width = product
            .parameters
            .get("blindWidth")
            .and_then(serde_json::Value::as_i64)
            .map(Length::from_nm)
            .filter(|&w| w > Length::ZERO)
            .ok_or("a blind corner cabinet needs a `blindWidth` greater than zero")?;
        Ok(Some(Self { side, width }))
    }

    /// Divides the box face between the doors and the blind panel. The
    /// doors' zone is what remains of `opening` (the face, or the face
    /// frame's opening); the blind panel's opening is the rest, and a
    /// face frame puts a `mullion` wide mullion between them. Its edge on
    /// the doors' side is placed so that nothing that opens reaches into
    /// the blind width: a door's edge lands on it exactly, or a gap past
    /// it when frameless.
    fn divide(
        self,
        face: Rect,
        opening: Rect,
        fit: Fit,
        mullion: Length,
    ) -> Result<(Rect, Rect, Option<Rect>), String> {
        // Distances from the blind edge of the face.
        let (reach, mullion) = match fit {
            Fit::Frameless { .. } => (Length::ZERO, Length::ZERO),
            Fit::Framed { by, gap } => {
                if by > Length::ZERO && mullion - by * 2 < gap {
                    return Err(
                        "the blind corner's mullion is too narrow for the fronts on each \
                         side of it to clear each other by the gap"
                            .into(),
                    );
                }
                (by, mullion)
            }
        };
        let doors_edge = self.width + reach;
        let blind_edge = doors_edge - mullion;
        let (near, far) = match self.side {
            Side::Left => (opening.x - face.x, opening.x + opening.width - face.x),
            Side::Right => (
                face.x + face.width - opening.x - opening.width,
                face.x + face.width - opening.x,
            ),
        };
        if blind_edge <= near {
            return Err("the blind width is too narrow for the face frame".into());
        }
        if doors_edge >= far {
            return Err("the blind width leaves no room for the doors".into());
        }
        // The part of `opening` from `from` to `to` away from the blind edge.
        let zone = |from: Length, to: Length| Rect {
            x: match self.side {
                Side::Left => face.x + from,
                Side::Right => face.x + face.width - to,
            },
            width: to - from,
            ..opening
        };
        let mullion = (mullion > Length::ZERO).then(|| zone(blind_edge, doors_edge));
        Ok((zone(doors_edge, far), zone(near, blind_edge), mullion))
    }
}

/// What the parts of a front's inserts depend on.
struct Cabinet<'c, 'p> {
    lookup: &'c Lookup<'p>,
    maker: &'c PartMaker<'p>,
    rules: &'c Rules,
    fit: Fit,
    /// A blind corner's blind side.
    blind: Option<Side>,
    /// The front of the box, from the floor: where the sides stand.
    face: Rect,
    /// Between the sides.
    inner_width: Length,
    /// Of the case, behind any face frame.
    depth: Length,
    /// The inside of the case up the sides, from their bottom end.
    inside: (Length, Length),
    /// How far the back reaches in from the rear edge, when known.
    back_front: Option<Length>,
    /// What the inserts machine into the left and right sides.
    sides: [Vec<Operation>; 2],
    /// The hardware the inserts place, for one copy.
    hardware: Vec<(HardwareKind, u32)>,
}

impl<'p> Cabinet<'_, 'p> {
    /// The bands on a slab front's four edges.
    fn front_bands(&self) -> Bands<'p> {
        banded(self.lookup.band("edgeBandFronts"), &[0, 1, 2, 3])
    }

    /// The parts an opening's insert produces.
    fn insert(&mut self, opening: &Opening<'_>, parts: &mut Vec<Part>) -> Result<(), String> {
        let insert = opening.insert;
        if let Some(FrontStyle::Other(style)) = &insert.style {
            return Err(format!("unknown front style `{style}`"));
        }
        let five_piece = insert.style == Some(FrontStyle::FivePiece);
        let prefix = format!("front/{}", opening.node.id);
        match &insert.kind {
            InsertKind::Doors => {
                let material = self.lookup.material("doors")?;
                let count = u32::from(insert.count.unwrap_or(1));
                if !(1..=2).contains(&count) {
                    return Err("an opening takes one or two doors".into());
                }
                let doors = self.fit.fronts(opening.rect, count)?;
                for (i, door) in doors.into_iter().enumerate() {
                    let key = format!("{prefix}/door.{}", i + 1);
                    let side = door_hinge(insert, count, i, self.blind)
                        .ok_or("a single door needs a hinge side")?;
                    let first = parts.len();
                    if five_piece {
                        self.five_piece(&key, door, parts)?;
                    } else {
                        // Slab doors run their grain up.
                        let (length, width) = (door.height, door.width);
                        let part = self.maker.part(
                            &key,
                            "door",
                            material,
                            length,
                            width,
                            self.front_bands(),
                        );
                        parts.push(part);
                    }
                    // The cups go in the door, or in the stile on its
                    // hinge side.
                    let target = match side {
                        Side::Right if five_piece => first + 1,
                        _ => first,
                    };
                    self.hinges(opening, door, side, &mut parts[target])?;
                }
                self.shelves(&prefix, insert.shelves, opening.rect, parts)?;
            }
            InsertKind::Drawer if self.blind.is_some() => {
                return Err(
                    "drawers in a blind corner cabinet cannot be built yet: there is no \
                     side to mount their slides on next to the blind part"
                        .into(),
                );
            }
            InsertKind::Drawer | InsertKind::FalseFront => {
                let material = self.lookup.material_or("drawerFronts", "doors")?;
                let front = self.fit.fronts(opening.rect, 1)?[0];
                let (key, role) = if insert.kind == InsertKind::Drawer {
                    (format!("{prefix}/drawer/front"), "drawerFront")
                } else {
                    (format!("{prefix}/falseFront"), "falseFront")
                };
                if five_piece {
                    self.five_piece(&key, front, parts)?;
                } else {
                    // Slab drawer and false fronts run their grain across.
                    let (length, width) = (front.width, front.height);
                    let part =
                        self.maker
                            .part(&key, role, material, length, width, self.front_bands());
                    parts.push(part);
                }
                if insert.kind == InsertKind::Drawer {
                    self.drawer_box(&prefix, opening.rect, parts)?;
                }
            }
            InsertKind::Open => self.shelves(&prefix, insert.shelves, opening.rect, parts)?,
            InsertKind::Appliance | InsertKind::None => {}
            InsertKind::Other(kind) => {
                return Err(format!(
                    "unknown insert `{kind}`; open with a newer OpenCabinet"
                ));
            }
        }
        Ok(())
    }

    /// Bores the cups of the hinges of `door` into `part`, the door or its
    /// stile on the hinge side, and in a frameless cabinet the holes for
    /// the hinges' plates into the side they mount on.
    fn hinges(
        &mut self,
        opening: &Opening<'_>,
        door: Rect,
        side: Side,
        part: &mut Part,
    ) -> Result<(), String> {
        let Some(rules) = &self.rules.hinges else {
            self.lookup
                .note("doors are not bored for hinges: the standard has no `hinges` rules");
            return Ok(());
        };
        let hinge = self.lookup.hardware(rules.hardware)?;
        let dimension = |name: &str| {
            hinge
                .dimensions
                .get(name)
                .copied()
                .ok_or_else(|| format!("the hinge `{}` has no `{name}`", hinge.name))
        };
        let (diameter, depth) = (dimension("cupDiameter")?, dimension("cupDepth")?);
        let count = rules
            .counts
            .iter()
            .find(|c| door.height <= c.up_to)
            .ok_or("the door is taller than any in the standard's hinge table")?
            .count;
        if count < 2 {
            return Err("the standard's hinge table gives a door fewer than two hinges".into());
        }
        if door.height <= rules.end_distance * 2 {
            return Err("the door is too short for its hinges".into());
        }
        if rules.boring_distance + diameter > part.width || depth >= part.thickness {
            return Err("the hinge cups do not fit in the door".into());
        }
        // The cup's center, from the hinge edge.
        let from_edge = rules.boring_distance + diameter.div_round(2);
        let y = match side {
            Side::Left => from_edge,
            Side::Right => part.width - from_edge,
        };
        let positions = hinge_positions(door.height, rules.end_distance, count);
        self.hardware
            .push((HardwareKind::Hinge(rules.hardware), count));
        for &x in &positions {
            part.operations.push(Operation::Hole {
                face: Face::A,
                at: Point { x, y },
                diameter,
                depth,
            });
        }
        if matches!(self.fit, Fit::Framed { .. }) {
            // Face frame hinges screw to the frame.
            return Ok(());
        }
        let (on_side, index) = match side {
            Side::Left => (opening.rect.x == self.face.x, 0),
            Side::Right => (
                opening.rect.x + opening.rect.width == self.face.x + self.face.width,
                1,
            ),
        };
        if !on_side {
            return Err(
                "a door hinged away from the cabinet's sides has nothing to mount its \
                 hinges on"
                    .into(),
            );
        }
        let Some(holes) = &self.rules.system_holes else {
            self.lookup.note(
                "the sides are not bored for hinge plates: the standard has no \
                 `systemHoles` rules",
            );
            return Ok(());
        };
        // Two holes on the front column, a spacing apart, centered on each
        // cup.
        let y = self.depth - holes.front_setback;
        let half = holes.spacing.div_round(2);
        for x in positions {
            let x = door.y - self.face.y + x;
            for at in [x - half, x - half + holes.spacing] {
                self.sides[index].push(Operation::Hole {
                    face: Face::A,
                    at: Point { x: at, y },
                    diameter: holes.diameter,
                    depth: holes.depth,
                });
            }
        }
        Ok(())
    }

    /// Bores both sides for shelf pins across `opening`: a front and a rear
    /// column on the 32 mm grid that starts at the top of the bottom,
    /// clear of the opening's ends.
    fn line_boring(&mut self, opening: Rect) {
        let Some(holes) = &self.rules.system_holes else {
            self.lookup.note(
                "the sides are not bored for shelf pins: the standard has no \
                 `systemHoles` rules",
            );
            return;
        };
        let Some(back_front) = self.back_front else {
            // Already noted: without the back's setback the rear column
            // has no place.
            return;
        };
        let (bottom, top) = self.inside;
        let low = (opening.y - self.face.y).max(bottom) + holes.end_clearance;
        let high = (opening.y + opening.height - self.face.y).min(top) - holes.end_clearance;
        let Some((first, count)) = holes_between(bottom, holes.spacing, low, high) else {
            return;
        };
        for y in [
            self.depth - holes.front_setback,
            back_front + holes.rear_setback,
        ] {
            let column = Operation::HolePattern {
                face: Face::A,
                start: Point { x: first, y },
                step: Point {
                    x: holes.spacing,
                    y: Length::ZERO,
                },
                count,
                diameter: holes.diameter,
                depth: holes.depth,
            };
            for side in &mut self.sides {
                side.push(column.clone());
            }
        }
    }

    /// A five-piece front the size of `front`: two stiles the full height,
    /// two rails between them reaching into the stiles by the joint depth,
    /// and a panel that floats in grooves. The frame is not banded.
    fn five_piece(&self, key: &str, front: Rect, parts: &mut Vec<Part>) -> Result<(), String> {
        let rules = self
            .rules
            .five_piece
            .as_ref()
            .ok_or("the standard has no `fivePiece` rules")?;
        let frame = self.lookup.material_or("frontFrames", "doors")?;
        let panel = self.lookup.material_or("frontPanels", "doors")?;
        let (stile, rail) = (rules.stile_width, rules.rail_width);
        let between = front.width - stile * 2;
        let panel_height = front.height - rail * 2;
        if between <= Length::ZERO || panel_height <= Length::ZERO {
            return Err("the front is too small for a five-piece frame".into());
        }
        let g = rules.panel_groove_depth;
        let float = rules.panel_clearance;
        let part = |name: &str, role, material, length, width| {
            self.maker.part(
                &format!("{key}/{name}"),
                role,
                material,
                length,
                width,
                BARE,
            )
        };
        parts.extend([
            // Stiles run their grain up, rails across, the panel up.
            part("stile.left", "stile", frame, front.height, stile),
            part("stile.right", "stile", frame, front.height, stile),
            part(
                "rail.top",
                "rail",
                frame,
                between + rules.joint_depth * 2,
                rail,
            ),
            part(
                "rail.bottom",
                "rail",
                frame,
                between + rules.joint_depth * 2,
                rail,
            ),
            part(
                "panel",
                "panel",
                panel,
                panel_height + g * 2 - float,
                between + g * 2 - float,
            ),
        ]);
        Ok(())
    }

    /// The box of a drawer for `opening`: sides the full depth, front and
    /// back between them, and the bottom in a groove in all four.
    fn drawer_box(
        &mut self,
        prefix: &str,
        opening: Rect,
        parts: &mut Vec<Part>,
    ) -> Result<(), String> {
        let rules = self
            .rules
            .drawers
            .as_ref()
            .ok_or("the standard has no `drawers` rules")?;
        let sides = self.lookup.material("drawerBox")?;
        let bottom = self.lookup.material("drawerBottom")?;
        let t = sides.actual_thickness;
        let room = self.depth - rules.rear_clearance;
        let depth = rules
            .slide_lengths
            .iter()
            .copied()
            .filter(|&l| l <= room)
            .max()
            .ok_or("no slide in the standard is short enough for this cabinet")?;
        self.hardware.push((HardwareKind::SlidePair(depth), 1));
        let height = opening.height - rules.vertical_clearance;
        // Behind a face frame the box must pass through the frame's
        // opening; the slides are set out to it on blocking.
        let room = match self.fit {
            Fit::Frameless { .. } => self.inner_width,
            Fit::Framed { .. } => {
                self.lookup.note(
                    "drawer slides in face frame cabinets mount on blocking out to the \
                     frame opening, which is not in the cut list yet",
                );
                opening.width
            }
        };
        let width = room - rules.side_clearance * 2;
        let between = width - t * 2;
        if height <= Length::ZERO || between <= Length::ZERO {
            return Err("the drawer opening is too small for a drawer box".into());
        }
        let g = rules.bottom_groove_depth;
        // The bottom's groove runs the full length of the sides, the front
        // and the back.
        let groove = if let Some(setback) = rules.bottom_groove_setback {
            let width = bottom.actual_thickness;
            if setback + width >= height {
                return Err("the drawer box is too low for its bottom's groove".into());
            }
            Some((setback + width.div_round(2), width))
        } else {
            self.lookup.note(
                "drawer bottoms' grooves are not machined: the standard's `drawers` \
                     rules have no `bottomGrooveSetback`",
            );
            None
        };
        let part = |name: &str, role, material, length, width| {
            let key = format!("{prefix}/drawer/box.{name}");
            let mut part = self.maker.part(&key, role, material, length, width, BARE);
            if let (Some((y, groove_width)), "drawerSide" | "drawerSubFront" | "drawerBack") =
                (groove, role)
            {
                part.operations
                    .push(groove_along(length, y, groove_width, g));
            }
            part
        };
        parts.extend([
            part("side.left", "drawerSide", sides, depth, height),
            part("side.right", "drawerSide", sides, depth, height),
            part("front", "drawerSubFront", sides, between, height),
            part("back", "drawerBack", sides, between, height),
            part(
                "bottom",
                "drawerBottom",
                bottom,
                between + g * 2,
                depth - t * 2 + g * 2,
            ),
        ]);
        Ok(())
    }

    /// `count` adjustable shelves across the cabinet's inside, in
    /// `opening`, and the line boring for their pins.
    fn shelves(
        &mut self,
        prefix: &str,
        count: Option<u8>,
        opening: Rect,
        parts: &mut Vec<Part>,
    ) -> Result<(), String> {
        let count = count.unwrap_or(0);
        if count == 0 {
            return Ok(());
        }
        let rules = self
            .rules
            .shelves
            .as_ref()
            .ok_or("the standard has no `shelves` rules")?;
        let material = self.lookup.material_or("shelves", "case")?;
        let front = banded(self.lookup.band("edgeBandCase"), &[Y1]);
        let length = self.inner_width - rules.side_clearance;
        let width = self.depth - rules.front_setback - rules.back_allowance;
        if length <= Length::ZERO || width <= Length::ZERO {
            return Err("the cabinet is too small for its shelves".into());
        }
        if self
            .back_front
            .is_some_and(|back| rules.back_allowance < back)
        {
            return Err(
                "the shelves reach into the back: their back allowance is less \
                 than the back's setback and thickness"
                    .into(),
            );
        }
        self.line_boring(opening);
        // Four pins hold each shelf.
        self.hardware
            .push((HardwareKind::ShelfPin, 4 * u32::from(count)));
        for i in 1..=count {
            let key = format!("{prefix}/shelf.{i}");
            let part = self
                .maker
                .part(&key, "shelf", material, length, width, front);
            parts.push(part);
        }
        Ok(())
    }
}

/// Finds the library items a product's material roles point to.
struct Lookup<'p> {
    product: &'p Product,
    library: &'p Library,
    notes: RefCell<Vec<String>>,
}

impl<'p> Lookup<'p> {
    fn material(&self, role: &str) -> Result<&'p SheetMaterial, String> {
        let id = self
            .product
            .materials
            .get(role)
            .ok_or_else(|| format!("no material chosen for `{role}`"))?;
        self.library
            .materials
            .iter()
            .find(|m| m.id == *id)
            .ok_or_else(|| format!("the material for `{role}` is not in the library"))
    }

    fn hardware(&self, id: Id) -> Result<&'p Hardware, String> {
        self.library
            .hardware
            .iter()
            .find(|h| h.id == id)
            .ok_or_else(|| "the standard's hinge is not in the library".into())
    }

    /// The material for `role`, or for `fallback` when `role` has none.
    fn material_or(&self, role: &str, fallback: &str) -> Result<&'p SheetMaterial, String> {
        if self.product.materials.contains_key(role) {
            self.material(role)
        } else {
            self.material(fallback)
        }
    }

    /// The edge band for `role`; without one, edges stay unbanded and the
    /// user is told.
    fn band(&self, role: &str) -> Option<&'p EdgeBand> {
        let band = self
            .product
            .materials
            .get(role)
            .and_then(|id| self.library.edge_bands.iter().find(|b| b.id == *id));
        if band.is_none() {
            self.note(&format!(
                "no edge band chosen for `{role}`; those edges are left bare"
            ));
        }
        band
    }

    /// Records a note once, however many parts it concerns.
    fn note(&self, message: &str) {
        let mut notes = self.notes.borrow_mut();
        if !notes.iter().any(|n| n == message) {
            notes.push(message.to_owned());
        }
    }
}

/// The bands on a part's four edges, while it is being built.
type Bands<'p> = [Option<&'p EdgeBand>; 4];

const BARE: Bands<'static> = [None; 4];

/// `band` on the given edges, the others bare.
fn banded<'p>(band: Option<&'p EdgeBand>, edges: &[usize]) -> Bands<'p> {
    let mut out = BARE;
    for &edge in edges {
        out[edge] = band;
    }
    out
}

/// Builds parts for one product, with its quantity.
struct PartMaker<'p> {
    product: &'p Product,
    quantity: u32,
}

impl PartMaker<'_> {
    fn part(
        &self,
        key: &str,
        role: &'static str,
        material: &SheetMaterial,
        length: Length,
        width: Length,
        bands: Bands<'_>,
    ) -> Part {
        let thickness = |edge: usize| bands[edge].map_or(Length::ZERO, |b| b.thickness);
        Part {
            product: self.product.id,
            key: key.into(),
            role,
            material: material.id,
            length,
            width,
            thickness: material.actual_thickness,
            cut_length: length - thickness(0) - thickness(1),
            cut_width: width - thickness(2) - thickness(3),
            edges: bands.map(|b| b.map(|b| b.id)),
            quantity: self.quantity,
            operations: Vec::new(),
        }
    }
}
