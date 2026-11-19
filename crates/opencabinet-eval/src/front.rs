// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Divides a product's front into the openings of its split tree
//! (RFC-0001 §6).

use opencabinet_model::{FrontNode, Insert, Length, Size, SplitDirection};

/// A rectangle on the product's front: `x` from the left side, `y` up from
/// the floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    /// Left edge.
    pub x: Length,
    /// Bottom edge.
    pub y: Length,
    /// Width.
    pub width: Length,
    /// Height.
    pub height: Length,
}

impl Rect {
    /// This rectangle shrunk by `gap` in each direction: half on one side,
    /// the rest on the other, so neighbors end exactly `gap` apart even when
    /// it is an odd number of nanometers.
    #[must_use]
    pub fn inset(self, gap: Length) -> Self {
        let half = gap.div_round(2);
        Self {
            x: self.x + half,
            y: self.y + half,
            width: self.width - gap,
            height: self.height - gap,
        }
    }
}

/// An opening of the front with its insert and where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opening<'p> {
    /// The tree node, whose identity makes part keys stable.
    pub node: &'p FrontNode,
    /// What fills it.
    pub insert: &'p Insert,
    /// Where it is, before any gap.
    pub rect: Rect,
}

/// The widths of the face frame members between openings: a rail between
/// openings one over the other, a mullion between openings side by side.
/// Frameless fronts have none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dividers {
    /// Between the children of a horizontal split.
    pub rail: Length,
    /// Between the children of a vertical split.
    pub mullion: Length,
}

/// A face frame member between two openings of a split.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Divider<'p> {
    /// The split it divides, whose identity makes part keys stable.
    pub split: &'p FrontNode,
    /// Which divider of the split, from 1, top down or left to right.
    pub index: usize,
    /// A rail for a horizontal split, a mullion for a vertical one.
    pub direction: SplitDirection,
    /// Where it is.
    pub rect: Rect,
}

/// A front tree laid out: its openings and the members between them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout<'p> {
    /// Top to bottom and left to right.
    pub openings: Vec<Opening<'p>>,
    /// In the order the tree reaches them.
    pub dividers: Vec<Divider<'p>>,
}

/// The openings of a front tree laid over `rect`, top to bottom and left to
/// right, with nothing between them.
///
/// # Errors
/// As for [`lay_out`].
pub fn openings(root: &FrontNode, rect: Rect) -> Result<Vec<Opening<'_>>, String> {
    Ok(lay_out(root, rect, Dividers::default())?.openings)
}

/// A front tree laid over `rect`, with `dividers` between the children of
/// each split.
///
/// The dividers are taken first; then fixed sizes, which are the sizes of
/// the openings; the space left is divided among the shares in proportion,
/// and the nanometers left over by that division go one each to the first
/// shares, so the sizes always add up to the whole.
///
/// # Errors
/// When the dividers or the fixed sizes do not fit, or a node is neither a
/// split with children nor an opening with an insert.
pub fn lay_out(root: &FrontNode, rect: Rect, dividers: Dividers) -> Result<Layout<'_>, String> {
    let mut layout = Layout::default();
    lay_out_node(root, rect, dividers, &mut layout)?;
    Ok(layout)
}

fn lay_out_node<'p>(
    node: &'p FrontNode,
    rect: Rect,
    dividers: Dividers,
    out: &mut Layout<'p>,
) -> Result<(), String> {
    match (&node.split, &node.insert) {
        (None, Some(insert)) if node.children.is_empty() => {
            out.openings.push(Opening { node, insert, rect });
            Ok(())
        }
        (Some(direction), None) if !node.children.is_empty() => {
            let (total, divider) = match direction {
                SplitDirection::Horizontal => (rect.height, dividers.rail),
                SplitDirection::Vertical => (rect.width, dividers.mullion),
            };
            let count = i64::try_from(node.children.len()).expect("a reasonable tree");
            let room = total - divider * (count - 1);
            if room < Length::ZERO {
                return Err("the front's rails and mullions are wider than the opening".into());
            }
            let sizes = divide(&node.children, room)?;
            // Horizontal splits run from the top down; vertical ones from
            // the left.
            let mut top = rect.y + rect.height;
            let mut left = rect.x;
            for (i, (child, size)) in node.children.iter().zip(sizes).enumerate() {
                if i > 0 {
                    let divider_rect = match direction {
                        SplitDirection::Horizontal => {
                            top -= divider;
                            Rect {
                                y: top,
                                height: divider,
                                ..rect
                            }
                        }
                        SplitDirection::Vertical => {
                            left += divider;
                            Rect {
                                x: left - divider,
                                width: divider,
                                ..rect
                            }
                        }
                    };
                    if divider > Length::ZERO {
                        out.dividers.push(Divider {
                            split: node,
                            index: i,
                            direction: *direction,
                            rect: divider_rect,
                        });
                    }
                }
                let child_rect = match direction {
                    SplitDirection::Horizontal => {
                        top -= size;
                        Rect {
                            y: top,
                            height: size,
                            ..rect
                        }
                    }
                    SplitDirection::Vertical => {
                        let r = Rect {
                            x: left,
                            width: size,
                            ..rect
                        };
                        left += size;
                        r
                    }
                };
                lay_out_node(child, child_rect, dividers, out)?;
            }
            Ok(())
        }
        _ => Err(format!(
            "front node {} must be a split with children or an insert",
            node.id
        )),
    }
}

/// The children's sizes along the split, adding up to `total` exactly.
fn divide(children: &[FrontNode], total: Length) -> Result<Vec<Length>, String> {
    let fixed: Length = children
        .iter()
        .filter_map(|c| match c.size {
            Some(Size::Length(length)) => Some(length),
            _ => None,
        })
        .sum();
    let shares: u64 = children
        .iter()
        .filter_map(|c| match c.size {
            Some(Size::Share(share)) => Some(u64::from(share)),
            _ => None,
        })
        .sum();
    let rest = total - fixed;
    if rest < Length::ZERO {
        return Err("the front's fixed sizes are larger than the opening".into());
    }
    if shares == 0 && rest != Length::ZERO {
        return Err("the front's fixed sizes do not fill the opening".into());
    }

    // Each share gets rest × share / shares, rounded down; the remainder
    // goes one nanometer at a time to the first shares.
    let rest_nm = i128::from(rest.nm());
    let mut given = 0;
    let mut sizes: Vec<i128> = children
        .iter()
        .map(|c| match c.size {
            Some(Size::Length(length)) => Ok(i128::from(length.nm())),
            Some(Size::Share(share)) => {
                let part = rest_nm * i128::from(share) / i128::from(shares);
                given += part;
                Ok(part)
            }
            None => Err(format!("front node {} needs a size", c.id)),
        })
        .collect::<Result<_, _>>()?;
    let mut left_over = rest_nm - given;
    for (child, size) in children.iter().zip(&mut sizes) {
        if left_over == 0 {
            break;
        }
        if matches!(child.size, Some(Size::Share(s)) if s > 0) {
            *size += 1;
            left_over -= 1;
        }
    }
    Ok(sizes
        .into_iter()
        .map(|nm| Length::from_nm(i64::try_from(nm).expect("within the opening")))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencabinet_model::{Id, InsertKind, Unknown};

    fn leaf(size: Size) -> FrontNode {
        FrontNode {
            id: Id::new(),
            size: Some(size),
            split: None,
            children: Vec::new(),
            insert: Some(Insert {
                kind: InsertKind::Drawer,
                style: None,
                count: None,
                hinge_side: None,
                shelves: None,
                adjustments: Vec::new(),
                unknown: Unknown::new(),
            }),
            unknown: Unknown::new(),
        }
    }

    fn split(direction: SplitDirection, children: Vec<FrontNode>) -> FrontNode {
        FrontNode {
            id: Id::new(),
            size: None,
            split: Some(direction),
            children,
            insert: None,
            unknown: Unknown::new(),
        }
    }

    fn rect(width: i64, height: i64) -> Rect {
        Rect {
            x: Length::ZERO,
            y: Length::from_nm(100),
            width: Length::from_nm(width),
            height: Length::from_nm(height),
        }
    }

    #[test]
    fn fixed_sizes_come_first_and_shares_split_the_rest_exactly() {
        let tree = split(
            SplitDirection::Horizontal,
            vec![
                leaf(Size::Length(Length::from_nm(30))),
                leaf(Size::Share(1)),
                leaf(Size::Share(2)),
            ],
        );
        let found = openings(&tree, rect(500, 101)).unwrap();
        let heights: Vec<i64> = found.iter().map(|o| o.rect.height.nm()).collect();
        // 71 left: 23.67 and 47.33 round down to 23 and 47; the extra
        // nanometer goes to the first share.
        assert_eq!(heights, [30, 24, 47]);
        // From the top down: the fixed one is highest.
        let bottoms: Vec<i64> = found.iter().map(|o| o.rect.y.nm()).collect();
        assert_eq!(bottoms, [171, 147, 100]);
    }

    #[test]
    fn vertical_splits_run_from_the_left() {
        let tree = split(
            SplitDirection::Vertical,
            vec![leaf(Size::Share(1)), leaf(Size::Share(1))],
        );
        let found = openings(&tree, rect(301, 50)).unwrap();
        let lefts: Vec<(i64, i64)> = found
            .iter()
            .map(|o| (o.rect.x.nm(), o.rect.width.nm()))
            .collect();
        assert_eq!(lefts, [(0, 151), (151, 150)]);
    }

    #[test]
    fn sizes_that_do_not_fit_are_errors() {
        let too_big = split(
            SplitDirection::Horizontal,
            vec![leaf(Size::Length(Length::from_nm(200)))],
        );
        assert!(openings(&too_big, rect(10, 100)).is_err());
        let too_small = split(
            SplitDirection::Horizontal,
            vec![leaf(Size::Length(Length::from_nm(20)))],
        );
        assert!(openings(&too_small, rect(10, 100)).is_err());
    }

    #[test]
    fn dividers_come_out_of_the_opening_before_the_sizes() {
        // 3 openings side by side over 320 with 10 mullions: 300 to share,
        // one fixed at 60 and the rest in halves.
        let tree = split(
            SplitDirection::Vertical,
            vec![
                leaf(Size::Length(Length::from_nm(60))),
                leaf(Size::Share(1)),
                leaf(Size::Share(1)),
            ],
        );
        let dividers = Dividers {
            rail: Length::from_nm(7),
            mullion: Length::from_nm(10),
        };
        let layout = lay_out(&tree, rect(320, 50), dividers).unwrap();
        let spans = |rects: Vec<Rect>| -> Vec<(i64, i64)> {
            rects.iter().map(|r| (r.x.nm(), r.width.nm())).collect()
        };
        assert_eq!(
            spans(layout.openings.iter().map(|o| o.rect).collect()),
            [(0, 60), (70, 120), (200, 120)]
        );
        assert_eq!(
            spans(layout.dividers.iter().map(|d| d.rect).collect()),
            [(60, 10), (190, 10)]
        );
        let indexes: Vec<usize> = layout.dividers.iter().map(|d| d.index).collect();
        assert_eq!(indexes, [1, 2]);
        // Too many mullions for the width.
        assert!(lay_out(&tree, rect(15, 50), dividers).is_err());
    }

    #[test]
    fn neighbors_end_one_gap_apart_even_for_an_odd_gap() {
        let gap = Length::from_nm(3);
        let lower = rect(100, 50).inset(gap);
        let upper = Rect {
            y: Length::from_nm(150),
            ..rect(100, 50)
        }
        .inset(gap);
        assert_eq!(upper.y - (lower.y + lower.height), gap);
    }
}
