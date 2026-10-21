//! Vectors, rotations and matrices for 3D scenes. They live here only: the
//! 2D crates never see them.

use std::ops::{Add, Mul, Neg, Sub};

use oxframe_core::Lerp;

/// A point or direction in 3D. Y is up, the camera looks down -Z by default.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    /// Right.
    pub x: f64,
    /// Up.
    pub y: f64,
    /// Toward the viewer.
    pub z: f64,
}

impl Vec3 {
    /// (0, 0, 0).
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    /// (1, 1, 1).
    pub const ONE: Vec3 = Vec3::new(1.0, 1.0, 1.0);
    /// The X axis.
    pub const X: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    /// The Y axis, up.
    pub const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);
    /// The Z axis.
    pub const Z: Vec3 = Vec3::new(0.0, 0.0, 1.0);

    /// A vector from its parts.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// The same value in every part.
    pub const fn splat(v: f64) -> Self {
        Self::new(v, v, v)
    }

    /// Dot product.
    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    /// Cross product.
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    /// Length.
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    /// The same direction with length 1 (zero stays zero).
    pub fn normalize(self) -> Vec3 {
        let l = self.length();
        if l > 1e-12 { self * (1.0 / l) } else { self }
    }

    /// Part-wise product.
    pub fn mul_each(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }
}

impl From<(f64, f64, f64)> for Vec3 {
    fn from((x, y, z): (f64, f64, f64)) -> Self {
        Vec3::new(x, y, z)
    }
}

impl From<[f64; 3]> for Vec3 {
    fn from([x, y, z]: [f64; 3]) -> Self {
        Vec3::new(x, y, z)
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, k: f64) -> Vec3 {
        Vec3::new(self.x * k, self.y * k, self.z * k)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

impl Lerp for Vec3 {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        *self + (*to - *self) * t
    }
}

/// A rotation. Animates along the shortest arc (slerp).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    /// Vector part.
    pub x: f64,
    /// Vector part.
    pub y: f64,
    /// Vector part.
    pub z: f64,
    /// Scalar part.
    pub w: f64,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    /// No rotation.
    pub const IDENTITY: Quat = Quat {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    /// `degrees` around `axis`, counterclockwise when the axis points at you.
    pub fn from_axis_angle(axis: Vec3, degrees: f64) -> Quat {
        let a = axis.normalize();
        let (s, c) = (degrees.to_radians() / 2.0).sin_cos();
        Quat {
            x: a.x * s,
            y: a.y * s,
            z: a.z * s,
            w: c,
        }
    }

    /// Rotation by Euler angles in degrees: X first, then Y, then Z.
    pub fn from_euler(degrees: Vec3) -> Quat {
        Quat::from_axis_angle(Vec3::Z, degrees.z)
            * Quat::from_axis_angle(Vec3::Y, degrees.y)
            * Quat::from_axis_angle(Vec3::X, degrees.x)
    }

    /// `v` rotated.
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let q = Vec3::new(self.x, self.y, self.z);
        let t = q.cross(v) * 2.0;
        v + t * self.w + q.cross(t)
    }

    fn dot(self, o: Quat) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z + self.w * o.w
    }

    fn normalize(self) -> Quat {
        let l = self.dot(self).sqrt();
        if l < 1e-12 {
            return Quat::IDENTITY;
        }
        Quat {
            x: self.x / l,
            y: self.y / l,
            z: self.z / l,
            w: self.w / l,
        }
    }
}

impl Mul for Quat {
    type Output = Quat;
    /// `self * o` rotates by `o` first, then by `self`.
    fn mul(self, o: Quat) -> Quat {
        Quat {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }
}

impl Lerp for Quat {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        let mut to = *to;
        let mut d = self.dot(to);
        if d < 0.0 {
            to = Quat {
                x: -to.x,
                y: -to.y,
                z: -to.z,
                w: -to.w,
            };
            d = -d;
        }
        if d > 0.9995 {
            let q = Quat {
                x: self.x + (to.x - self.x) * t,
                y: self.y + (to.y - self.y) * t,
                z: self.z + (to.z - self.z) * t,
                w: self.w + (to.w - self.w) * t,
            };
            return q.normalize();
        }
        let theta = d.clamp(-1.0, 1.0).acos();
        let (a, b) = (
            ((1.0 - t) * theta).sin() / theta.sin(),
            (t * theta).sin() / theta.sin(),
        );
        Quat {
            x: self.x * a + to.x * b,
            y: self.y * a + to.y * b,
            z: self.z * a + to.z * b,
            w: self.w * a + to.w * b,
        }
    }
}

/// A 4 × 4 matrix, column-major, for transforms and projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Mat4(pub [[f64; 4]; 4]);

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    /// Scale, then rotate, then translate.
    pub fn trs(t: Vec3, r: Quat, s: Vec3) -> Mat4 {
        let x = r.rotate(Vec3::X) * s.x;
        let y = r.rotate(Vec3::Y) * s.y;
        let z = r.rotate(Vec3::Z) * s.z;
        Mat4([
            [x.x, x.y, x.z, 0.0],
            [y.x, y.y, y.z, 0.0],
            [z.x, z.y, z.z, 0.0],
            [t.x, t.y, t.z, 1.0],
        ])
    }

    /// View matrix of a camera at `eye` looking at `target`.
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let f = (target - eye).normalize();
        let mut s = f.cross(up).normalize();
        if s.length() < 1e-9 {
            s = f.cross(Vec3::Z).normalize();
        }
        let u = s.cross(f);
        Mat4([
            [s.x, u.x, -f.x, 0.0],
            [s.y, u.y, -f.y, 0.0],
            [s.z, u.z, -f.z, 0.0],
            [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
        ])
    }

    /// Perspective projection; depth maps `near..far` to `0..1`.
    pub fn perspective(fov_y_degrees: f64, aspect: f64, near: f64, far: f64) -> Mat4 {
        let f = 1.0 / (fov_y_degrees.to_radians() / 2.0).tan();
        Mat4([
            [f / aspect, 0.0, 0.0, 0.0],
            [0.0, f, 0.0, 0.0],
            [0.0, 0.0, far / (near - far), -1.0],
            [0.0, 0.0, near * far / (near - far), 0.0],
        ])
    }

    pub fn mul(&self, o: &Mat4) -> Mat4 {
        let mut out = [[0.0; 4]; 4];
        for (c, col) in out.iter_mut().enumerate() {
            for (r, v) in col.iter_mut().enumerate() {
                *v = (0..4).map(|k| self.0[k][r] * o.0[c][k]).sum();
            }
        }
        Mat4(out)
    }

    /// `(x, y, z, 1)` through the matrix, before the perspective divide.
    pub fn project(&self, p: Vec3) -> [f64; 4] {
        let m = &self.0;
        let mut out = [0.0; 4];
        for (r, v) in out.iter_mut().enumerate() {
            *v = m[0][r] * p.x + m[1][r] * p.y + m[2][r] * p.z + m[3][r];
        }
        out
    }

    /// A point through the matrix (affine part only).
    pub fn point(&self, p: Vec3) -> Vec3 {
        let [x, y, z, _] = self.project(p);
        Vec3::new(x, y, z)
    }

    /// A direction through the matrix, without translation.
    pub fn vector(&self, v: Vec3) -> Vec3 {
        let m = &self.0;
        Vec3::new(
            m[0][0] * v.x + m[1][0] * v.y + m[2][0] * v.z,
            m[0][1] * v.x + m[1][1] * v.y + m[2][1] * v.z,
            m[0][2] * v.x + m[1][2] * v.y + m[2][2] * v.z,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-9
    }

    #[test]
    fn rotations_turn_the_axes() {
        let q = Quat::from_axis_angle(Vec3::Y, 90.0);
        assert!(near(q.rotate(Vec3::X), -Vec3::Z));
        let q = Quat::from_euler(Vec3::new(0.0, 0.0, 90.0));
        assert!(near(q.rotate(Vec3::X), Vec3::Y));
        let half = Quat::IDENTITY.lerp(&Quat::from_axis_angle(Vec3::Y, 90.0), 0.5);
        assert!(near(
            half.rotate(Vec3::X),
            Quat::from_axis_angle(Vec3::Y, 45.0).rotate(Vec3::X)
        ));
    }

    #[test]
    fn the_camera_sees_the_target_in_front() {
        let view = Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let p = view.point(Vec3::ZERO);
        assert!((p.z + 5.0).abs() < 1e-9, "{p:?}");
        let proj = Mat4::perspective(60.0, 1.0, 0.1, 100.0).mul(&view);
        let [_, _, z, w] = proj.project(Vec3::ZERO);
        assert!(w > 0.0 && (0.0..1.0).contains(&(z / w)));
    }

    #[test]
    fn trs_scales_rotates_then_moves() {
        let m = Mat4::trs(
            Vec3::new(1.0, 0.0, 0.0),
            Quat::from_axis_angle(Vec3::Z, 90.0),
            Vec3::splat(2.0),
        );
        assert!(near(m.point(Vec3::X), Vec3::new(1.0, 2.0, 0.0)));
    }
}
