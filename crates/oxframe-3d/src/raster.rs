//! A software rasterizer: triangles, a depth buffer, metal-rough shading
//! and supersampling. Pure Rust, single-threaded, so the same scene gives
//! the same pixels in a render, in `check` and in the browser.

use std::f64::consts::PI;
use std::hash::{Hash, Hasher};

use oxframe_core::Color;

use crate::math::{Mat4, Vec3};
use crate::mesh::SharedGeometry;
use crate::scene::Material;

/// Closest distance from the camera that is drawn.
const NEAR: f64 = 0.01;

/// A rendered picture: straight-alpha sRGB RGBA8, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

pub(crate) struct ObjectDraw {
    pub geometry: SharedGeometry,
    pub world: Mat4,
    pub material: Material,
}

pub(crate) struct LightDraw {
    pub point: bool,
    pub vector: Vec3,
    pub color: Color,
    pub intensity: f64,
}

/// A snapshot of the scene for one picture.
pub(crate) struct Look {
    pub objects: Vec<ObjectDraw>,
    pub lights: Vec<LightDraw>,
    pub eye: Vec3,
    pub target: Vec3,
    pub fov: f64,
    pub ambient: Color,
    pub background: Color,
}

fn hash_f(h: &mut impl Hasher, v: f64) {
    v.to_bits().hash(h);
}

fn hash_v(h: &mut impl Hasher, v: Vec3) {
    hash_f(h, v.x);
    hash_f(h, v.y);
    hash_f(h, v.z);
}

fn hash_c(h: &mut impl Hasher, c: Color) {
    for v in [c.r, c.g, c.b, c.a] {
        v.to_bits().hash(h);
    }
}

impl Look {
    pub fn hash_into(&self, h: &mut impl Hasher) {
        for o in &self.objects {
            (std::rc::Rc::as_ptr(&o.geometry) as usize).hash(h);
            for col in o.world.0 {
                for v in col {
                    hash_f(h, v);
                }
            }
            let m = &o.material;
            hash_c(h, m.color);
            hash_c(h, m.emissive);
            for v in [m.metallic, m.roughness, m.opacity] {
                hash_f(h, v);
            }
        }
        for l in &self.lights {
            l.point.hash(h);
            hash_v(h, l.vector);
            hash_c(h, l.color);
            hash_f(h, l.intensity);
        }
        hash_v(h, self.eye);
        hash_v(h, self.target);
        hash_f(h, self.fov);
        hash_c(h, self.ambient);
        hash_c(h, self.background);
    }
}

fn to_linear(c: f32) -> f64 {
    let c = f64::from(c);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f64) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0 + 0.5) as u8
}

fn linear(c: Color) -> Vec3 {
    Vec3::new(to_linear(c.r), to_linear(c.g), to_linear(c.b))
}

/// A vertex in clip space with what the shader needs.
#[derive(Clone, Copy)]
struct Vert {
    clip: [f64; 4],
    world: Vec3,
    normal: Vec3,
}

impl Vert {
    fn mix(a: Vert, b: Vert, t: f64) -> Vert {
        let c = |i| a.clip[i] + (b.clip[i] - a.clip[i]) * t;
        Vert {
            clip: [c(0), c(1), c(2), c(3)],
            world: a.world + (b.world - a.world) * t,
            normal: a.normal + (b.normal - a.normal) * t,
        }
    }
}

/// The part of a triangle in front of the near plane, as a fan.
fn clip_near(tri: [Vert; 3]) -> Vec<Vert> {
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let (a, b) = (tri[i], tri[(i + 1) % 3]);
        let (da, db) = (a.clip[3] - NEAR, b.clip[3] - NEAR);
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            out.push(Vert::mix(a, b, da / (da - db)));
        }
    }
    out
}

/// Inverse transpose of the 3 × 3 part: turns normals correctly under a
/// non-uniform scale.
fn normal_matrix(m: &Mat4) -> [[f64; 3]; 3] {
    let a = |c: usize, r: usize| m.0[c][r];
    let cof = [
        [
            a(1, 1) * a(2, 2) - a(2, 1) * a(1, 2),
            a(2, 1) * a(0, 2) - a(0, 1) * a(2, 2),
            a(0, 1) * a(1, 2) - a(1, 1) * a(0, 2),
        ],
        [
            a(2, 0) * a(1, 2) - a(1, 0) * a(2, 2),
            a(0, 0) * a(2, 2) - a(2, 0) * a(0, 2),
            a(1, 0) * a(0, 2) - a(0, 0) * a(1, 2),
        ],
        [
            a(1, 0) * a(2, 1) - a(2, 0) * a(1, 1),
            a(2, 0) * a(0, 1) - a(0, 0) * a(2, 1),
            a(0, 0) * a(1, 1) - a(1, 0) * a(0, 1),
        ],
    ];
    // The cofactor matrix is the inverse transpose up to a scale; normals
    // are normalized after, so the determinant does not matter.
    cof
}

struct Target {
    w: usize,
    h: usize,
    /// Premultiplied linear RGBA.
    color: Vec<[f32; 4]>,
    /// Distance from the camera (view-space w).
    depth: Vec<f32>,
}

/// A light with its color already in linear light.
struct LinearLight {
    point: bool,
    /// Position of a bulb, or the direction toward the sun.
    vector: Vec3,
    radiance: Vec3,
}

struct Shading {
    eye: Vec3,
    ambient: Vec3,
    lights: Vec<LinearLight>,
}

/// A material with everything per object worked out once.
struct Surface {
    base: Vec3,
    emissive: Vec3,
    f0: Vec3,
    metallic: f64,
    roughness: f64,
    a2: f64,
    k: f64,
}

impl Surface {
    fn new(m: &Material) -> Surface {
        let base = linear(m.color);
        let a = m.roughness * m.roughness;
        Surface {
            base,
            emissive: linear(m.emissive),
            f0: Vec3::splat(0.04) * (1.0 - m.metallic) + base * m.metallic,
            metallic: m.metallic,
            roughness: m.roughness,
            a2: a * a,
            k: (m.roughness + 1.0).powi(2) / 8.0,
        }
    }
}

impl Shading {
    fn shade(&self, m: &Surface, p: Vec3, n: Vec3) -> Vec3 {
        let mut n = n.normalize();
        let v = (self.eye - p).normalize();
        if n.dot(v) < 0.0 {
            n = -n;
        }
        let (base, f0, a2, k) = (m.base, m.f0, m.a2, m.k);
        let nv = n.dot(v).max(1e-4);
        // Ambient light from a sky above and a darker ground below, so
        // sides facing away from the lights keep their shape and metals
        // have something to reflect.
        let hemi = |dir: Vec3| self.ambient * (0.55 + 0.9 * (dir.y * 0.5 + 0.5));
        let r = n * (2.0 * n.dot(v)) - v;
        let env_f = f0
            + (Vec3::splat((1.0 - m.roughness).max(f0.x.max(f0.y).max(f0.z))) - f0)
                * (1.0 - nv).powi(5);
        let mut out = hemi(n).mul_each(base) * (1.0 - m.metallic)
            + hemi(r).mul_each(env_f) * (1.0 - 0.6 * m.roughness);
        for light in &self.lights {
            let (l, radiance) = if light.point {
                let d = light.vector - p;
                (d.normalize(), light.radiance * (1.0 / (1.0 + d.dot(d))))
            } else {
                (light.vector, light.radiance)
            };
            let nl = n.dot(l);
            if nl <= 0.0 {
                continue;
            }
            let hv = (l + v).normalize();
            let nh = n.dot(hv).max(0.0);
            let vh = v.dot(hv).max(0.0);
            let d = a2 / (PI * (nh * nh * (a2 - 1.0) + 1.0).powi(2));
            let g = (nv / (nv * (1.0 - k) + k)) * (nl / (nl * (1.0 - k) + k));
            let f = f0 + (Vec3::ONE - f0) * (1.0 - vh).powi(5);
            let spec = f * (d * g / (4.0 * nv * nl + 1e-4));
            let kd = (Vec3::ONE - f) * (1.0 - m.metallic);
            out = out + (kd.mul_each(base) + spec * PI).mul_each(radiance) * nl;
        }
        out + m.emissive
    }
}

pub(crate) fn render(look: &Look, width: u32, height: u32, samples: u32) -> Frame {
    let s = samples.clamp(1, 4) as usize;
    let (w, h) = (width.max(1) as usize * s, height.max(1) as usize * s);
    let bg = linear(look.background);
    let ba = f64::from(look.background.a);
    let clear = [
        (bg.x * ba) as f32,
        (bg.y * ba) as f32,
        (bg.z * ba) as f32,
        ba as f32,
    ];
    let mut target = Target {
        w,
        h,
        color: vec![clear; w * h],
        depth: vec![f32::INFINITY; w * h],
    };
    let aspect = f64::from(width.max(1)) / f64::from(height.max(1));
    let view = Mat4::look_at(look.eye, look.target, Vec3::Y);
    let vp = Mat4::perspective(look.fov, aspect, NEAR, 1000.0).mul(&view);
    let shading = Shading {
        eye: look.eye,
        ambient: linear(look.ambient),
        lights: look
            .lights
            .iter()
            .map(|l| LinearLight {
                point: l.point,
                vector: if l.point {
                    l.vector
                } else {
                    -l.vector.normalize()
                },
                radiance: linear(l.color) * l.intensity,
            })
            .collect(),
    };
    // Opaque objects first, then see-through ones from far to near.
    let distance = |i: usize| (look.objects[i].world.point(Vec3::ZERO) - look.eye).length();
    let mut order: Vec<usize> = (0..look.objects.len()).collect();
    order.sort_by(|&a, &b| {
        let (ta, tb) = (
            look.objects[a].material.opacity < 1.0,
            look.objects[b].material.opacity < 1.0,
        );
        ta.cmp(&tb).then_with(|| {
            if ta {
                distance(b).total_cmp(&distance(a))
            } else {
                std::cmp::Ordering::Equal
            }
        })
    });
    for i in order {
        draw_object(&mut target, &look.objects[i], &vp, &shading);
    }
    resolve(&target, width.max(1), height.max(1), s)
}

fn draw_object(t: &mut Target, o: &ObjectDraw, vp: &Mat4, shading: &Shading) {
    let m = &o.material;
    let alpha = f64::from(m.color.a) * m.opacity;
    if alpha <= 0.0 {
        return;
    }
    let surface = Surface::new(m);
    let mvp = vp.mul(&o.world);
    let nm = normal_matrix(&o.world);
    let g = &o.geometry;
    let verts: Vec<Vert> = g
        .positions
        .iter()
        .zip(&g.normals)
        .map(|(&p, &n)| Vert {
            clip: mvp.project(p),
            world: o.world.point(p),
            normal: Vec3::new(
                nm[0][0] * n.x + nm[0][1] * n.y + nm[0][2] * n.z,
                nm[1][0] * n.x + nm[1][1] * n.y + nm[1][2] * n.z,
                nm[2][0] * n.x + nm[2][1] * n.y + nm[2][2] * n.z,
            ),
        })
        .collect();
    for tri in &g.triangles {
        let poly = clip_near(tri.map(|i| verts[i as usize]));
        for k in 1..poly.len().saturating_sub(1) {
            fill(t, [poly[0], poly[k], poly[k + 1]], &surface, alpha, shading);
        }
    }
}

fn fill(t: &mut Target, tri: [Vert; 3], m: &Surface, alpha: f64, shading: &Shading) {
    let (wf, hf) = (t.w as f64, t.h as f64);
    let screen = tri.map(|v| {
        let iw = 1.0 / v.clip[3];
        (
            (v.clip[0] * iw * 0.5 + 0.5) * wf,
            (0.5 - v.clip[1] * iw * 0.5) * hf,
            iw,
        )
    });
    let [(x0, y0, w0), (x1, y1, w1), (x2, y2, w2)] = screen;
    let area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
    if area.abs() < 1e-12 {
        return;
    }
    let min_x = x0.min(x1).min(x2).floor().max(0.0) as usize;
    let max_x = (x0.max(x1).max(x2).ceil().min(wf)) as usize;
    let min_y = y0.min(y1).min(y2).floor().max(0.0) as usize;
    let max_y = (y0.max(y1).max(y2).ceil().min(hf)) as usize;
    let inv_area = 1.0 / area;
    let (a32, k32) = (alpha as f32, (1.0 - alpha) as f32);
    for py in min_y..max_y {
        let y = py as f64 + 0.5;
        for px in min_x..max_x {
            let x = px as f64 + 0.5;
            let b0 = ((x1 - x) * (y2 - y) - (x2 - x) * (y1 - y)) * inv_area;
            let b1 = ((x2 - x) * (y0 - y) - (x0 - x) * (y2 - y)) * inv_area;
            let b2 = 1.0 - b0 - b1;
            if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                continue;
            }
            let iw = b0 * w0 + b1 * w1 + b2 * w2;
            let depth = (1.0 / iw) as f32;
            let at = py * t.w + px;
            if depth >= t.depth[at] {
                continue;
            }
            let (c0, c1, c2) = (b0 * w0 / iw, b1 * w1 / iw, b2 * w2 / iw);
            let p = tri[0].world * c0 + tri[1].world * c1 + tri[2].world * c2;
            let n = tri[0].normal * c0 + tri[1].normal * c1 + tri[2].normal * c2;
            let c = shading.shade(m, p, n);
            let (r, g, b) = (c.x as f32, c.y as f32, c.z as f32);
            let dst = &mut t.color[at];
            if alpha >= 1.0 {
                *dst = [r, g, b, 1.0];
                t.depth[at] = depth;
            } else {
                *dst = [
                    r * a32 + dst[0] * k32,
                    g * a32 + dst[1] * k32,
                    b * a32 + dst[2] * k32,
                    a32 + dst[3] * k32,
                ];
            }
        }
    }
}

/// Linear light to sRGB bytes through a table: every output pixel is
/// converted, and `powf` per channel was most of the resolve step.
const LUT_SIZE: usize = 4096;

fn srgb_lut() -> &'static [u8; LUT_SIZE] {
    static LUT: std::sync::OnceLock<[u8; LUT_SIZE]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        let mut t = [0u8; LUT_SIZE];
        for (i, v) in t.iter_mut().enumerate() {
            *v = to_srgb(i as f64 / (LUT_SIZE - 1) as f64);
        }
        t
    })
}

fn resolve(t: &Target, width: u32, height: u32, s: usize) -> Frame {
    let (w, h) = (width as usize, height as usize);
    let mut rgba = Vec::with_capacity(w * h * 4);
    let n = (s * s) as f32;
    let lut = srgb_lut();
    let enc = |v: f32| lut[(v.clamp(0.0, 1.0) * (LUT_SIZE - 1) as f32 + 0.5) as usize];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 4];
            for sy in 0..s {
                let row = (y * s + sy) * t.w + x * s;
                for c in &t.color[row..row + s] {
                    for (a, v) in acc.iter_mut().zip(c) {
                        *a += v;
                    }
                }
            }
            let a = acc[3] / n;
            if a <= 0.0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let un = |v: f32| enc(v / n / a);
            rgba.extend_from_slice(&[
                un(acc[0]),
                un(acc[1]),
                un(acc[2]),
                (a.min(1.0) * 255.0 + 0.5) as u8,
            ]);
        }
    }
    Frame {
        width,
        height,
        rgba,
    }
}
