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

pub(crate) fn primitives(n: &NodeData, size: (f64, f64), opacity: f64) -> Vec<Primitive> {
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
