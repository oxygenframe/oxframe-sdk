//! Core of oxframe: signals, tweens, easing and a deterministic,
//! single-threaded coroutine executor.
//!
//! > **Status:** oxframe is under very active development. The API is not stable yet and
//! > may change from version to version. Pin the exact version in your Cargo.toml and check
//! > the [changelog](https://github.com/oxygenframe/oxframe-sdk/blob/main/CHANGELOG.md)
//! > before upgrading.
//!
//! An animation is an `async fn`; every `.await` on a [`Tween`],
//! [`wait_for`] or [`next_frame`] is a frame boundary. One poll of the root
//! future is exactly one frame, with a fixed `dt = 1 / fps`.

#![warn(missing_docs)]

mod color;
mod ease;
mod executor;
mod flow;
mod frame;
mod lerp;
mod paint;
mod path;
mod runtime;
mod signal;
mod sound;
mod transition;
mod tween;

pub use color::Color;
pub use ease::{CubicBezier, Ease, presets};
pub use ease::{bounce_in, bounce_in_out, bounce_out, elastic_in, elastic_in_out, elastic_out};
pub use executor::Executor;
#[allow(deprecated)]
pub use flow::all_vec;
pub use flow::{
    All, Animation, Any, BoxFuture, Branches, Delayed, IntoAll, JoinHandle, VecBranches, WaitFor,
    all, any, loop_while, next_frame, repeat, sequence, spawn, stagger, wait_for, wait_until,
};
pub use frame::{FrameSource, Image};
pub use lerp::{Lerp, Transform};
pub use paint::{BlendMode, Gradient, GradientKind};
pub use runtime::{
    SeededRng, begin_transition, dt, ease_named, fps, frame, has_previous, meta_snapshot, now,
    random, random_range, time, tunable, with_rng,
};
pub use transition::{Blend, Dir, Kind, LayerPose, Recorded};

pub use rand;
pub use signal::Signal;
pub use sound::{SoundClip, VoiceLine, record_sound, record_voice};
pub use tween::{Seconds, Tween};

pub use kurbo::{self, Affine, BezPath, Point, Vec2};

/// Times are compared with this tolerance so that sums of `f64` seconds
/// (`0.1 + 0.2`) land on the frame they were meant for.
pub const TIME_EPSILON: f64 = 1e-9;
