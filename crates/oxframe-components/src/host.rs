//! Shared builder bookkeeping: a hidden root, and reveals advanced by `next`.

use std::cell::Cell;
use std::rc::Rc;

use oxframe_2d::{BaseProps, Fast, Group, Node, NodeBuilder, NodeId};
use oxframe_core::Tween;

pub struct Host {
    pub root: Node,
    reveals: Rc<Vec<Node>>,
    cursor: Rc<Cell<usize>>,
}

impl Host {
    pub fn new(root: Node, reveals: Vec<Node>) -> Self {
        Self {
            root,
            reveals: Rc::new(reveals),
            cursor: Rc::new(Cell::new(0)),
        }
    }

    pub fn enter(&self) -> Tween<f64> {
        self.root.fade_in()
    }

    pub fn exit(&self) -> Tween<f64> {
        self.root.fade_out()
    }

    /// Fades in the next prepared child. Past the end, holds the root.
    pub fn next(&self) -> Tween<f64> {
        let i = self.cursor.get();
        if let Some(node) = self.reveals.get(i) {
            self.cursor.set(i + 1);
            node.fade_in()
        } else {
            self.root.opacity().to(1.0, Fast)
        }
    }

    pub fn len(&self) -> usize {
        self.reveals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.reveals.is_empty()
    }
}

pub fn root(parent: NodeId, base: BaseProps) -> Group {
    let parent = Node::from_id(parent);
    let mut group = Group::new().opacity(0.0).x(base.x).y(base.y);
    if let Some(name) = base.name {
        group = group.name(name);
    }
    parent.add(group)
}
