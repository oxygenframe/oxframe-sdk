//! A name plate at the bottom of the frame, and a chapter label at the top.

use oxframe_2d::{
    BaseProps, Edge, Group, Lg, Md, Muted, NodeBuilder, NodeId, Rect, Surface, Txt, pct,
};
use oxframe_core::Tween;

use crate::host::{self, Host};

/// Name and role pinned to the bottom safe area. `next` reveals the role.
pub struct LowerThird {
    host: Host,
}

impl LowerThird {
    /// The name is visible after `enter`. The role waits for `next`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(name: impl Into<String>) -> LowerThirdBuilder {
        LowerThirdBuilder {
            base: BaseProps::default(),
            name: name.into(),
            role: None,
        }
    }

    /// Fades the plate in.
    pub fn enter(&self) -> Tween<f64> {
        self.host.enter()
    }
    /// Fades the role in.
    pub fn next(&self) -> Tween<f64> {
        self.host.next()
    }
    /// Fades the plate out.
    pub fn exit(&self) -> Tween<f64> {
        self.host.exit()
    }
}

/// Builder for [`LowerThird`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct LowerThirdBuilder {
    base: BaseProps,
    name: String,
    role: Option<String>,
}

impl LowerThirdBuilder {
    /// Second line, revealed by `next`.
    pub fn role(mut self, text: impl Into<String>) -> Self {
        self.role = Some(text.into());
        self
    }
}

impl NodeBuilder for LowerThirdBuilder {
    type Handle = LowerThird;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(self, _: &oxframe_2d::SceneGraph, parent: NodeId) -> LowerThird {
        let root: Group = host::root(parent, self.base);
        let root = root.to_edge(Edge::Bottom);
        root.add(
            Rect::new()
                .width(pct(70))
                .height(140.0)
                .radius(Lg)
                .fill(Surface)
                .elevation(Md)
                .name("plate"),
        );
        root.add(Txt::new().text(self.name).h2().y(-24.0).name("name"));
        let mut reveals = Vec::new();
        if let Some(role) = self.role {
            let line = root.add(
                Txt::new()
                    .text(role)
                    .caption()
                    .fill(Muted)
                    .y(36.0)
                    .opacity(0.0)
                    .name("role"),
            );
            reveals.push(line.node());
        }
        LowerThird {
            host: Host::new(root, reveals),
        }
    }
}

/// A short label pinned to the top safe area.
pub struct ChapterMarker {
    host: Host,
}

impl ChapterMarker {
    /// The label is the whole marker. `next` holds it.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(label: impl Into<String>) -> ChapterMarkerBuilder {
        ChapterMarkerBuilder {
            base: BaseProps::default(),
            label: label.into(),
        }
    }

    /// Fades the label in.
    pub fn enter(&self) -> Tween<f64> {
        self.host.enter()
    }
    /// Holds the label.
    pub fn next(&self) -> Tween<f64> {
        self.host.next()
    }
    /// Fades the label out.
    pub fn exit(&self) -> Tween<f64> {
        self.host.exit()
    }
}

/// Builder for [`ChapterMarker`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct ChapterMarkerBuilder {
    base: BaseProps,
    label: String,
}

impl NodeBuilder for ChapterMarkerBuilder {
    type Handle = ChapterMarker;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(self, _: &oxframe_2d::SceneGraph, parent: NodeId) -> ChapterMarker {
        let root: Group = host::root(parent, self.base);
        let root = root.to_edge(Edge::Top);
        root.add(Txt::new().text(self.label).h2().name("chapter"));
        ChapterMarker {
            host: Host::new(root, Vec::new()),
        }
    }
}
