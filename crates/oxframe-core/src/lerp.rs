use kurbo::{Affine, Point, Vec2};

use crate::Color;

/// Linear interpolation; `t` is already eased and may leave `[0, 1]`
/// (e.g. with back easings).
pub trait Lerp: Sized {
    /// Value at `t` between `self` (0) and `to` (1); `t` may leave 0..=1.
    fn lerp(&self, to: &Self, t: f64) -> Self;
}

impl Lerp for f64 {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        self + (to - self) * t
    }
}

impl Lerp for f32 {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        self + (to - self) * t as f32
    }
}

impl Lerp for Vec2 {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        Vec2::new(self.x.lerp(&to.x, t), self.y.lerp(&to.y, t))
    }
}

impl Lerp for Point {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        Point::new(self.x.lerp(&to.x, t), self.y.lerp(&to.y, t))
    }
}

/// Interpolates in OKLab so that midpoints keep perceived lightness.
impl Lerp for Color {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        if self == to {
            return *self;
        }
        let a = self.to_oklab();
        let b = to.to_oklab();
        let t32 = t as f32;
        let mix = |i: usize| a[i] + (b[i] - a[i]) * t32;
        Color::from_oklab(
            [mix(0), mix(1), mix(2)],
            self.a.lerp(&to.a, t).clamp(0.0, 1.0),
        )
    }
}

impl<T: Lerp> Lerp for Option<T>
where
    T: Clone,
{
    /// `None` on either side snaps at the midpoint.
    fn lerp(&self, to: &Self, t: f64) -> Self {
        match (self, to) {
            (Some(a), Some(b)) => Some(a.lerp(b, t)),
            _ if t < 0.5 => self.clone(),
            _ => to.clone(),
        }
    }
}

/// Element-wise when lengths match; otherwise snaps at the midpoint.
impl<T: Lerp + Clone> Lerp for Vec<T> {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        if self.len() == to.len() {
            self.iter().zip(to).map(|(a, b)| a.lerp(b, t)).collect()
        } else if t < 0.5 {
            self.clone()
        } else {
            to.clone()
        }
    }
}

/// Typewriter: keeps the common prefix, deletes the rest of `self` during
/// the first part of the tween and types the rest of `to` afterwards. The
/// time split is proportional to the number of characters.
impl Lerp for String {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        if t <= 0.0 {
            return self.clone();
        }
        if t >= 1.0 {
            return to.clone();
        }
        let a: Vec<char> = self.chars().collect();
        let b: Vec<char> = to.chars().collect();
        let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        let (del, add) = (a.len() - common, b.len() - common);
        let total = del + add;
        if total == 0 {
            return to.clone();
        }
        let steps = (t * total as f64).floor() as usize;
        if steps < del {
            a[..a.len() - steps].iter().collect()
        } else {
            b[..common + (steps - del)].iter().collect()
        }
    }
}

/// Decomposed 2D transform. Interpolates component-wise, so rotations turn
/// instead of shearing through the matrix midpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Offset.
    pub translation: Vec2,
    /// Radians, clockwise in screen space (y down).
    pub rotation: f64,
    /// Scale per axis.
    pub scale: Vec2,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// No translation, rotation or scaling.
    pub const IDENTITY: Transform = Transform {
        translation: Vec2::ZERO,
        rotation: 0.0,
        scale: Vec2::new(1.0, 1.0),
    };

    /// The equivalent matrix: translate · rotate · scale.
    pub fn to_affine(&self) -> Affine {
        Affine::translate(self.translation)
            * Affine::rotate(self.rotation)
            * Affine::scale_non_uniform(self.scale.x, self.scale.y)
    }
}

impl Lerp for Transform {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        Transform {
            translation: Lerp::lerp(&self.translation, &to.translation, t),
            rotation: self.rotation.lerp(&to.rotation, t),
            scale: Lerp::lerp(&self.scale, &to.scale, t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_and_vectors() {
        assert_eq!(2.0f64.lerp(&4.0, 0.5), 3.0);
        assert_eq!(
            Lerp::lerp(&Vec2::new(0.0, 10.0), &Vec2::new(10.0, 0.0), 0.25),
            Vec2::new(2.5, 7.5)
        );
    }

    #[test]
    fn color_endpoints_and_midpoint() {
        let red = Color::from_hex("#ff0000").unwrap();
        let blue = Color::from_hex("#0000ff").unwrap();
        assert_eq!(red.lerp(&blue, 0.0).to_rgba8(), red.to_rgba8());
        assert_eq!(red.lerp(&blue, 1.0).to_rgba8(), blue.to_rgba8());
        let mid = red.lerp(&blue, 0.5).to_oklab();
        let (a, b) = (red.to_oklab(), blue.to_oklab());
        assert!((mid[0] - (a[0] + b[0]) / 2.0).abs() < 1e-3);
    }

    #[test]
    fn string_typewriter() {
        let a = String::from("Hello");
        let b = String::from("Help me");
        // common "Hel", delete "lo" (2), type "p me" (4): 6 steps.
        assert_eq!(a.lerp(&b, 0.0), "Hello");
        assert_eq!(a.lerp(&b, 1.0 / 6.0 + 1e-9), "Hell");
        assert_eq!(a.lerp(&b, 2.0 / 6.0 + 1e-9), "Hel");
        assert_eq!(a.lerp(&b, 3.0 / 6.0 + 1e-9), "Help");
        assert_eq!(a.lerp(&b, 1.0), "Help me");
        assert_eq!(String::new().lerp(&"Привет".to_string(), 0.5), "При");
    }

    #[test]
    fn transform_rotates_through_angle() {
        let a = Transform::IDENTITY;
        let b = Transform {
            rotation: std::f64::consts::PI,
            ..a
        };
        let mid = a.lerp(&b, 0.5);
        assert!((mid.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert_eq!(mid.scale, Vec2::new(1.0, 1.0));
    }
}
