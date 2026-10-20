//! Title card, end card, and a list whose items appear one by one.

use oxframe_2d::{
    Accent, BaseProps, Group, Lg, Md, Muted, NodeBuilder, NodeId, Rect, Surface, Txt, pct,
};
use oxframe_core::Tween;

use crate::host::{self, Host};

fn plate(root: &Group, title: String, subtitle: Option<String>) -> Host {
    root.add(
        Rect::new()
            .width(pct(64))
            .height(260.0)
            .radius(Lg)
            .fill(Surface)
            .elevation(Md)
            .name("plate"),
    );
    root.add(Txt::new().text(title).h1().y(-28.0).name("title"));
    let mut reveals = Vec::new();
    if let Some(text) = subtitle {
        let line = root.add(
            Txt::new()
                .text(text)
                .caption()
                .fill(Muted)
                .y(56.0)
                .opacity(0.0)
                .name("subtitle"),
        );
        reveals.push(line.node());
    }
    Host::new(root.node(), reveals)
}

/// A centered title. `enter` shows the card and the title, `next` shows the subtitle.
pub struct TitleCard {
    host: Host,
}

impl TitleCard {
    /// Starts a card. The subtitle stays hidden until [`next`](Self::next).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(title: impl Into<String>) -> TitleCardBuilder {
        TitleCardBuilder {
            base: BaseProps::default(),
            title: title.into(),
            subtitle: None,
        }
    }

    /// Fades the card in.
    pub fn enter(&self) -> Tween<f64> {
        self.host.enter()
    }
    /// Fades the subtitle in, or holds the card if there is none.
    pub fn next(&self) -> Tween<f64> {
        self.host.next()
    }
    /// Fades the card out.
    pub fn exit(&self) -> Tween<f64> {
        self.host.exit()
    }
}

/// Builder for [`TitleCard`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct TitleCardBuilder {
    base: BaseProps,
    title: String,
    subtitle: Option<String>,
}

impl TitleCardBuilder {
    /// Second line, revealed by `next`.
    pub fn subtitle(mut self, text: impl Into<String>) -> Self {
        self.subtitle = Some(text.into());
        self
    }
}

impl NodeBuilder for TitleCardBuilder {
    type Handle = TitleCard;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(self, _: &oxframe_2d::SceneGraph, parent: NodeId) -> TitleCard {
        let root = host::root(parent, self.base);
        TitleCard {
            host: plate(&root, self.title, self.subtitle),
        }
    }
}

/// Closing card. `next` reveals the note.
pub struct EndCard {
    host: Host,
}

impl EndCard {
    /// Starts an end card.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(title: impl Into<String>) -> EndCardBuilder {
        EndCardBuilder {
            base: BaseProps::default(),
            title: title.into(),
            note: None,
        }
    }

    /// Fades the card in.
    pub fn enter(&self) -> Tween<f64> {
        self.host.enter()
    }
    /// Fades the note in.
    pub fn next(&self) -> Tween<f64> {
        self.host.next()
    }
    /// Fades the card out.
    pub fn exit(&self) -> Tween<f64> {
        self.host.exit()
    }
}

/// Builder for [`EndCard`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct EndCardBuilder {
    base: BaseProps,
    title: String,
    note: Option<String>,
}

impl EndCardBuilder {
    /// Second line, revealed by `next`.
    pub fn note(mut self, text: impl Into<String>) -> Self {
        self.note = Some(text.into());
        self
    }
}

impl NodeBuilder for EndCardBuilder {
    type Handle = EndCard;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(self, _: &oxframe_2d::SceneGraph, parent: NodeId) -> EndCard {
        let root = host::root(parent, self.base);
        EndCard {
            host: plate(&root, self.title, self.note),
        }
    }
}

/// Items appear one per `next`, after `enter` shows the stack.
pub struct BulletList {
    host: Host,
}

impl BulletList {
    /// One row per string. Rows stay hidden until `next`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(items: impl IntoIterator<Item = impl Into<String>>) -> BulletListBuilder {
        BulletListBuilder {
            base: BaseProps::default(),
            items: items.into_iter().map(Into::into).collect(),
        }
    }

    /// Fades the list in. The rows are still hidden.
    pub fn enter(&self) -> Tween<f64> {
        self.host.enter()
    }
    /// Reveals the next row.
    pub fn next(&self) -> Tween<f64> {
        self.host.next()
    }
    /// Fades the list out.
    pub fn exit(&self) -> Tween<f64> {
        self.host.exit()
    }
    /// How many rows `next` can reveal.
    pub fn len(&self) -> usize {
        self.host.len()
    }
    /// No rows were given.
    pub fn is_empty(&self) -> bool {
        self.host.is_empty()
    }
}

/// Builder for [`BulletList`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct BulletListBuilder {
    base: BaseProps,
    items: Vec<String>,
}

impl NodeBuilder for BulletListBuilder {
    type Handle = BulletList;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(self, _: &oxframe_2d::SceneGraph, parent: NodeId) -> BulletList {
        let root = host::root(parent, self.base);
        let n = self.items.len().max(1) as f64;
        let pitch = 72.0;
        let y0 = -(n - 1.0) * pitch / 2.0;
        let mut reveals = Vec::new();
        for (i, text) in self.items.into_iter().enumerate() {
            let row: Group = root.add(
                Group::new()
                    .y(y0 + i as f64 * pitch)
                    .opacity(0.0)
                    .name(format!("item{}", i + 1)),
            );
            row.add(Rect::new().size(18.0).fill(Accent).x(-220.0).name("mark"));
            row.add(Txt::new().text(text).body().x(16.0).name("line"));
            reveals.push(row.node());
        }
        BulletList {
            host: Host::new(root.node(), reveals),
        }
    }
}
