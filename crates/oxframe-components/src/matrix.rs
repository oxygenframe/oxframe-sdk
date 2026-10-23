//! A matrix of values in square brackets.

use std::fmt::Display;
use std::ops::Deref;

use oxframe_2d::{Accent, BaseProps, Group, Node, NodeBuilder, NodeId, Path, Rect, Text, Txt};
use oxframe_core::Color;
use oxframe_core::kurbo::BezPath;

/// Values in a grid between square brackets. Each cell is a [`Txt`], so
/// `cell(1, 0).text().set("7")` changes it and `.fill()` colors it.
/// [`row_frame`](Self::row_frame) and [`column_frame`](Self::column_frame)
/// give outlines that fade in to point at a row or a column.
///
/// ```ignore
/// let a = view.add(Matrix::new([[1, 2], [3, 4]]));
/// let r = a.row_frame(0);
/// r.fade_in().await;
/// a.cell(0, 1).fill().to(Accent.into(), 0.3).await;
/// ```
#[derive(Clone)]
pub struct Matrix {
    root: Group,
    cells: Vec<Vec<Txt>>,
    brackets: [Path; 2],
    /// Center x of each column.
    xs: Vec<f64>,
    /// Center y of each row.
    ys: Vec<f64>,
    widths: Vec<f64>,
    row_height: f64,
}

impl Matrix {
    /// A matrix from rows of values: numbers, `&str` or anything that
    /// implements `Display`. Rows shorter than the longest get empty cells.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<R, V>(rows: impl IntoIterator<Item = R>) -> MatrixBuilder
    where
        R: IntoIterator<Item = V>,
        V: Display,
    {
        MatrixBuilder {
            base: BaseProps::default(),
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(|v| v.to_string()).collect())
                .collect(),
            cell_width: 0.0,
            column_gap: 40.0,
            row_gap: 16.0,
            font_size: None,
        }
    }

    /// The cell in row `row`, column `column` (both from 0).
    ///
    /// # Panics
    ///
    /// If the cell is outside the matrix.
    pub fn cell(&self, row: usize, column: usize) -> Txt {
        self.cells[row][column]
    }

    /// The cells of a row, left to right.
    pub fn row(&self, row: usize) -> Vec<Txt> {
        self.cells[row].clone()
    }

    /// The cells of a column, top to bottom.
    pub fn column(&self, column: usize) -> Vec<Txt> {
        self.cells.iter().map(|r| r[column]).collect()
    }

    /// Number of rows.
    pub fn rows(&self) -> usize {
        self.cells.len()
    }

    /// Number of columns.
    pub fn columns(&self) -> usize {
        self.xs.len()
    }

    /// The left and right bracket.
    pub fn brackets(&self) -> [Path; 2] {
        self.brackets
    }

    /// A rounded outline around row `row`, hidden: `.fade_in()` shows it.
    pub fn row_frame(&self, row: usize) -> Rect {
        let width = self.inner_width() + 24.0;
        self.frame(
            0.0,
            self.ys[row],
            width,
            self.row_height + 12.0,
            "row_frame",
        )
    }

    /// A rounded outline around column `column`, hidden.
    pub fn column_frame(&self, column: usize) -> Rect {
        let height = self.inner_height() + 12.0;
        self.frame(
            self.xs[column],
            0.0,
            self.widths[column] + 24.0,
            height,
            "column_frame",
        )
    }

    /// A rounded outline around one cell, hidden.
    pub fn cell_frame(&self, row: usize, column: usize) -> Rect {
        self.frame(
            self.xs[column],
            self.ys[row],
            self.widths[column] + 24.0,
            self.row_height + 12.0,
            "cell_frame",
        )
    }

    fn frame(&self, x: f64, y: f64, w: f64, h: f64, name: &str) -> Rect {
        self.root.add(
            Rect::new()
                .width(w)
                .height(h)
                .radius(12.0)
                .fill(Color::TRANSPARENT)
                .stroke(Accent)
                .line_width(4.0)
                .x(x)
                .y(y)
                .opacity(0.0)
                .name(name),
        )
    }

    fn inner_width(&self) -> f64 {
        match (self.xs.first(), self.xs.last()) {
            (Some(a), Some(b)) => {
                b - a + (self.widths[0] + self.widths[self.widths.len() - 1]) / 2.0
            }
            _ => 0.0,
        }
    }

    fn inner_height(&self) -> f64 {
        match (self.ys.first(), self.ys.last()) {
            (Some(a), Some(b)) => b - a + self.row_height,
            _ => 0.0,
        }
    }

    /// The untyped node handle.
    pub fn node(&self) -> Node {
        *self.root
    }
}

impl Deref for Matrix {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.root
    }
}

/// Builder for [`Matrix`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct MatrixBuilder {
    base: BaseProps,
    rows: Vec<Vec<String>>,
    cell_width: f64,
    column_gap: f64,
    row_gap: f64,
    font_size: Option<f64>,
}

impl MatrixBuilder {
    /// The narrowest a column is, in pixels. Set it when values will be
    /// replaced by longer ones, such as a result filled in step by step.
    pub fn cell_width(mut self, px: f64) -> Self {
        self.cell_width = px;
        self
    }

    /// Space between columns. Default 40.
    pub fn column_gap(mut self, px: f64) -> Self {
        self.column_gap = px;
        self
    }

    /// Space between rows. Default 16.
    pub fn row_gap(mut self, px: f64) -> Self {
        self.row_gap = px;
        self
    }

    /// Size of the values in pixels. Default: the theme's body size.
    pub fn font_size(mut self, px: f64) -> Self {
        self.font_size = Some(px);
        self
    }
}

impl NodeBuilder for MatrixBuilder {
    type Handle = Matrix;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(self, _: &oxframe_2d::SceneGraph, parent: NodeId) -> Matrix {
        let base = self.base;
        let mut group = Group::new().x(base.x).y(base.y).opacity(base.opacity);
        if let Some(name) = base.name {
            group = group.name(name);
        }
        let root = Node::from_id(parent).add(group);
        let columns = self.rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut cells: Vec<Vec<Txt>> = Vec::new();
        let mut widths = vec![self.cell_width.max(24.0); columns];
        let mut row_height: f64 = 0.0;
        for (r, row) in self.rows.iter().enumerate() {
            let mut out = Vec::new();
            for (c, width) in widths.iter_mut().enumerate() {
                let text = row.get(c).cloned().unwrap_or_default();
                let mut b = Txt::new().text(text).body().name(format!("m{r}{c}"));
                if let Some(px) = self.font_size {
                    b = b.font_size(px);
                }
                let txt = root.add(b);
                let (w, h) = txt.size();
                *width = width.max(w);
                row_height = row_height.max(h);
                out.push(txt);
            }
            cells.push(out);
        }
        let row_height = row_height.max(32.0);
        let total_w =
            widths.iter().sum::<f64>() + self.column_gap * columns.saturating_sub(1) as f64;
        let mut xs = Vec::with_capacity(columns);
        let mut left = -total_w / 2.0;
        for w in &widths {
            xs.push(left + w / 2.0);
            left += w + self.column_gap;
        }
        let n = cells.len();
        let total_h = row_height * n as f64 + self.row_gap * n.saturating_sub(1) as f64;
        let ys: Vec<f64> = (0..n)
            .map(|r| -total_h / 2.0 + row_height / 2.0 + r as f64 * (row_height + self.row_gap))
            .collect();
        for (r, row) in cells.iter().enumerate() {
            for (c, txt) in row.iter().enumerate() {
                txt.x().set(xs[c]);
                txt.y().set(ys[r]);
            }
        }
        // Square brackets a little taller than the rows, with short serifs.
        let (bx, by, serif) = (total_w / 2.0 + 24.0, total_h / 2.0 + 10.0, 14.0);
        let bracket = |side: f64, name: &str| {
            let mut d = BezPath::new();
            d.move_to((side * (bx - serif), -by));
            d.line_to((side * bx, -by));
            d.line_to((side * bx, by));
            d.line_to((side * (bx - serif), by));
            root.add(
                Path::new()
                    .data(d)
                    .fill(Color::TRANSPARENT)
                    .stroke(Text)
                    .line_width(4.0)
                    .name(name),
            )
        };
        let brackets = [bracket(-1.0, "left_bracket"), bracket(1.0, "right_bracket")];
        Matrix {
            root,
            cells,
            brackets,
            xs,
            ys,
            widths,
            row_height,
        }
    }
}

into_node! {
    Matrix => |c| c.node();
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxframe_2d::{SceneGraph, View};
    use oxframe_core::kurbo::Shape;

    #[test]
    fn cells_rows_and_columns_line_up() {
        let graph = SceneGraph::new(1920, 1080);
        graph.enter(|| {
            let m = View::current().add(Matrix::new([[1, 20, 3], [4, 5, 600]]));
            assert_eq!((m.rows(), m.columns()), (2, 3));
            assert_eq!(m.cell(1, 2).text().get(), "600");
            let col = m.column(1);
            assert_eq!(col[0].x().get(), col[1].x().get());
            let row = m.row(0);
            assert_eq!(row[0].y().get(), row[2].y().get());
            assert!(row[0].x().get() < row[1].x().get());
            assert!(m.cell(0, 0).y().get() < m.cell(1, 0).y().get());
            let frame = m.row_frame(1);
            assert_eq!(frame.opacity().get(), 0.0);
            assert_eq!(frame.y().get(), m.cell(1, 0).y().get());
            let [l, r] = m.brackets();
            assert!(l.data().get().bounding_box().x1 < m.cell(0, 0).x().get());
            assert!(r.data().get().bounding_box().x0 > m.cell(0, 2).x().get());
            // Centered on the node: the brackets mirror each other.
            let (lb, rb) = (l.data().get().bounding_box(), r.data().get().bounding_box());
            assert!((lb.x0 + rb.x1).abs() < 1e-9);
        });
    }

    #[test]
    fn short_rows_get_empty_cells() {
        let graph = SceneGraph::new(1920, 1080);
        graph.enter(|| {
            let m = View::current().add(Matrix::new(vec![vec!["a", "b"], vec!["c"]]));
            assert_eq!(m.cell(1, 1).text().get(), "");
        });
    }
}
