// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A camera that orbits a point: drag to turn around it, drag with the
//! other button to slide it, scroll to come closer.

use crate::math::{Mat4, Ray, Vec3};

/// An orbiting camera, in millimeters and radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// The point it looks at and turns around.
    pub target: Vec3,
    /// How far it has turned around the vertical, from looking north.
    pub yaw: f32,
    /// How far above the horizontal it looks down from.
    pub pitch: f32,
    /// From the camera to the target.
    pub distance: f32,
    /// How much of the scene the view's height takes, in radians.
    pub fov: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 450.0, 0.0),
            yaw: -0.6,
            pitch: 0.45,
            distance: 3000.0,
            fov: 0.75,
        }
    }
}

/// The lowest and highest the camera may look down from.
const PITCH: (f32, f32) = (-0.2, 1.5);

impl Camera {
    /// Where the camera is.
    #[must_use]
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        // At yaw 0 the camera stands south of the target, looking north
        // (toward −z, the top of a plan).
        self.target + Vec3::new(sy * cp, sp, cy * cp) * self.distance
    }

    /// The view and projection for an image `aspect` wide for its height.
    #[must_use]
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        let near = (self.distance * 0.01).clamp(5.0, 200.0);
        let far = self.distance * 20.0 + 20_000.0;
        Mat4::perspective(self.fov, aspect.max(0.01), near, far)
            .then(Mat4::look_at(self.eye(), self.target))
    }

    /// Turns around the target by a drag of `dx` and `dy` pixels.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(PITCH.0, PITCH.1);
    }

    /// Slides the camera and its target sideways and up by a drag of `dx`
    /// and `dy` pixels in a view `height` pixels high.
    pub fn pan(&mut self, dx: f32, dy: f32, height: f32) {
        let per_pixel = 2.0 * self.distance * (self.fov / 2.0).tan() / height.max(1.0);
        let forward = (self.target - self.eye()).normalized();
        let right = forward.cross(Vec3::UP).normalized();
        let up = right.cross(forward);
        self.target = self.target + (right * -dx + up * dy) * per_pixel;
    }

    /// Comes closer by `steps` notches of the wheel, away when negative.
    pub fn zoom(&mut self, steps: f32) {
        self.distance = (self.distance * 0.88_f32.powf(steps)).clamp(100.0, 100_000.0);
    }

    /// Looks at the box from `low` to `high` so that it fills the view, from
    /// where the camera stands now.
    pub fn frame(&mut self, low: Vec3, high: Vec3) {
        self.target = (low + high) * 0.5;
        let radius = ((high - low) * 0.5).length().max(100.0);
        self.distance = radius / (self.fov / 2.0).sin() * 1.05;
    }

    /// The ray through the pixel `(x, y)` of a view `width` by `height`.
    #[must_use]
    pub fn ray(&self, x: f32, y: f32, width: f32, height: f32) -> Ray {
        let eye = self.eye();
        let forward = (self.target - eye).normalized();
        let right = forward.cross(Vec3::UP).normalized();
        let up = right.cross(forward);
        let half = (self.fov / 2.0).tan();
        let aspect = width / height.max(1.0);
        let px = (2.0 * x / width.max(1.0) - 1.0) * half * aspect;
        let py = (1.0 - 2.0 * y / height.max(1.0)) * half;
        Ray {
            origin: eye,
            direction: (forward + right * px + up * py).normalized(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ray_through_the_middle_reaches_the_target() {
        let camera = Camera::default();
        let ray = camera.ray(400.0, 300.0, 800.0, 600.0);
        let reached = ray.origin + ray.direction * camera.distance;
        assert!((reached - camera.target).length() < 0.5);
    }

    #[test]
    fn framing_puts_the_box_in_the_middle() {
        let mut camera = Camera::default();
        camera.frame(Vec3::new(0.0, 0.0, 0.0), Vec3::new(762.0, 876.0, 610.0));
        assert_eq!(camera.target, Vec3::new(381.0, 438.0, 305.0));
        assert!(camera.distance > 1000.0);
    }
}
