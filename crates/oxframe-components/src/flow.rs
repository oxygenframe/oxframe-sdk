//! A graph of named nodes, laid out in layers from the edges.

use std::collections::BTreeMap;

use oxframe_2d::{
    Accent, BaseProps, Group, Icon, IconBuilder, Len, Lg, Line, Md, Muted, NodeBuilder, NodeId,
    Rect, Surface, Txt, Xl,
};
use oxframe_core::Tween;

use crate::host::{self, Host};

/// Nodes and arrows. The diagram places itself: left to right, or top to
/// bottom after [`down`](FlowDiagramBuilder::down). `next` highlights the
/// next node.
pub struct FlowDiagram {
    host: Host,
    icons: BTreeMap<String, Icon>,
}

impl FlowDiagram {
    /// An empty diagram. Add links with [`edge`](FlowDiagramBuilder::edge).
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> FlowDiagramBuilder {
        FlowDiagramBuilder {
            base: BaseProps::default(),
            edges: Vec::new(),
            icons: BTreeMap::new(),
            down: false,
        }
    }

    /// Fades the diagram in. Highlights are still hidden.
    pub fn enter(&self) -> Tween<f64> {
        self.host.enter()
    }
    /// Shows the accent bar on the next node.
    pub fn next(&self) -> Tween<f64> {
        self.host.next()
    }
    /// Fades the diagram out.
    pub fn exit(&self) -> Tween<f64> {
        self.host.exit()
    }
    /// How many nodes `next` can highlight.
    pub fn len(&self) -> usize {
        self.host.len()
    }
    /// No nodes were declared.
    pub fn is_empty(&self) -> bool {
        self.host.is_empty()
    }
    /// The icon of the node `name`, from [`icon`](FlowDiagramBuilder::icon).
    pub fn icon(&self, name: &str) -> Option<Icon> {
        self.icons.get(name).cloned()
    }
}

/// Builder for [`FlowDiagram`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct FlowDiagramBuilder {
    base: BaseProps,
    edges: Vec<(String, String)>,
    icons: BTreeMap<String, IconBuilder>,
    down: bool,
}

impl FlowDiagramBuilder {
    /// A directed link. Nodes are created the first time their name appears.
    pub fn edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.edges.push((from.into(), to.into()));
        self
    }
    /// An icon at the start of the node `name`, such as
    /// `.icon("db", icon!("lucide:database"))`. Its size is set to fit the card.
    pub fn icon(mut self, name: impl Into<String>, icon: IconBuilder) -> Self {
        self.icons.insert(name.into(), icon);
        self
    }
    /// Stack layers top to bottom instead of left to right.
    pub fn down(mut self) -> Self {
        self.down = true;
        self
    }
}

impl NodeBuilder for FlowDiagramBuilder {
    type Handle = FlowDiagram;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(mut self, _: &oxframe_2d::SceneGraph, parent: NodeId) -> FlowDiagram {
        let root: Group = host::root(parent, self.base);
        let mut index = BTreeMap::<String, usize>::new();
        let mut names = Vec::new();
        let mut links = Vec::new();
        for (from, to) in self.edges {
            let a = *index.entry(from.clone()).or_insert_with(|| {
                names.push(from);
                names.len() - 1
            });
            let b = *index.entry(to.clone()).or_insert_with(|| {
                names.push(to);
                names.len() - 1
            });
            links.push((a, b));
        }
        let layers = layers_of(names.len(), &links);
        // An icon takes the start of the card, so cards with icons are wider.
        let card_w = if self.icons.is_empty() { 200.0 } else { 260.0 };
        let card_h = 84.0;
        let gap = Md.px();
        let layer_gap = Xl.px();
        let max_layer = layers.iter().copied().max().unwrap_or(0);
        let mut counts = vec![0usize; max_layer + 1];
        for layer in &layers {
            counts[*layer] += 1;
        }
        let along = (max_layer as f64) * (card_w + layer_gap);
        let mut slot = vec![0usize; max_layer + 1];
        let mut pos = vec![(0.0, 0.0); names.len()];
        for (i, layer) in layers.iter().copied().enumerate() {
            let n = counts[layer].max(1) as f64;
            let across = (n - 1.0) * (card_h + gap);
            let a = -(along / 2.0) + layer as f64 * (card_w + layer_gap);
            let b = -(across / 2.0) + slot[layer] as f64 * (card_h + gap);
            slot[layer] += 1;
            pos[i] = if self.down { (b, a) } else { (a, b) };
        }
        let mut reveals = Vec::new();
        let mut icons = BTreeMap::new();
        for (i, name) in names.iter().enumerate() {
            let (x, y) = pos[i];
            root.add(
                Rect::new()
                    .width(card_w)
                    .height(card_h)
                    .radius(Lg)
                    .fill(Surface)
                    .x(x)
                    .y(y)
                    .name(name.clone()),
            );
            let label = root.add(
                Txt::new()
                    .text(name.clone())
                    .body()
                    .x(x)
                    .y(y)
                    .name(format!("{name}-label")),
            );
            // With an icon, the label starts right of it.
            if let Some(icon) = self.icons.remove(name) {
                let size = 40.0;
                let at = x - card_w / 2.0 + 36.0 + size / 2.0;
                icons.insert(
                    name.clone(),
                    root.add(icon.size(size).x(at).y(y).name(format!("{name}-icon"))),
                );
                label.x().set(at + size / 2.0 + 16.0 + label.size().0 / 2.0);
            }
            let bar = root.add(
                Rect::new()
                    .width(8.0)
                    .height(card_h - 28.0)
                    .fill(Accent)
                    .x(x - card_w / 2.0 + 16.0)
                    .y(y)
                    .opacity(0.0)
                    .name(format!("{name}-bar")),
            );
            reveals.push(bar.node());
        }
        for (a, b) in links {
            let (x1, y1) = pos[a];
            let (x2, y2) = pos[b];
            let (start, end) = if self.down {
                ((x1, y1 + card_h / 2.0), (x2, y2 - card_h / 2.0))
            } else {
                ((x1 + card_w / 2.0, y1), (x2 - card_w / 2.0, y2))
            };
            root.add(
                Line::new()
                    .points([start, end])
                    .end_arrow(true)
                    .stroke(Muted)
                    .line_width(4.0)
                    .name("link"),
            );
        }
        FlowDiagram {
            host: Host::new(root.node(), reveals),
            icons,
        }
    }
}

fn layers_of(n: usize, links: &[(usize, usize)]) -> Vec<usize> {
    let mut layer = vec![0; n];
    for _ in 0..n.max(1) {
        let mut changed = false;
        for &(from, to) in links {
            let next = layer[from] + 1;
            if next > layer[to] && next < n {
                layer[to] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    layer
}

into_node! {
    FlowDiagram => |c| c.host.root;
}
