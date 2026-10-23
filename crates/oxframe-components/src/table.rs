//! A table with a header and rows that are added, removed and sorted with
//! animation.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt::Display;
use std::future::Future;
use std::ops::Deref;
use std::rc::Rc;

use oxframe_2d::{
    BaseProps, Group, Line, Muted, Node, NodeBuilder, NodeId, Rect, Surface, Text, Txt,
};
use oxframe_core::{Tween, all};

/// One row of a [`Table`]: a group with a background, its cells and a rule
/// under it. Derefs to [`Node`].
#[derive(Clone)]
pub struct TableRow {
    group: Group,
    cells: Vec<Txt>,
    background: Rect,
}

impl TableRow {
    /// The cells, left to right.
    pub fn cells(&self) -> Vec<Txt> {
        self.cells.clone()
    }

    /// One cell.
    ///
    /// # Panics
    ///
    /// If the table has no such column.
    pub fn cell(&self, column: usize) -> Txt {
        self.cells[column]
    }

    /// The row's background, hidden at first: `row.background().fade_in()`
    /// highlights the row.
    pub fn background(&self) -> Rect {
        self.background
    }

    /// The text of each cell now.
    pub fn values(&self) -> Vec<String> {
        self.cells.iter().map(|c| c.text().get()).collect()
    }

    /// The row's group.
    pub fn node(&self) -> Node {
        *self.group
    }
}

impl Deref for TableRow {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.group
    }
}

struct Layout {
    xs: Vec<f64>,
    width: f64,
    row_height: f64,
    /// Center y of the first data row.
    top: f64,
    font_size: Option<f64>,
}

impl Layout {
    fn text(&self, text: String) -> oxframe_2d::TxtBuilder {
        let b = Txt::new().text(text).body();
        match self.font_size {
            Some(px) => b.font_size(px),
            None => b,
        }
    }
}

impl Layout {
    fn y(&self, i: usize) -> f64 {
        self.top + i as f64 * self.row_height
    }
}

/// A header and rows. The rows are a list you can change while the scene
/// runs: [`insert_row`](Self::insert_row), [`remove_row`](Self::remove_row)
/// and [`sort_by_column`](Self::sort_by_column) move the other rows into
/// place.
///
/// ```ignore
/// let t = view.add(Table::new(["Renderer", "Frame"]).row(["vello", "4 ms"]).row(["skia", "9 ms"]));
/// t.insert_row(1, ["cairo", "14 ms"]).await;
/// t.sort_by_column(1, 0.8).await;
/// ```
#[derive(Clone)]
pub struct Table {
    root: Group,
    header: Vec<Txt>,
    rows: Rc<RefCell<Vec<TableRow>>>,
    layout: Rc<Layout>,
}

impl Table {
    /// A table with these column titles. Add rows with
    /// [`row`](TableBuilder::row).
    #[allow(clippy::new_ret_no_self)]
    pub fn new<V: Display>(header: impl IntoIterator<Item = V>) -> TableBuilder {
        TableBuilder {
            base: BaseProps::default(),
            header: header.into_iter().map(|v| v.to_string()).collect(),
            rows: Vec::new(),
            widths: Vec::new(),
            row_height: 64.0,
            font_size: None,
        }
    }

    /// The column titles.
    pub fn header(&self) -> Vec<Txt> {
        self.header.clone()
    }

    /// The row shown at position `i` (0 is the first under the header).
    ///
    /// # Panics
    ///
    /// If there is no such row.
    pub fn row(&self, i: usize) -> TableRow {
        self.rows.borrow()[i].clone()
    }

    /// The cell at row `row`, column `column`, by the current order.
    pub fn cell(&self, row: usize, column: usize) -> Txt {
        self.row(row).cell(column)
    }

    /// Number of rows, without the header.
    pub fn len(&self) -> usize {
        self.rows.borrow().len()
    }

    /// The table has no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.borrow().is_empty()
    }

    /// Adds a row at position `at` (clamped to the end): the rows below
    /// slide down and the new one fades in. Await it.
    pub fn insert_row<V: Display>(
        &self,
        at: usize,
        cells: impl IntoIterator<Item = V>,
    ) -> impl Future<Output = ()> + 'static {
        let at = at.min(self.len());
        let values: Vec<String> = cells.into_iter().map(|v| v.to_string()).collect();
        let row = make_row(&self.root, &self.layout, &values, self.layout.y(at), 0.0);
        let mut moves: Vec<Tween<f64>> = Vec::new();
        {
            let mut rows = self.rows.borrow_mut();
            rows.insert(at, row.clone());
            for (i, r) in rows.iter().enumerate().skip(at + 1) {
                moves.push(r.y().to(self.layout.y(i), 0.4));
            }
        }
        async move {
            all(moves).await;
            row.opacity().to(1.0, 0.3).await;
        }
    }

    /// Adds a row at the end.
    pub fn push_row<V: Display>(
        &self,
        cells: impl IntoIterator<Item = V>,
    ) -> impl Future<Output = ()> + 'static {
        self.insert_row(usize::MAX, cells)
    }

    /// Fades out row `i` and slides the rows below up. Await it.
    ///
    /// # Panics
    ///
    /// If there is no such row.
    pub fn remove_row(&self, i: usize) -> impl Future<Output = ()> + 'static {
        let row = self.rows.borrow_mut().remove(i);
        let fade = row.opacity().to(0.0, 0.3);
        let moves: Vec<Tween<f64>> = self
            .rows
            .borrow()
            .iter()
            .enumerate()
            .skip(i)
            .map(|(k, r)| r.y().to(self.layout.y(k), 0.4))
            .collect();
        async move {
            fade.await;
            all(moves).await;
            row.remove();
        }
    }

    /// Reorders the rows by `compare` on their current values and moves
    /// each one to its new place over `duration` seconds. The order is
    /// stable: equal rows keep theirs.
    pub fn sort_by(
        &self,
        compare: impl Fn(&[String], &[String]) -> Ordering,
        duration: f64,
    ) -> impl Future<Output = ()> + 'static {
        let mut rows = self.rows.borrow_mut();
        let mut keyed: Vec<(Vec<String>, TableRow)> =
            rows.drain(..).map(|r| (r.values(), r)).collect();
        keyed.sort_by(|a, b| compare(&a.0, &b.0));
        rows.extend(keyed.into_iter().map(|(_, r)| r));
        let moves: Vec<Tween<f64>> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| r.y().to(self.layout.y(i), duration))
            .collect();
        async move {
            all(moves).await;
        }
    }

    /// Sorts by one column, smallest first. Cells that start with a number
    /// (`"12 ms"`, `"-3.5"`) compare as numbers, the rest as text.
    pub fn sort_by_column(
        &self,
        column: usize,
        duration: f64,
    ) -> impl Future<Output = ()> + 'static {
        self.sort_by(
            move |a, b| compare_cells(a.get(column), b.get(column)),
            duration,
        )
    }

    /// The untyped node handle.
    pub fn node(&self) -> Node {
        *self.root
    }
}

impl Deref for Table {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.root
    }
}

/// The number a cell starts with, such as 12 in `"12 ms"`.
fn leading_number(s: &str) -> Option<f64> {
    let s = s.trim();
    let end = s
        .char_indices()
        .take_while(|&(i, c)| c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i == 0))
        .map(|(i, c)| i + c.len_utf8())
        .last()?;
    s[..end].parse().ok()
}

fn compare_cells(a: Option<&String>, b: Option<&String>) -> Ordering {
    let (a, b) = (a.map_or("", String::as_str), b.map_or("", String::as_str));
    match (leading_number(a), leading_number(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => a.cmp(b),
    }
}

fn make_row(root: &Group, layout: &Layout, values: &[String], y: f64, opacity: f64) -> TableRow {
    let group = root.add(Group::new().y(y).opacity(opacity).name("row"));
    let background = group.add(
        Rect::new()
            .width(layout.width)
            .height(layout.row_height - 4.0)
            .radius(8.0)
            .fill(Surface)
            .opacity(0.0)
            .name("row_background"),
    );
    let cells = layout
        .xs
        .iter()
        .enumerate()
        .map(|(c, &x)| {
            let text = values.get(c).cloned().unwrap_or_default();
            group.add(layout.text(text).x(x).name(format!("cell{c}")))
        })
        .collect();
    let h = layout.row_height / 2.0;
    group.add(
        Line::new()
            .points([(-layout.width / 2.0, h), (layout.width / 2.0, h)])
            .stroke(Muted)
            .line_width(1.0)
            .name("rule"),
    );
    TableRow {
        group,
        cells,
        background,
    }
}

/// Builder for [`Table`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct TableBuilder {
    base: BaseProps,
    header: Vec<String>,
    rows: Vec<Vec<String>>,
    widths: Vec<f64>,
    row_height: f64,
    font_size: Option<f64>,
}

impl TableBuilder {
    /// A row of values under the header.
    pub fn row<V: Display>(mut self, cells: impl IntoIterator<Item = V>) -> Self {
        self.rows
            .push(cells.into_iter().map(|v| v.to_string()).collect());
        self
    }

    /// Column widths in pixels. By default each column fits the widest of
    /// its header and the starting rows; set widths when rows added later
    /// are wider.
    pub fn widths(mut self, px: impl IntoIterator<Item = f64>) -> Self {
        self.widths = px.into_iter().collect();
        self
    }

    /// Height of every row, header included. Default 64.
    pub fn row_height(mut self, px: f64) -> Self {
        self.row_height = px;
        self
    }

    /// Size of the text in pixels. Default: the theme's body size. Raise
    /// [`row_height`](Self::row_height) with it.
    pub fn font_size(mut self, px: f64) -> Self {
        self.font_size = Some(px);
        self
    }
}

impl NodeBuilder for TableBuilder {
    type Handle = Table;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(self, _: &oxframe_2d::SceneGraph, parent: NodeId) -> Table {
        let base = self.base;
        let mut group = Group::new().x(base.x).y(base.y).opacity(base.opacity);
        if let Some(name) = base.name {
            group = group.name(name);
        }
        let root = Node::from_id(parent).add(group);
        let columns = self
            .rows
            .iter()
            .map(Vec::len)
            .chain([self.header.len()])
            .max()
            .unwrap_or(0);
        let font_size = self.font_size;
        let text = move |t: String| {
            let b = Txt::new().text(t).body();
            match font_size {
                Some(px) => b.font_size(px),
                None => b,
            }
        };
        // Header cells first: their sizes and those of the rows set the widths.
        let header: Vec<Txt> = (0..columns)
            .map(|c| {
                let t = self.header.get(c).cloned().unwrap_or_default();
                root.add(text(t).fill(Muted).name(format!("head{c}")))
            })
            .collect();
        let mut widths: Vec<f64> = header.iter().map(|t| t.size().0).collect();
        for row in &self.rows {
            for (c, v) in row.iter().enumerate() {
                let probe = root.add(text(v.clone()));
                widths[c] = widths[c].max(probe.size().0);
                probe.remove();
            }
        }
        for (c, w) in self.widths.iter().enumerate().take(columns) {
            widths[c] = *w;
        }
        let pad = 48.0;
        let width = widths.iter().map(|w| w + pad).sum::<f64>();
        let mut xs = Vec::with_capacity(columns);
        let mut left = -width / 2.0;
        for w in &widths {
            xs.push(left + (w + pad) / 2.0);
            left += w + pad;
        }
        let rh = self.row_height;
        let total = rh * (self.rows.len() + 1) as f64;
        let head_y = -total / 2.0 + rh / 2.0;
        for (t, x) in header.iter().zip(&xs) {
            t.x().set(*x);
            t.y().set(head_y);
        }
        root.add(
            Line::new()
                .points([
                    (-width / 2.0, head_y + rh / 2.0),
                    (width / 2.0, head_y + rh / 2.0),
                ])
                .stroke(Text)
                .line_width(2.0)
                .name("header_rule"),
        );
        let layout = Rc::new(Layout {
            xs,
            width,
            row_height: rh,
            top: head_y + rh,
            font_size,
        });
        let rows = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, values)| make_row(&root, &layout, values, layout.y(i), 1.0))
            .collect();
        Table {
            root,
            header,
            rows: Rc::new(RefCell::new(rows)),
            layout,
        }
    }
}

into_node! {
    Table => |c| c.node();
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxframe_2d::{SceneGraph, View};
    use oxframe_core::Executor;

    fn table(graph: &Rc<SceneGraph>) -> Table {
        graph.enter(|| {
            View::current().add(
                Table::new(["Renderer", "Frame"])
                    .row(["vello", "4 ms"])
                    .row(["skia", "12 ms"])
                    .row(["cairo", "9 ms"]),
            )
        })
    }

    #[test]
    fn numbers_compare_as_numbers() {
        assert_eq!(leading_number("12 ms"), Some(12.0));
        assert_eq!(leading_number("-3.5"), Some(-3.5));
        assert_eq!(leading_number("ms"), None);
        let (a, b) = ("9 ms".to_string(), "12 ms".to_string());
        assert_eq!(compare_cells(Some(&a), Some(&b)), Ordering::Less);
        let (a, b) = ("b".to_string(), "a".to_string());
        assert_eq!(compare_cells(Some(&a), Some(&b)), Ordering::Greater);
    }

    #[test]
    fn rows_sit_under_the_header_in_order() {
        let graph = SceneGraph::new(1920, 1080);
        let t = table(&graph);
        graph.enter(|| {
            assert_eq!(t.len(), 3);
            assert!(t.header()[0].y().get() < t.row(0).y().get());
            assert!(t.row(0).y().get() < t.row(1).y().get());
            assert_eq!(t.cell(2, 0).text().get(), "cairo");
            assert_eq!(t.row(1).cell(0).x().get(), t.header()[0].x().get());
        });
    }

    #[test]
    fn sorting_insertion_and_removal_move_rows_into_place() {
        let graph = SceneGraph::new(1920, 1080);
        let t = table(&graph);
        let ys: Vec<f64> = graph.enter(|| (0..3).map(|i| t.row(i).y().get()).collect());
        let t2 = t.clone();
        graph.enter(|| {
            let mut ex = Executor::new(
                60.0,
                oxframe_protocol::Meta::default(),
                0,
                std::pin::Pin::from(Box::new(async move {
                    t2.sort_by_column(1, 0.5).await;
                    t2.insert_row(0, ["wgpu", "2 ms"]).await;
                    t2.remove_row(3).await;
                }) as Box<dyn Future<Output = ()>>),
            );
            while !ex.step() {}
            let names: Vec<String> = (0..t.len()).map(|i| t.cell(i, 0).text().get()).collect();
            assert_eq!(names, ["wgpu", "vello", "cairo"]);
            for (i, y) in ys.iter().enumerate() {
                assert!((t.row(i).y().get() - y).abs() < 1e-6, "row {i}");
            }
            assert_eq!(t.row(0).opacity().get(), 1.0);
        });
    }
}
