// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a product looks like from the front, for elevations: its outline,
//! toe kick, face frame and fronts, laid out by the same rules as its parts.

use opencabinet_model::{Id, InsertKind, Length, Library, Product, Side};

use crate::front::Rect;
use crate::rules::Rules;
use crate::{Blind, FrameSizes, FrontPlan, Kind, door_hinge, face_frame, lay_out_front};

/// A product seen from the front, in its own coordinates: `x` from its
/// left side, `y` up from its bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontView {
    /// The whole product, `W` × `H`.
    pub outline: Rect,
    /// The toe kick under a base or tall cabinet.
    pub toe_kick: Option<Rect>,
    /// The members of a face frame.
    pub frame: Vec<Rect>,
    /// Doors, drawer fronts and false fronts.
    pub fronts: Vec<FrontShape>,
}

/// A front and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontShape {
    /// Its outline.
    pub rect: Rect,
    /// What it is.
    pub kind: FrontKind,
}

/// What a front is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontKind {
    /// A door, hinged on a side when the front says which.
    Door {
        /// The hinge side, seen from the front.
        hinge: Option<Side>,
    },
    /// A drawer's front.
    Drawer,
    /// A front that does not open.
    FalseFront,
    /// A blind corner's fixed panel, behind the adjoining cabinet.
    BlindPanel,
}

/// Where the openings of `product`'s front come out, each with its node's
/// identity, in the tree's order: for a stack, from top to bottom.
///
/// # Errors
/// When its type cannot be evaluated or its front cannot be laid out.
pub fn front_openings(product: &Product, library: &Library) -> Result<Vec<(Id, Rect)>, String> {
    let (kind, _) = Kind::of(&product.product_type).ok_or("this product type has no view yet")?;
    let blind = Blind::of(product)?;
    let standard = library
        .standards
        .iter()
        .find(|s| s.id == product.standard)
        .ok_or("the standard is not in the library")?;
    let rules = Rules::of(standard)?;
    let face_frame = face_frame(standard.method, &rules)?;
    let toe_kick = match kind {
        Kind::Wall => Length::ZERO,
        Kind::Base | Kind::Tall => rules.toe_kick.height,
    };
    let face = Rect {
        x: Length::ZERO,
        y: toe_kick,
        width: product.width,
        height: product.height - toe_kick,
    };
    let plan = lay_out_front(product, face_frame, face, &rules.fronts, blind)?;
    Ok(plan
        .layout
        .openings
        .iter()
        .map(|opening| (opening.node.id, opening.rect))
        .collect())
}

/// How `product` looks from the front.
///
/// # Errors
/// When its type cannot be evaluated or its front cannot be laid out;
/// an elevation then shows only its outline.
pub fn front_view(product: &Product, library: &Library) -> Result<FrontView, String> {
    let (kind, _) = Kind::of(&product.product_type).ok_or("this product type has no view yet")?;
    let blind = Blind::of(product)?;
    let standard = library
        .standards
        .iter()
        .find(|s| s.id == product.standard)
        .ok_or("the standard is not in the library")?;
    let rules = Rules::of(standard)?;
    let face_frame = face_frame(standard.method, &rules)?;
    let toe_kick = match kind {
        Kind::Wall => Length::ZERO,
        Kind::Base | Kind::Tall => rules.toe_kick.height,
    };
    let (w, h) = (product.width, product.height);
    let face = Rect {
        x: Length::ZERO,
        y: toe_kick,
        width: w,
        height: h - toe_kick,
    };
    let FrontPlan {
        layout,
        fit,
        blind: blind_panel,
    } = lay_out_front(product, face_frame, face, &rules.fronts, blind)?;

    let mut frame = Vec::new();
    if let Some(rules) = face_frame {
        let s = rules.stile_width;
        let between = w - s * 2;
        frame.extend([
            Rect { width: s, ..face },
            Rect {
                x: face.x + w - s,
                width: s,
                ..face
            },
            Rect {
                x: face.x + s,
                y: face.y + face.height - rules.top_rail(),
                width: between,
                height: rules.top_rail(),
            },
            Rect {
                x: face.x + s,
                y: face.y,
                width: between,
                height: rules.bottom_rail(),
            },
        ]);
        frame.extend(layout.dividers.iter().map(|d| d.rect));
        frame.extend(blind_panel.and_then(|(_, mullion)| mullion));
    }

    let mut fronts: Vec<FrontShape> = blind_panel
        .map(|(rect, _)| FrontShape {
            rect,
            kind: FrontKind::BlindPanel,
        })
        .into_iter()
        .collect();
    for opening in &layout.openings {
        let insert = opening.insert;
        match insert.kind {
            InsertKind::Doors => {
                let count = u32::from(insert.count.unwrap_or(1)).clamp(1, 2);
                let doors = fit.fronts(opening.rect, count)?;
                for (i, rect) in doors.into_iter().enumerate() {
                    let hinge = door_hinge(insert, count, i, blind.map(|b| b.side));
                    fronts.push(FrontShape {
                        rect,
                        kind: FrontKind::Door { hinge },
                    });
                }
            }
            InsertKind::Drawer | InsertKind::FalseFront => fronts.push(FrontShape {
                rect: fit.fronts(opening.rect, 1)?[0],
                kind: if insert.kind == InsertKind::Drawer {
                    FrontKind::Drawer
                } else {
                    FrontKind::FalseFront
                },
            }),
            InsertKind::Open | InsertKind::Appliance | InsertKind::None | InsertKind::Other(_) => {}
        }
    }
    Ok(FrontView {
        outline: Rect {
            x: Length::ZERO,
            y: Length::ZERO,
            width: w,
            height: h,
        },
        toe_kick: (toe_kick > Length::ZERO).then_some(Rect {
            x: Length::ZERO,
            y: Length::ZERO,
            width: w,
            height: toe_kick,
        }),
        frame,
        fronts,
    })
}
