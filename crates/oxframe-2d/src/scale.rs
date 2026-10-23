//! The scale of a node: one number for both axes, or a pair.

use std::ops::Deref;

use oxframe_core::{Seconds, Signal, Tween, Vec2};

/// A scale: `1.5` (both axes), `(2.0, 0.5)`, `[2.0, 0.5]` or a [`Vec2`].
pub trait IntoScale {
    /// The horizontal and vertical scale.
    fn into_scale(self) -> Vec2;
}

impl IntoScale for f64 {
    fn into_scale(self) -> Vec2 {
        Vec2::new(self, self)
    }
}

impl IntoScale for i32 {
    fn into_scale(self) -> Vec2 {
        f64::from(self).into_scale()
    }
}

impl IntoScale for (f64, f64) {
    fn into_scale(self) -> Vec2 {
        Vec2::new(self.0, self.1)
    }
}

impl IntoScale for [f64; 2] {
    fn into_scale(self) -> Vec2 {
        Vec2::new(self[0], self[1])
    }
}

impl IntoScale for Vec2 {
    fn into_scale(self) -> Vec2 {
        self
    }
}

/// The scale of a node, from [`Node::scale`](crate::Node::scale): a
/// `Signal<Vec2>` whose [`to`](ScaleSignal::to) and [`set`](ScaleSignal::set) also take
/// one number for both axes, `node.scale().to(1.2, 0.3)`.
#[derive(Clone)]
pub struct ScaleSignal(pub(crate) Signal<Vec2>);

impl ScaleSignal {
    /// Animates to `target` (`1.2`, `(2.0, 0.5)`, a [`Vec2`]).
    pub fn to(&self, target: impl IntoScale, duration: impl Seconds) -> Tween<Vec2> {
        self.0.to(target.into_scale(), duration)
    }

    /// Sets a plain value; removes any binding.
    pub fn set(&self, value: impl IntoScale) {
        self.0.set(value.into_scale());
    }

    /// The underlying signal.
    pub fn signal(&self) -> Signal<Vec2> {
        self.0.clone()
    }
}

impl Deref for ScaleSignal {
    type Target = Signal<Vec2>;
    fn deref(&self) -> &Signal<Vec2> {
        &self.0
    }
}

impl From<ScaleSignal> for Signal<Vec2> {
    fn from(s: ScaleSignal) -> Self {
        s.0
    }
}
