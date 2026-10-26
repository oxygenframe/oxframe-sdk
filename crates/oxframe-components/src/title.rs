//! Title card, end card, and a list whose items appear one by one.

use oxframe_2d::{
    Accent, BaseProps, Circle, Group, Lg, Md, Muted, NodeBuilder, NodeId, Rect, RichSpan,
    SceneGraph, Surface, Text, TextAlign, Txt, TxtBuilder, pct,
};
use oxframe_core::{Color, Tween};

use crate::host::{self, Host};

/// The card behind a title and an optional second line. Both wrap inside
/// the card, and a card whose text wraps grows to hold it; text that fits on
/// one line keeps the usual layout.
fn plate(
    g: &SceneGraph,
    root: &Group,
    title: String,
    spans: Vec<RichSpan>,
    made_by: bool,
    subtitle: Option<String>,
) -> Host {
    const HEIGHT: f64 = 260.0;
    const PAD: f64 = 48.0;
    const GAP: f64 = 20.0;
    let wrap = (g.frame_size().0 * 0.64 - 2.0 * PAD).max(200.0);
    let card = root.add(
        Rect::new()
            .width(pct(64))
            .height(HEIGHT)
            .radius(Lg)
            .fill(Surface)
            .elevation(Md)
            .name("plate"),
    );
    let title = root.add(
        Txt::new()
            .text(title)
            .rich(spans)
            .h1()
            .max_width(wrap)
            .align(TextAlign::Center)
            .y(-28.0)
            .name("title"),
    );
    if made_by {
        // The credit next to the (empty) title, at the title's size.
        credit(root, || Txt::new().h1()).y().set(-28.0);
    }
    let mut reveals = Vec::new();
    let mut second = None;
    if let Some(text) = subtitle {
        let line = root.add(
            Txt::new()
                .text(text)
                .caption()
                .fill(Muted)
                .max_width(wrap)
                .align(TextAlign::Center)
                .y(56.0)
                .opacity(0.0)
                .name("subtitle"),
        );
        reveals.push(line.node());
        second = Some(line);
    }
    // One line each: the layout above. Otherwise stack them in the middle.
    let one_line = |txt: &Txt, probe: TxtBuilder| {
        let p = root.add(probe.text("Ag").opacity(0.0));
        let h = p.size().1;
        p.remove();
        txt.size().1 <= h * 1.5
    };
    let fits = one_line(&title, Txt::new().h1())
        && second
            .as_ref()
            .is_none_or(|s| one_line(s, Txt::new().caption()));
    if !fits {
        let th = title.size().1;
        let sh = second.as_ref().map_or(0.0, |s| s.size().1);
        let total = th + second.as_ref().map_or(0.0, |_| GAP + sh);
        card.height().set((total + 2.0 * PAD).max(HEIGHT));
        title.y().set(-total / 2.0 + th / 2.0);
        if let Some(s) = &second {
            s.y().set(total / 2.0 - sh / 2.0);
        }
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
    fn build(self, g: &oxframe_2d::SceneGraph, parent: NodeId) -> TitleCard {
        let root = host::root(parent, self.base);
        TitleCard {
            host: plate(g, &root, self.title, Vec::new(), false, self.subtitle),
        }
    }
}

/// The oxframe logo, a rounded square and a circle that overlaps it, `height`
/// tall, its left edge at `x`. Returns its width.
fn logo(parent: &Group, x: f64, height: f64) -> f64 {
    // The favicon's geometry: a 14-unit square at 3 and a 14-unit circle
    // centered at 22, out of 32.
    let unit = height / 14.0;
    parent.add(
        Rect::new()
            .size(height)
            .radius(3.0 * unit)
            .fill(Color::from_hex("#e13238").expect("a color"))
            .x(x + height / 2.0)
            .name("logo-square"),
    );
    parent.add(
        Circle::new()
            .size(height)
            .fill(Color::from_hex("#68abdf").expect("a color"))
            .x(x + 19.0 * unit)
            .name("logo-circle"),
    );
    26.0 * unit
}

/// "made by", the logo and "Oxframe" on one line, centered on a new group;
/// `style` sets the size of the words.
fn credit(parent: &Group, style: impl Fn() -> TxtBuilder) -> Group {
    let row = parent.add(Group::new().name("made-by"));
    let lead = row.add(style().text("made by").fill(Muted));
    let name = row.add(style().text("Oxframe").weight(700.0).fill(Text));
    // The line box is about 1.2 times the font size; the logo is a bit
    // taller than a capital.
    let line = lead.size().1;
    let height = line * 0.6;
    let gap = line * 0.26;
    let (lw, nw) = (lead.size().0, name.size().0);
    let mark = 26.0 / 14.0 * height;
    let left = -(lw + gap + mark + gap + nw) / 2.0;
    lead.x().set(left + lw / 2.0);
    logo(&row, left + lw + gap, height);
    name.x().set(left + lw + gap + mark + gap + nw / 2.0);
    row
}

/// "made by", the oxframe logo, and "Oxframe" in the text color, on one line.
/// `enter` fades it in.
pub struct MadeBy {
    host: Host,
}

impl MadeBy {
    /// Starts the credit line. Default size: 48.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> MadeByBuilder {
        MadeByBuilder {
            base: BaseProps::default(),
            size: 48.0,
        }
    }

    /// Fades the line in.
    pub fn enter(&self) -> Tween<f64> {
        self.host.enter()
    }
    /// Fades the line out.
    pub fn exit(&self) -> Tween<f64> {
        self.host.exit()
    }
}

/// Builder for [`MadeBy`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct MadeByBuilder {
    base: BaseProps,
    size: f64,
}

impl MadeByBuilder {
    /// Font size of the words in pixels; the logo follows it.
    pub fn font_size(mut self, size: impl Into<f64>) -> Self {
        self.size = size.into();
        self
    }
}

impl NodeBuilder for MadeByBuilder {
    type Handle = MadeBy;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(self, _: &oxframe_2d::SceneGraph, parent: NodeId) -> MadeBy {
        let root = host::root(parent, self.base);
        let size = self.size;
        credit(&root, || Txt::new().font_size(size));
        MadeBy {
            host: Host::new(root.node(), Vec::new()),
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
            spans: Vec::new(),
            made_by: false,
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
    spans: Vec<RichSpan>,
    made_by: bool,
    note: Option<String>,
}

impl EndCardBuilder {
    /// "made by Oxframe" with the logo, as [`MadeBy`] draws it, in place of
    /// the title: `EndCard::new("").made_by()`.
    pub fn made_by(mut self) -> Self {
        self.made_by = true;
        self
    }
    /// Styled runs for the title instead of plain text.
    pub fn rich(mut self, spans: impl IntoIterator<Item = RichSpan>) -> Self {
        self.spans = spans.into_iter().collect();
        self
    }
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
    fn build(self, g: &oxframe_2d::SceneGraph, parent: NodeId) -> EndCard {
        let root = host::root(parent, self.base);
        EndCard {
            host: plate(g, &root, self.title, self.spans, self.made_by, self.note),
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

into_node! {
    TitleCard => |c| c.host.root;
    EndCard => |c| c.host.root;
    BulletList => |c| c.host.root;
}

#[cfg(test)]
mod tests {
    use oxframe_2d::{SceneGraph, View};

    use super::*;

    /// Bounds of the card, the title and the second line, by name.
    fn card_parts(graph: &SceneGraph) -> Vec<(String, [f64; 4])> {
        graph
            .inspect_nodes()
            .into_iter()
            .filter(|n| n.kind == "Rect" || n.kind == "Txt")
            .map(|n| {
                (
                    n.name.rsplit('/').next().unwrap_or_default().to_owned(),
                    n.bounds,
                )
            })
            .collect()
    }

    #[test]
    fn a_long_subtitle_wraps_inside_the_card_and_the_card_grows() {
        let graph = SceneGraph::new(1920, 1080);
        graph.enter(|| {
            View::current().add(TitleCard::new("Fixed").subtitle(
                "check reported too_fast at a cut between two scenes when the first node of each \
                 had the same id: node ids start again in every scene, and a move is now only a \
                 node added at the same place.",
            ));
        });
        let parts = card_parts(&graph);
        let get = |name: &str| parts.iter().find(|(n, _)| n == name).expect(name).1;
        let (card, title, line) = (get("plate"), get("title"), get("subtitle"));
        assert!(
            line[0] >= card[0] && line[2] <= card[2],
            "past the card: {line:?} {card:?}"
        );
        assert!(
            line[1] >= card[1] && line[3] <= card[3],
            "past the card: {line:?} {card:?}"
        );
        assert!(
            title[3] <= line[1],
            "title and line overlap: {title:?} {line:?}"
        );

        // Twice as much text no longer fits the usual card, so it grows.
        let graph = SceneGraph::new(1920, 1080);
        let long = "A sentence that keeps going so the second line needs room. ".repeat(6);
        graph.enter(|| {
            View::current().add(TitleCard::new("Changed").subtitle(long));
        });
        let parts = card_parts(&graph);
        let get = |name: &str| parts.iter().find(|(n, _)| n == name).expect(name).1;
        let (card, title, line) = (get("plate"), get("title"), get("subtitle"));
        assert!(card[3] - card[1] > 260.0, "the card grows: {card:?}");
        assert!(
            line[1] >= card[1] && line[3] <= card[3],
            "past the card: {line:?} {card:?}"
        );
        assert!(
            title[1] >= card[1] && title[3] <= line[1],
            "{title:?} {line:?} {card:?}"
        );
    }

    #[test]
    fn short_text_keeps_the_usual_layout() {
        let graph = SceneGraph::new(1920, 1080);
        graph.enter(|| {
            View::current().add(EndCard::new("Thanks").note("oxframe.dev"));
        });
        let parts = card_parts(&graph);
        let get = |name: &str| parts.iter().find(|(n, _)| n == name).expect(name).1;
        let (card, title, line) = (get("plate"), get("title"), get("subtitle"));
        assert_eq!(card[3] - card[1], 260.0);
        assert_eq!((title[1] + title[3]) / 2.0, -28.0);
        assert_eq!((line[1] + line[3]) / 2.0, 56.0);
    }

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
