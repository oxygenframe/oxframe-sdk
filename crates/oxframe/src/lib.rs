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
mod audio;
#[cfg(not(target_arch = "wasm32"))]
mod cli;
mod engine;
#[cfg(not(target_arch = "wasm32"))]
pub mod feedback;
#[cfg(not(target_arch = "wasm32"))]
pub mod lint;
mod parts;
#[cfg(not(target_arch = "wasm32"))]
mod pdf;
#[cfg(not(target_arch = "wasm32"))]
mod preview;
mod project;
mod scene;
mod transition;
#[cfg(all(feature = "video", not(target_arch = "wasm32")))]
mod video;
mod voice;
#[cfg(feature = "wasm")]
pub mod wasm;

pub use adapter::PreviewAdapter;
pub use audio::{Audio, sound};
pub use engine::{Engine, EngineError, MetaSection};
pub use project::{Aspect, Project};
pub use scene::{SceneDef, SourceLocation};
pub use transition::{Transition, transition};
pub use voice::{Synthesis, TimedWord, TtsProvider, TtsRequest, voice};

/// Nodes of your own: `#[derive(Node)]` on a struct that implements
/// [`Draw`]. Behind the `unstable-node-api` feature.
#[cfg(feature = "unstable-node-api")]
pub use oxframe_2d::Draw;
pub use oxframe_2d::{
    Accent, Background, Camera, Circle, Code, CodeText, CodeTheme, CodeTween, Custom, Danger,
    Direction, DrawCtx, DrawRng, Edge, Effect, Fast, Grid, Group, Icon, IconStyle, Img, Ink, Input,
    Justify, Layout, LayoutAlign, Lg, Line, LineCap, LineJoin, MONO, Md, Motion, Muted, Node,
    NodeBuilder, NodeId, Normal, OnAccent, Pace, Particles, Path, Pen, Percent, Polygon, Rect,
    RichSpan, SANS, SafeArea, SceneGraph, Shader, Side, Slow, Sm, Span, Star, Step, Success,
    Surface, Svg, Text, TextAlign, Theme, Txt, Video, View, Warning, Xl, Xs, pct, span, waves,
};
pub use oxframe_components::{
    Axes, Bars, BulletList, Callout, ChapterMarker, CodeExplainer, EndCard, FlowDiagram, Interval,
    LowerThird, Mat2, Matrix, NumberLine, Plot, Series, Table, TableRow, TitleCard, Vector,
};
#[allow(deprecated)]
pub use oxframe_core::all_vec;
pub use oxframe_core::{
    Affine, BezPath, BlendMode, BoxFuture, Color, CubicBezier, Ease, Executor, Gradient,
    JoinHandle, Lerp, Point, Signal, Transform, Tween, Vec2, all, any, bounce_in, bounce_in_out,
    bounce_out, dt, ease_named, elastic_in, elastic_in_out, elastic_out, fps, frame, loop_while,
    next_frame, now, presets, random, random_range, repeat, sequence, spawn, stagger, time,
    tunable, wait_for, wait_until, with_rng,
};
pub use oxframe_core::{Blend, Dir, LayerPose, ShaderTransition};
pub use oxframe_core::{SlideMark, notes, slide_break};
/// An icon by set and name: `icon!("lucide:server")`, `icon!("tabler:database")`,
/// `icon!("simple:github")`. Behind the `icons` feature.
#[cfg(feature = "icons")]
pub use oxframe_icons::icon;
#[cfg(feature = "unstable-node-api")]
pub use oxframe_macros::Node;
pub use oxframe_macros::{color, ease, scene, tune};

pub use oxframe_2d as nodes;
pub use oxframe_core as core;
pub use oxframe_protocol as protocol;
pub use oxframe_render as render;
pub use oxframe_text as text;
pub use oxframe_text::register_font;

/// Preset shaders. A scene starts here and writes its own WGSL only when
/// no preset fits.
///
/// - [`waves`]: a node that paints water in its box.
/// - [`lens_drops`](crate::shaders::lens_drops): a node that puts drops over the frame drawn before it.
/// - [`ripple`](crate::shaders::ripple): an effect for [`Node::effect`](crate::Node::effect).
/// - [`water_wipe`](crate::shaders::water_wipe): a transition for [`Transition::shader`](crate::Transition::shader).
pub mod shaders {
    pub use crate::waves;
    pub use oxframe_2d::{Input, lens_drops, ripple};

    /// A wavy waterline rises over the frame and uncovers the next scene.
    /// Pass it to [`Transition::shader`](crate::Transition::shader).
    pub fn water_wipe() -> crate::ShaderTransition {
        crate::ShaderTransition::water_wipe()
    }
}

/// Everything a scene usually needs: `use oxframe::prelude::*;`.
pub mod prelude {
    #[cfg(feature = "unstable-node-api")]
    pub use crate::Draw;
    #[cfg(feature = "icons")]
    pub use crate::icon;
    pub use crate::{
        Accent, Aspect, Audio, Axes, Background, BezPath, Blend, BlendMode, BulletList, Callout,
        Camera, ChapterMarker, Circle, Code, CodeExplainer, CodeTheme, Color, Custom, Danger, Dir,
        Direction, DrawCtx, Ease, Edge, EndCard, Fast, FlowDiagram, Gradient, Grid, Group, Icon,
        IconStyle, Img, Input, Interval, Justify, LayerPose, Layout, LayoutAlign, Lerp, Lg, Line,
        LineCap, LineJoin, LowerThird, MONO, Mat2, Matrix, Md, Motion, Muted, Node, NodeBuilder,
        Normal, NumberLine, OnAccent, Particles, Path, Point, Polygon, Project, Rect, RichSpan,
        SANS, SafeArea, Shader, Side, Signal, Slow, Sm, Span, Star, Step, Success, Surface, Svg,
        Table, Text, TextAlign, Theme, TitleCard, Transition, Txt, Vec2, Vector, Video, View,
        Warning, Xl, Xs, all, any, color, ease, loop_while, next_frame, pct, presets, random,
        random_range, repeat, sequence, shaders, sound, span, spawn, stagger, transition, tune,
        voice, wait_for, wait_until, waves,
    };
    pub use crate::{notes, slide_break};
}
