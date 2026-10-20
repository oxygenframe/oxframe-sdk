//! # oxframe
//!
//! > **Status:** oxframe is under very active development. The API is not stable yet and
//! > may change from version to version. Pin the exact version in your Cargo.toml and check
//! > the [changelog](https://github.com/oxygenframe/oxframe-sdk/blob/main/CHANGELOG.md)
//! > before upgrading.
//!
//! Procedural animation modelled after Motion Canvas: a scene is an
//! `async fn`, and every `.await` on an animation is a frame boundary.
//!
//! ```no_run
//! use oxframe::prelude::*;
//!
//! #[oxframe::scene]
//! async fn intro(view: View) {
//!     let r = view.add(Rect::new().size(200.0).x(-300.0).fill(color!("#e13238")));
//!     all((r.x().to(300.0, 1.0), r.radius().to(100.0, 1.0))).await;
//! }
//!
//! fn main() {
//!     Project::new("intro").scene(intro()).main();
//! }
//! ```

#![warn(missing_docs)]

mod adapter;
#[cfg(not(target_arch = "wasm32"))]
mod cli;
mod engine;
#[cfg(not(target_arch = "wasm32"))]
pub mod feedback;
#[cfg(not(target_arch = "wasm32"))]
pub mod lint;
#[cfg(not(target_arch = "wasm32"))]
mod preview;
mod project;
mod scene;
#[cfg(feature = "wasm")]
pub mod wasm;

pub use adapter::PreviewAdapter;
pub use engine::{Engine, EngineError, MetaSection};
pub use project::Project;
pub use scene::{SceneDef, SourceLocation};

pub use oxframe_2d::{
    Accent, Background, Circle, Code, CodeTheme, Danger, Direction, Fast, Grid, Group, Img, Ink,
    Justify, Layout, LayoutAlign, Lg, Line, LineCap, LineJoin, MONO, Md, Motion, Muted, Node,
    NodeBuilder, NodeId, Normal, OnAccent, Pace, Path, Polygon, Rect, RichSpan, SANS, SceneGraph,
    Slow, Sm, Star, Step, Success, Surface, Text, TextAlign, Theme, Txt, View, Warning, Xl, Xs,
    span,
};
pub use oxframe_core::{
    Affine, BezPath, BlendMode, BoxFuture, Color, CubicBezier, Ease, Executor, Gradient,
    JoinHandle, Lerp, Point, Signal, Transform, Tween, Vec2, all, all_vec, bounce_in,
    bounce_in_out, bounce_out, dt, ease_named, elastic_in, elastic_in_out, elastic_out, fps, frame,
    next_frame, now, presets, random, random_range, sequence, spawn, time, tunable, wait_for,
    wait_until, with_rng,
};
pub use oxframe_macros::{color, ease, scene, tune};

pub use oxframe_2d as nodes;
pub use oxframe_core as core;
pub use oxframe_protocol as protocol;
pub use oxframe_render as render;
pub use oxframe_text as text;
pub use oxframe_text::register_font;

/// Everything a scene usually needs: `use oxframe::prelude::*;`.
pub mod prelude {
    pub use crate::{
        Accent, Background, BezPath, BlendMode, Circle, Code, CodeTheme, Color, Danger, Direction,
        Ease, Fast, Gradient, Grid, Group, Img, Justify, Layout, LayoutAlign, Lerp, Lg, Line,
        LineCap, LineJoin, MONO, Md, Motion, Muted, Node, NodeBuilder, Normal, OnAccent, Path,
        Point, Polygon, Project, Rect, RichSpan, SANS, Signal, Slow, Sm, Star, Step, Success,
        Surface, Text, TextAlign, Theme, Txt, Vec2, View, Warning, Xl, Xs, all, all_vec, color,
        ease, next_frame, presets, random, random_range, sequence, span, spawn, tune, wait_for,
        wait_until,
    };
}
