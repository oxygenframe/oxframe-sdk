use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;

use kurbo::{Affine, BezPath, Point, Shape, Vec2};
use oxframe_core::{BlendMode, Color, Gradient, Signal};
use oxframe_text::{Lang, TextStyle};

use crate::text::{CodeTheme, RichSpan, Styled};

use crate::draw::{DrawItem, Layer, LineCap, LineJoin, Primitive, Shadow};
use crate::flex::{FlexBox, FlexMap};
use crate::image::ImageSource;

/// Index of a node in its [`SceneGraph`]. Assigned in creation order, so it
/// is deterministic too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(
    /// Position in creation order, starting at 0 for the root view.
    pub u32,
);

/// Main axis of a [`Layout`](crate::Layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// Left to right.
    #[default]
    Row,
    /// Top to bottom.
    Column,
}

/// Cross-axis alignment of children in a [`Layout`](crate::Layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutAlign {
    /// Top (in a row) or left (in a column).
    Start,
    /// Centered.
    #[default]
    Center,
    /// Bottom (in a row) or right (in a column).
    End,
    /// Fill the cross axis (resizes rects, images and nested layouts).
    Stretch,
}

/// Main-axis distribution of children; only matters with a fixed size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Justify {
    /// Packed at the start.
    Start,
    /// Packed in the middle.
    #[default]
    Center,
    /// Packed at the end.
    End,
    /// First and last at the edges, equal space between.
    SpaceBetween,
    /// Equal space around each child (half-size at the edges).
    SpaceAround,
    /// Equal space between children and at the edges.
    SpaceEvenly,
}

/// Node type and its type-specific properties (internal representation).
#[doc(hidden)]
pub enum NodeKind {
    Group,
    Rect {
        width: Signal<f64>,
        height: Signal<f64>,
        radius: Signal<f64>,
    },
    Circle {
        size: Signal<f64>,
        /// Degrees clockwise from 12 o'clock; a full turn draws a circle.
        start_angle: Signal<f64>,
        end_angle: Signal<f64>,
        /// Close an arc through the center (pie slice).
        pie: bool,
    },
    Polygon {
        sides: Signal<f64>,
        size: Signal<f64>,
    },
    Star {
        points: Signal<f64>,
        size: Signal<f64>,
        /// Inner radius as a fraction of the outer one.
        inner: Signal<f64>,
    },
    Path {
        data: Signal<BezPath>,
    },
    Line {
        points: Signal<Vec<Vec2>>,
        closed: bool,
        /// Smooth curve through the points (Catmull-Rom).
        smooth: bool,
        start_arrow: bool,
        end_arrow: bool,
        arrow_size: Signal<f64>,
    },
    Txt {
        text: Signal<String>,
        /// When non-empty, replaces `text` with styled runs.
        spans: Signal<Vec<RichSpan>>,
        font_size: Signal<f64>,
        weight: Signal<f64>,
        style: TextStyle,
        /// Active glyph morph: (from, to) and progress 0..1.
        morph: Signal<Option<(String, String)>>,
        morph_t: Signal<f64>,
    },
    Code {
        code: Signal<String>,
        lang: Lang,
        theme: CodeTheme,
        font_size: Signal<f64>,
        style: TextStyle,
        /// Focused lines (1-based, inclusive); (0, 0) focuses all.
        focus: Signal<Vec2>,
        /// Alpha of unfocused lines.
        dim: Signal<f64>,
    },
    Img {
        source: ImageSource,
        width: Signal<f64>,
        height: Signal<f64>,
    },
    Layout {
        direction: Direction,
        align: LayoutAlign,
        justify: Justify,
        /// Wrap children onto several lines.
        wrap: bool,
        /// CSS grid with this many equal columns instead of flexbox.
        columns: Option<u16>,
        gap: Signal<f64>,
        padding: Signal<f64>,
        width: Signal<Option<f64>>,
        height: Signal<Option<f64>>,
        radius: Signal<f64>,
    },
    Grid {
        width: Signal<f64>,
        height: Signal<f64>,
        spacing: Signal<f64>,
    },
}

impl NodeKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Group => "Group",
            Self::Rect { .. } => "Rect",
            Self::Circle { .. } => "Circle",
            Self::Polygon { .. } => "Polygon",
            Self::Star { .. } => "Star",
            Self::Path { .. } => "Path",
            Self::Line { .. } => "Line",
            Self::Txt { .. } => "Txt",
            Self::Code { .. } => "Code",
            Self::Img { .. } => "Img",
            Self::Layout { .. } => "Layout",
            Self::Grid { .. } => "Grid",
        }
    }
}

/// Stored state of one node (internal representation).
#[doc(hidden)]
pub struct NodeData {
    pub id: NodeId,
    pub name: Option<String>,
    /// `file:line` of the `add` call that created the node, when known.
    pub source: Option<String>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub kind: NodeKind,
    pub x: Signal<f64>,
    pub y: Signal<f64>,
    /// Degrees, clockwise (y points down).
    pub rotation: Signal<f64>,
    pub scale: Signal<Vec2>,
    pub opacity: Signal<f64>,
    pub fill: Signal<Color>,
    pub stroke: Signal<Color>,
    pub line_width: Signal<f64>,
    /// Drawn part of the outline, as fractions of its length.
    pub start: Signal<f64>,
    pub end: Signal<f64>,
    /// Overrides `fill` when set (local coordinates).
    pub fill_gradient: Signal<Option<Gradient>>,
    pub stroke_gradient: Signal<Option<Gradient>>,
    /// Dash pattern (on, off, on, …) in local units; empty = solid.
    pub dash: Signal<Vec<f64>>,
    pub dash_offset: Signal<f64>,
    pub line_cap: Signal<LineCap>,
    pub line_join: Signal<LineJoin>,
    /// Gaussian blur of the node and its children (local units).
    pub blur: Signal<f64>,
    pub shadow_color: Signal<Color>,
    pub shadow_blur: Signal<f64>,
    pub shadow_offset: Signal<Vec2>,
    /// Children are clipped to this node's outline.
    pub clip: Signal<bool>,
    pub blend: Signal<BlendMode>,
    /// Paint order among siblings (higher on top; ties keep insertion order).
    pub z_index: Signal<f64>,
    /// Which point of the node's box sits at (x, y) and is the pivot of
    /// rotation and scale: (-1, -1) top-left, (0, 0) center, (1, 1)
    /// bottom-right.
    pub anchor: Signal<Vec2>,
    // Flex item properties (used when the parent is a `Layout`).
    pub grow: Signal<f64>,
    pub shrink: Signal<f64>,
    pub basis: Signal<Option<f64>>,
    pub margin: Signal<f64>,
    pub align_self: Signal<Option<LayoutAlign>>,
}

impl NodeData {
    /// Transform relative to the parent, plus a layout-assigned offset.
    /// `size` is the node's box, needed for the anchor.
    pub fn local_transform(&self, offset: Vec2, size: (f64, f64)) -> Affine {
        let s = self.scale.get();
        let a = self.anchor.get();
        Affine::translate(offset + Vec2::new(self.x.get(), self.y.get()))
            * Affine::rotate(self.rotation.get().to_radians())
            * Affine::scale_non_uniform(s.x, s.y)
            * Affine::translate((-a.x * size.0 / 2.0, -a.y * size.1 / 2.0))
    }

    /// Text to draw for `Txt` and `Code` nodes.
    pub(crate) fn text_draw(&self) -> Option<TextDraw> {
        match &self.kind {
            NodeKind::Txt {
                text,
                spans,
                font_size,
                weight,
                style,
                morph,
                morph_t,
            } => {
                let style = TextStyle {
                    size: font_size.get().max(0.1),
                    weight: weight.get().clamp(1.0, 1000.0),
                    ..style.clone()
                };
                if let Some((from, to)) = morph.get() {
                    let a = oxframe_text::layout(&from, &style);
                    let b = oxframe_text::layout(&to, &style);
                    let (glyphs, size) = crate::text::morph(&a, &b, morph_t.get().clamp(0.0, 1.0));
                    return Some(TextDraw::Morph { glyphs, size });
                }
                let spans = spans.get();
                Some(TextDraw::Styled(if spans.is_empty() {
                    crate::text::rich(&[RichSpan::new(text.get())], &style)
                } else {
                    crate::text::rich(&spans, &style)
                }))
            }
            NodeKind::Code {
                code,
                lang,
                theme,
                font_size,
                style,
                focus,
                dim,
            } => {
                let style = TextStyle {
                    size: font_size.get().max(0.1),
                    ..style.clone()
                };
                Some(TextDraw::Styled(crate::text::code(
                    &code.get(),
                    *lang,
                    theme,
                    &style,
                    focus.get(),
                    dim.get(),
                )))
            }
            _ => None,
        }
    }

    /// Untrimmed outline in local coordinates, centered on the origin.
    pub fn outline(&self, size: (f64, f64)) -> Option<BezPath> {
        const TOLERANCE: f64 = 0.1;
        match &self.kind {
            NodeKind::Group
            | NodeKind::Img { .. }
            | NodeKind::Txt { .. }
            | NodeKind::Code { .. }
            | NodeKind::Grid { .. } => None,
            NodeKind::Rect { radius, .. } | NodeKind::Layout { radius, .. } => {
                let (w, h) = (size.0.max(0.0), size.1.max(0.0));
                let r = radius.get().clamp(0.0, w.min(h) / 2.0);
                let rect = kurbo::Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0);
                Some(if r > 0.0 {
                    rect.to_rounded_rect(r).to_path(TOLERANCE)
                } else {
                    rect.to_path(TOLERANCE)
                })
            }
            NodeKind::Circle {
                start_angle,
                end_angle,
                pie,
                ..
            } => {
                // Start at the top, clockwise — like a clock hand, which is
                // what `end` animations expect.
                let r = size.0.max(0.0) / 2.0;
                let (a0, a1) = (start_angle.get(), end_angle.get());
                let sweep = (a1 - a0).clamp(-360.0, 360.0);
                if sweep.abs() >= 360.0 - 1e-9 {
                    let circle = kurbo::Circle::new((0.0, 0.0), r).to_path(TOLERANCE);
                    return Some(Affine::rotate((a0 - 90.0).to_radians()) * circle);
                }
                let arc = kurbo::Arc::new(
                    (0.0, 0.0),
                    (r, r),
                    (a0 - 90.0).to_radians(),
                    sweep.to_radians(),
                    0.0,
                );
                let mut p = BezPath::new();
                if *pie {
                    p.move_to((0.0, 0.0));
                    p.line_to(arc.center + kurbo::Vec2::from_angle(arc.start_angle) * r);
                    arc.to_cubic_beziers(TOLERANCE, |a, b, c| p.curve_to(a, b, c));
                    p.close_path();
                } else {
                    p.move_to(arc.center + kurbo::Vec2::from_angle(arc.start_angle) * r);
                    arc.to_cubic_beziers(TOLERANCE, |a, b, c| p.curve_to(a, b, c));
                }
                Some(p)
            }
            NodeKind::Polygon { sides, .. } => {
                let n = sides.get().round().max(3.0) as usize;
                Some(regular(n, size.0.max(0.0) / 2.0, None))
            }
            NodeKind::Star { points, inner, .. } => {
                let n = points.get().round().max(2.0) as usize;
                let r = size.0.max(0.0) / 2.0;
                Some(regular(n, r, Some(r * inner.get().clamp(0.0, 1.0))))
            }
            NodeKind::Path { data } => Some(data.get()),
            NodeKind::Line {
                points,
                closed,
                smooth,
                ..
            } => {
                let pts = points.get();
                if *smooth && pts.len() > 2 {
                    return Some(catmull_rom(&pts, *closed));
                }
                let mut p = BezPath::new();
                for (i, v) in pts.iter().enumerate() {
                    if i == 0 {
                        p.move_to(v.to_point());
                    } else {
                        p.line_to(v.to_point());
                    }
                }
                if *closed && pts.len() > 2 {
                    p.close_path();
                }
                Some(p)
            }
        }
    }
}

/// Regular polygon (or star with an inner radius), first vertex at the top.
fn regular(n: usize, r: f64, inner: Option<f64>) -> BezPath {
    let count = if inner.is_some() { 2 * n } else { n };
    let mut p = BezPath::new();
    for i in 0..count {
        let radius = match inner {
            Some(ri) if i % 2 == 1 => ri,
            _ => r,
        };
        let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / count as f64;
        let pt = Point::new(a.cos() * radius, a.sin() * radius);
        if i == 0 {
            p.move_to(pt);
        } else {
            p.line_to(pt);
        }
    }
    p.close_path();
    p
}

/// Smooth curve through the points (uniform Catmull-Rom as cubic Béziers).
fn catmull_rom(pts: &[Vec2], closed: bool) -> BezPath {
    let n = pts.len();
    let at = |i: isize| -> Vec2 {
        if closed {
            pts[i.rem_euclid(n as isize) as usize]
        } else {
            pts[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let mut p = BezPath::new();
    p.move_to(pts[0].to_point());
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs as isize {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        let c1 = p1 + (p2 - p0) / 6.0;
        let c2 = p2 - (p3 - p1) / 6.0;
        p.curve_to(c1.to_point(), c2.to_point(), p2.to_point());
    }
    if closed {
        p.close_path();
    }
    p
}

pub(crate) enum TextDraw {
    Styled(Styled),
    /// Glyphs with their alpha, already centered.
    Morph {
        glyphs: Vec<(BezPath, f64)>,
        size: (f64, f64),
    },
}

/// Arena of nodes for one scene run. Node 0 is the view (root).
pub struct SceneGraph {
    nodes: RefCell<Vec<NodeData>>,
    size: (u32, u32),
}

impl SceneGraph {
    /// The root node (the scene's view).
    pub const ROOT: NodeId = NodeId(0);

    /// An empty scene of the given size in pixels, with only the root.
    pub fn new(width: u32, height: u32) -> Rc<Self> {
        let graph = Rc::new(Self {
            nodes: RefCell::new(Vec::new()),
            size: (width, height),
        });
        graph.insert(
            None,
            NodeKind::Group,
            crate::BaseProps {
                name: Some("view".into()),
                ..Default::default()
            },
        );
        graph
    }

    /// Scene size in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Number of nodes, including the root.
    pub fn len(&self) -> usize {
        self.nodes.borrow().len()
    }

    /// Never true: the root always exists.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrows a node's stored state.
    ///
    /// # Panics
    ///
    /// If `id` is not a node of this graph.
    pub fn node(&self, id: NodeId) -> Ref<'_, NodeData> {
        Ref::map(self.nodes.borrow(), |n| &n[id.0 as usize])
    }

    pub(crate) fn next_id(&self) -> u32 {
        self.len() as u32
    }

    pub(crate) fn insert(
        &self,
        parent: Option<NodeId>,
        kind: NodeKind,
        base: crate::BaseProps,
    ) -> NodeId {
        let mut nodes = self.nodes.borrow_mut();
        let id = NodeId(u32::try_from(nodes.len()).expect("too many nodes"));
        let label = format!("{}#{}", kind.name(), id.0);
        let sig = |field: &str| format!("{label}.{field}");
        nodes.push(NodeData {
            id,
            name: base.name,
            source: current_add_at().map(|(file, line)| source_label(file, line)),
            parent,
            children: Vec::new(),
            x: Signal::new(base.x).with_name(sig("x")),
            y: Signal::new(base.y).with_name(sig("y")),
            rotation: Signal::new(base.rotation).with_name(sig("rotation")),
            scale: Signal::new(base.scale).with_name(sig("scale")),
            opacity: Signal::new(base.opacity).with_name(sig("opacity")),
            fill: Signal::new(base.fill).with_name(sig("fill")),
            stroke: Signal::new(base.stroke).with_name(sig("stroke")),
            line_width: Signal::new(base.line_width).with_name(sig("line_width")),
            start: Signal::new(base.start).with_name(sig("start")),
            end: Signal::new(base.end).with_name(sig("end")),
            fill_gradient: Signal::new(base.fill_gradient).with_name(sig("fill_gradient")),
            stroke_gradient: Signal::new(base.stroke_gradient).with_name(sig("stroke_gradient")),
            dash: Signal::new(base.dash).with_name(sig("dash")),
            dash_offset: Signal::new(base.dash_offset).with_name(sig("dash_offset")),
            line_cap: Signal::new(base.line_cap).with_name(sig("line_cap")),
            line_join: Signal::new(base.line_join).with_name(sig("line_join")),
            blur: Signal::new(base.blur).with_name(sig("blur")),
            shadow_color: Signal::new(base.shadow_color).with_name(sig("shadow_color")),
            shadow_blur: Signal::new(base.shadow_blur).with_name(sig("shadow_blur")),
            shadow_offset: Signal::new(base.shadow_offset).with_name(sig("shadow_offset")),
            clip: Signal::new(base.clip).with_name(sig("clip")),
            blend: Signal::new(base.blend).with_name(sig("blend")),
            z_index: Signal::new(base.z_index).with_name(sig("z_index")),
            anchor: Signal::new(base.anchor).with_name(sig("anchor")),
            grow: Signal::new(base.grow).with_name(sig("grow")),
            shrink: Signal::new(base.shrink).with_name(sig("shrink")),
            basis: Signal::new(base.basis).with_name(sig("basis")),
            margin: Signal::new(base.margin).with_name(sig("margin")),
            align_self: Signal::new(base.align_self).with_name(sig("align_self")),
            kind,
        });
        if let Some(p) = parent {
            nodes[p.0 as usize].children.push(id);
        }
        id
    }

    pub(crate) fn detach(&self, id: NodeId) {
        let mut nodes = self.nodes.borrow_mut();
        if let Some(p) = nodes[id.0 as usize].parent.take() {
            nodes[p.0 as usize].children.retain(|c| *c != id);
        }
    }

    pub(crate) fn reparent(&self, id: NodeId, parent: NodeId) {
        self.detach(id);
        let mut nodes = self.nodes.borrow_mut();
        nodes[id.0 as usize].parent = Some(parent);
        nodes[parent.0 as usize].children.push(id);
    }

    /// Size of a node's box in its own (unscaled) coordinates.
    pub fn measure(&self, id: NodeId) -> (f64, f64) {
        let nodes = self.nodes.borrow();
        measure(&nodes, id)
    }

    /// Flattened, paint-ordered list of everything to draw.
    pub fn draw_list(&self) -> Vec<DrawItem> {
        let nodes = self.nodes.borrow();
        let mut out = Vec::new();
        visit(&nodes, Self::ROOT, Affine::IDENTITY, None, None, &mut out);
        out
    }

    /// Topmost node under `p` (scene coordinates), excluding the view.
    pub fn pick(&self, p: Point) -> Option<NodeId> {
        self.draw_list()
            .iter()
            .rev()
            .find(|item| item.node != Self::ROOT && item.hit(p))
            .map(|item| item.node)
    }

    /// World transform of a node (including layout offsets).
    pub fn world_transform(&self, id: NodeId) -> Option<Affine> {
        // The draw list already composes transforms; reuse it for nodes
        // that draw, and fall back to walking parents for those that don't.
        if let Some(item) = self.draw_list().into_iter().find(|i| i.node == id) {
            return Some(item.transform);
        }
        let nodes = self.nodes.borrow();
        let mut tf = Affine::IDENTITY;
        let mut cur = Some(id);
        while let Some(c) = cur {
            let n = nodes.get(c.0 as usize)?;
            tf = n.local_transform(Vec2::ZERO, measure(&nodes, c)) * tf;
            cur = n.parent;
        }
        Some(tf)
    }

    /// `(id, parent, kind, name)` of every attached node, depth-first.
    pub fn tree(&self) -> Vec<(NodeId, Option<NodeId>, &'static str, Option<String>)> {
        let nodes = self.nodes.borrow();
        let mut out = Vec::new();
        let mut stack = vec![Self::ROOT];
        while let Some(id) = stack.pop() {
            let n = &nodes[id.0 as usize];
            out.push((id, n.parent, n.kind.name(), n.name.clone()));
            stack.extend(n.children.iter().rev());
        }
        out
    }

    /// Current signal values of a node, for the inspector.
    pub fn describe(&self, id: NodeId) -> Vec<(String, String)> {
        let nodes = self.nodes.borrow();
        let Some(n) = nodes.get(id.0 as usize) else {
            return Vec::new();
        };
        let f = |v: f64| format!("{v:.3}");
        let s = n.scale.get();
        let mut out = vec![
            ("x".into(), f(n.x.get())),
            ("y".into(), f(n.y.get())),
            ("rotation".into(), f(n.rotation.get())),
            ("scale".into(), format!("{:.3}, {:.3}", s.x, s.y)),
            ("opacity".into(), f(n.opacity.get())),
            ("fill".into(), n.fill.get().to_string()),
            ("stroke".into(), n.stroke.get().to_string()),
            ("line_width".into(), f(n.line_width.get())),
            ("start".into(), f(n.start.get())),
            ("end".into(), f(n.end.get())),
        ];
        let (w, h) = measure(&nodes, id);
        out.push(("size".into(), format!("{w:.1} × {h:.1}")));
        match &n.kind {
            NodeKind::Rect { radius, .. } => out.push(("radius".into(), f(radius.get()))),
            NodeKind::Code { code, .. } => out.push(("code".into(), code.get())),
            NodeKind::Txt {
                text,
                font_size,
                weight,
                ..
            } => {
                out.push(("text".into(), text.get()));
                out.push(("font_size".into(), f(font_size.get())));
                out.push(("weight".into(), f(weight.get())));
            }
            NodeKind::Line { points, .. } => {
                out.push(("points".into(), points.get().len().to_string()))
            }
            NodeKind::Layout { gap, padding, .. } => {
                out.push(("gap".into(), f(gap.get())));
                out.push(("padding".into(), f(padding.get())));
            }
            NodeKind::Grid { spacing, .. } => out.push(("spacing".into(), f(spacing.get()))),
            _ => {}
        }
        out
    }

    /// One record per attached node, in tree order, with scene-space bounds.
    ///
    /// `order` is paint order (0 is the back). Nodes that are not drawn
    /// (fully transparent) get `u32::MAX`.
    pub fn inspect_nodes(&self) -> Vec<InspectNode> {
        let list = self.draw_list();
        let mut drawn: Vec<Option<(Affine, u32)>> = vec![None; self.len()];
        let mut order = 0u32;
        for item in &list {
            let slot = &mut drawn[item.node.0 as usize];
            if slot.is_none() {
                *slot = Some((item.transform, order));
                order += 1;
            }
        }
        let nodes = self.nodes.borrow();
        let mut out = Vec::new();
        let mut stack = vec![Self::ROOT];
        while let Some(id) = stack.pop() {
            let n = &nodes[id.0 as usize];
            stack.extend(n.children.iter().rev());
            let (w, h) = measure(&nodes, id);
            let (tf, order) = drawn[id.0 as usize]
                .unwrap_or_else(|| (transform_without_layout(&nodes, id), u32::MAX));
            let (text, text_overflow) = text_info(n);
            out.push(InspectNode {
                id: id.0,
                parent: n.parent.map(|p| p.0),
                kind: n.kind.name(),
                name: n.name.clone().unwrap_or_else(|| {
                    n.source
                        .clone()
                        .unwrap_or_else(|| format!("{}#{}", n.kind.name(), id.0))
                }),
                source: n.source.clone(),
                bounds: aabb(tf, w, h),
                opacity: n.opacity.get().clamp(0.0, 1.0),
                order,
                text,
                fill: n.fill.get(),
                text_overflow,
                parent_kind: n.parent.map(|p| nodes[p.0 as usize].kind.name()),
            });
        }
        out
    }
}

/// A node as `oxframe inspect` and the visual linter see it.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectNode {
    /// Node index.
    pub id: u32,
    /// Parent index; `None` for the root.
    pub parent: Option<u32>,
    /// Node type, e.g. `"Rect"`.
    pub kind: &'static str,
    /// `.name(..)`, or the `add` call site, or `Kind#id`.
    pub name: String,
    /// `file:line` of the `add` call, when it was recorded.
    pub source: Option<String>,
    /// Axis-aligned bounds in scene coordinates: min x, min y, max x, max y.
    /// The origin is the center of the frame, y points down.
    pub bounds: [f64; 4],
    /// 0 (invisible) to 1.
    pub opacity: f64,
    /// Paint order among drawn nodes; `u32::MAX` if the node is not drawn.
    pub order: u32,
    /// Text of a `Txt` or `Code` node.
    pub text: Option<String>,
    /// Fill color.
    pub fill: Color,
    /// How many pixels the text is wider than its `max_width` (0 if it fits
    /// or `max_width` is unset).
    pub text_overflow: f64,
    /// Type of the parent, if any.
    pub parent_kind: Option<&'static str>,
}

thread_local! {
    static ADD_AT: Cell<Option<(&'static str, u32)>> = const { Cell::new(None) };
}

pub(crate) fn push_add_at(file: &'static str, line: u32) -> Option<(&'static str, u32)> {
    ADD_AT.with(|c| c.replace(Some((file, line))))
}

pub(crate) fn pop_add_at(prev: Option<(&'static str, u32)>) {
    ADD_AT.with(|c| c.set(prev));
}

fn current_add_at() -> Option<(&'static str, u32)> {
    ADD_AT.with(|c| c.get())
}

/// `src/main.rs:42` rather than an absolute build path.
fn source_label(file: &str, line: u32) -> String {
    let file = file.replace('\\', "/");
    let rel = file
        .rfind("/src/")
        .map_or(file.as_str(), |i| &file[i + 1..]);
    format!("{rel}:{line}")
}

fn aabb(tf: Affine, w: f64, h: f64) -> [f64; 4] {
    let pts = [
        (-w / 2.0, -h / 2.0),
        (w / 2.0, -h / 2.0),
        (w / 2.0, h / 2.0),
        (-w / 2.0, h / 2.0),
    ];
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for (x, y) in pts {
        let p = tf * Point::new(x, y);
        b[0] = b[0].min(p.x);
        b[1] = b[1].min(p.y);
        b[2] = b[2].max(p.x);
        b[3] = b[3].max(p.y);
    }
    b
}

fn transform_without_layout(nodes: &[NodeData], id: NodeId) -> Affine {
    let mut chain = Vec::new();
    let mut cur = Some(id);
    while let Some(c) = cur {
        chain.push(c);
        cur = nodes.get(c.0 as usize).and_then(|n| n.parent);
    }
    let mut tf = Affine::IDENTITY;
    for c in chain.into_iter().rev() {
        let Some(n) = nodes.get(c.0 as usize) else {
            break;
        };
        tf *= n.local_transform(Vec2::ZERO, measure(nodes, c));
    }
    tf
}

fn text_info(n: &NodeData) -> (Option<String>, f64) {
    let (body, max_width) = match &n.kind {
        NodeKind::Txt {
            text, spans, style, ..
        } => {
            let spans = spans.get();
            let body = if spans.is_empty() {
                text.get()
            } else {
                spans.into_iter().map(|s| s.text).collect()
            };
            (body, style.max_width)
        }
        NodeKind::Code { code, style, .. } => (code.get(), style.max_width),
        _ => return (None, 0.0),
    };
    let overflow = match (max_width, n.text_draw()) {
        (Some(max), Some(TextDraw::Styled(s))) => (s.layout.width - max).max(0.0),
        (Some(max), Some(TextDraw::Morph { size, .. })) => (size.0 - max).max(0.0),
        _ => 0.0,
    };
    (Some(body), overflow)
}

fn measure(nodes: &[NodeData], id: NodeId) -> (f64, f64) {
    let n = &nodes[id.0 as usize];
    match &n.kind {
        NodeKind::Rect { width, height, .. } => (width.get(), height.get()),
        NodeKind::Circle { size, .. }
        | NodeKind::Polygon { size, .. }
        | NodeKind::Star { size, .. } => (size.get(), size.get()),
        NodeKind::Img { width, height, .. } | NodeKind::Grid { width, height, .. } => {
            (width.get(), height.get())
        }
        NodeKind::Txt { .. } | NodeKind::Code { .. } => match n.text_draw() {
            Some(TextDraw::Styled(s)) => (s.layout.width, s.layout.height),
            Some(TextDraw::Morph { size, .. }) => size,
            None => (0.0, 0.0),
        },
        NodeKind::Path { .. } | NodeKind::Line { .. } => {
            let bb = n
                .outline((0.0, 0.0))
                .map_or(kurbo::Rect::ZERO, |p| p.bounding_box());
            (bb.width(), bb.height())
        }
        NodeKind::Layout { .. } => crate::flex::compute(nodes, id, &|c| measure(nodes, c)).0,
        NodeKind::Group => {
            let mut bb: Option<kurbo::Rect> = None;
            for &c in &n.children {
                let child = &nodes[c.0 as usize];
                let (w, h) = measure(nodes, c);
                let r = child
                    .local_transform(Vec2::ZERO, (w, h))
                    .transform_rect_bbox(kurbo::Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0));
                bb = Some(bb.map_or(r, |b| b.union(r)));
            }
            bb.map_or((0.0, 0.0), |b| (b.width(), b.height()))
        }
    }
}

/// Kinds whose drawn size follows the box a layout gives them.
fn resizable(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Rect { .. }
            | NodeKind::Layout { .. }
            | NodeKind::Img { .. }
            | NodeKind::Grid { .. }
    )
}

fn visit(
    nodes: &[NodeData],
    id: NodeId,
    parent_tf: Affine,
    placement: Option<FlexBox>,
    flex: Option<&FlexMap>,
    out: &mut Vec<DrawItem>,
) {
    let n = &nodes[id.0 as usize];
    let opacity = n.opacity.get().clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let size = match placement {
        Some(b) if resizable(&n.kind) => {
            let s = n.scale.get();
            (
                b.size.0 / s.x.abs().max(1e-9),
                b.size.1 / s.y.abs().max(1e-9),
            )
        }
        _ => measure(nodes, id),
    };
    let offset = placement.map_or(Vec2::ZERO, |b| b.center);
    let tf = parent_tf * n.local_transform(offset, size);

    // A layer is needed when the node's effects apply to it and its children
    // as a whole; plain leaves just fold opacity into their paints.
    let blend = n.blend.get();
    let blur = n.blur.get().max(0.0);
    let shadow_color = n.shadow_color.get();
    let shadow = (shadow_color.a > 0.0).then(|| Shadow {
        color: shadow_color,
        blur: n.shadow_blur.get().max(0.0),
        offset: n.shadow_offset.get(),
    });
    let layered = blend != BlendMode::Normal
        || blur > 0.0
        || shadow.is_some()
        || (opacity < 1.0 && !n.children.is_empty());
    if layered {
        out.push(DrawItem {
            node: id,
            transform: tf,
            prim: Primitive::PushLayer(Layer {
                opacity,
                blend,
                clip: None,
                blur,
                shadow,
            }),
        });
    }
    let paint_opacity = if layered { 1.0 } else { opacity };
    for prim in crate::draw::primitives(n, size, paint_opacity) {
        out.push(DrawItem {
            node: id,
            transform: tf,
            prim,
        });
    }

    if !n.children.is_empty() {
        // Nodes without an outline (groups, text, images) clip to their box.
        let clip = n.clip.get().then(|| {
            n.outline(size).unwrap_or_else(|| {
                kurbo::Rect::new(-size.0 / 2.0, -size.1 / 2.0, size.0 / 2.0, size.1 / 2.0)
                    .to_path(0.1)
            })
        });
        let clipped = clip.is_some();
        if let Some(path) = clip {
            out.push(DrawItem {
                node: id,
                transform: tf,
                prim: Primitive::PushLayer(Layer::clip(path)),
            });
        }
        // Boxes of the children: from the enclosing layout tree, or a new
        // tree when this is a top-level layout.
        let own;
        let map: Option<&FlexMap> = match (&n.kind, flex) {
            (NodeKind::Layout { .. }, Some(m)) => Some(m),
            (NodeKind::Layout { .. }, None) => {
                own = crate::flex::compute(nodes, id, &|c| measure(nodes, c)).1;
                Some(&own)
            }
            _ => None,
        };
        let mut order: Vec<(usize, NodeId)> = n.children.iter().copied().enumerate().collect();
        // Stable: equal z keeps insertion order.
        order.sort_by(|a, b| {
            let za = nodes[a.1.0 as usize].z_index.get();
            let zb = nodes[b.1.0 as usize].z_index.get();
            za.total_cmp(&zb)
        });
        for (_, c) in order {
            let placement = map.and_then(|m| m.get(&c).copied());
            visit(nodes, c, tf, placement, map, out);
        }
        if clipped {
            out.push(DrawItem {
                node: id,
                transform: tf,
                prim: Primitive::PopLayer,
            });
        }
    }
    if layered {
        out.push(DrawItem {
            node: id,
            transform: tf,
            prim: Primitive::PopLayer,
        });
    }
}

impl DrawItem {
    fn hit(&self, p: Point) -> bool {
        let local = self.transform.inverse() * p;
        match &self.prim {
            Primitive::Fill { path, .. } => {
                (is_text_like(path) && path.bounding_box().contains(local))
                    || closed(path).contains(local)
            }
            Primitive::Stroke { path, style, .. } => {
                let tol = (style.width / 2.0)
                    .max(4.0 / self.transform.determinant().abs().sqrt().max(1e-9));
                path.segments().any(|s| {
                    kurbo::ParamCurveNearest::nearest(&s, local, 0.1).distance_sq <= tol * tol
                })
            }
            Primitive::Image { rect, .. } => rect.contains(local),
            Primitive::PushLayer(_) | Primitive::PopLayer => false,
        }
    }
}

/// Fills implicitly close open subpaths; kurbo's winding does not.
fn closed(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut open = false;
    for el in path.elements() {
        match el {
            kurbo::PathEl::MoveTo(_) => {
                if open {
                    out.close_path();
                }
                open = true;
            }
            kurbo::PathEl::ClosePath => open = false,
            _ => {}
        }
        out.push(*el);
    }
    if open {
        out.close_path();
    }
    out
}

/// Glyph runs are hard to hit exactly; treat many-subpath fills (text) by
/// their bounding box.
fn is_text_like(path: &BezPath) -> bool {
    path.elements()
        .iter()
        .filter(|e| matches!(e, kurbo::PathEl::MoveTo(_)))
        .count()
        > 3
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<SceneGraph>>> = const { RefCell::new(None) };
}

impl SceneGraph {
    /// Makes this graph the target of node handles for the duration of `f`.
    pub fn enter<R>(self: &Rc<Self>, f: impl FnOnce() -> R) -> R {
        let prev = CURRENT.with(|c| c.borrow_mut().replace(self.clone()));
        struct Restore(Option<Rc<SceneGraph>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                let prev = self.0.take();
                CURRENT.with(|c| *c.borrow_mut() = prev);
            }
        }
        let _restore = Restore(prev);
        f()
    }
}

pub(crate) fn with_graph<R>(f: impl FnOnce(&SceneGraph) -> R) -> R {
    CURRENT.with(|c| {
        let c = c.borrow();
        f(c.as_ref()
            .expect("oxframe: node handles can only be used while a scene is running"))
    })
}
