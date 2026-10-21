//! Pixel shaders that read what was drawn: the frame before a node, or the
//! node's own layer.
//!
//! The engine draws the list in parts. It renders everything before a
//! marker, runs the operation on those pixels, and draws the rest over the
//! result. Both backends hand over pixels, so a frame with `lens_drops` or
//! `ripple` looks the same on the CPU renderer and on the GPU one, and the
//! operations themselves are plain functions of the pixels, the time and
//! the parameters.

use kurbo::{Affine, Point, Rect};
use oxframe_core::{Image, Signal};

/// What a shader reads besides its own parameters.
///
/// The frame and the backdrop are the pixels drawn before the node. A node
/// that sits inside an unfinished layer (a group with opacity below 1, a
/// blur, a clip, a blend or a shadow) cannot read them, and `check` reports
/// `input_cycle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Input {
    /// Everything painted before this node, in screen coordinates.
    Frame,
    /// What the parent painted before this node. On a node that is not in a
    /// layer this is the same as [`Input::Frame`].
    Backdrop,
}

impl Input {
    /// The whole frame before this node.
    pub fn frame() -> Self {
        Self::Frame
    }

    /// What was painted before this node in its parent's layer.
    pub fn backdrop() -> Self {
        Self::Backdrop
    }

    /// `"frame"` or `"backdrop"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Frame => "frame",
            Self::Backdrop => "backdrop",
        }
    }
}

/// What a marker in the draw list does to pixels.
#[derive(Debug, Clone, PartialEq)]
pub enum ShadeOp {
    /// Drops on a lens that flip and magnify what is behind them.
    LensDrops {
        /// 0 is a dry lens, 1 is the most drops.
        wetness: f64,
        /// Same seed, same drops.
        seed: u64,
        /// Drop size multiplier.
        scale: f64,
        /// Scene time in seconds. Drops slide down slowly.
        time: f64,
    },
    /// Rings that push the colors of a node outward from its center. The
    /// node's outline stays where it is.
    Ripple {
        /// Center in scene coordinates.
        center: (f64, f64),
        /// 0 leaves the layer alone, 1 is a strong ring.
        strength: f64,
        /// Ring speed in radians per second.
        speed: f64,
        /// Scene time in seconds.
        time: f64,
    },
}

impl ShadeOp {
    /// `"lens_drops"` or `"ripple"`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::LensDrops { .. } => "lens_drops",
            Self::Ripple { .. } => "ripple",
        }
    }
}

/// A [`ShadeOp`] and how much of it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Shade {
    /// The operation.
    pub op: ShadeOp,
    /// 0 keeps the original pixels, 1 shows the result.
    pub opacity: f64,
    /// Part of the scene the operation may change, in scene coordinates.
    /// `None` is the whole image.
    pub area: Option<Rect>,
}

/// Runs `shade` on `img`. `to_px` maps scene coordinates to the pixels of `img`.
pub fn apply(shade: &Shade, img: &mut Image, to_px: Affine) {
    let opacity = shade.opacity.clamp(0.0, 1.0) as f32;
    if opacity <= 0.0 || img.width == 0 || img.height == 0 {
        return;
    }
    let Some(area) = shade.area else {
        run(shade, img, to_px, opacity);
        return;
    };
    let r = to_px.transform_rect_bbox(area);
    let x0 = r.x0.floor().clamp(0.0, f64::from(img.width)) as u32;
    let y0 = r.y0.floor().clamp(0.0, f64::from(img.height)) as u32;
    let x1 = r.x1.ceil().clamp(0.0, f64::from(img.width)) as u32;
    let y1 = r.y1.ceil().clamp(0.0, f64::from(img.height)) as u32;
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    if (x0, y0, x1, y1) == (0, 0, img.width, img.height) {
        run(shade, img, to_px, opacity);
        return;
    }
    let (cw, ch) = (x1 - x0, y1 - y0);
    let width = img.width;
    let row = |y: u32| ((y * width + x0) * 4) as usize..((y * width + x1) * 4) as usize;
    let mut crop = Image {
        width: cw,
        height: ch,
        data: Vec::with_capacity((cw * ch * 4) as usize),
    };
    for y in y0..y1 {
        crop.data.extend_from_slice(&img.data[row(y)]);
    }
    let shift = Affine::translate((-f64::from(x0), -f64::from(y0))) * to_px;
    run(shade, &mut crop, shift, opacity);
    let stride = (cw * 4) as usize;
    for (i, y) in (y0..y1).enumerate() {
        img.data[row(y)].copy_from_slice(&crop.data[i * stride..(i + 1) * stride]);
    }
}

fn run(shade: &Shade, img: &mut Image, to_px: Affine, opacity: f32) {
    match &shade.op {
        ShadeOp::LensDrops {
            wetness,
            seed,
            scale,
            time,
        } => lens_drops(img, *wetness, *seed, *scale, *time, opacity),
        ShadeOp::Ripple {
            center,
            strength,
            speed,
            time,
        } => ripple(img, to_px, *center, *strength, *speed, *time, opacity),
    }
}

const MAX_DROPS: usize = 28;

fn lens_drops(img: &mut Image, wetness: f64, seed: u64, scale: f64, time: f64, opacity: f32) {
    let wet = wetness.clamp(0.0, 1.0);
    if wet <= 0.0 || !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let (w, h) = (img.width as usize, img.height as usize);
    let (fw, fh) = (w as f32, h as f32);
    let side = fw.min(fh);
    let src = img.data.clone();
    let count = wet * MAX_DROPS as f64;
    for i in 0..MAX_DROPS {
        let grow = (count - i as f64).clamp(0.0, 1.0) as f32;
        if grow <= 0.0 {
            break;
        }
        let (a, b, c) = (
            unit(seed, i as u64, 0),
            unit(seed, i as u64, 1),
            unit(seed, i as u64, 2),
        );
        let r = (0.035 + 0.05 * c) * side * scale as f32 * smooth(0.0, 1.0, grow);
        let speed = 0.01 + 0.03 * b;
        let cx = a * fw;
        let cy = (b * 1.7 % 1.0 + (time as f32).max(0.0) * speed).rem_euclid(1.0) * fh;
        if r < 1.0 {
            continue;
        }
        let x0 = ((cx - r - 1.0).floor().max(0.0)) as usize;
        let x1 = (((cx + r + 1.0).ceil()) as usize).min(w);
        let y0 = ((cy - r * 1.15 - 1.0).floor().max(0.0)) as usize;
        let y1 = (((cy + r * 1.15 + 1.0).ceil()) as usize).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + (dy / 1.15) * (dy / 1.15)).sqrt() / r;
                if d >= 1.0 {
                    continue;
                }
                let edge = 1.0 - smooth(0.9, 1.0, d);
                let mut px = sample(&src, w, h, cx - dx * 0.8, cy - dy * 0.8);
                let rim = smooth(0.7, 0.97, d);
                let light = if d > 1.0e-4 {
                    (dx / (r * d)) * -0.7 + (dy / (r * d)) * -0.7
                } else {
                    0.0
                };
                let highlight = light.max(0.0).powi(3) * rim * 0.4;
                let shade = (-light).max(0.0) * rim * 0.25;
                for ch in px.iter_mut().take(3) {
                    *ch = *ch * (1.0 - shade) + (1.0 - *ch) * highlight;
                }
                let i = (y * w + x) * 4;
                let mix = edge * opacity;
                for (k, v) in px.iter().enumerate() {
                    let old = f32::from(img.data[i + k]) / 255.0;
                    img.data[i + k] = byte(old + (v - old) * mix);
                }
            }
        }
    }
}

fn ripple(
    img: &mut Image,
    to_px: Affine,
    center: (f64, f64),
    strength: f64,
    speed: f64,
    time: f64,
    opacity: f32,
) {
    let strength = strength.clamp(0.0, 1.0) as f32;
    if strength <= 0.0 {
        return;
    }
    let (w, h) = (img.width as usize, img.height as usize);
    let c = to_px * Point::new(center.0, center.1);
    let k = to_px.determinant().abs().sqrt().max(1.0e-6) as f32;
    let (cx, cy) = (c.x as f32, c.y as f32);
    let amp = 18.0 * strength * k;
    let wavelength = 64.0 * k;
    let reach = 520.0 * k;
    let phase = (time * speed) as f32;
    let src = img.data.clone();
    let span = reach * 2.5;
    let x0 = ((cx - span).floor().max(0.0)) as usize;
    let x1 = (((cx + span).ceil().max(0.0)) as usize).min(w);
    let y0 = ((cy - span).floor().max(0.0)) as usize;
    let y1 = (((cy + span).ceil().max(0.0)) as usize).min(h);
    for y in y0..y1 {
        for x in x0..x1 {
            let vx = x as f32 + 0.5 - cx;
            let vy = y as f32 + 0.5 - cy;
            let d = (vx * vx + vy * vy).sqrt();
            if d < 1.0e-3 {
                continue;
            }
            let env = (-d / reach).exp() * (1.0 - (-d / (12.0 * k)).exp());
            let shift = amp * (d / wavelength * std::f32::consts::TAU - phase).sin() * env;
            if shift.abs() < 1.0e-3 {
                continue;
            }
            let px = sample(
                &src,
                w,
                h,
                x as f32 + 0.5 - vx / d * shift,
                y as f32 + 0.5 - vy / d * shift,
            );
            // Only colors move: a pixel pulled from outside the node
            // would cut a hole in it, so the outline (alpha) stays.
            if px[3] < 1.0e-3 {
                continue;
            }
            let i = (y * w + x) * 4;
            for (k, v) in px.iter().enumerate().take(3) {
                let old = f32::from(img.data[i + k]) / 255.0;
                img.data[i + k] = byte(old + (v - old) * opacity);
            }
        }
    }
}

/// Bilinear sample at pixel coordinates (centers at .5), straight alpha out.
fn sample(data: &[u8], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    let fx = (x - 0.5).clamp(0.0, (w - 1) as f32);
    let fy = (y - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let mut acc = [0.0f32; 4];
    for (ix, iy, wgt) in [
        (x0, y0, (1.0 - tx) * (1.0 - ty)),
        (x1, y0, tx * (1.0 - ty)),
        (x0, y1, (1.0 - tx) * ty),
        (x1, y1, tx * ty),
    ] {
        let i = (iy * w + ix) * 4;
        let a = f32::from(data[i + 3]) / 255.0;
        acc[0] += f32::from(data[i]) / 255.0 * a * wgt;
        acc[1] += f32::from(data[i + 1]) / 255.0 * a * wgt;
        acc[2] += f32::from(data[i + 2]) / 255.0 * a * wgt;
        acc[3] += a * wgt;
    }
    if acc[3] > 1.0e-6 {
        [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3], acc[3]]
    } else {
        [0.0, 0.0, 0.0, 0.0]
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Deterministic value in `0..1` for `(seed, index, lane)`.
fn unit(seed: u64, index: u64, lane: u64) -> f32 {
    let mut z = seed
        .wrapping_add(index.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(lane.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(0x94D0_49BB_1331_11EB);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

/// Paints `top` over `dst` (both straight alpha, same size).
///
/// Drawing a list in two parts and laying the second over the first gives
/// the same picture as drawing it at once, up to rounding, for every node
/// with the normal blend.
pub fn over(dst: &mut Image, top: &Image) {
    debug_assert_eq!((dst.width, dst.height), (top.width, top.height));
    for (d, t) in dst.data.chunks_exact_mut(4).zip(top.data.chunks_exact(4)) {
        if t[3] == 0 {
            continue;
        }
        if t[3] == 255 {
            d.copy_from_slice(t);
            continue;
        }
        let ta = f32::from(t[3]) / 255.0;
        let da = f32::from(d[3]) / 255.0;
        let a = ta + da * (1.0 - ta);
        for k in 0..3 {
            let c = f32::from(t[k]) / 255.0 * ta + f32::from(d[k]) / 255.0 * da * (1.0 - ta);
            d[k] = byte(c / a);
        }
        d[3] = byte(a);
    }
}

/// A shader effect stored on a node. Built with [`ripple`](crate::ripple).
#[doc(hidden)]
#[derive(Clone)]
pub struct EffectData {
    /// Which operation.
    pub kind: EffectKind,
    /// 0 leaves the node alone, 1 is the full effect.
    pub strength: Signal<f64>,
    /// How fast the effect moves.
    pub speed: Signal<f64>,
}

/// Effects a node can carry.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// Rings from the node's center.
    Ripple,
}

impl EffectData {
    /// The marker for a node whose center is `center` (scene coordinates).
    pub(crate) fn shade(&self, center: Point, time: f64) -> Shade {
        match self.kind {
            EffectKind::Ripple => Shade {
                op: ShadeOp::Ripple {
                    center: (center.x, center.y),
                    strength: self.strength.get(),
                    speed: self.speed.get(),
                    time,
                },
                opacity: 1.0,
                area: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Image {
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 90, 255]);
            }
        }
        Image {
            width: w,
            height: h,
            data,
        }
    }

    fn lens(wetness: f64, seed: u64, time: f64) -> Shade {
        Shade {
            op: ShadeOp::LensDrops {
                wetness,
                seed,
                scale: 1.0,
                time,
            },
            opacity: 1.0,
            area: None,
        }
    }

    #[test]
    fn a_dry_lens_changes_nothing() {
        let mut img = gradient(64, 48);
        let before = img.data.clone();
        apply(&lens(0.0, 1, 0.0), &mut img, Affine::IDENTITY);
        assert_eq!(img.data, before);
    }

    #[test]
    fn drops_change_pixels_and_follow_the_seed() {
        let run = |seed| {
            let mut img = gradient(160, 90);
            apply(&lens(1.0, seed, 0.0), &mut img, Affine::IDENTITY);
            img.data
        };
        let base = gradient(160, 90).data;
        assert_ne!(run(7), base);
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }

    #[test]
    fn a_wetter_lens_covers_more() {
        let changed = |wetness| {
            let mut img = gradient(160, 90);
            let before = img.data.clone();
            apply(&lens(wetness, 3, 0.0), &mut img, Affine::IDENTITY);
            img.data.iter().zip(&before).filter(|(a, b)| a != b).count()
        };
        assert!(changed(1.0) > changed(0.3));
    }

    #[test]
    fn ripple_moves_pixels_near_the_center_and_not_far_from_it() {
        let mut img = gradient(200, 100);
        let before = img.data.clone();
        let shade = Shade {
            op: ShadeOp::Ripple {
                center: (0.0, 0.0),
                strength: 1.0,
                speed: 6.0,
                time: 0.4,
            },
            opacity: 1.0,
            area: None,
        };
        apply(&shade, &mut img, Affine::translate((100.0, 50.0)));
        let at =
            |d: &[u8], x: usize, y: usize| d[(y * 200 + x) * 4..(y * 200 + x) * 4 + 3].to_vec();
        assert_ne!(at(&img.data, 130, 50), at(&before, 130, 50));
    }

    #[test]
    fn an_area_keeps_the_rest_of_the_image() {
        let mut img = gradient(160, 90);
        let before = img.data.clone();
        let mut shade = lens(1.0, 5, 0.0);
        // Scene and image share pixels here; the area is the left half.
        shade.area = Some(Rect::new(0.0, 0.0, 80.0, 90.0));
        apply(&shade, &mut img, Affine::IDENTITY);
        for y in 0..90usize {
            for x in 80..160usize {
                let i = (y * 160 + x) * 4;
                assert_eq!(img.data[i..i + 4], before[i..i + 4], "({x}, {y})");
            }
        }
        assert_ne!(img.data, before);
    }

    #[test]
    fn over_an_opaque_pixel_with_a_clear_one_keeps_it() {
        let mut dst = gradient(4, 4);
        let before = dst.data.clone();
        let clear = Image {
            width: 4,
            height: 4,
            data: vec![0; 64],
        };
        over(&mut dst, &clear);
        assert_eq!(dst.data, before);
    }

    #[test]
    fn a_ripple_with_no_strength_is_a_no_op() {
        let mut img = gradient(40, 40);
        let before = img.data.clone();
        let shade = Shade {
            op: ShadeOp::Ripple {
                center: (0.0, 0.0),
                strength: 0.0,
                speed: 6.0,
                time: 1.0,
            },
            opacity: 1.0,
            area: None,
        };
        apply(&shade, &mut img, Affine::translate((20.0, 20.0)));
        assert_eq!(img.data, before);
    }
}
