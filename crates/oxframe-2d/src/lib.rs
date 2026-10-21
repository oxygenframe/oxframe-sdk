//! 2D scene graph of oxframe.
//!
//! > **Status:** oxframe is under very active development. The API is not stable yet and
//! > may change from version to version. Pin the exact version in your Cargo.toml and check
//! > the [changelog](https://github.com/oxygenframe/oxframe-sdk/blob/main/CHANGELOG.md)
//! > before upgrading.
//!
//! Scene code creates nodes with builders (`Rect::new().width(100.0)`) and
//! adds them with [`Node::add`] (`view.add(..)`), which returns a `Copy`
//! handle whose zero-argument methods of the same names return
//! [`Signal`]s:
//!
//! ```ignore
//! let r = view.add(Rect::new().size(200.0).x(-300.0));
//! r.x().to(300.0, 1.0).await;
//! ```
//!
//! Renderers consume a flat [`DrawItem`] list produced by
//! [`SceneGraph::draw_list`], so every backend draws exactly the same thing.

#![warn(missing_docs)]

mod builder;
mod draw;
mod flex;
mod graph;
mod handles;
mod image;
mod passes;
mod place;
pub mod post;
mod shader;
mod sim;
mod svg;
mod text;
mod theme;
mod trim;

pub use builder::{
    BaseProps, CameraBuilder, CircleBuilder, CodeBuilder, CustomBuilder, EffectBuilder,
    GridBuilder, GroupBuilder, ImgBuilder, LayoutBuilder, LineBuilder, NodeBuilder,
    ParticlesBuilder, PathBuilder, PolygonBuilder, RectBuilder, ShaderBuilder, StarBuilder,
    TxtBuilder,
};
pub use builder::{lens_drops, ripple, waves};
pub use draw::{
    DrawCtx, DrawItem, DrawRng, Layer, LineCap, LineJoin, Paint, Pen, Primitive, Shadow,
    StrokeStyle,
};
pub use graph::{
    Direction, InspectNode, Justify, LayoutAlign, NodeData, NodeId, NodeKind, SceneGraph, add_site,
};
pub use handles::{
    Camera, Circle, Code, CodeText, CodeTween, Custom, Effect, Grid, Group, Img, Layout, Line,
    Node, Particles, Path, Polygon, Rect, SLIDE_DISTANCE, Shader, Star, Svg, Txt, View,
};
pub use image::{ImageSource, RasterImage, VectorImage, VectorShape};
pub use passes::{Pass, PassKind, layer_fingerprint, pass_graph};
pub use place::{Edge, Percent, Place, SafeArea, Side, Span, pct};
pub use post::{Input, Shade, ShadeOp, apply as apply_shade};
pub use shader::{ShaderDiag, draft_shaders, set_shader_budget_ms};
pub use trim::trim;

pub use oxframe_core::{BlendMode, Color, Gradient, GradientKind, Signal};
pub use oxframe_text::{Align as TextAlign, Lang, MONO, SANS};
pub use text::{CodeTheme, RichSpan, span};
/// Theme of the scene that is running, or the dark theme outside one.
pub use theme::current as current_theme;
pub use theme::enter as enter_theme;
pub use theme::{
    Corner, Ink,
    Ink::{Accent, Background, Danger, Muted, OnAccent, Success, Surface, Text, Warning},
    IntoPaint, Len, Motion, Pace,
    Pace::{Fast, Normal, Slow},
    Scale, ShadowSpec, Step,
    Step::{Lg, Md, Sm, Xl, Xs},
    Theme, TypeRole, TypeScale, TypeStyle,
};
