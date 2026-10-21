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
    fn build(self, g: &oxframe_2d::SceneGraph, parent: NodeId) -> BulletList {
        let root = host::root(parent, self.base);
        // A long row wraps inside 70 % of the frame instead of running off
        // both edges; the text starts right of the mark.
        const MARK: f64 = 18.0;
        const INDENT: f64 = 34.0;
        const GAP: f64 = 24.0;
        let wrap = (g.frame_size().0 * 0.7 - INDENT).max(200.0);
        let probe = root.add(Txt::new().text("Ag").body().opacity(0.0));
        let line_height = probe.size().1.max(MARK);
        probe.remove();
        let mut rows = Vec::new();
        for (i, text) in self.items.into_iter().enumerate() {
            let row: Group = root.add(Group::new().opacity(0.0).name(format!("item{}", i + 1)));
            let mark = row.add(Rect::new().size(MARK).fill(Accent).name("mark"));
            let line = row.add(
                Txt::new()
                    .text(text)
                    .body()
                    .max_width(wrap)
                    .anchor((-1.0, -1.0))
                    .name("line"),
            );
            let (w, h) = line.size();
            rows.push((row, mark, line, w, h.max(line_height)));
        }
        let widest = rows.iter().map(|r| r.3).fold(0.0, f64::max);
        let left = -(INDENT + widest) / 2.0;
        let total: f64 =
            rows.iter().map(|r| r.4).sum::<f64>() + GAP * rows.len().saturating_sub(1) as f64;
        let mut top = -total / 2.0;
        let mut reveals = Vec::new();
        for (row, mark, line, _, h) in rows {
            row.y().set(top);
            line.x().set(left + INDENT);
            mark.x().set(left + MARK / 2.0);
            // Level with the first line of a wrapped row.
            mark.y().set(line_height / 2.0);
            top += h + GAP;
            reveals.push(row.node());
        }
        BulletList {
            host: Host::new(root.node(), reveals),
        }
    }
}

#[cfg(test)]
mod tests {
    use oxframe_2d::{SceneGraph, View};

    use super::*;

    #[test]
    fn a_long_row_wraps_inside_the_frame_and_rows_do_not_overlap() {
        let graph = SceneGraph::new(1920, 1080);
        graph.enter(|| {
            View::current().add(BulletList::new([
                "Short",
                "A row much longer than the frame is wide, so that it has to wrap onto a second \
                 line or it would run off both edges of the picture",
                "Last",
            ]));
        });
        let lines: Vec<_> = graph
            .inspect_nodes()
            .into_iter()
            .filter(|n| n.kind == "Txt")
            .map(|n| n.bounds)
            .collect();
        assert_eq!(lines.len(), 3);
        for b in &lines {
            assert!(
                b[0] >= -960.0 && b[2] <= 960.0,
                "row leaves the frame: {b:?}"
            );
        }
        assert!(lines[1][2] - lines[1][0] <= 1920.0 * 0.7, "{:?}", lines[1]);
        assert!(
            lines[1][3] - lines[1][1] > lines[0][3] - lines[0][1],
            "the long row wraps"
        );
        for pair in lines.windows(2) {
            assert!(pair[0][3] <= pair[1][1], "rows overlap: {pair:?}");
        }
    }
}
