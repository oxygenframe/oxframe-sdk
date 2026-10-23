//! An arrow and a caption aimed at another node.

use oxframe_2d::{
    Accent, BaseProps, Group, Len, Line, Md, Muted, Node, NodeBuilder, NodeId, Side, Txt,
};
use oxframe_core::Tween;

use crate::host::{self, Host};

/// A caption above `target`, with an arrow pointing down at it.
/// `next` reveals the caption; the arrow comes in with `enter`.
pub struct Callout {
    host: Host,
}

impl Callout {
    /// The caption stays hidden until `next`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(target: impl Into<Node>) -> CalloutBuilder {
        CalloutBuilder {
            base: BaseProps::default(),
            target: target.into(),
            text: String::new(),
        }
    }

    /// Fades the arrow in.
    pub fn enter(&self) -> Tween<f64> {
        self.host.enter()
    }
    /// Fades the caption in.
    pub fn next(&self) -> Tween<f64> {
        self.host.next()
    }
    /// Fades the callout out.
    pub fn exit(&self) -> Tween<f64> {
        self.host.exit()
    }
}

/// Builder for [`Callout`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct CalloutBuilder {
    base: BaseProps,
    target: Node,
    text: String,
}

impl CalloutBuilder {
    /// Caption text.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }
}

impl NodeBuilder for CalloutBuilder {
    type Handle = Callout;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(self, _: &oxframe_2d::SceneGraph, parent: NodeId) -> Callout {
        let root: Group = host::root(parent, self.base);
        let root = root.next_to(self.target, Side::Top);
        let gap = Md.px();
        root.add(
            Line::new()
                .points([(0.0, 28.0), (0.0, 28.0 + gap)])
                .end_arrow(true)
                .stroke(Accent)
                .line_width(4.0)
                .name("arrow"),
        );
        let caption = root.add(
            Txt::new()
                .text(self.text)
                .caption()
                .fill(Muted)
                .opacity(0.0)
                .name("caption"),
        );
        Callout {
            host: Host::new(root, vec![caption.node()]),
        }
    }
}

into_node! {
    Callout => |c| c.host.root;
}
