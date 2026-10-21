use std::collections::BTreeMap;

use kurbo::{Affine, BezPath, PathEl, Shape};
use oxframe_2d::{
    DrawItem, Layer, LineCap, LineJoin, Paint as MPaint, Primitive, RasterImage, SceneGraph,
    StrokeStyle,
};
use oxframe_core::{BlendMode, Color, Gradient, GradientKind};
use tiny_skia::{
    FillRule, FilterQuality, GradientStop, LinearGradient, Mask, Paint, PathBuilder, Pixmap,
    PixmapPaint, RadialGradient, SpreadMode, Stroke, StrokeDash, Transform,
};

use crate::blur::gaussian_blur;
use crate::{Image, RenderOptions, Renderer, scene_to_pixels};

/// tiny-skia backend. Deterministic: same scene, same bytes.
#[derive(Default)]
pub struct CpuRenderer {
    pixmap: Option<Pixmap>,
    images: BTreeMap<u64, Pixmap>,
    /// Reused layer buffers.
    pool: Vec<Pixmap>,
    /// Pre-composite pixels of layers whose vectors did not change.
    layers: Vec<CachedLayer>,
    /// Items left to ignore because their layer was replayed from [`Self::layers`].
    skip: u32,
}

struct CachedLayer {
    key: u64,
    region: Region,
    buf: Pixmap,
    seen: bool,
}

impl CpuRenderer {
    /// A CPU renderer; buffers are reused between frames.
    pub fn new() -> Self {
        Self::default()
    }

    /// Renders draw items (with `view` mapping scene → pixels) into a
    /// transparent pixmap. Used by the GPU renderer for filtered layers.
    pub fn render_items(&mut self, items: &[DrawItem], view: Affine, w: u32, h: u32) -> Pixmap {
        let mut target = Pixmap::new(w.max(1), h.max(1)).expect("valid pixmap size");
        self.draw_items(&mut target, items, view);
        target
    }

    fn take_layer(&mut self, w: u32, h: u32) -> Pixmap {
        let (w, h) = (w.max(1), h.max(1));
        match self
            .pool
            .iter()
            .position(|p| p.width() == w && p.height() == h)
        {
            Some(i) => {
                let mut p = self.pool.swap_remove(i);
                p.fill(tiny_skia::Color::TRANSPARENT);
                p
            }
            None => Pixmap::new(w, h).expect("valid pixmap size"),
        }
    }

    fn draw_items(&mut self, base: &mut Pixmap, items: &[DrawItem], view: Affine) {
        let full = Region {
            x: 0,
            y: 0,
            w: base.width(),
            h: base.height(),
        };
        for slot in &mut self.layers {
            slot.seen = false;
        }
        self.skip = 0;
        // Open layers: buffer, the region it covers, layer, transform, cache key, replayed.
        let mut stack: Vec<(Pixmap, Region, Layer, Affine, u64, bool)> = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let tf = view * item.transform;
            if self.skip > 0 {
                match &item.prim {
                    Primitive::PushLayer(_) => self.skip += 1,
                    Primitive::PopLayer => {
                        self.skip -= 1;
                        if self.skip == 0 {
                            self.pop_layer(&mut stack, base, &full);
                        }
                    }
                    _ => {}
                }
                continue;
            }
            match &item.prim {
                Primitive::PushLayer(layer) => {
                    let parent = stack.last().map_or(full, |(_, r, _, _, _, _)| *r);
                    let region = layer_region(items, i, view, parent).unwrap_or(Region {
                        x: parent.x,
                        y: parent.y,
                        w: 1,
                        h: 1,
                    });
                    let key = cache_key(oxframe_2d::layer_fingerprint(items, i), region);
                    if let Some(slot) = self
                        .layers
                        .iter_mut()
                        .find(|slot| slot.key == key && slot.region == region)
                    {
                        slot.seen = true;
                        stack.push((slot.buf.clone(), region, layer.clone(), tf, key, true));
                        self.skip = 1;
                    } else {
                        let buf = self.take_layer(region.w, region.h);
                        stack.push((buf, region, layer.clone(), tf, key, false));
                    }
                }
                Primitive::PopLayer => self.pop_layer(&mut stack, base, &full),
                prim => {
                    let (dst, region) = match stack.last_mut() {
                        Some((p, r, _, _, _, _)) => (p, *r),
                        None => (&mut *base, full),
                    };
                    draw_prim(dst, prim, region.shift() * tf, &mut self.images);
                }
            }
        }
        while !stack.is_empty() {
            self.pop_layer(&mut stack, base, &full);
        }
        self.layers.retain(|slot| slot.seen);
        self.skip = 0;
    }

    fn pop_layer(
        &mut self,
        stack: &mut Vec<(Pixmap, Region, Layer, Affine, u64, bool)>,
        base: &mut Pixmap,
        full: &Region,
    ) {
        let Some((buf, region, layer, ltf, key, replay)) = stack.pop() else {
            return;
        };
        if !replay {
            if let Some(slot) = self.layers.iter_mut().find(|slot| slot.key == key) {
                slot.buf = buf.clone();
                slot.region = region;
                slot.seen = true;
            } else {
                self.layers.push(CachedLayer {
                    key,
                    region,
                    buf: buf.clone(),
                    seen: true,
                });
            }
        }
        let (dst, dst_region) = match stack.last_mut() {
            Some((p, r, _, _, _, _)) => (p, *r),
            None => (&mut *base, *full),
        };
        composite(dst, dst_region, buf, region, &layer, ltf, &mut self.pool);
    }
}

/// A pixel rectangle of the output frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Region {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

impl Region {
    /// Maps frame pixels to this region's buffer.
    fn shift(self) -> Affine {
        Affine::translate((-f64::from(self.x), -f64::from(self.y)))
    }
}

fn cache_key(fingerprint: u64, region: Region) -> u64 {
    fingerprint
        ^ (region.w as u64).wrapping_shl(1)
        ^ (region.h as u64).wrapping_shl(17)
        ^ (region.x as u32 as u64).wrapping_shl(33)
        ^ (region.y as u32 as u64).wrapping_shl(49)
}

/// Pixel bounds of everything between the `PushLayer` at `start` and its
/// `PopLayer`, grown by the reach of blurs and shadows, clipped to `within`.
fn layer_region(items: &[DrawItem], start: usize, view: Affine, within: Region) -> Option<Region> {
    let mut bb: Option<kurbo::Rect> = None;
    let mut grow = 0.0f64;
    let mut depth = 0usize;
    for item in &items[start..] {
        let tf = view * item.transform;
        let r = match &item.prim {
            Primitive::PushLayer(l) => {
                depth += 1;
                let k = scale_of(tf);
                grow += 3.0 * l.blur * k;
                if let Some(sh) = &l.shadow {
                    let off = linear(tf) * sh.offset.to_point();
                    grow += 3.0 * sh.blur * k + off.x.abs().max(off.y.abs());
                }
                None
            }
            Primitive::PopLayer => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                None
            }
            Primitive::Fill { path, .. } => Some(tf.transform_rect_bbox(path.bounding_box())),
            Primitive::Stroke { path, style, .. } => {
                let pad = style.width * scale_of(tf);
                Some(
                    tf.transform_rect_bbox(path.bounding_box())
                        .inflate(pad, pad),
                )
            }
            Primitive::Image { rect, .. } => Some(tf.transform_rect_bbox(*rect)),
        };
        if let Some(r) = r {
            bb = Some(bb.map_or(r, |b| b.union(r)));
        }
    }
    let r = bb?.inflate(grow + 2.0, grow + 2.0);
    let x0 = r.x0.floor().max(f64::from(within.x)) as i32;
    let y0 = r.y0.floor().max(f64::from(within.y)) as i32;
    let x1 = r.x1.ceil().min(f64::from(within.x) + f64::from(within.w)) as i32;
    let y1 = r.y1.ceil().min(f64::from(within.y) + f64::from(within.h)) as i32;
    (x1 > x0 && y1 > y0).then(|| Region {
        x: x0,
        y: y0,
        w: (x1 - x0) as u32,
        h: (y1 - y0) as u32,
    })
}

fn scale_of(tf: Affine) -> f64 {
    tf.determinant().abs().sqrt()
}

fn composite(
    dst: &mut Pixmap,
    dst_region: Region,
    mut buf: Pixmap,
    region: Region,
    layer: &Layer,
    tf: Affine,
    pool: &mut Vec<Pixmap>,
) {
    let (w, h) = (buf.width() as usize, buf.height() as usize);
    let k = scale_of(tf);
    if let Some(clip) = &layer.clip
        && let Some(path) = to_skia_path(clip)
        && let Some(mut mask) = Mask::new(buf.width(), buf.height())
    {
        mask.fill_path(
            &path,
            FillRule::Winding,
            true,
            to_skia_transform(region.shift() * tf),
        );
        buf.apply_mask(&mask);
    }
    // Where this buffer goes in the destination buffer.
    let (dx, dy) = (region.x - dst_region.x, region.y - dst_region.y);
    if layer.blur > 0.0 {
        gaussian_blur(buf.data_mut(), w, h, layer.blur * k);
    }
    let paint = PixmapPaint {
        opacity: layer.opacity.clamp(0.0, 1.0) as f32,
        blend_mode: blend_mode(layer.blend),
        quality: FilterQuality::Nearest,
    };
    if let Some(shadow) = &layer.shadow {
        // Shadow = the layer's coverage tinted, blurred and offset.
        let mut sh = buf.clone();
        let [r, g, b, a] = shadow.color.to_rgba8().map(u32::from);
        for px in sh.data_mut().chunks_exact_mut(4) {
            let cov = u32::from(px[3]) * a / 255;
            px[0] = (r * cov / 255) as u8;
            px[1] = (g * cov / 255) as u8;
            px[2] = (b * cov / 255) as u8;
            px[3] = cov as u8;
        }
        gaussian_blur(sh.data_mut(), w, h, shadow.blur * k);
        let off = linear(tf) * shadow.offset.to_point();
        dst.draw_pixmap(
            dx,
            dy,
            sh.as_ref(),
            &paint,
            Transform::from_translate(off.x as f32, off.y as f32),
            None,
        );
        pool.push(sh);
    }
    dst.draw_pixmap(dx, dy, buf.as_ref(), &paint, Transform::identity(), None);
    pool.push(buf);
}

/// The linear part of an affine map (for offsets).
fn linear(tf: Affine) -> Affine {
    let [a, b, c, d, _, _] = tf.as_coeffs();
    Affine::new([a, b, c, d, 0.0, 0.0])
}

pub(crate) fn blend_mode(b: BlendMode) -> tiny_skia::BlendMode {
    use tiny_skia::BlendMode as T;
    match b {
        BlendMode::Normal => T::SourceOver,
        BlendMode::Multiply => T::Multiply,
        BlendMode::Screen => T::Screen,
        BlendMode::Overlay => T::Overlay,
        BlendMode::Darken => T::Darken,
        BlendMode::Lighten => T::Lighten,
        BlendMode::ColorDodge => T::ColorDodge,
        BlendMode::ColorBurn => T::ColorBurn,
        BlendMode::HardLight => T::HardLight,
        BlendMode::SoftLight => T::SoftLight,
        BlendMode::Difference => T::Difference,
        BlendMode::Exclusion => T::Exclusion,
        BlendMode::Hue => T::Hue,
        BlendMode::Saturation => T::Saturation,
        BlendMode::Color => T::Color,
        BlendMode::Luminosity => T::Luminosity,
    }
}

fn draw_prim(dst: &mut Pixmap, prim: &Primitive, tf: Affine, images: &mut BTreeMap<u64, Pixmap>) {
    match prim {
        Primitive::Fill { path, paint } => {
            if let (Some(path), Some(p)) = (to_skia_path(path), skia_paint(paint)) {
                dst.fill_path(&path, &p, FillRule::Winding, to_skia_transform(tf), None);
            }
        }
        Primitive::Stroke { path, paint, style } => {
            if let (Some(path), Some(p)) = (to_skia_path(path), skia_paint(paint)) {
                dst.stroke_path(&path, &p, &skia_stroke(style), to_skia_transform(tf), None);
            }
        }
        Primitive::Image {
            image,
            rect,
            opacity,
        } => {
            let src = match images.entry(image.id) {
                std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::btree_map::Entry::Vacant(e) => {
                    let Some(pm) = to_pixmap(image) else { return };
                    e.insert(pm)
                }
            };
            let place = tf
                * Affine::translate((rect.x0, rect.y0))
                * Affine::scale_non_uniform(
                    rect.width() / f64::from(image.width),
                    rect.height() / f64::from(image.height),
                );
            let paint = PixmapPaint {
                opacity: *opacity as f32,
                quality: FilterQuality::Bicubic,
                ..Default::default()
            };
            dst.draw_pixmap(0, 0, src.as_ref(), &paint, to_skia_transform(place), None);
        }
        Primitive::PushLayer(_) | Primitive::PopLayer => {}
    }
}

pub(crate) fn to_skia_path(path: &BezPath) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(a, p) => pb.quad_to(a.x as f32, a.y as f32, p.x as f32, p.y as f32),
            PathEl::CurveTo(a, b, p) => pb.cubic_to(
                a.x as f32, a.y as f32, b.x as f32, b.y as f32, p.x as f32, p.y as f32,
            ),
            PathEl::ClosePath => pb.close(),
        }
    }
    pb.finish()
}

fn to_skia_transform(a: Affine) -> Transform {
    let [a, b, c, d, e, f] = a.as_coeffs().map(|v| v as f32);
    Transform::from_row(a, b, c, d, e, f)
}

fn skia_color(c: Color) -> Option<tiny_skia::Color> {
    tiny_skia::Color::from_rgba(
        c.r.clamp(0.0, 1.0),
        c.g.clamp(0.0, 1.0),
        c.b.clamp(0.0, 1.0),
        c.a.clamp(0.0, 1.0),
    )
}

fn shader(g: &Gradient) -> Option<tiny_skia::Shader<'static>> {
    let stops: Vec<GradientStop> = g
        .stops
        .iter()
        .filter_map(|(o, c)| Some(GradientStop::new(*o as f32, skia_color(*c)?)))
        .collect();
    let pt = |p: kurbo::Point| tiny_skia::Point::from_xy(p.x as f32, p.y as f32);
    match g.kind {
        GradientKind::Linear { from, to } => LinearGradient::new(
            pt(from),
            pt(to),
            stops,
            SpreadMode::Pad,
            Transform::identity(),
        ),
        GradientKind::Radial { center, radius } => RadialGradient::new(
            pt(center),
            0.0,
            pt(center),
            radius.max(1e-3) as f32,
            stops,
            SpreadMode::Pad,
            Transform::identity(),
        ),
    }
}

fn skia_paint(p: &MPaint) -> Option<Paint<'static>> {
    let mut paint = Paint::default();
    match p {
        MPaint::Solid(c) => paint.set_color(skia_color(*c)?),
        MPaint::Gradient(g) => paint.shader = shader(g)?,
    }
    paint.anti_alias = true;
    Some(paint)
}

fn skia_stroke(s: &StrokeStyle) -> Stroke {
    Stroke {
        width: s.width as f32,
        line_cap: match s.cap {
            LineCap::Butt => tiny_skia::LineCap::Butt,
            LineCap::Round => tiny_skia::LineCap::Round,
            LineCap::Square => tiny_skia::LineCap::Square,
        },
        line_join: match s.join {
            LineJoin::Miter => tiny_skia::LineJoin::Miter,
            LineJoin::Round => tiny_skia::LineJoin::Round,
            LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
        },
        dash: (s.dash.len() >= 2 && s.dash.iter().any(|d| *d > 0.0))
            .then(|| {
                // tiny-skia needs an even count.
                let mut d: Vec<f32> = s.dash.iter().map(|v| *v as f32).collect();
                if d.len() % 2 == 1 {
                    d.extend_from_within(..);
                }
                StrokeDash::new(d, s.dash_offset as f32)
            })
            .flatten(),
        ..Default::default()
    }
}

fn to_pixmap(img: &RasterImage) -> Option<Pixmap> {
    let mut pm = Pixmap::new(img.width, img.height)?;
    for (dst, src) in pm.pixels_mut().iter_mut().zip(img.rgba.chunks_exact(4)) {
        *dst = tiny_skia::ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
    }
    Some(pm)
}

/// Converts a premultiplied pixmap to straight-alpha RGBA8.
pub(crate) fn demultiply(pixmap: &Pixmap) -> Vec<u8> {
    let mut data = Vec::with_capacity(pixmap.data().len());
    for px in pixmap.pixels() {
        let c = px.demultiply();
        data.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    data
}

impl Renderer for CpuRenderer {
    fn render(&mut self, graph: &SceneGraph, options: &RenderOptions) -> Image {
        let (w, h) = (options.width.max(1), options.height.max(1));
        let mut base = match self.pixmap.take() {
            Some(p) if p.width() == w && p.height() == h => p,
            _ => Pixmap::new(w, h).expect("valid pixmap size"),
        };
        let bg = options.background;
        base.fill(skia_color(bg).unwrap_or(tiny_skia::Color::BLACK));
        let view = scene_to_pixels(graph.size(), (w, h));
        let items = graph.draw_list();
        self.draw_items(&mut base, &items, view);
        let data = demultiply(&base);
        self.pixmap = Some(base);
        Image {
            width: w,
            height: h,
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use oxframe_2d::{NodeBuilder, Rect, View};

    use super::*;

    #[test]
    fn draws_centered_rect() {
        let graph = SceneGraph::new(100, 100);
        graph.enter(|| {
            View::current().add(
                Rect::new()
                    .size(50.0)
                    .fill(Color::from_hex("#ff0000").unwrap()),
            );
        });
        let img = CpuRenderer::new().render(
            &graph,
            &RenderOptions {
                width: 100,
                height: 100,
                background: Color::BLACK,
            },
        );
        let px = |x: usize, y: usize| &img.data[(y * 100 + x) * 4..][..4];
        assert_eq!(px(50, 50), [255, 0, 0, 255]);
        assert_eq!(px(10, 10), [0, 0, 0, 255]);
        assert_eq!(px(74, 50), [255, 0, 0, 255]);
        assert_eq!(px(76, 50), [0, 0, 0, 255]);
    }

    #[test]
    fn a_static_blur_replays_the_same_bytes() {
        let graph = SceneGraph::new(64, 64);
        graph.enter(|| {
            View::current().add(
                Rect::new()
                    .size(24.0)
                    .fill(Color::from_hex("#ff0000").unwrap())
                    .blur(1.5),
            );
        });
        let opts = RenderOptions {
            width: 64,
            height: 64,
            background: Color::BLACK,
        };
        let mut cached = CpuRenderer::new();
        let first = cached.render(&graph, &opts);
        let second = cached.render(&graph, &opts);
        let fresh = CpuRenderer::new().render(&graph, &opts);
        assert_eq!(first.data, fresh.data);
        assert_eq!(second.data, fresh.data);
    }

    #[test]
    fn a_moved_blur_is_not_served_from_the_old_buffer() {
        let opts = RenderOptions {
            width: 64,
            height: 64,
            background: Color::BLACK,
        };
        let at = |x: f64| {
            let graph = SceneGraph::new(64, 64);
            graph.enter(|| {
                View::current().add(Rect::new().size(16.0).x(x).fill(Color::WHITE).blur(1.0));
            });
            graph
        };
        let mut renderer = CpuRenderer::new();
        let before = renderer.render(&at(0.0), &opts);
        let after = renderer.render(&at(18.0), &opts);
        assert_ne!(before.data, after.data);
    }
}
