//! 2D scene graph of oxframe.
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
mod text;
mod trim;

pub use builder::{
    BaseProps, CircleBuilder, CodeBuilder, GridBuilder, GroupBuilder, ImgBuilder, LayoutBuilder,
    LineBuilder, NodeBuilder, PathBuilder, PolygonBuilder, RectBuilder, StarBuilder, TxtBuilder,
};
pub use draw::{DrawItem, Layer, LineCap, LineJoin, Paint, Primitive, Shadow, StrokeStyle};
pub use graph::{Direction, Justify, LayoutAlign, NodeData, NodeId, NodeKind, SceneGraph};
pub use handles::{
    Circle, Code, Grid, Group, Img, Layout, Line, Node, Path, Polygon, Rect, Star, Txt, View,
};
pub use image::{ImageSource, RasterImage, VectorImage, VectorShape};
pub use trim::trim;

pub use oxframe_core::{BlendMode, Color, Gradient, GradientKind, Signal};
pub use oxframe_text::{Align as TextAlign, Lang, MONO, SANS};
pub use text::{CodeTheme, RichSpan, span};
