use std::rc::Rc;

use kurbo::{Affine, BezPath, ParamCurve, ParamCurveDeriv, PathSeg, Point, Shape, Vec2};
use oxframe_core::{BlendMode, Color, Gradient};

use crate::graph::{NodeData, NodeId, NodeKind, TextDraw};
use crate::image::{ImageSource, RasterImage};
use crate::trim::trim;

/// One thing to paint, in local coordinates of `transform`. Paints already
/// include the node's opacity (unless the node draws into a layer).
#[derive(Debug, Clone)]
pub struct DrawItem {
    /// Node that produced this item (for hit testing and the inspector).
    pub node: NodeId,
    /// Local → scene transform.
    pub transform: Affine,
    /// What to draw.
    pub prim: Primitive,
}

/// What a shape is filled or stroked with.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    /// A single color.
    Solid(Color),
    /// In the item's local coordinates.
    Gradient(Gradient),
}

impl From<Color> for Paint {
    fn from(c: Color) -> Self {
        Self::Solid(c)
    }
}

/// Shape of the ends of open outlines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    /// Flat, ending exactly at the end point.
    Butt,
    #[default]
    /// Rounded, extending half the line width.
    Round,
    /// Flat, extending half the line width.
    Square,
}

/// Shape of outline corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    /// Sharp corner.
    Miter,
    #[default]
    /// Rounded corner.
    Round,
    /// Cut-off corner.
    Bevel,
}

/// How an outline is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    /// Line width in local units.
    pub width: f64,
    /// End shape.
    pub cap: LineCap,
    /// Corner shape.
    pub join: LineJoin,
    /// On/off lengths in local units; empty means solid.
    pub dash: Vec<f64>,
    /// Shift of the dash pattern along the outline.
    pub dash_offset: f64,
}

impl StrokeStyle {
    /// A solid line with round caps and joins.
    pub fn solid(width: f64) -> Self {
        Self {
            width,
            cap: LineCap::Round,
            join: LineJoin::Round,
            dash: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

/// Drop shadow of a layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    /// Shadow color.
    pub color: Color,
    /// Blur standard deviation in local units.
    pub blur: f64,
    /// Offset in local units.
    pub offset: Vec2,
}

/// Everything drawn until the matching [`Primitive::PopLayer`] is rendered
/// into its own buffer, filtered, clipped and composited as a whole.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// Group opacity, 0..=1.
    pub opacity: f64,
    /// How the layer is composited onto what is below.
    pub blend: BlendMode,
    /// Clip path in the item's local coordinates.
    pub clip: Option<BezPath>,
    /// Gaussian blur standard deviation in local units.
    pub blur: f64,
    /// Drop shadow of the layer's content.
    pub shadow: Option<Shadow>,
}

impl Layer {
    /// A layer that only clips to `path`.
    pub fn clip(path: BezPath) -> Self {
        Self {
            opacity: 1.0,
            blend: BlendMode::Normal,
            clip: Some(path),
            blur: 0.0,
            shadow: None,
        }
    }

    /// Needs pixel processing that is not expressible as a plain layer.
    pub fn has_filter(&self) -> bool {
        self.blur > 0.0 || self.shadow.is_some()
    }
}

/// A drawing operation in a [`DrawItem`].
#[derive(Debug, Clone)]
pub enum Primitive {
    /// Non-zero winding fill.
    Fill {
        /// Outline to fill.
        path: BezPath,
        /// Fill paint.
        paint: Paint,
    },
    /// Stroked outline.
    Stroke {
        /// Outline to stroke.
        path: BezPath,
        /// Stroke paint.
        paint: Paint,
        /// Width, caps, joins and dashes.
        style: StrokeStyle,
    },
    /// A raster image.
    Image {
        /// Decoded pixels.
        image: Rc<RasterImage>,
        /// Destination rectangle in local coordinates.
        rect: kurbo::Rect,
        /// Image opacity, 0..=1.
        opacity: f64,
    },
    /// Starts a layer; items until the matching `PopLayer` go into it.
    PushLayer(Layer),
    /// Ends the innermost layer and composites it.
    PopLayer,
    /// A marker for a shader that reads the frame: the engine renders what
    /// comes before it, runs the shader on those pixels and draws the rest
    /// over the result. Renderers skip it.
    ReadFrame(crate::post::Shade),
    /// A marker for a node whose own pixels a shader changes. Items up to
    /// the matching [`Primitive::EndEffect`] are drawn alone, shaded, and
    /// laid back over the frame. Renderers skip it.
    BeginEffect(crate::post::Shade),
    /// Ends the node of the matching [`Primitive::BeginEffect`].
    EndEffect,
}

impl Primitive {
    /// A marker that the engine handles instead of a renderer.
    pub fn is_marker(&self) -> bool {
        matches!(
            self,
            Self::ReadFrame(_) | Self::BeginEffect(_) | Self::EndEffect
        )
    }
}

fn faded(c: Color, opacity: f64) -> Option<Color> {
    let a = (f64::from(c.a) * opacity) as f32;
    (a > 0.0).then_some(c.with_alpha(a))
}

fn paint_of(color: Color, gradient: Option<Gradient>, opacity: f64) -> Option<Paint> {
    match gradient {
        Some(g) => {
            let g = g.with_opacity(opacity);
            g.is_visible().then_some(Paint::Gradient(g))
        }
        None => faded(color, opacity).map(Paint::Solid),
    }
}

fn stroke_style(n: &NodeData) -> StrokeStyle {
    StrokeStyle {
        width: n.line_width.get(),
        cap: n.line_cap.get(),
        join: n.line_join.get(),
        dash: n.dash.get().into_iter().filter(|d| *d >= 0.0).collect(),
        dash_offset: n.dash_offset.get(),
    }
}

fn fill_paint(n: &NodeData, opacity: f64) -> Option<Paint> {
    paint_of(n.fill.get(), n.fill_gradient.get(), opacity)
}

fn stroke_paint(n: &NodeData, opacity: f64) -> Option<Paint> {
    paint_of(n.stroke.get(), n.stroke_gradient.get(), opacity)
}

fn fill_and_stroke(n: &NodeData, path: BezPath, opacity: f64, out: &mut Vec<Primitive>) {
    if let Some(paint) = fill_paint(n, opacity) {
        out.push(Primitive::Fill {
            path: path.clone(),
            paint,
        });
    }
    let style = stroke_style(n);
    if style.width > 0.0
        && let Some(paint) = stroke_paint(n, opacity)
    {
        out.push(Primitive::Stroke { path, paint, style });
    }
}

pub(crate) fn primitives(
    n: &NodeData,
    size: (f64, f64),
    opacity: f64,
    time: f64,
) -> Vec<Primitive> {
    let mut out = Vec::new();
    let (start, end) = (n.start.get(), n.end.get());
    match &n.kind {
        NodeKind::Group | NodeKind::Svg { .. } | NodeKind::Camera { .. } => {}
        NodeKind::Rect { .. }
        | NodeKind::Circle { .. }
        | NodeKind::Polygon { .. }
        | NodeKind::Star { .. }
        | NodeKind::Path { .. }
        | NodeKind::Layout { .. } => {
            if let Some(p) = n.outline(size) {
                fill_and_stroke(n, trim(&p, start, end), opacity, &mut out);
            }
        }
        NodeKind::Line {
            start_arrow,
            end_arrow,
            arrow_size,
            ..
        } => {
            let Some(p) = n.outline(size) else { return out };
            let trimmed = trim(&p, start, end);
            if let Some(fill) = fill_paint(n, opacity) {
                out.push(Primitive::Fill {
                    path: trimmed.clone(),
                    paint: fill,
                });
            }
            let arrow = arrow_size.get().max(0.0);
            let style = stroke_style(n);
            if style.width > 0.0
                && let Some(paint) = stroke_paint(n, opacity)
            {
                // Order: fill, line, arrow heads on top.
                let segs: Vec<PathSeg> = trimmed.segments().collect();
                out.push(Primitive::Stroke {
                    path: trimmed,
                    paint: paint.clone(),
                    style,
                });
                if arrow > 0.0 {
                    if *end_arrow && let Some(s) = segs.last() {
                        out.push(Primitive::Fill {
                            path: arrow_head(s.end(), tangent(s, 1.0), arrow),
                            paint: paint.clone(),
                        });
                    }
                    if *start_arrow && let Some(s) = segs.first() {
                        out.push(Primitive::Fill {
                            path: arrow_head(s.start(), -tangent(s, 0.0), arrow),
                            paint,
                        });
                    }
                }
            }
        }
        NodeKind::Txt { .. } | NodeKind::Code { .. } => match n.text_draw() {
            Some(TextDraw::Styled(st)) => {
                let layout = &st.layout;
                let center = Affine::translate((-layout.width / 2.0, -layout.height / 2.0));
                // `start`/`end` reveal glyph by glyph.
                let count = layout.glyphs.len() as f64;
                let (a, b) = (
                    (start.clamp(0.0, 1.0) * count).floor() as usize,
                    (end.clamp(0.0, 1.0) * count).ceil() as usize,
                );
                let base = fill_paint(n, opacity);
                let mut all = BezPath::new();
                // One fill per span color; consecutive glyphs of a span merge.
                let mut current: Option<(u16, BezPath)> = None;
                let flush = |cur: Option<(u16, BezPath)>, out: &mut Vec<Primitive>| {
                    let Some((span, path)) = cur else { return };
                    let paint = match st.colors.get(span as usize).copied().flatten() {
                        Some(c) => faded(c, opacity).map(Paint::Solid),
                        None => base.clone(),
                    };
                    if let Some(paint) = paint {
                        out.push(Primitive::Fill {
                            path: center * path,
                            paint,
                        });
                    }
                };
                for (g, info) in layout.glyphs.iter().zip(&layout.info).take(b).skip(a) {
                    all.extend(g.elements().iter().copied());
                    match &mut current {
                        Some((span, path)) if *span == info.span => {
                            path.extend(g.elements().iter().copied())
                        }
                        _ => {
                            flush(current.take(), &mut out);
                            current = Some((info.span, g.clone()));
                        }
                    }
                }
                flush(current, &mut out);
                let style = stroke_style(n);
                if style.width > 0.0
                    && let Some(paint) = stroke_paint(n, opacity)
                {
                    out.push(Primitive::Stroke {
                        path: center * all,
                        paint,
                        style,
                    });
                }
            }
            Some(TextDraw::Morph { glyphs, .. }) => {
                for (path, alpha) in glyphs {
                    if let Some(paint) = fill_paint(n, opacity * alpha) {
                        out.push(Primitive::Fill { path, paint });
                    }
                }
            }
            Some(TextDraw::Diff(frame)) => {
                let center = Affine::translate((-frame.size.0 / 2.0, -frame.size.1 / 2.0));
                for flash in &frame.flashes {
                    if let Some(c) = faded(flash.color, opacity) {
                        out.push(Primitive::Fill {
                            path: center * flash.rect.to_path(0.1),
                            paint: Paint::Solid(c),
                        });
                    }
                }
                let count = frame.glyphs.len() as f64;
                let (a, b) = (
                    (start.clamp(0.0, 1.0) * count).floor() as usize,
                    (end.clamp(0.0, 1.0) * count).ceil() as usize,
                );
                for glyph in frame.glyphs.iter().take(b).skip(a) {
                    if let Some(c) = faded(glyph.color, opacity) {
                        out.push(Primitive::Fill {
                            path: center * glyph.path.clone(),
                            paint: Paint::Solid(c),
                        });
                    }
                }
            }
            None => {}
        },
        NodeKind::Img { source, .. } => {
            let (w, h) = size;
            let rect = kurbo::Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0);
            match source {
                ImageSource::Raster(image) => out.push(Primitive::Image {
                    image: image.clone(),
                    rect,
                    opacity,
                }),
                ImageSource::Vector(v) => {
                    let tf = Affine::translate((rect.x0, rect.y0))
                        * Affine::scale_non_uniform(w / v.width.max(1e-9), h / v.height.max(1e-9));
                    let k = tf.determinant().abs().sqrt();
                    for s in &v.shapes {
                        let path = tf * s.path.clone();
                        if let Some(c) = s.fill.and_then(|c| faded(c, opacity)) {
                            out.push(Primitive::Fill {
                                path: path.clone(),
                                paint: Paint::Solid(c),
                            });
                        }
                        if let Some((c, width)) = s.stroke
                            && let Some(c) = faded(c, opacity)
                        {
                            out.push(Primitive::Stroke {
                                path,
                                paint: Paint::Solid(c),
                                style: StrokeStyle::solid(width * k),
                            });
                        }
                    }
                }
            }
        }
        NodeKind::Grid { spacing, .. } => {
            let (w, h) = size;
            let step = spacing.get();
            let style = stroke_style(n);
            if step > 0.0
                && style.width > 0.0
                && let Some(paint) = stroke_paint(n, opacity)
            {
                let mut p = BezPath::new();
                // Lines through the center, then outwards, so the grid is
                // symmetric and an origin line always exists.
                let mut k = 0.0;
                while k <= w / 2.0 {
                    for x in [k, -k] {
                        p.move_to((x, -h / 2.0));
                        p.line_to((x, h / 2.0));
                    }
                    k += step;
                }
                let mut k = 0.0;
                while k <= h / 2.0 {
                    for y in [k, -k] {
                        p.move_to((-w / 2.0, y));
                        p.line_to((w / 2.0, y));
                    }
                    k += step;
                }
                out.push(Primitive::Stroke {
                    path: p,
                    paint,
                    style,
                });
            }
        }
        NodeKind::Custom { draw, .. } => {
            let mut ctx = DrawCtx::new(time, size, opacity);
            draw(&mut ctx, time);
            out.extend(ctx.finish());
        }
        NodeKind::Shader { .. } => {}
        NodeKind::Video {
            data, time, radius, ..
        } => {
            let (w, h) = size;
            let rect = kurbo::Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0);
            let r = radius.get().max(0.0);
            if r > 0.0 {
                let clip = kurbo::RoundedRect::from_rect(rect, r).to_path(0.1);
                out.push(Primitive::PushLayer(Layer::clip(clip)));
            }
            match data.source.frame(data.index_at(time.get())) {
                Some(image) => out.push(Primitive::Image {
                    image,
                    rect,
                    opacity,
                }),
                None => {
                    // No frames: a dark box with a play mark, so the layout
                    // still shows where the clip goes.
                    let dark = Color::rgba(0.08, 0.08, 0.1, 1.0);
                    if let Some(c) = faded(dark, opacity) {
                        out.push(Primitive::Fill {
                            path: rect.to_path(0.1),
                            paint: Paint::Solid(c),
                        });
                    }
                    let s = w.min(h) * 0.12;
                    let mut tri = BezPath::new();
                    tri.move_to((-s * 0.6, -s));
                    tri.line_to((s, 0.0));
                    tri.line_to((-s * 0.6, s));
                    tri.close_path();
                    if let Some(c) = faded(Color::rgba(1.0, 1.0, 1.0, 0.5), opacity) {
                        out.push(Primitive::Fill {
                            path: tri,
                            paint: Paint::Solid(c),
                        });
                    }
                }
            }
            if r > 0.0 {
                out.push(Primitive::PopLayer);
            }
        }
        NodeKind::Particles {
            seed,
            gravity,
            toward_camera,
            life,
            speed,
            radius,
            state,
        } => {
            let params = crate::sim::SimParams {
                seed: *seed,
                gravity: gravity.get(),
                toward_camera: *toward_camera,
                life: *life,
                speed: *speed,
                radius: *radius,
            };
            {
                let bursts = state.bursts.borrow();
                state.sim.borrow_mut().at(time, &bursts, params);
            }
            let particles: Vec<_> = state.sim.borrow().particles().to_vec();
            let fill = fill_paint(n, 1.0);
            let Some(paint) = fill else {
                return out;
            };
            for p in particles {
                let depth = if *toward_camera { 0.28 / p.z } else { 1.0 };
                let fade = (1.0 - p.age / p.life).clamp(0.0, 1.0);
                let Paint::Solid(color) = &paint else {
                    continue;
                };
                let a = (f64::from(color.a) * opacity * fade) as f32;
                if a <= 0.0 {
                    continue;
                }
                out.push(Primitive::Fill {
                    path: kurbo::Circle::new((p.x, p.y), p.radius * depth).to_path(0.2),
                    paint: Paint::Solid(color.with_alpha(a)),
                });
            }
        }
    }
    out
}

fn tangent(seg: &PathSeg, t: f64) -> Vec2 {
    let c = seg.to_cubic();
    let d = c.deriv().eval(t).to_vec2();
    // Degenerate handles (e.g. lines raised to cubics) can give a zero
    // derivative at the ends; fall back to the chord.
    if d.hypot() > 1e-9 { d } else { c.p3 - c.p0 }
}

/// Closure a [`crate::Custom`] node paints with: the context and the frame time.
pub type DrawFn = std::rc::Rc<dyn Fn(&mut DrawCtx, f64)>;

/// What a [`crate::Custom`] node draws into. Coordinates are the node's own:
/// the origin is the node center, positive x is right, positive y is down.
///
/// The closure runs every time the frame is painted. It has to be a function
/// of `t`, of signals it captured, and of [`DrawCtx::rng`]: the same inputs
/// must produce the same shapes, because a frame can be painted more than
/// once and a seek paints it without the frames in between.
pub struct DrawCtx {
    prims: Vec<Primitive>,
    time: f64,
    size: (f64, f64),
    opacity: f64,
    quality: f64,
}

impl DrawCtx {
    pub(crate) fn new(time: f64, size: (f64, f64), opacity: f64) -> Self {
        Self {
            prims: Vec::new(),
            time,
            size,
            opacity,
            quality: 1.0,
        }
    }

    pub(crate) fn finish(self) -> Vec<Primitive> {
        self.prims
    }

    /// Time of the frame being painted, in seconds.
    pub fn time(&self) -> f64 {
        self.time
    }

    /// The node's box, in its own coordinates.
    pub fn size(&self) -> (f64, f64) {
        self.size
    }

    /// 1 is a full frame. A later draft mode will pass a smaller value so a
    /// closure can draw fewer shapes; it does not change positions.
    pub fn quality(&self) -> f64 {
        self.quality
    }

    /// Generator for `seed`. It does not touch the scene's `random()`, and
    /// the same seed is the same sequence on every paint of this frame.
    pub fn rng(&self, seed: u64) -> DrawRng {
        DrawRng(oxframe_core::SeededRng::new(seed))
    }

    /// A straight segment from `a` to `b`.
    pub fn line(&mut self, a: impl Into<Point>, b: impl Into<Point>) -> Pen<'_> {
        let mut path = BezPath::new();
        path.move_to(a.into());
        path.line_to(b.into());
        Pen { ctx: self, path }
    }

    /// A filled or stroked circle.
    pub fn circle(&mut self, center: impl Into<Point>, radius: f64) -> Pen<'_> {
        Pen {
            path: kurbo::Circle::new(center.into(), radius.max(0.0)).to_path(0.2),
            ctx: self,
        }
    }

    /// A rectangle in the node's coordinates.
    pub fn rect(&mut self, rect: kurbo::Rect) -> Pen<'_> {
        Pen {
            path: rect.to_path(0.1),
            ctx: self,
        }
    }

    /// An arbitrary outline.
    pub fn path(&mut self, path: BezPath) -> Pen<'_> {
        Pen { ctx: self, path }
    }

    /// An arc of a circle around `center`, starting at `start` degrees
    /// (0 is to the right, clockwise like rotation) and turning `sweep`
    /// degrees. Stroke it for a gauge or a ring.
    pub fn arc(
        &mut self,
        center: impl Into<Point>,
        radius: f64,
        start: f64,
        sweep: f64,
    ) -> Pen<'_> {
        let arc = kurbo::Arc::new(
            center.into(),
            (radius.max(0.0), radius.max(0.0)),
            start.to_radians(),
            sweep.to_radians(),
            0.0,
        );
        Pen {
            path: BezPath::from_iter(arc.path_elements(0.1)),
            ctx: self,
        }
    }
}

/// A generator from [`DrawCtx::rng`].
pub struct DrawRng(oxframe_core::SeededRng);

impl DrawRng {
    /// Uniform number in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        self.0.next_f64()
    }

    /// Uniform number in `span` (`end` is excluded).
    pub fn range(&mut self, span: std::ops::Range<f64>) -> f64 {
        self.0.range(span)
    }
}

/// A shape just added to a [`DrawCtx`], waiting for a fill or a stroke.
pub struct Pen<'a> {
    ctx: &'a mut DrawCtx,
    path: BezPath,
}

impl Pen<'_> {
    /// Fills the shape. A theme role such as `Accent` uses the current theme.
    pub fn fill(self, paint: impl crate::theme::IntoPaint) {
        self.paint(paint.into_paint(), None);
    }

    /// Strokes the shape with `width` in the node's coordinates.
    pub fn stroke(self, paint: impl crate::theme::IntoPaint, width: f64) {
        self.paint(paint.into_paint(), Some(width));
    }

    fn paint(self, color: oxframe_core::Color, stroke: Option<f64>) {
        let a = (f64::from(color.a) * self.ctx.opacity) as f32;
        if a <= 0.0 {
            return;
        }
        let color = color.with_alpha(a);
        match stroke {
            None => self.ctx.prims.push(Primitive::Fill {
                path: self.path,
                paint: Paint::Solid(color),
            }),
            Some(width) if width > 0.0 => self.ctx.prims.push(Primitive::Stroke {
                path: self.path,
                paint: Paint::Solid(color),
                style: StrokeStyle::solid(width),
            }),
            Some(_) => {}
        }
    }
}

fn arrow_head(tip: Point, dir: Vec2, size: f64) -> BezPath {
    let d = if dir.hypot() > 1e-9 {
        dir.normalize()
    } else {
        Vec2::new(1.0, 0.0)
    };
    let n = Vec2::new(-d.y, d.x);
    let base = tip - d * size;
    let mut p = BezPath::new();
    p.move_to(tip);
    p.line_to(base + n * (size * 0.5));
    p.line_to(base - n * (size * 0.5));
    p.close_path();
    p
}
