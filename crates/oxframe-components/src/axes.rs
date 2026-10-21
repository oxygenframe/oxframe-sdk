//! Axes, number lines and plots.
//!
//! Data coordinates map into the axes' box. The ranges are signals: when one
//! animates, the grid, the ticks, the labels, every plot and every placed
//! node follow it on the same frame. Ticks sit at multiples of a round step
//! (1, 2 or 5 times a power of ten), so labels slide with the data instead
//! of jumping between values.

use std::cell::{Cell, RefCell};
use std::ops::{Deref, Range};
use std::rc::Rc;

use oxframe_2d::{
    Accent, BaseProps, Circle, Custom, Group, IntoPaint, Muted, Node, NodeBuilder, NodeId, Path,
    Rect, Span, Text, Txt, pct,
};
use oxframe_core::kurbo::{BezPath, Point, Vec2};
use oxframe_core::{Color, Lerp, Signal};

/// A data range, `min` to `max`. Animates with `.to(-10.0..10.0, 1.0)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interval {
    /// Left or bottom end.
    pub min: f64,
    /// Right or top end.
    pub max: f64,
}

impl Interval {
    /// `max - min`, never zero.
    pub fn len(&self) -> f64 {
        let d = self.max - self.min;
        if d.abs() < 1e-12 { 1e-12 } else { d }
    }

    /// Never true: an interval always has a length.
    pub fn is_empty(&self) -> bool {
        false
    }
}

impl From<Range<f64>> for Interval {
    fn from(r: Range<f64>) -> Self {
        Self {
            min: r.start,
            max: r.end,
        }
    }
}

impl From<(f64, f64)> for Interval {
    fn from((min, max): (f64, f64)) -> Self {
        Self { min, max }
    }
}

impl Lerp for Interval {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        Self {
            min: self.min.lerp(&to.min, t),
            max: self.max.lerp(&to.max, t),
        }
    }
}

/// Labels kept per axis. Ticks beyond this many are drawn without a label.
const LABELS: usize = 24;
/// Most ticks an axis aims for.
const TARGET_TICKS: f64 = 8.0;

/// Round step for `span` that gives at most about [`TARGET_TICKS`] ticks.
fn tick_step(span: f64) -> f64 {
    let raw = span.abs().max(1e-12) / TARGET_TICKS;
    let mag = 10f64.powf(raw.log10().floor());
    for k in [1.0, 2.0, 5.0, 10.0] {
        if k * mag >= raw {
            return k * mag;
        }
    }
    10.0 * mag
}

/// Tick indices `k` (value `k * step`) inside `range`.
fn tick_indices(range: Interval, step: f64) -> Range<i64> {
    let (lo, hi) = (range.min.min(range.max), range.min.max(range.max));
    let first = (lo / step - 1e-9).ceil() as i64;
    let last = (hi / step + 1e-9).floor() as i64;
    first..last + 1
}

/// `value` with as many decimals as `step` needs, and no `-0`.
fn label(value: f64, step: f64) -> String {
    let decimals = if step >= 1.0 {
        0
    } else {
        (-step.log10().floor()) as usize
    };
    let v = if value.abs() < step * 1e-6 {
        0.0
    } else {
        value
    };
    format!("{v:.decimals$}")
}

struct Shared {
    x: Signal<Interval>,
    y: Signal<Interval>,
    width: f64,
    height: f64,
    /// The y axis is drawn.
    y_axis: bool,
    /// Bars: x is categories, so the y axis sits at the left edge and the x
    /// axis has no numeric ticks.
    categories: Cell<bool>,
}

impl Shared {
    fn c2p(&self, x: f64, y: f64) -> Vec2 {
        map(self.x.get(), self.y.get(), self.width, self.height, x, y)
    }
}

fn map(xr: Interval, yr: Interval, w: f64, h: f64, x: f64, y: f64) -> Vec2 {
    Vec2::new(
        ((x - xr.min) / xr.len() - 0.5) * w,
        -((y - yr.min) / yr.len() - 0.5) * h,
    )
}

/// A plotting area: a box with ranges, a grid, axes with arrows, ticks and
/// labels. Derefs to [`Node`], so it moves and fades like any node.
///
/// ```ignore
/// let axes = view.add(Axes::new().x_range(-5.0..5.0).y_range(-2.0..2.0).width(pct(80)));
/// let sin = axes.plot(|x| x.sin());
/// sin.draw_on().await;
/// axes.x_range().to(-10.0..10.0, 1.0).await;
/// ```
#[derive(Clone)]
pub struct Axes {
    node: Node,
    shared: Rc<Shared>,
}

impl Axes {
    /// Axes over `-5..5` by `-5..5`, 80 % of the frame wide and 60 % high.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> AxesBuilder {
        AxesBuilder {
            base: BaseProps::default(),
            x: (-5.0..5.0).into(),
            y: (-5.0..5.0).into(),
            width: pct(80),
            height: pct(60),
            grid: true,
            y_axis: true,
        }
    }

    /// The horizontal data range. `.to(0.0..20.0, 1.0)` zooms the axes.
    pub fn x_range(&self) -> Signal<Interval> {
        self.shared.x.clone()
    }

    /// The vertical data range.
    pub fn y_range(&self) -> Signal<Interval> {
        self.shared.y.clone()
    }

    /// The scene point (relative to the axes) of data point `(x, y)`, for
    /// the ranges right now.
    pub fn c2p(&self, x: f64, y: f64) -> Vec2 {
        self.shared.c2p(x, y)
    }

    /// Keeps `node` at data point `(x, y)` while the ranges change. The
    /// node becomes a child of the axes.
    pub fn place(&self, node: impl Into<Node>, x: f64, y: f64) {
        let node = node.into();
        node.reparent(self.node);
        let s = self.shared.clone();
        node.x().bind(move || s.c2p(x, y).x);
        let s = self.shared.clone();
        node.y().bind(move || s.c2p(x, y).y);
    }

    /// A curve of `f` over the visible x range. Sampled again every frame,
    /// finer where it bends, and broken where it jumps (no line across the
    /// gap of `tan`). Accent, 4 px; `.stroke()` and `.line_width()` change it.
    #[track_caller]
    pub fn plot(&self, f: impl Fn(f64) -> f64 + 'static) -> Plot {
        oxframe_2d::add_site(|| self.plot_at(f))
    }

    fn plot_at(&self, f: impl Fn(f64) -> f64 + 'static) -> Plot {
        let curve = Rc::new(Curve {
            f: RefCell::new(Rc::new(f)),
            from: RefCell::new(None),
            mix: Signal::new(1.0),
        });
        let path = self.node.add(
            Path::new()
                .fill(Color::TRANSPARENT)
                .stroke(Accent)
                .line_width(4.0)
                .name("plot"),
        );
        let s = self.shared.clone();
        let c = curve.clone();
        path.data().bind(move || sample(&s, &|x| c.eval(x)));
        Plot {
            path,
            curve,
            shared: self.shared.clone(),
        }
    }

    /// A dot that sits on `plot` at data x `x` and follows it.
    #[track_caller]
    pub fn point_on(&self, plot: &Plot, x: Signal<f64>) -> Circle {
        oxframe_2d::add_site(|| self.point_on_at(plot, x))
    }

    fn point_on_at(&self, plot: &Plot, x: Signal<f64>) -> Circle {
        let dot = self
            .node
            .add(Circle::new().size(22.0).fill(Text).name("point"));
        let (s, c, xs) = (self.shared.clone(), plot.curve.clone(), x.clone());
        dot.x().bind(move || {
            let x = xs.get();
            s.c2p(x, c.eval(x)).x
        });
        let (s, c) = (self.shared.clone(), plot.curve.clone());
        dot.y().bind(move || {
            let x = x.get();
            s.c2p(x, c.eval(x)).y
        });
        dot
    }

    /// A polyline through data points. Each point is a signal of the series.
    #[track_caller]
    pub fn line_series(&self, points: impl IntoIterator<Item = (f64, f64)>) -> Series {
        oxframe_2d::add_site(|| self.line_series_at(points))
    }

    fn line_series_at(&self, points: impl IntoIterator<Item = (f64, f64)>) -> Series {
        let points: Vec<Signal<Vec2>> = points
            .into_iter()
            .map(|(x, y)| Signal::new(Vec2::new(x, y)))
            .collect();
        let path = self.node.add(
            Path::new()
                .fill(Color::TRANSPARENT)
                .stroke(Accent)
                .line_width(4.0)
                .name("series"),
        );
        let (s, pts) = (self.shared.clone(), points.clone());
        path.data().bind(move || {
            let mut out = BezPath::new();
            for (i, p) in pts.iter().enumerate() {
                let v = p.get();
                let q = s.c2p(v.x, v.y).to_point();
                if i == 0 {
                    out.move_to(q);
                } else {
                    out.line_to(q);
                }
            }
            out
        });
        Series { path, points }
    }

    /// A dot per data point. Returns the dots in order.
    #[track_caller]
    pub fn scatter(&self, points: impl IntoIterator<Item = (f64, f64)>) -> Vec<Circle> {
        oxframe_2d::add_site(|| self.scatter_at(points))
    }

    fn scatter_at(&self, points: impl IntoIterator<Item = (f64, f64)>) -> Vec<Circle> {
        points
            .into_iter()
            .map(|(x, y)| {
                let dot = self
                    .node
                    .add(Circle::new().size(16.0).fill(Accent).name("dot"));
                self.place(dot, x, y);
                dot
            })
            .collect()
    }

    /// A bar per value at x = 0, 1, 2, …, growing up from y = 0, with the
    /// value printed above it. Set the x range to `-0.5..n - 0.5`.
    #[track_caller]
    pub fn bars(&self, values: impl IntoIterator<Item = f64>) -> Bars {
        oxframe_2d::add_site(|| self.bars_at(values))
    }

    fn bars_at(&self, values: impl IntoIterator<Item = f64>) -> Bars {
        self.shared.categories.set(true);
        let mut bars = Vec::new();
        for (i, v) in values.into_iter().enumerate() {
            let value = Signal::new(v).with_name(format!("bar{}.value", i + 1));
            let x = i as f64;
            let rect = self
                .node
                .add(Rect::new().fill(Accent).name(format!("bar{}", i + 1)));
            let s = self.shared.clone();
            rect.width().bind(move || {
                let a = s.c2p(x - 0.35, 0.0).x;
                let b = s.c2p(x + 0.35, 0.0).x;
                (b - a).abs()
            });
            let (s, val2) = (self.shared.clone(), value.clone());
            rect.height()
                .bind(move || (s.c2p(x, val2.get()).y - s.c2p(x, 0.0).y).abs());
            let s2 = self.shared.clone();
            rect.x().bind(move || s2.c2p(x, 0.0).x);
            let (s3, val3) = (self.shared.clone(), value.clone());
            rect.y()
                .bind(move || (s3.c2p(x, val3.get()).y + s3.c2p(x, 0.0).y) / 2.0);
            let text = self
                .node
                .add(Txt::new().caption().name(format!("bar{}_label", i + 1)));
            let val4 = value.clone();
            text.text().bind(move || label(val4.get(), 0.1));
            let (s4, val5) = (self.shared.clone(), value.clone());
            text.x().bind(move || s4.c2p(x, 0.0).x);
            let s5 = self.shared.clone();
            text.y()
                .bind(move || s5.c2p(x, val5.get().max(0.0)).y - 28.0);
            bars.push((rect, value));
        }
        Bars {
            bars,
            axes: self.clone(),
        }
    }

    /// The untyped node handle.
    pub fn node(&self) -> Node {
        self.node
    }
}

impl Deref for Axes {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.node
    }
}

/// A plotted function, shared by the curve and the dots on it.
type Func = Rc<dyn Fn(f64) -> f64>;

struct Curve {
    f: RefCell<Func>,
    /// The function a `to_fn` tween comes from.
    from: RefCell<Option<Func>>,
    mix: Signal<f64>,
}

impl Curve {
    fn eval(&self, x: f64) -> f64 {
        let y = (self.f.borrow())(x);
        match &*self.from.borrow() {
            Some(g) => g(x).lerp(&y, self.mix.get()),
            None => y,
        }
    }
}

/// A function plot. Derefs to its [`Path`], so `.draw_on()`, `.stroke()`
/// and `.line_width()` work as on any path.
#[derive(Clone)]
pub struct Plot {
    path: Path,
    curve: Rc<Curve>,
    shared: Rc<Shared>,
}

impl Plot {
    /// Line color: a theme role such as `Accent`, or a color.
    pub fn stroke(self, paint: impl IntoPaint) -> Self {
        self.path.stroke().set(paint.into_paint());
        self
    }

    /// Line width in pixels.
    pub fn width(self, px: f64) -> Self {
        self.path.line_width().set(px);
        self
    }

    /// Morphs the curve into `g` over `seconds`: every x moves from the old
    /// value to the new one.
    pub fn to_fn(
        &self,
        g: impl Fn(f64) -> f64 + 'static,
        seconds: f64,
    ) -> impl std::future::Future<Output = ()> + 'static {
        let curve = self.curve.clone();
        let old = curve.f.replace(Rc::new(g));
        *curve.from.borrow_mut() = Some(old);
        curve.mix.set(0.0);
        let tween = curve.mix.to(1.0, seconds);
        async move {
            tween.await;
            *curve.from.borrow_mut() = None;
        }
    }

    /// The value the curve shows at `x` right now.
    pub fn eval(&self, x: f64) -> f64 {
        self.curve.eval(x)
    }

    /// The scene point of the curve at data `x`.
    pub fn point_at(&self, x: f64) -> Vec2 {
        self.shared.c2p(x, self.curve.eval(x))
    }
}

impl Deref for Plot {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}

/// A line through data points; see [`Axes::line_series`].
pub struct Series {
    path: Path,
    points: Vec<Signal<Vec2>>,
}

impl Series {
    /// Point `i` in data coordinates, as a signal.
    ///
    /// # Panics
    ///
    /// If the series has fewer than `i + 1` points.
    pub fn point(&self, i: usize) -> Signal<Vec2> {
        self.points[i].clone()
    }
}

impl Deref for Series {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}

/// Bars from [`Axes::bars`].
pub struct Bars {
    bars: Vec<(Rect, Signal<f64>)>,
    axes: Axes,
}

impl Bars {
    /// Names under the bars, in order.
    #[track_caller]
    pub fn labels(self, names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        oxframe_2d::add_site(|| self.labels_at(names))
    }

    fn labels_at(self, names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for (i, name) in names.into_iter().enumerate().take(self.bars.len()) {
            let txt = self.axes.node.add(
                Txt::new()
                    .text(name)
                    .caption()
                    .name(format!("bar{}_name", i + 1)),
            );
            let s = self.axes.shared.clone();
            let x = i as f64;
            txt.x().bind(move || s.c2p(x, 0.0).x);
            let s = self.axes.shared.clone();
            txt.y().bind(move || axis_cross(&s).1 + 30.0);
        }
        self
    }

    /// The value of bar `i`. `.to(8.0, 1.0)` grows it and its label.
    ///
    /// # Panics
    ///
    /// If there are fewer than `i + 1` bars.
    pub fn value(&self, i: usize) -> Signal<f64> {
        self.bars[i].1.clone()
    }

    /// The rectangle of bar `i`.
    ///
    /// # Panics
    ///
    /// If there are fewer than `i + 1` bars.
    pub fn bar(&self, i: usize) -> Rect {
        self.bars[i].0
    }

    /// How many bars.
    pub fn len(&self) -> usize {
        self.bars.len()
    }

    /// No values were given.
    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }
}

/// The curve of `f` in scene coordinates for the current ranges, cut at
/// the top and bottom of the box.
fn sample(s: &Shared, f: &dyn Fn(f64) -> f64) -> BezPath {
    let (xr, yr) = (s.x.get(), s.y.get());
    let (w, h) = (s.width, s.height);
    let to_p = |x: f64, y: f64| map(xr, yr, w, h, x, y).to_point();
    let n = 240usize;
    let x_at = |i: usize| xr.min + (xr.max - xr.min) * i as f64 / n as f64;
    // Points in order; a non-finite y lifts the pen.
    let mut pts = Vec::with_capacity(n * 2);
    let mut prev: Option<(f64, Point)> = None;
    for i in 0..=n {
        let x = x_at(i);
        let p = to_p(x, f(x));
        match prev {
            Some(a) => refine(&to_p, f, a, (x, p), 0, h, &mut |q| pts.push(q)),
            None => pts.push(p),
        }
        prev = Some((x, p));
    }
    clip(&pts, h / 2.0)
}

/// A polyline through `pts`, kept between `-half` and `half` in y. A
/// segment that crosses the edge ends exactly on it.
fn clip(pts: &[Point], half: f64) -> BezPath {
    let inside = |p: Point| p.y.is_finite() && p.y.abs() <= half + 1e-9;
    // Where the segment from `inn` (inside) to `out` (outside) meets the edge.
    let edge = |inn: Point, out: Point| {
        let bound = if out.y > 0.0 { half } else { -half };
        let t = ((bound - inn.y) / (out.y - inn.y)).clamp(0.0, 1.0);
        Point::new(inn.x + (out.x - inn.x) * t, bound)
    };
    let mut out = BezPath::new();
    let mut pen = false;
    let mut last: Option<Point> = None;
    for &p in pts {
        match (last, inside(p)) {
            (Some(_), true) if pen => out.line_to(p),
            (Some(a), true) if a.y.is_finite() && !inside(a) => {
                out.move_to(edge(p, a));
                out.line_to(p);
                pen = true;
            }
            (_, true) => {
                out.move_to(p);
                pen = true;
            }
            (Some(a), false) if pen && p.y.is_finite() => {
                out.line_to(edge(a, p));
                pen = false;
            }
            _ => pen = false,
        }
        last = Some(p);
    }
    out
}

/// Emits the points after `a` up to and including `b`, splitting where the
/// curve bends or jumps. A jump that does not shrink when split is a gap:
/// `emit` gets a non-finite point there so the pen lifts.
fn refine(
    to_p: &dyn Fn(f64, f64) -> Point,
    f: &dyn Fn(f64) -> f64,
    a: (f64, Point),
    b: (f64, Point),
    depth: u32,
    h: f64,
    emit: &mut dyn FnMut(Point),
) {
    let jump = (b.1.y - a.1.y).abs();
    if !(a.1.y.is_finite() && b.1.y.is_finite()) || jump > h * 0.05 {
        if depth >= 6 {
            if jump > h * 0.5 || !b.1.y.is_finite() {
                emit(Point::new(b.1.x, f64::NAN));
            }
            emit(b.1);
            return;
        }
        let mx = (a.0 + b.0) / 2.0;
        let m = to_p(mx, f(mx));
        refine(to_p, f, a, (mx, m), depth + 1, h, emit);
        refine(to_p, f, (mx, m), b, depth + 1, h, emit);
        return;
    }
    emit(b.1);
}

/// Builder for [`Axes`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct AxesBuilder {
    base: BaseProps,
    x: Interval,
    y: Interval,
    width: Span,
    height: Span,
    grid: bool,
    y_axis: bool,
}

impl AxesBuilder {
    /// Horizontal data range, e.g. `-5.0..5.0`.
    pub fn x_range(mut self, r: impl Into<Interval>) -> Self {
        self.x = r.into();
        self
    }

    /// Vertical data range.
    pub fn y_range(mut self, r: impl Into<Interval>) -> Self {
        self.y = r.into();
        self
    }

    /// Box width in pixels, or `pct()` of the frame.
    pub fn width(mut self, v: impl Into<Span>) -> Self {
        self.width = v.into();
        self
    }

    /// Box height in pixels, or `pct()` of the frame.
    pub fn height(mut self, v: impl Into<Span>) -> Self {
        self.height = v.into();
        self
    }

    /// Leaves out the grid lines.
    pub fn no_grid(mut self) -> Self {
        self.grid = false;
        self
    }
}

fn resolve(span: Span, frame: f64) -> f64 {
    match span {
        Span::Px(v) => v,
        Span::Pct(p) => frame * p / 100.0,
    }
}

impl NodeBuilder for AxesBuilder {
    type Handle = Axes;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(self, g: &oxframe_2d::SceneGraph, parent: NodeId) -> Axes {
        let (fw, fh) = g.frame_size();
        let width = resolve(self.width, fw).max(1.0);
        let height = if self.y_axis {
            resolve(self.height, fh).max(1.0)
        } else {
            0.0
        };
        let base = self.base;
        let mut group = Group::new().x(base.x).y(base.y).opacity(base.opacity);
        if let Some(name) = base.name {
            group = group.name(name);
        }
        let root = Node::from_id(parent).add(group);
        let shared = Rc::new(Shared {
            x: Signal::new(self.x).with_name("Axes.x_range"),
            y: Signal::new(self.y).with_name("Axes.y_range"),
            width,
            height,
            y_axis: self.y_axis,
            categories: Cell::new(false),
        });
        let (grid, y_axis) = (self.grid, self.y_axis);
        let s = shared.clone();
        root.add(
            Custom::new(move |ctx, _| draw_frame(ctx, &s, grid, y_axis))
                .size(width + 60.0, height.max(1.0) + 60.0)
                .name("frame"),
        );
        labels(&root, &shared, true);
        if y_axis {
            labels(&root, &shared, false);
        }
        Axes {
            node: *root,
            shared,
        }
    }
}

/// Grid, axis lines with arrows, and tick marks, from the current ranges.
fn draw_frame(ctx: &mut oxframe_2d::DrawCtx, s: &Shared, grid: bool, y_axis: bool) {
    let (xr, yr) = (s.x.get(), s.y.get());
    let (w, h) = (s.width, s.height);
    let (hw, hh) = (w / 2.0, h / 2.0);
    let p = |x: f64, y: f64| map(xr, yr, w, h, x, y);
    let (ax_x, ax_y) = axis_cross(s);
    let x_ticks = !s.categories.get();
    let xs = tick_step(xr.max - xr.min);
    let ys = tick_step(yr.max - yr.min);
    if grid && y_axis {
        for k in tick_indices(xr, xs).filter(|_| x_ticks) {
            let x = p(k as f64 * xs, 0.0).x;
            ctx.line((x, -hh), (x, hh)).stroke(Muted, 1.0);
        }
        for k in tick_indices(yr, ys) {
            let y = p(0.0, k as f64 * ys).y;
            ctx.line((-hw, y), (hw, y)).stroke(Muted, 1.0);
        }
    }
    ctx.line((-hw - 12.0, ax_y), (hw + 18.0, ax_y))
        .stroke(Text, 3.0);
    arrow(ctx, Point::new(hw + 30.0, ax_y), Vec2::new(1.0, 0.0));
    for k in tick_indices(xr, xs).filter(|_| x_ticks) {
        let x = p(k as f64 * xs, 0.0).x;
        ctx.line((x, ax_y - 8.0), (x, ax_y + 8.0)).stroke(Text, 2.0);
    }
    if y_axis {
        ctx.line((ax_x, hh + 12.0), (ax_x, -hh - 18.0))
            .stroke(Text, 3.0);
        arrow(ctx, Point::new(ax_x, -hh - 30.0), Vec2::new(0.0, -1.0));
        for k in tick_indices(yr, ys) {
            let y = p(0.0, k as f64 * ys).y;
            ctx.line((ax_x - 8.0, y), (ax_x + 8.0, y)).stroke(Text, 2.0);
        }
    }
}

fn arrow(ctx: &mut oxframe_2d::DrawCtx, tip: Point, dir: Vec2) {
    let side = Vec2::new(-dir.y, dir.x);
    let mut path = BezPath::new();
    path.move_to(tip);
    path.line_to(tip - dir * 16.0 + side * 8.0);
    path.line_to(tip - dir * 16.0 - side * 8.0);
    path.close_path();
    ctx.path(path).fill(Text);
}

/// Tick labels for one axis. Label `j` shows the tick whose index is `j`
/// modulo the pool, so a value keeps its node while the range slides.
fn labels(root: &Group, s: &Rc<Shared>, horizontal: bool) {
    for j in 0..LABELS {
        let txt = if horizontal {
            Txt::new().caption().fill(Muted).name(format!("x_label{j}"))
        } else {
            // Right edge just left of the y axis, so "-0.5" never crosses it.
            Txt::new()
                .caption()
                .fill(Muted)
                .anchor((1.0, 0.0))
                .name(format!("y_label{j}"))
        };
        let txt = root.add(txt);
        // The tick index this label shows now, if any.
        let pick = {
            let s = s.clone();
            move || -> Option<(f64, f64)> {
                if horizontal && s.categories.get() {
                    return None;
                }
                let r = if horizontal { s.x.get() } else { s.y.get() };
                let step = tick_step(r.max - r.min);
                let (cx, cy) = axis_cross_data(&s);
                // Where the other axis crosses, a label would sit on it.
                let skip = if horizontal {
                    s.y_axis.then_some(cx)
                } else {
                    Some(cy)
                };
                tick_indices(r, step)
                    .find(|k| k.rem_euclid(LABELS as i64) == j as i64)
                    .map(|k| (k as f64 * step, step))
                    .filter(|(v, step)| skip.is_none_or(|c| (v - c).abs() > step * 1e-6))
            }
        };
        let p = pick.clone();
        txt.text()
            .bind(move || p().map(|(v, st)| label(v, st)).unwrap_or_default());
        let p = pick.clone();
        txt.opacity()
            .bind(move || if p().is_some() { 1.0 } else { 0.0 });
        let (p, s2) = (pick.clone(), s.clone());
        txt.x().bind(move || match p() {
            Some((v, _)) if horizontal => s2.c2p(v, axis_cross(&s2).1).x,
            Some(_) => axis_cross(&s2).0 - 14.0,
            None => 0.0,
        });
        let (p, s2) = (pick, s.clone());
        txt.y().bind(move || match p() {
            Some(_) if horizontal => axis_cross(&s2).1 + 30.0,
            Some((v, _)) => s2.c2p(axis_cross_data(&s2).0, v).y,
            None => 0.0,
        });
    }
}

/// Where the axes cross, in scene coordinates.
fn axis_cross(s: &Shared) -> (f64, f64) {
    let (x, y) = axis_cross_data(s);
    let p = s.c2p(x, y);
    (p.x, if s.height > 0.0 { p.y } else { 0.0 })
}

/// Where the axes cross, in data coordinates: 0 when in range.
fn axis_cross_data(s: &Shared) -> (f64, f64) {
    let (xr, yr) = (s.x.get(), s.y.get());
    // With bars, x is categories and the y axis sits at the left edge.
    let x = if s.categories.get() {
        xr.min.min(xr.max)
    } else {
        0f64.clamp(xr.min.min(xr.max), xr.min.max(xr.max))
    };
    (x, 0f64.clamp(yr.min.min(yr.max), yr.min.max(yr.max)))
}

/// A single horizontal axis with ticks, labels and an arrow.
pub struct NumberLine;

impl NumberLine {
    /// A number line over `range`, 80 % of the frame wide. It is an
    /// [`Axes`] without a y axis, so `x_range`, `c2p` and `place` work.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(range: impl Into<Interval>) -> AxesBuilder {
        AxesBuilder {
            base: BaseProps::default(),
            x: range.into(),
            y: (0.0..1.0).into(),
            width: pct(80),
            height: Span::Px(0.0),
            grid: false,
            y_axis: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_round_and_give_few_ticks() {
        assert_eq!(tick_step(10.0), 2.0);
        assert_eq!(tick_step(1.0), 0.2);
        assert_eq!(tick_step(100.0), 20.0);
        let r: Interval = (-5.0..5.0).into();
        assert_eq!(tick_indices(r, 2.0), -2..3);
    }

    use oxframe_2d::{SceneGraph, View};
    use oxframe_core::kurbo::PathEl;

    fn axes(graph: &std::rc::Rc<SceneGraph>) -> Axes {
        graph.enter(|| {
            View::current().add(
                Axes::new()
                    .x_range(-5.0..5.0)
                    .y_range(-2.0..2.0)
                    .width(800.0)
                    .height(400.0),
            )
        })
    }

    #[test]
    fn tan_has_no_bridge_across_its_gaps_and_stays_in_the_box() {
        let graph = SceneGraph::new(1920, 1080);
        let a = axes(&graph);
        let tan = graph.enter(|| a.plot(f64::tan));
        let data = graph.enter(|| tan.data().get());
        let mut last: Option<Point> = None;
        let mut moves = 0;
        for el in data.elements() {
            match *el {
                PathEl::MoveTo(p) => {
                    moves += 1;
                    last = Some(p);
                }
                PathEl::LineTo(p) => {
                    let a = last.unwrap();
                    assert!(p.y.abs() <= 200.0 + 1e-6, "{p:?} leaves the box");
                    assert!(
                        (p.y - a.y).abs() < 200.0,
                        "a line jumps from {a:?} to {p:?}"
                    );
                    last = Some(p);
                }
                _ => {}
            }
        }
        // Poles at ±π/2 and ±3π/2. The outer pieces stay above or below the
        // box (tan(±5) ≈ ±3.4), so three pieces are drawn.
        assert_eq!(moves, 3);
    }

    #[test]
    fn a_label_keeps_its_value_while_the_range_slides() {
        let graph = SceneGraph::new(1920, 1080);
        let a = axes(&graph);
        let read = |graph: &SceneGraph| -> Vec<(String, String)> {
            graph
                .inspect_nodes()
                .into_iter()
                .filter(|n| n.name.starts_with("x_label"))
                .map(|n| (n.name, n.text.unwrap_or_default()))
                .collect()
        };
        let before = read(&graph);
        graph.enter(|| a.x_range().set((-4.8..5.2).into()));
        let after = read(&graph);
        for ((name, v), (_, w)) in before.iter().zip(&after) {
            if !v.is_empty() && !w.is_empty() {
                assert_eq!(v, w, "{name} changed its value while sliding");
            }
        }
    }

    #[test]
    fn a_range_change_moves_the_curve() {
        let graph = SceneGraph::new(1920, 1080);
        let a = axes(&graph);
        let sin = graph.enter(|| a.plot(f64::sin));
        let first = graph.enter(|| sin.data().get());
        graph.enter(|| a.x_range().set((0.0..10.0).into()));
        assert_ne!(first, graph.enter(|| sin.data().get()));
        let p = graph.enter(|| a.c2p(5.0, 0.0));
        assert!(p.x.abs() < 1e-9 && p.y.abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn plots_report_the_line_that_added_them() {
        let graph = SceneGraph::new(1920, 1080);
        let a = axes(&graph);
        graph.enter(|| {
            let _ = a.plot(f64::sin);
        });
        let line = line!() - 2;
        let plot = graph
            .inspect_nodes()
            .into_iter()
            .find(|n| n.kind == "Path")
            .unwrap();
        assert_eq!(
            plot.source.as_deref(),
            Some(format!("src/axes.rs:{line}").as_str())
        );
    }

    #[test]
    fn labels_have_the_decimals_the_step_needs() {
        assert_eq!(label(0.4, 0.2), "0.4");
        assert_eq!(label(3.0, 1.0), "3");
        assert_eq!(label(-1e-12, 0.5), "0.0");
    }
}
