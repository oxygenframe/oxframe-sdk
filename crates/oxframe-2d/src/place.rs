//! Positions relative to another node, the frame, or the safe area.
//!
//! A placement is stored on the node and resolved on every draw, so it
//! follows a neighbor that moves or changes size. Percent sizes
//! ([`pct`]) track the frame, or the parent when the parent has a fixed size.

use oxframe_core::Signal;

use crate::graph::{NodeData, NodeId, NodeKind, SceneGraph, measure};
use crate::theme::{self, Step};

/// Which side of a neighbor [`Node::next_to`](crate::Node::next_to) uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// To the left.
    Left,
    /// To the right.
    Right,
    /// Above.
    Top,
    /// Below.
    Bottom,
}

/// An edge of a node or of the safe area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// Left edge.
    Left,
    /// Right edge.
    Right,
    /// Top edge.
    Top,
    /// Bottom edge.
    Bottom,
}

/// A length in pixels, or a percentage of the frame (or of a sized parent).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Span {
    /// An absolute length in pixels.
    Px(f64),
    /// A percentage, `100` meaning the whole reference size.
    Pct(f64),
}

impl From<f64> for Span {
    fn from(px: f64) -> Self {
        Self::Px(px)
    }
}

/// A number that can be a percentage. `pct(80)` and `pct(80.0)` are the same.
pub trait Percent {
    /// `100` is the whole reference size.
    fn percent(self) -> f64;
}

impl Percent for f64 {
    fn percent(self) -> f64 {
        self
    }
}

impl Percent for i32 {
    fn percent(self) -> f64 {
        f64::from(self)
    }
}

/// `80` percent of the frame, or of the parent when it has a fixed size.
///
/// ```ignore
/// Rect::new().width(pct(80)).height(300.0)
/// ```
pub fn pct(percent: impl Percent) -> Span {
    Span::Pct(percent.percent())
}

/// Which axis a [`Span::Pct`] is a fraction of.
#[derive(Clone, Copy)]
pub(crate) enum Axis {
    X,
    Y,
}

/// The rectangle `to_edge` keeps inside, in scene coordinates (origin at
/// the center, y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SafeArea {
    /// Left edge.
    pub left: f64,
    /// Top edge.
    pub top: f64,
    /// Right edge.
    pub right: f64,
    /// Bottom edge.
    pub bottom: f64,
}

impl SafeArea {
    /// Insets for a frame. A portrait frame leaves room for the status bar
    /// and the buttons of a vertical video; a landscape frame uses a fixed
    /// margin.
    pub fn for_frame(width: f64, height: f64) -> Self {
        let (top, right, bottom, left) = if height > width {
            (height * 0.12, width * 0.06, height * 0.18, width * 0.06)
        } else {
            (64.0, 64.0, 64.0, 64.0)
        };
        let hx = width / 2.0;
        let hy = height / 2.0;
        Self {
            left: -hx + left,
            top: -hy + top,
            right: hx - right,
            bottom: hy - bottom,
        }
    }

    /// Width of the safe rectangle.
    pub fn width(self) -> f64 {
        (self.right - self.left).max(0.0)
    }

    /// Height of the safe rectangle.
    pub fn height(self) -> f64 {
        (self.bottom - self.top).max(0.0)
    }
}

/// How a node is placed. Resolved every frame; not set by scene code directly.
#[derive(Debug, Clone, Copy)]
pub enum Place {
    /// Outside `other`, on `side`, separated by `gap` pixels.
    NextTo {
        /// The neighbor.
        other: NodeId,
        /// Which side of the neighbor.
        side: Side,
        /// Pixels between the two boxes.
        gap: f64,
    },
    /// This node's `edge` lines up with the neighbor's same edge.
    AlignTo {
        /// The neighbor.
        other: NodeId,
        /// The edge to share.
        edge: Edge,
    },
    /// Against the safe area, on `edge`. The other axis stays where it is.
    ToEdge {
        /// Which edge of the safe area.
        edge: Edge,
    },
    /// Same center as `other`. The view centers on the frame.
    CenterIn {
        /// The node to center on.
        other: NodeId,
    },
}

impl Place {
    pub(crate) fn next_to(other: NodeId, side: Side) -> Self {
        Self::NextTo {
            other,
            side,
            gap: theme::current().space(Step::Md),
        }
    }

    pub(crate) fn align_to(other: NodeId, edge: Edge) -> Self {
        Self::AlignTo { other, edge }
    }

    pub(crate) fn to_edge(edge: Edge) -> Self {
        Self::ToEdge { edge }
    }

    pub(crate) fn center_in(other: NodeId) -> Self {
        Self::CenterIn { other }
    }
}

/// A width or height signal. Percentages read the frame size, or the
/// parent's fixed size when it has one, and follow that size.
pub(crate) fn span_signal(
    graph: &SceneGraph,
    parent: NodeId,
    axis: Axis,
    span: Span,
    name: String,
) -> Signal<f64> {
    match span {
        Span::Px(v) => Signal::new(v).with_name(name),
        Span::Pct(p) => {
            let base = reference(graph, parent, axis);
            let sig = Signal::new(0.0).with_name(name);
            sig.bind(move || base() * p / 100.0);
            sig
        }
    }
}

/// Like [`span_signal`] for a layout's optional fixed size.
pub(crate) fn span_option(
    graph: &SceneGraph,
    parent: NodeId,
    axis: Axis,
    span: Option<Span>,
    name: String,
) -> Signal<Option<f64>> {
    match span {
        None => Signal::new(None).with_name(name),
        Some(Span::Px(v)) => Signal::new(Some(v)).with_name(name),
        Some(Span::Pct(p)) => {
            let base = reference(graph, parent, axis);
            let sig = Signal::new(None).with_name(name);
            sig.bind(move || Some(base() * p / 100.0));
            sig
        }
    }
}

fn reference(graph: &SceneGraph, parent: NodeId, axis: Axis) -> Box<dyn Fn() -> f64> {
    let (fw, fh) = graph.size();
    let frame = match axis {
        Axis::X => f64::from(fw),
        Axis::Y => f64::from(fh),
    };
    if parent == SceneGraph::ROOT {
        return Box::new(move || frame);
    }
    let node = graph.node(parent);
    match &node.kind {
        NodeKind::Rect { width, height, .. } => {
            let s = match axis {
                Axis::X => width.clone(),
                Axis::Y => height.clone(),
            };
            Box::new(move || s.get())
        }
        NodeKind::Img { width, height, .. } | NodeKind::Grid { width, height, .. } => {
            let s = match axis {
                Axis::X => width.clone(),
                Axis::Y => height.clone(),
            };
            Box::new(move || s.get())
        }
        NodeKind::Layout { width, height, .. } => {
            let s = match axis {
                Axis::X => width.clone(),
                Axis::Y => height.clone(),
            };
            Box::new(move || s.get().unwrap_or(frame))
        }
        _ => Box::new(move || frame),
    }
}

/// Writes each node's resolved position into [`NodeData::placed`].
pub(crate) fn resolve(nodes: &[NodeData], frame: (f64, f64)) {
    let passes = nodes.len().clamp(1, 8);
    for _ in 0..passes {
        let mut changed = false;
        for (i, n) in nodes.iter().enumerate() {
            let id = NodeId(u32::try_from(i).unwrap_or(u32::MAX));
            let (w, h) = measure(nodes, id);
            let xy = match n.place {
                None => (n.x.get(), n.y.get()),
                Some(place) => placed(nodes, id, (w, h), place, frame),
            };
            let prev = n.placed.get();
            if (prev.0 - xy.0).abs() > 1e-4 || (prev.1 - xy.1).abs() > 1e-4 {
                n.placed.set(xy);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

fn placed(
    nodes: &[NodeData],
    id: NodeId,
    size: (f64, f64),
    place: Place,
    frame: (f64, f64),
) -> (f64, f64) {
    let n = &nodes[id.0 as usize];
    let (w, h) = size;
    match place {
        Place::ToEdge { edge } => {
            let safe = SafeArea::for_frame(frame.0, frame.1);
            let (mut cx, mut cy) = anchor_center(n, w, h, n.x.get(), n.y.get());
            match edge {
                Edge::Top => cy = safe.top + h / 2.0,
                Edge::Bottom => cy = safe.bottom - h / 2.0,
                Edge::Left => cx = safe.left + w / 2.0,
                Edge::Right => cx = safe.right - w / 2.0,
            }
            from_center(n, w, h, cx, cy)
        }
        Place::CenterIn { other } => {
            let (cx, cy) = scene_center(nodes, other);
            let (cx, cy) = to_parent(nodes, id, cx, cy);
            from_center(n, w, h, cx, cy)
        }
        Place::AlignTo { other, edge } => {
            let (ox, oy) = scene_center(nodes, other);
            let (ow, oh) = measure(nodes, other);
            let (sx, sy) = scene_center(nodes, id);
            let (mut cx, mut cy) = (sx, sy);
            match edge {
                Edge::Top => cy = oy - oh / 2.0 + h / 2.0,
                Edge::Bottom => cy = oy + oh / 2.0 - h / 2.0,
                Edge::Left => cx = ox - ow / 2.0 + w / 2.0,
                Edge::Right => cx = ox + ow / 2.0 - w / 2.0,
            }
            let (cx, cy) = to_parent(nodes, id, cx, cy);
            from_center(n, w, h, cx, cy)
        }
        Place::NextTo { other, side, gap } => {
            let (ox, oy) = scene_center(nodes, other);
            let (ow, oh) = measure(nodes, other);
            let (cx, cy) = match side {
                Side::Right => (ox + ow / 2.0 + gap + w / 2.0, oy),
                Side::Left => (ox - ow / 2.0 - gap - w / 2.0, oy),
                Side::Bottom => (ox, oy + oh / 2.0 + gap + h / 2.0),
                Side::Top => (ox, oy - oh / 2.0 - gap - h / 2.0),
            };
            let (cx, cy) = to_parent(nodes, id, cx, cy);
            from_center(n, w, h, cx, cy)
        }
    }
}

fn anchor_center(n: &NodeData, w: f64, h: f64, x: f64, y: f64) -> (f64, f64) {
    let a = n.anchor.get();
    (x - a.x * w / 2.0, y - a.y * h / 2.0)
}

fn from_center(n: &NodeData, w: f64, h: f64, cx: f64, cy: f64) -> (f64, f64) {
    let a = n.anchor.get();
    (cx + a.x * w / 2.0, cy + a.y * h / 2.0)
}

/// Center of a node in scene space. The view is the frame center.
fn scene_center(nodes: &[NodeData], id: NodeId) -> (f64, f64) {
    if id == SceneGraph::ROOT {
        return (0.0, 0.0);
    }
    let n = &nodes[id.0 as usize];
    let (w, h) = measure(nodes, id);
    let (x, y) = n.placed.get();
    let (mut cx, mut cy) = anchor_center(n, w, h, x, y);
    let mut cur = n.parent;
    while let Some(p) = cur {
        if p == SceneGraph::ROOT {
            break;
        }
        let parent = &nodes[p.0 as usize];
        let (px, py) = parent.placed.get();
        cx += px;
        cy += py;
        cur = parent.parent;
    }
    (cx, cy)
}

fn to_parent(nodes: &[NodeData], id: NodeId, mut x: f64, mut y: f64) -> (f64, f64) {
    let mut cur = nodes[id.0 as usize].parent;
    while let Some(p) = cur {
        if p == SceneGraph::ROOT {
            break;
        }
        let (px, py) = nodes[p.0 as usize].placed.get();
        x -= px;
        y -= py;
        cur = nodes[p.0 as usize].parent;
    }
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::NodeBuilder;
    use crate::{Edge, Rect, SceneGraph, Side, View, pct};

    fn scene(w: u32, h: u32, f: impl FnOnce(View)) -> std::rc::Rc<SceneGraph> {
        let graph = SceneGraph::new(w, h);
        graph.enter(|| f(View::current()));
        graph
    }

    fn bounds(g: &SceneGraph, name: &str) -> [f64; 4] {
        g.inspect_nodes()
            .into_iter()
            .find(|n| n.name == name)
            .unwrap()
            .bounds
    }

    #[test]
    fn next_to_follows_move_and_resize() {
        let g = SceneGraph::new(1920, 1080);
        g.enter(|| {
            let view = View::current();
            let card = view.add(Rect::new().width(200.0).height(100.0).name("card"));
            view.add(
                Rect::new()
                    .size(20.0)
                    .next_to(card, Side::Right)
                    .name("mark"),
            );
            let gap = theme::current().space(Step::Md);
            let b = bounds(&g, "mark");
            assert!((b[0] - (100.0 + gap)).abs() < 1.0, "{b:?}");

            card.x().set(80.0);
            card.width().set(400.0);
            let b = bounds(&g, "mark");
            // card center 80, half width 200, then gap, then the mark's left edge.
            assert!((b[0] - (80.0 + 200.0 + gap)).abs() < 1.0, "{b:?}");
        });
    }

    #[test]
    fn percent_width_and_to_edge_use_the_frame() {
        let g = scene(1000, 500, |view| {
            view.add(Rect::new().width(pct(80.0)).height(100.0).name("wide"));
            view.add(Rect::new().size(100.0).to_edge(Edge::Top).name("title"));
            assert!(view.safe_area().top < 0.0);
            assert!(!view.is_portrait());
        });
        let wide = bounds(&g, "wide");
        assert!((wide[2] - wide[0] - 800.0).abs() < 1.0, "{wide:?}");
        let title = bounds(&g, "title");
        let safe = SafeArea::for_frame(1000.0, 500.0);
        assert!((title[1] - safe.top).abs() < 1.0, "{title:?} vs {safe:?}");
    }

    #[test]
    fn portrait_safe_area_clears_the_social_chrome() {
        let g = scene(1080, 1920, |view| {
            assert!(view.is_portrait());
            view.add(Rect::new().size(80.0).to_edge(Edge::Bottom).name("end"));
        });
        let end = bounds(&g, "end");
        let safe = SafeArea::for_frame(1080.0, 1920.0);
        assert!((end[3] - safe.bottom).abs() < 1.0, "{end:?}");
        assert!(safe.bottom < 960.0 - 64.0);
    }
}
