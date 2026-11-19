// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where each part sits in its product: the transform of RFC-0001 §9, so
//! that a product can be drawn in 3D, checked for parts that run into each
//! other, and its machining shown where it is.
//!
//! A product has its own frame, in nanometers like everything else: `x`
//! to the right seen from the front, `y` up from the bottom of the product
//! (the floor, for a cabinet on a toe kick), and `z` forward from the
//! product's back, toward someone standing in front of it. The origin is
//! the left, back, bottom corner of the product's outline: the cabinet is
//! `W` wide, `D` deep and `H` high from there, and its fronts stand in
//! front of `z = D`.
//!
//! A part is a box. Its own coordinates are `u` along its length from the
//! `x0` end, `v` across its width from the `y0` edge, and `w` through its
//! thickness from face B to face A; machining uses `u` and `v` as its `x`
//! and `y`. A [`Placement`] says where the part's corner `(0, 0, 0)` is in
//! the product and which of the product's axes `u`, `v` and `w` run along.
//! Every part lies along the product's axes, so the three are always some
//! order of `x`, `y` and `z`, each one way or the other: a right side is
//! the mirror of a left side, not a left side turned around.

use opencabinet_model::Length;

/// One of the product's axes, one way or the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    /// To the right.
    X,
    /// Up.
    Y,
    /// Forward.
    Z,
    /// To the left.
    NegX,
    /// Down.
    NegY,
    /// Back.
    NegZ,
}

impl Axis {
    /// The axis as a vector of `-1`, `0` and `1`.
    #[must_use]
    pub const fn unit(self) -> [i64; 3] {
        match self {
            Self::X => [1, 0, 0],
            Self::Y => [0, 1, 0],
            Self::Z => [0, 0, 1],
            Self::NegX => [-1, 0, 0],
            Self::NegY => [0, -1, 0],
            Self::NegZ => [0, 0, -1],
        }
    }
}

/// Where a part sits in its product.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Placement {
    /// The part's corner `(0, 0, 0)`: the `x0` end, the `y0` edge, on
    /// face B.
    pub origin: [Length; 3],
    /// The product's axes along the part's length, width and thickness.
    pub axes: [Axis; 3],
}

impl Default for Placement {
    /// At the product's origin, its length along `x`, its width along `z`
    /// and face A up, as a bottom lies.
    fn default() -> Self {
        Self {
            origin: [Length::ZERO; 3],
            axes: [Axis::X, Axis::Z, Axis::Y],
        }
    }
}

impl Placement {
    /// A part with its corner at `(x, y, z)` and its length, width and
    /// thickness along `axes`.
    #[must_use]
    pub const fn new(x: Length, y: Length, z: Length, axes: [Axis; 3]) -> Self {
        Self {
            origin: [x, y, z],
            axes,
        }
    }

    /// The point `(u, v, w)` of the part, in the product.
    #[must_use]
    pub fn point(&self, u: Length, v: Length, w: Length) -> [Length; 3] {
        let mut point = self.origin;
        for (axis, along) in self.axes.iter().zip([u, v, w]) {
            for (p, d) in point.iter_mut().zip(axis.unit()) {
                *p += along * d;
            }
        }
        point
    }

    /// The lowest and highest corners of a part `length` by `width` by
    /// `thickness` placed here.
    #[must_use]
    pub fn bounds(
        &self,
        length: Length,
        width: Length,
        thickness: Length,
    ) -> ([Length; 3], [Length; 3]) {
        let (a, b) = (self.origin, self.point(length, width, thickness));
        let mut low = a;
        let mut high = b;
        for i in 0..3 {
            if low[i] > high[i] {
                std::mem::swap(&mut low[i], &mut high[i]);
            }
        }
        (low, high)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mm(value: i64) -> Length {
        Length::from_mm(value)
    }

    #[test]
    fn a_placement_runs_the_part_along_its_axes() {
        // A right side 700 high and 500 deep, 18 thick, its face A toward
        // the inside: its corner is on the outside of a 600 wide cabinet.
        let side = Placement::new(mm(600), mm(100), mm(0), [Axis::Y, Axis::Z, Axis::NegX]);
        assert_eq!(
            side.point(mm(700), mm(0), mm(18)),
            [mm(582), mm(800), mm(0)]
        );
        assert_eq!(
            side.bounds(mm(700), mm(500), mm(18)),
            ([mm(582), mm(100), mm(0)], [mm(600), mm(800), mm(500)])
        );
    }
}
