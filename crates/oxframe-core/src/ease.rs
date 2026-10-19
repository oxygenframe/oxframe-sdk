use std::f64::consts::PI;

/// Maps linear progress `t ∈ [0, 1]` to eased progress.
///
/// Implemented by [`CubicBezier`] and by any `Fn(f64) -> f64`, which is how
/// [`elastic_out`], [`bounce_out`] and friends plug in.
pub trait Ease {
    /// Eased progress for `t` in 0..=1 (may overshoot for back/elastic).
    fn ease(&self, t: f64) -> f64;
}

impl<F: Fn(f64) -> f64> Ease for F {
    fn ease(&self, t: f64) -> f64 {
        self(t)
    }
}

/// CSS-style cubic Bézier easing with control points `(x1, y1)`, `(x2, y2)`;
/// the end points are fixed at `(0, 0)` and `(1, 1)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    /// First control point, x (0..=1).
    pub x1: f64,
    /// First control point, y.
    pub y1: f64,
    /// Second control point, x (0..=1).
    pub x2: f64,
    /// Second control point, y.
    pub y2: f64,
}

impl CubicBezier {
    const NEWTON_ITERATIONS: usize = 8;
    const PRECISION: f64 = 1e-7;

    /// Curve with control points `(x1, y1)` and `(x2, y2)`, like CSS `cubic-bezier()`.
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// From `[x1, y1, x2, y2]`, the format of `meta.json`.
    pub const fn from_array([x1, y1, x2, y2]: [f64; 4]) -> Self {
        Self::new(x1, y1, x2, y2)
    }

    /// As `[x1, y1, x2, y2]`.
    pub const fn to_array(self) -> [f64; 4] {
        [self.x1, self.y1, self.x2, self.y2]
    }

    /// Polynomial coefficients `(a, b, c)` of `B(s) = ((a·s + b)·s + c)·s`.
    fn coeffs(p1: f64, p2: f64) -> (f64, f64, f64) {
        let c = 3.0 * p1;
        let b = 3.0 * (p2 - p1) - c;
        (1.0 - c - b, b, c)
    }

    #[cfg(test)]
    fn bezier(p1: f64, p2: f64, s: f64) -> f64 {
        let (a, b, c) = Self::coeffs(p1, p2);
        ((a * s + b) * s + c) * s
    }

    /// Curve parameter `s` such that `x(s) == x`.
    fn solve_s(&self, x: f64) -> f64 {
        let (a, b, c) = Self::coeffs(self.x1, self.x2);
        let eval = |s: f64| ((a * s + b) * s + c) * s;
        let mut s = x;
        for _ in 0..Self::NEWTON_ITERATIONS {
            let err = eval(s) - x;
            if err.abs() < Self::PRECISION {
                return s;
            }
            let slope = (3.0 * a * s + 2.0 * b) * s + c;
            if slope.abs() < 1e-6 {
                break;
            }
            s = (s - err / slope).clamp(0.0, 1.0);
        }
        // Newton failed to converge (flat slope): x(s) is monotonic for
        // x1, x2 ∈ [0, 1], so bisection always works.
        let (mut lo, mut hi) = (0.0, 1.0);
        s = x;
        for _ in 0..40 {
            let v = eval(s);
            if (v - x).abs() < Self::PRECISION {
                break;
            }
            if v < x {
                lo = s;
            } else {
                hi = s;
            }
            s = 0.5 * (lo + hi);
        }
        s
    }
}

impl Ease for CubicBezier {
    fn ease(&self, t: f64) -> f64 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        if self.x1 == self.y1 && self.x2 == self.y2 {
            return t;
        }
        let (a, b, c) = Self::coeffs(self.y1, self.y2);
        let s = self.solve_s(t);
        ((a * s + b) * s + c) * s
    }
}

/// Named Bézier presets (values follow easings.net / CSS).
pub mod presets {
    use super::CubicBezier;

    /// Constant speed.
    pub const LINEAR: CubicBezier = CubicBezier::new(0.0, 0.0, 1.0, 1.0);
    /// CSS `ease`.
    pub const EASE: CubicBezier = CubicBezier::new(0.25, 0.1, 0.25, 1.0);
    /// CSS `ease-in`.
    pub const EASE_IN: CubicBezier = CubicBezier::new(0.42, 0.0, 1.0, 1.0);
    /// CSS `ease-out`.
    pub const EASE_OUT: CubicBezier = CubicBezier::new(0.0, 0.0, 0.58, 1.0);
    /// CSS `ease-in-out`.
    pub const EASE_IN_OUT: CubicBezier = CubicBezier::new(0.42, 0.0, 0.58, 1.0);

    /// Gentle start.
    pub const IN_SINE: CubicBezier = CubicBezier::new(0.12, 0.0, 0.39, 0.0);
    /// Gentle stop.
    pub const OUT_SINE: CubicBezier = CubicBezier::new(0.61, 1.0, 0.88, 1.0);
    /// Gentle start and stop.
    pub const IN_OUT_SINE: CubicBezier = CubicBezier::new(0.37, 0.0, 0.63, 1.0);
    /// Accelerates (quadratic).
    pub const IN_QUAD: CubicBezier = CubicBezier::new(0.11, 0.0, 0.5, 0.0);
    /// Decelerates (quadratic).
    pub const OUT_QUAD: CubicBezier = CubicBezier::new(0.5, 1.0, 0.89, 1.0);
    /// Accelerates, then decelerates (quadratic).
    pub const IN_OUT_QUAD: CubicBezier = CubicBezier::new(0.45, 0.0, 0.55, 1.0);
    /// Accelerates (cubic).
    pub const IN_CUBIC: CubicBezier = CubicBezier::new(0.32, 0.0, 0.67, 0.0);
    /// Decelerates (cubic).
    pub const OUT_CUBIC: CubicBezier = CubicBezier::new(0.33, 1.0, 0.68, 1.0);
    /// Accelerates, then decelerates (cubic); the default.
    pub const IN_OUT_CUBIC: CubicBezier = CubicBezier::new(0.65, 0.0, 0.35, 1.0);
    /// Accelerates (quartic).
    pub const IN_QUART: CubicBezier = CubicBezier::new(0.5, 0.0, 0.75, 0.0);
    /// Decelerates (quartic).
    pub const OUT_QUART: CubicBezier = CubicBezier::new(0.25, 1.0, 0.5, 1.0);
    /// Accelerates, then decelerates (quartic).
    pub const IN_OUT_QUART: CubicBezier = CubicBezier::new(0.76, 0.0, 0.24, 1.0);
    /// Very slow start.
    pub const IN_EXPO: CubicBezier = CubicBezier::new(0.7, 0.0, 0.84, 0.0);
    /// Very fast start, long settle.
    pub const OUT_EXPO: CubicBezier = CubicBezier::new(0.16, 1.0, 0.3, 1.0);
    /// Very slow start and stop.
    pub const IN_OUT_EXPO: CubicBezier = CubicBezier::new(0.87, 0.0, 0.13, 1.0);
    /// Pulls back before starting.
    pub const IN_BACK: CubicBezier = CubicBezier::new(0.36, 0.0, 0.66, -0.56);
    /// Overshoots, then settles.
    pub const OUT_BACK: CubicBezier = CubicBezier::new(0.34, 1.56, 0.64, 1.0);
    /// Pulls back and overshoots.
    pub const IN_OUT_BACK: CubicBezier = CubicBezier::new(0.68, -0.6, 0.32, 1.6);

    /// Default easing of every tween.
    pub const DEFAULT: CubicBezier = IN_OUT_CUBIC;
}

/// Overshoots and oscillates into place, like a spring.
pub fn elastic_out(t: f64) -> f64 {
    if t <= 0.0 || t >= 1.0 {
        return t.clamp(0.0, 1.0);
    }
    let c4 = 2.0 * PI / 3.0;
    2f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
}

/// Oscillates before leaving, like a spring being wound.
pub fn elastic_in(t: f64) -> f64 {
    1.0 - elastic_out(1.0 - t)
}

/// Spring at both ends.
pub fn elastic_in_out(t: f64) -> f64 {
    if t <= 0.0 || t >= 1.0 {
        return t.clamp(0.0, 1.0);
    }
    let c5 = 2.0 * PI / 4.5;
    if t < 0.5 {
        -(2f64.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0
    } else {
        2f64.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * c5).sin() / 2.0 + 1.0
    }
}

/// Bounces into place, like a dropped ball.
pub fn bounce_out(t: f64) -> f64 {
    const N: f64 = 7.5625;
    const D: f64 = 2.75;
    let t = t.clamp(0.0, 1.0);
    if t < 1.0 / D {
        N * t * t
    } else if t < 2.0 / D {
        let t = t - 1.5 / D;
        N * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        N * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        N * t * t + 0.984_375
    }
}

/// Bounces before leaving.
pub fn bounce_in(t: f64) -> f64 {
    1.0 - bounce_out(1.0 - t)
}

/// Bounces at both ends.
pub fn bounce_in_out(t: f64) -> f64 {
    if t < 0.5 {
        (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0
    } else {
        (1.0 + bounce_out(2.0 * t - 1.0)) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::presets::*;
    use super::*;

    /// Reference by dense bisection, independent of the Newton solver.
    fn reference(c: &CubicBezier, x: f64) -> f64 {
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if CubicBezier::bezier(c.x1, c.x2, mid) < x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        CubicBezier::bezier(c.y1, c.y2, 0.5 * (lo + hi))
    }

    #[test]
    fn bezier_matches_reference_within_tolerance() {
        let curves = [
            EASE,
            EASE_IN,
            EASE_OUT,
            EASE_IN_OUT,
            IN_OUT_CUBIC,
            IN_EXPO,
            OUT_EXPO,
            IN_OUT_EXPO,
            IN_OUT_BACK,
            OUT_BACK,
            IN_BACK,
            CubicBezier::new(1.0, 0.0, 0.0, 1.0),
            CubicBezier::new(0.0, 1.0, 1.0, 0.0),
        ];
        for c in curves {
            for i in 0..=1000 {
                let x = i as f64 / 1000.0;
                let got = c.ease(x);
                let want = reference(&c, x);
                assert!((got - want).abs() <= 1e-4, "{c:?} at {x}: {got} vs {want}");
            }
        }
    }

    #[test]
    fn endpoints_and_linear() {
        for c in [LINEAR, EASE, IN_OUT_BACK] {
            assert_eq!(c.ease(0.0), 0.0);
            assert_eq!(c.ease(1.0), 1.0);
        }
        assert_eq!(LINEAR.ease(0.3), 0.3);
        for f in [
            elastic_in,
            elastic_out,
            elastic_in_out,
            bounce_in,
            bounce_out,
            bounce_in_out,
        ] {
            assert!(f.ease(0.0).abs() < 1e-9);
            assert!((f.ease(1.0) - 1.0).abs() < 1e-9);
        }
    }
}
