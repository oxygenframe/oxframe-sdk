use std::ops::Deref;

use kurbo::{BezPath, Point};
use oxframe_core::{BlendMode, Color, Gradient, Signal, Vec2};

use crate::builder::{
    self, CircleBuilder, CodeBuilder, GridBuilder, GroupBuilder, ImgBuilder, LayoutBuilder,
    LineBuilder, NodeBuilder, PathBuilder, PolygonBuilder, RectBuilder, StarBuilder, TxtBuilder,
};
use crate::draw::{LineCap, LineJoin};
use crate::graph::{NodeId, NodeKind, SceneGraph, with_graph};
use crate::text::RichSpan;

/// Untyped node handle with the properties every node has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Node(NodeId);

macro_rules! base_signal {
    ($($(#[$doc:meta])* $field:ident: $ty:ty),* $(,)?) => {
        $(
            $(#[$doc])*
            #[doc = ""]
            #[doc = concat!("The `", stringify!($field), "` property as an animatable signal.")]
            pub fn $field(self) -> Signal<$ty> {
                with_graph(|g| g.node(self.0).$field.clone())
            }
        )*
    };
}

impl Node {
    /// Index of the node in the scene graph.
    pub fn id(self) -> NodeId {
        self.0
    }

    base_signal! {
        x: f64,
        y: f64,
        /// Degrees, clockwise.
        rotation: f64,
        scale: Vec2,
        opacity: f64,
        fill: Color,
        stroke: Color,
        line_width: f64,
        /// Start of the drawn part of the outline (fraction of its length).
        start: f64,
        /// End of the drawn part of the outline.
        end: f64,
        fill_gradient: Option<Gradient>,
        stroke_gradient: Option<Gradient>,
        dash: Vec<f64>,
        dash_offset: f64,
        line_cap: LineCap,
        line_join: LineJoin,
        blur: f64,
        shadow_color: Color,
        shadow_blur: f64,
        shadow_offset: Vec2,
        clip: bool,
        blend: BlendMode,
        z_index: f64,
        anchor: Vec2,
        grow: f64,
        shrink: f64,
        basis: Option<f64>,
        margin: f64,
        align_self: Option<crate::LayoutAlign>,
    }

    /// Adds a child node; children are positioned relative to the parent.
    #[allow(clippy::should_implement_trait)] // `view.add(..)` mirrors Motion Canvas
    pub fn add<B: NodeBuilder>(self, child: B) -> B::Handle {
        builder::add(self.0, child)
    }

    /// Detaches the node from its parent; it stops being drawn.
    pub fn remove(self) {
        with_graph(|g| g.detach(self.0));
    }

    /// Moves the node under another parent.
    pub fn reparent(self, parent: impl Into<Node>) {
        let parent = parent.into();
        with_graph(|g| g.reparent(self.0, parent.0));
    }

    /// Direct children, in creation order.
    pub fn children(self) -> Vec<Node> {
        with_graph(|g| g.node(self.0).children.iter().map(|&c| Node(c)).collect())
    }

    /// Size of the node's box in its own coordinates.
    pub fn size(self) -> (f64, f64) {
        with_graph(|g| g.measure(self.0))
    }

    /// Position of the node's origin in scene coordinates.
    pub fn absolute_position(self) -> Vec2 {
        with_graph(|g| (g.world_transform(self.0).unwrap_or_default() * Point::ZERO).to_vec2())
    }
}

macro_rules! typed_handle {
    ($name:ident, $builder:ident) => {
        #[doc = concat!("Handle to a `", stringify!($name), "` node in the scene: cheap to copy, its")]
        #[doc = "methods return the node's properties as [`Signal`]s. Derefs to [`Node`]."]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(Node);

        impl $name {
            /// Starts a builder; add it with [`Node::add`] (e.g. `view.add(..)`).
            #[allow(clippy::new_ret_no_self)]
            pub fn new() -> $builder {
                $builder::default()
            }

            pub(crate) fn from_id(id: NodeId) -> Self {
                Self(Node(id))
            }

            /// The untyped node handle (same as `*self`).
            pub fn node(self) -> Node {
                self.0
            }
        }

        impl Deref for $name {
            type Target = Node;
            fn deref(&self) -> &Node {
                &self.0
            }
        }

        impl From<$name> for Node {
            fn from(h: $name) -> Node {
                h.0
            }
        }
    };
}

typed_handle!(Group, GroupBuilder);
typed_handle!(Rect, RectBuilder);
typed_handle!(Circle, CircleBuilder);
typed_handle!(Path, PathBuilder);
typed_handle!(Line, LineBuilder);
typed_handle!(Txt, TxtBuilder);
typed_handle!(Img, ImgBuilder);
typed_handle!(Layout, LayoutBuilder);
typed_handle!(Grid, GridBuilder);
typed_handle!(Polygon, PolygonBuilder);
typed_handle!(Star, StarBuilder);
typed_handle!(Code, CodeBuilder);

macro_rules! kind_signal {
    ($handle:ident :: $kind:ident { $($(#[$doc:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        impl $handle {
            $(
                $(#[$doc])*
                #[doc = ""]
                #[doc = concat!("The `", stringify!($field), "` property as an animatable signal.")]
                pub fn $field(self) -> Signal<$ty> {
                    with_graph(|g| match &g.node(self.id()).kind {
                        NodeKind::$kind { $field, .. } => $field.clone(),
                        _ => unreachable!(concat!("handle does not point to a ", stringify!($kind))),
                    })
                }
            )*
        }
    };
}

kind_signal!(Rect::Rect {
    width: f64,
    height: f64,
    radius: f64
});
kind_signal!(Circle::Circle {
    /// Diameter.
    size: f64,
    start_angle: f64,
    end_angle: f64,
});
kind_signal!(Polygon::Polygon {
    sides: f64,
    size: f64
});
kind_signal!(Star::Star {
    points: f64,
    size: f64,
    inner: f64
});
kind_signal!(Path::Path { data: BezPath });
kind_signal!(Line::Line { points: Vec<Vec2>, arrow_size: f64 });
kind_signal!(Txt::Txt { text: String, spans: Vec<RichSpan>, font_size: f64, weight: f64 });
kind_signal!(Code::Code {
    code: String,
    font_size: f64,
    /// Focused lines as (first, last), 1-based; (0, 0) = all.
    focus: Vec2,
    /// Alpha of unfocused lines.
    dim: f64,
});

impl Txt {
    /// Morphs to `text` glyph by glyph: letters present in both strings
    /// travel to their new places, the others fade.
    pub fn morph_to(self, text: impl Into<String>, duration: f64) -> oxframe_core::BoxFuture {
        let to = text.into();
        let (txt, morph, t) = with_graph(|g| match &g.node(self.id()).kind {
            NodeKind::Txt {
                text,
                morph,
                morph_t,
                ..
            } => (text.clone(), morph.clone(), morph_t.clone()),
            _ => unreachable!("handle does not point to a Txt"),
        });
        Box::pin(async move {
            morph.set(Some((txt.get(), to.clone())));
            t.set(0.0);
            t.to(1.0, duration).await;
            txt.set(to);
            morph.set(None);
        })
    }
}

impl Code {
    /// Moves the focus to lines `first..=last` over `duration`.
    pub fn focus_lines(
        self,
        first: usize,
        last: usize,
        duration: f64,
    ) -> oxframe_core::Tween<Vec2> {
        self.focus()
            .to(Vec2::new(first as f64, last as f64), duration)
    }
}
kind_signal!(Img::Img {
    width: f64,
    height: f64
});
kind_signal!(Layout::Layout {
    gap: f64,
    padding: f64,
    radius: f64,
    /// Fixed width; `None` sizes to content.
    width: Option<f64>,
    /// Fixed height; `None` sizes to content.
    height: Option<f64>,
});
kind_signal!(Grid::Grid {
    width: f64,
    height: f64,
    spacing: f64
});

/// Root of a scene. Origin is the center of the frame, y points down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View(Node);

impl View {
    /// The view of the scene being executed.
    pub fn current() -> Self {
        Self(Node(SceneGraph::ROOT))
    }

    /// Frame size in pixels.
    pub fn size(self) -> (u32, u32) {
        with_graph(|g| g.size())
    }
}

impl Deref for View {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.0
    }
}

impl From<View> for Node {
    fn from(v: View) -> Node {
        v.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Primitive;

    fn scene(f: impl FnOnce(View)) -> std::rc::Rc<SceneGraph> {
        let graph = SceneGraph::new(400, 400);
        graph.enter(|| f(View::current()));
        graph
    }

    #[test]
    fn builders_set_initial_values_and_handles_return_signals() {
        scene(|view| {
            let r = view.add(
                Rect::new()
                    .size(50.0)
                    .radius(5.0)
                    .x(-10.0)
                    .fill(Color::BLACK),
            );
            assert_eq!(r.width().get(), 50.0);
            assert_eq!(r.height().get(), 50.0);
            assert_eq!(r.radius().get(), 5.0);
            assert_eq!(r.x().get(), -10.0);
            assert_eq!(r.fill().get(), Color::BLACK);
            r.x().set(3.0);
            assert_eq!(r.x().get(), 3.0);
            let c = r.add(Circle::new().size(10.0));
            assert_eq!(r.children(), vec![c.node()]);
            assert_eq!(view.children(), vec![r.node()]);
            assert_eq!(c.size().name().as_deref(), Some("Circle#2.size"));
        });
    }

    #[test]
    fn draw_list_composes_transforms() {
        let g = scene(|view| {
            let grp = view.add(Group::new().x(10.0).scale(2.0));
            grp.add(Circle::new().x(5.0));
        });
        let items = g.draw_list();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].transform * Point::ZERO, Point::new(20.0, 0.0));
    }

    #[test]
    fn layout_places_children_in_a_row() {
        let g = scene(|view| {
            view.add(
                Layout::new()
                    .gap(10.0)
                    .padding(5.0)
                    .child(Rect::new().size(20.0))
                    .child(Rect::new().width(40.0).height(10.0)),
            );
        });
        let layout = NodeId(1);
        assert_eq!(g.measure(layout), (5.0 + 20.0 + 10.0 + 40.0 + 5.0, 30.0));
        let centers: Vec<_> = g
            .draw_list()
            .iter()
            .map(|i| i.transform * Point::ZERO)
            .collect();
        // Total 80 wide: first box spans -35..-15, second -5..35.
        assert_eq!(centers, vec![Point::new(-25.0, 0.0), Point::new(15.0, 0.0)]);
    }

    #[test]
    fn column_with_fixed_size_and_alignment() {
        let g = scene(|view| {
            view.add(
                Layout::new()
                    .column()
                    .height(100.0)
                    .width(100.0)
                    .justify(crate::Justify::Start)
                    .align(crate::LayoutAlign::Start)
                    .child(Rect::new().size(20.0)),
            );
        });
        let c = g.draw_list()[0].transform * Point::ZERO;
        assert_eq!(c, Point::new(-40.0, -40.0));
    }

    #[test]
    fn text_line_path_and_img_draw() {
        let png = {
            let mut v = Vec::new();
            image::RgbaImage::from_pixel(4, 2, image::Rgba([255, 0, 0, 255]))
                .write_to(&mut std::io::Cursor::new(&mut v), image::ImageFormat::Png)
                .unwrap();
            v
        };
        let g = scene(|view| {
            let t = view.add(Txt::new().text("Hi").font_size(40.0));
            assert!(t.size().0 > 20.0);
            view.add(
                Line::new()
                    .points([(0.0, 0.0), (100.0, 0.0)])
                    .end_arrow(true),
            );
            view.add(Path::new().svg("M0 0 L10 0 L10 10 Z"));
            let img = view.add(Img::new().src(&png).width(40.0));
            assert_eq!(img.height().get(), 20.0);
        });
        let kinds: Vec<_> = g
            .draw_list()
            .iter()
            .map(|i| match i.prim {
                Primitive::Fill { .. } => "fill",
                Primitive::Stroke { .. } => "stroke",
                Primitive::Image { .. } => "image",
                Primitive::PushLayer(_) => "push",
                Primitive::PopLayer => "pop",
            })
            .collect();
        assert_eq!(kinds, vec!["fill", "stroke", "fill", "fill", "image"]);
    }

    #[test]
    fn trimming_and_pick() {
        let g = scene(|view| {
            view.add(Rect::new().size(100.0).name("back"));
            view.add(Circle::new().size(40.0).x(30.0).end(0.5));
        });
        assert_eq!(g.pick(Point::new(-40.0, 0.0)), Some(NodeId(1)));
        assert_eq!(g.pick(Point::new(300.0, 0.0)), None);
        // The half circle covers the right half of its disc.
        assert_eq!(g.pick(Point::new(45.0, 0.0)), Some(NodeId(2)));
    }

    fn kinds(g: &SceneGraph) -> Vec<&'static str> {
        g.draw_list()
            .iter()
            .map(|i| match i.prim {
                Primitive::Fill { .. } => "fill",
                Primitive::Stroke { .. } => "stroke",
                Primitive::Image { .. } => "image",
                Primitive::PushLayer(_) => "push",
                Primitive::PopLayer => "pop",
            })
            .collect()
    }

    #[test]
    fn layers_only_when_needed_and_balanced() {
        // Translucent leaf: opacity folds into the paint, no layer.
        let g = scene(|v| {
            v.add(Rect::new().opacity(0.5));
        });
        assert_eq!(kinds(&g), ["fill"]);
        // Translucent group with children: one layer around them.
        let g = scene(|v| {
            v.add(
                Group::new()
                    .opacity(0.5)
                    .child(Rect::new())
                    .child(Circle::new()),
            );
        });
        assert_eq!(kinds(&g), ["push", "fill", "fill", "pop"]);
        // Clip wraps only the children; shadow wraps node + children.
        let g = scene(|v| {
            v.add(
                Circle::new()
                    .clip(true)
                    .shadow(Color::BLACK, 4.0, (2.0, 2.0))
                    .child(Rect::new()),
            );
        });
        assert_eq!(kinds(&g), ["push", "fill", "push", "fill", "pop", "pop"]);
    }

    #[test]
    fn z_index_orders_siblings_and_anchor_moves_box() {
        let g = scene(|v| {
            v.add(Rect::new().name("a").z_index(2.0));
            v.add(Rect::new().name("b"));
            v.add(Rect::new().name("c").size(20.0).anchor((-1.0, -1.0)).x(5.0));
        });
        let items = g.draw_list();
        let order: Vec<u32> = items.iter().map(|i| i.node.0).collect();
        assert_eq!(order, vec![2, 3, 1]);
        // Anchored at the top-left: the box center sits at (5+10, 0+10).
        assert_eq!(items[1].transform * Point::ZERO, Point::new(15.0, 10.0));
    }

    fn boxes(g: &SceneGraph) -> Vec<(Point, f64)> {
        g.draw_list()
            .iter()
            .filter_map(|i| match &i.prim {
                Primitive::Fill { path, .. } => {
                    let bb = kurbo::Shape::bounding_box(path);
                    Some((i.transform * Point::ZERO, bb.width()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn flex_grow_stretch_wrap_and_grid() {
        use crate::{Justify, LayoutAlign};
        // grow: a 200-wide row; the second child takes the free space.
        let g = scene(|v| {
            v.add(
                Layout::new()
                    .width(200.0)
                    .justify(Justify::Start)
                    .child(Rect::new().size(40.0))
                    .child(Rect::new().size(40.0).grow(1.0)),
            );
        });
        let b = boxes(&g);
        assert_eq!(b[0], (Point::new(-80.0, 0.0), 40.0));
        assert_eq!(b[1], (Point::new(20.0, 0.0), 160.0));

        // stretch: children fill the cross axis.
        let g = scene(|v| {
            v.add(
                Layout::new()
                    .column()
                    .width(120.0)
                    .align(LayoutAlign::Stretch)
                    .child(Rect::new().size(30.0)),
            );
        });
        assert_eq!(boxes(&g)[0].1, 120.0);

        // wrap: three 60-wide boxes in 130 → two lines.
        let g = scene(|v| {
            v.add(
                Layout::new()
                    .width(130.0)
                    .wrap(true)
                    .child(Rect::new().size(60.0))
                    .child(Rect::new().size(60.0))
                    .child(Rect::new().size(60.0)),
            );
        });
        let ys: Vec<f64> = boxes(&g).iter().map(|b| b.0.y).collect();
        assert_eq!(ys[0], ys[1]);
        assert!(ys[2] > ys[0]);

        // grid: 2 columns of equal width.
        let g = scene(|v| {
            v.add(
                Layout::new()
                    .grid(2)
                    .width(200.0)
                    .align(LayoutAlign::Stretch)
                    .child(Rect::new().size(10.0))
                    .child(Rect::new().size(10.0))
                    .child(Rect::new().size(10.0)),
            );
        });
        let b = boxes(&g);
        assert_eq!(
            b.iter().map(|b| b.1).collect::<Vec<_>>(),
            vec![100.0, 100.0, 100.0]
        );
        assert_eq!(b[2].0.x, b[0].0.x);
        assert!(b[2].0.y > b[0].0.y);
    }

    #[test]
    fn describe_lists_signals() {
        let g = scene(|view| {
            view.add(Txt::new().text("abc"));
        });
        let d = g.describe(NodeId(1));
        assert!(d.iter().any(|(k, v)| k == "text" && v == "abc"));
    }
}
