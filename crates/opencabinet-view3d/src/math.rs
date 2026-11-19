// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The little vector and matrix arithmetic the viewer needs, in `f32`
//! millimeters: a room is far below the size where `f32` loses a tenth of
//! a millimeter.

use std::ops::{Add, Mul, Neg, Sub};

/// A point or a direction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    /// To the right in plan.
    pub x: f32,
    /// Up.
    pub y: f32,
    /// Down the plan, toward someone looking at it.
    pub z: f32,
}

impl Vec3 {
    /// The point `(x, y, z)`.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Up.
    pub const UP: Self = Self::new(0.0, 1.0, 0.0);

    /// The dot product.
    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// The cross product.
    #[must_use]
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    /// How long it is.
    #[must_use]
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    /// The same direction, one long; zero stays zero.
    #[must_use]
    pub fn normalized(self) -> Self {
        let length = self.length();
        if length > 0.0 {
            self * (1.0 / length)
        } else {
            self
        }
    }

    /// Each coordinate the smaller of the two.
    #[must_use]
    pub fn min(self, o: Self) -> Self {
        Self::new(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }

    /// Each coordinate the larger of the two.
    #[must_use]
    pub fn max(self, o: Self) -> Self {
        Self::new(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }

    /// As an array, for the GPU.
    #[must_use]
    pub const fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, k: f32) -> Self {
        Self::new(self.x * k, self.y * k, self.z * k)
    }
}

impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

/// A 4 × 4 matrix, column by column as the GPU reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [[f32; 4]; 4]);

impl Mat4 {
    /// A camera at `eye` looking at `target`, `y` up, right-handed.
    #[must_use]
    pub fn look_at(eye: Vec3, target: Vec3) -> Self {
        let f = (target - eye).normalized();
        let s = f.cross(Vec3::UP).normalized();
        let u = s.cross(f);
        Self([
            [s.x, u.x, -f.x, 0.0],
            [s.y, u.y, -f.y, 0.0],
            [s.z, u.z, -f.z, 0.0],
            [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
        ])
    }

    /// A perspective `fov` radians high, depth from 0 at `near` to 1 at
    /// `far`, as wgpu has it.
    #[must_use]
    pub fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / (fov / 2.0).tan();
        let range = far / (near - far);
        Self([
            [f / aspect, 0.0, 0.0, 0.0],
            [0.0, f, 0.0, 0.0],
            [0.0, 0.0, range, -1.0],
            [0.0, 0.0, range * near, 0.0],
        ])
    }

    /// `self` after `other`: `self × other`.
    #[must_use]
    pub fn then(self, other: Self) -> Self {
        let (a, b) = (self.0, other.0);
        let mut out = [[0.0; 4]; 4];
        for (c, column) in out.iter_mut().enumerate() {
            for (r, cell) in column.iter_mut().enumerate() {
                *cell = (0..4).map(|k| a[k][r] * b[c][k]).sum();
            }
        }
        Self(out)
    }
}

/// A ray from `origin` along `direction`, which is one long.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    /// Where it starts.
    pub origin: Vec3,
    /// Where it goes.
    pub direction: Vec3,
}

impl Ray {
    /// How far along the ray it enters the box from `low` to `high`, if it
    /// meets it in front of its origin.
    #[must_use]
    pub fn hits_box(&self, low: Vec3, high: Vec3) -> Option<f32> {
        let (mut near, mut far) = (0.0_f32, f32::INFINITY);
        for (o, d, l, h) in [
            (self.origin.x, self.direction.x, low.x, high.x),
            (self.origin.y, self.direction.y, low.y, high.y),
            (self.origin.z, self.direction.z, low.z, high.z),
        ] {
            if d.abs() < 1e-12 {
                if o < l || o > h {
                    return None;
                }
                continue;
            }
            let (a, b) = ((l - o) / d, (h - o) / d);
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
        Some(near)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ray_enters_a_box_on_its_near_face() {
        let ray = Ray {
            origin: Vec3::new(0.0, 0.0, 10.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        let hit = ray.hits_box(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(hit, Some(9.0));
        let miss = ray.hits_box(Vec3::new(2.0, 2.0, -1.0), Vec3::new(3.0, 3.0, 1.0));
        assert_eq!(miss, None);
    }

    #[test]
    fn the_camera_looks_down_its_negative_z() {
        // A point in front of the camera lands in the middle of the view,
        // between the near and far planes.
        let view = Mat4::look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::default());
        let projection = Mat4::perspective(1.0, 1.0, 1.0, 100.0);
        let m = projection.then(view).0;
        let clip: Vec<f32> = (0..4).map(|r| m[3][r]).collect();
        let depth = clip[2] / clip[3];
        assert!(clip[0].abs() < 1e-6 && clip[1].abs() < 1e-6);
        assert!(depth > 0.0 && depth < 1.0, "{depth}");
    }
}
