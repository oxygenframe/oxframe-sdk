//! Nodes of your own: a struct that draws itself with a [`DrawCtx`].
//!
//! Most code reaches this through `#[derive(Node)]` in `oxframe`, which
//! writes the builder, the handle and the signals. The functions here are
//! what that derive calls; they are public so the expansion compiles in
//! your crate.

use std::any::Any;
use std::rc::Rc;

use crate::draw::DrawCtx;
use crate::graph::{NodeId, NodeKind, SceneGraph, with_graph};
use crate::handles::Node;

/// Size of a node, computed from its current values.
pub type SizeFn = Rc<dyn Fn() -> (f64, f64)>;

/// What a node of your own draws, in its own coordinates (origin at the
/// center of its box, y down).
///
/// `draw` must be a function of the node's values and `ctx.time()`: the
/// frame is painted again on seek, and `check` reports a node whose two
/// paints of one frame differ. Use `ctx.rng(seed)` for randomness.
pub trait Draw: 'static {
    /// Paints the node with `ctx.line`, `ctx.circle`, `ctx.rect`,
    /// `ctx.arc` and `ctx.path`.
    fn draw(&self, ctx: &mut DrawCtx);

    /// The node's box: what layout, `Camera::fit` and `check` use. The
    /// default is 100 by 100.
    fn size(&self) -> (f64, f64) {
        (100.0, 100.0)
    }
}

/// Adds a node that draws `snapshot()` every frame.
///
/// `snapshot` rebuilds the value from the node's signals, so `draw` sees
/// the values of the frame being painted. `state` holds those signals; the
/// handle gets it back with [`state`].
#[doc(hidden)]
pub fn add<T: Draw>(
    graph: &SceneGraph,
    parent: NodeId,
    base: crate::BaseProps,
    type_name: &'static str,
    snapshot: Rc<dyn Fn() -> T>,
    state: Rc<dyn Any>,
) -> NodeId {
    let paint = snapshot.clone();
    let measure = snapshot;
    let kind = NodeKind::Custom {
        draw: Rc::new(move |ctx: &mut DrawCtx, _| paint().draw(ctx)),
        width: 0.0,
        height: 0.0,
        measure: Some(Rc::new(move || measure().size())),
        state: Some(state),
        type_name,
    };
    crate::builder::insert(graph, parent, kind, base)
}

/// The signals stored by [`add`] for `node`.
///
/// # Panics
///
/// If `node` was not added with state of type `S`.
#[doc(hidden)]
pub fn state<S: 'static>(node: Node) -> Rc<S> {
    with_graph(|g| match &g.node(node.id()).kind {
        NodeKind::Custom {
            state: Some(s),
            type_name,
            ..
        } => s
            .clone()
            .downcast::<S>()
            .unwrap_or_else(|_| panic!("{type_name} node has state of another type")),
        _ => panic!("this handle does not point to a derived node"),
    })
}
