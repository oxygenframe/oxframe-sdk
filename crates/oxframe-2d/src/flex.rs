//! Flexbox and grid layout for `Layout` nodes, computed with taffy.
//!
//! One taffy tree is built per top-level `Layout` (nested layouts are
//! containers in the same tree). Results are the boxes of every node inside,
//! relative to the top-left of their parent layout.

use std::collections::BTreeMap;

use kurbo::Vec2;
use taffy::prelude::*;

use crate::graph::{Direction, Justify, LayoutAlign, NodeData, NodeId, NodeKind};

/// Box of a node inside its parent layout: offset of its center from the
/// parent layout's center, and its size (in the parent's units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FlexBox {
    pub center: Vec2,
    pub size: (f64, f64),
}

pub(crate) type FlexMap = BTreeMap<NodeId, FlexBox>;

fn align(a: LayoutAlign) -> AlignItems {
    match a {
        LayoutAlign::Start => AlignItems::FLEX_START,
        LayoutAlign::Center => AlignItems::CENTER,
        LayoutAlign::End => AlignItems::FLEX_END,
        LayoutAlign::Stretch => AlignItems::STRETCH,
    }
}

fn justify(j: Justify) -> JustifyContent {
    match j {
        Justify::Start => JustifyContent::FLEX_START,
        Justify::Center => JustifyContent::CENTER,
        Justify::End => JustifyContent::FLEX_END,
        Justify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
        Justify::SpaceAround => JustifyContent::SPACE_AROUND,
        Justify::SpaceEvenly => JustifyContent::SPACE_EVENLY,
    }
}

fn len(v: f64) -> LengthPercentage {
    LengthPercentage::length(v as f32)
}

fn dim(v: Option<f64>) -> Dimension {
    v.map_or(Dimension::auto(), |v| Dimension::length(v.max(0.0) as f32))
}

/// Flex item properties every node has.
fn item_style(n: &NodeData, mut style: Style) -> Style {
    style.flex_grow = n.grow.get().max(0.0) as f32;
    style.flex_shrink = n.shrink.get().max(0.0) as f32;
    style.flex_basis = dim(n.basis.get());
    let m = LengthPercentageAuto::length(n.margin.get() as f32);
    style.margin = Rect {
        left: m,
        right: m,
        top: m,
        bottom: m,
    };
    style.align_self = n.align_self.get().map(align);
    style
}

/// How the parent container lays this node out.
#[derive(Clone, Copy)]
struct Parent {
    direction: Direction,
    align: LayoutAlign,
    grid: bool,
}

fn build(
    tree: &mut TaffyTree<()>,
    nodes: &[NodeData],
    id: NodeId,
    parent: Option<Parent>,
    intrinsic: &dyn Fn(NodeId) -> (f64, f64),
    ids: &mut Vec<(NodeId, taffy::NodeId)>,
) -> taffy::NodeId {
    let n = &nodes[id.0 as usize];
    let NodeKind::Layout {
        direction,
        align: items,
        justify: content,
        wrap,
        columns,
        gap,
        padding,
        width,
        height,
        ..
    } = &n.kind
    else {
        // Leaf: its intrinsic (scaled) size; grow/shrink may change it.
        // Stretched leaves give up their cross-axis size, so that stretch
        // works on rects and images that have an explicit size.
        let (w, h) = intrinsic(id);
        let s = n.scale.get();
        let mut width = Dimension::length((w * s.x.abs()) as f32);
        let mut height = Dimension::length((h * s.y.abs()) as f32);
        if let Some(p) = parent
            && n.align_self.get().unwrap_or(p.align) == LayoutAlign::Stretch
        {
            match (p.grid, p.direction) {
                (true, _) | (false, Direction::Column) => width = Dimension::auto(),
                (false, Direction::Row) => height = Dimension::auto(),
            }
        }
        let style = item_style(
            n,
            Style {
                size: Size { width, height },
                ..Default::default()
            },
        );
        let t = tree.new_leaf(style).expect("taffy leaf");
        ids.push((id, t));
        return t;
    };
    let me = Parent {
        direction: *direction,
        align: *items,
        grid: columns.is_some(),
    };
    let children: Vec<taffy::NodeId> = n
        .children
        .iter()
        .map(|&c| build(tree, nodes, c, Some(me), intrinsic, ids))
        .collect();
    let (g, p) = (gap.get().max(0.0), padding.get().max(0.0));
    let mut style = Style {
        display: if columns.is_some() {
            Display::Grid
        } else {
            Display::Flex
        },
        flex_direction: match direction {
            Direction::Row => FlexDirection::Row,
            Direction::Column => FlexDirection::Column,
        },
        flex_wrap: if *wrap {
            FlexWrap::Wrap
        } else {
            FlexWrap::NoWrap
        },
        align_items: align(*items),
        align_content: match content {
            Justify::Start => AlignContent::FLEX_START,
            Justify::Center => AlignContent::CENTER,
            Justify::End => AlignContent::FLEX_END,
            Justify::SpaceBetween => AlignContent::SPACE_BETWEEN,
            Justify::SpaceAround => AlignContent::SPACE_AROUND,
            Justify::SpaceEvenly => AlignContent::SPACE_EVENLY,
        },
        justify_content: justify(*content),
        gap: Size {
            width: len(g),
            height: len(g),
        },
        padding: Rect {
            left: len(p),
            right: len(p),
            top: len(p),
            bottom: len(p),
        },
        size: Size {
            width: dim(width.get()),
            height: dim(height.get()),
        },
        ..Default::default()
    };
    if let Some(c) = columns {
        style.grid_template_columns = evenly_sized_tracks(*c);
        style.justify_items = align(*items);
    }
    let style = item_style(n, style);
    let t = tree
        .new_with_children(style, &children)
        .expect("taffy node");
    ids.push((id, t));
    t
}

/// Lays out the layout `root` and everything nested in it. Returns the
/// root's size and the box of every descendant.
pub(crate) fn compute(
    nodes: &[NodeData],
    root: NodeId,
    intrinsic: &dyn Fn(NodeId) -> (f64, f64),
) -> ((f64, f64), FlexMap) {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    // Sub-pixel positions keep animations smooth.
    tree.disable_rounding();
    let mut ids = Vec::new();
    let t_root = build(&mut tree, nodes, root, None, intrinsic, &mut ids);
    tree.compute_layout(t_root, Size::MAX_CONTENT)
        .expect("taffy layout");
    let root_layout = tree.layout(t_root).expect("root layout");
    let root_size = (
        f64::from(root_layout.size.width),
        f64::from(root_layout.size.height),
    );
    let mut out = FlexMap::new();
    let by_id: BTreeMap<NodeId, taffy::NodeId> = ids.iter().copied().collect();
    for &(id, t) in &ids {
        if id == root {
            continue;
        }
        let Some(parent) = nodes[id.0 as usize].parent else {
            continue;
        };
        let Some(&tp) = by_id.get(&parent) else {
            continue;
        };
        let l = tree.layout(t).expect("layout");
        let pl = tree.layout(tp).expect("parent layout");
        let size = (f64::from(l.size.width), f64::from(l.size.height));
        let center = Vec2::new(
            f64::from(l.location.x) + size.0 / 2.0 - f64::from(pl.size.width) / 2.0,
            f64::from(l.location.y) + size.1 / 2.0 - f64::from(pl.size.height) / 2.0,
        );
        out.insert(id, FlexBox { center, size });
    }
    (root_size, out)
}
