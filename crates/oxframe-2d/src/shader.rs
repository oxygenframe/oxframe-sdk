//! A shader node: WGSL `Params` plus `shade`, checked with naga.
//!
//! The waves preset is rasterized on the CPU, so a check without a GPU
//! still paints it. Both backends then draw that image, which keeps the
//! preset the same on either one. The lens_drops preset reads the frame
//! drawn before it (see [`crate::post`]). A custom `shade` is checked on
//! every draw (a changed file is picked up without a rebuild) and run on
//! the CPU by [`crate::wgsl_cpu`], natively and in the browser.

use std::cell::Cell;

use kurbo::{Affine, Point};
use oxframe_core::{Color, Signal};

use crate::draw::Primitive;
use crate::graph::{NodeData, NodeKind};
use crate::image::RasterImage;

/// Where the WGSL text comes from. Stored on a shader node.
#[doc(hidden)]
#[derive(Clone)]
pub enum ShaderCode {
    /// Source kept on the node.
    Wgsl(String),
    /// Read again on every draw, so an edit shows up without a rebuild.
    File(std::path::PathBuf),
}

/// Which built-in a shader node runs. Stored on a shader node.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// User WGSL: checked, and run on the CPU per pixel (not in wasm).
    Custom,
    /// [`waves`](crate::waves): painted inside the node's box.
    Waves,
    /// [`lens_drops`](crate::lens_drops): changes the frame drawn before it.
    LensDrops,
}

impl Preset {
    /// `"custom"`, `"waves"` or `"lens_drops"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::Waves => "waves",
            Self::LensDrops => "lens_drops",
        }
    }
}

/// Parameters of the lens_drops preset, all numbers.
const LENS_PARAMS: [&str; 3] = ["wetness", "seed", "drop_size"];

/// Inputs the lens_drops preset reads.
const LENS_INPUTS: [&str; 1] = ["scene"];

/// One value passed to `Params`. Stored on a shader node.
#[doc(hidden)]
pub struct Uniform {
    pub name: String,
    pub value: UniformValue,
}

/// A number, a point, a color, or a bool stored as 0 or 1.
#[doc(hidden)]
#[allow(dead_code)]
pub enum UniformValue {
    /// An `f32` field.
    Num(Signal<f64>),
    /// A `vec2<f32>` field.
    Vec2(Signal<kurbo::Vec2>),
    /// A `vec4<f32>` color field.
    Color(Signal<Color>),
    /// A `bool` field, stored as 0 or 1 so it can be animated.
    Bool(Signal<f64>),
}

/// A WGSL problem found while painting.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderDiag {
    /// `.name(..)`, or the node kind and id.
    pub node: String,
    /// `file:line` of the `add` call, when it was recorded.
    pub source: Option<String>,
    /// `shader_error`, `param_unknown`, or `shader_slow`.
    pub rule: &'static str,
    /// What to change.
    pub hint: String,
}

const WAVES: &str = r#"struct Params { amp: f32, speed: f32, deep: vec4<f32>, foam: vec4<f32> }
@group(1) @binding(0) var<uniform> p: Params;

fn shade(i: ShadeIn) -> vec4<f32> {
    let h = sin(i.world.x * 0.008 + ox.time * p.speed) * p.amp;
    let t = smoothstep(h - 0.02, h + 0.02, i.uv.y - 0.5);
    return mix(p.foam, p.deep, t);
}
"#;

/// WGSL of the waves preset.
pub(crate) fn waves_wgsl() -> &'static str {
    WAVES
}

const PRELUDE: &str = r#"struct Ox {
    time: f32,
    frame: f32,
    resolution: vec2<f32>,
    size: vec2<f32>,
    seed: f32,
    view: mat4x4<f32>,
    progress: f32,
}
@group(0) @binding(0) var<uniform> ox: Ox;

struct ShadeIn {
    uv: vec2<f32>,
    local: vec2<f32>,
    world: vec2<f32>,
    screen: vec2<f32>,
}

"#;

const EPILOGUE: &str = r#"
struct _OxVertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) world: vec2<f32>,
    @location(3) screen: vec2<f32>,
}

@vertex
fn _ox_vs(@builtin(vertex_index) index: u32) -> _OxVertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = positions[index];
    var out: _OxVertexOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    out.local = out.uv;
    out.world = out.uv;
    out.screen = out.uv;
    return out;
}

@fragment
fn _ox_fs(in: _OxVertexOut) -> @location(0) vec4<f32> {
    var s: ShadeIn;
    s.uv = in.uv;
    s.local = in.local;
    s.world = in.world;
    s.screen = in.screen;
    return shade(s);
}
"#;

thread_local! {
    /// Scene size and frame rate of the graph being drawn, for `ox`.
    static SCENE: Cell<((f64, f64), f64)> = const { Cell::new(((1920.0, 1080.0), 60.0)) };
    static DRAFT: Cell<bool> = const { Cell::new(false) };
    static BUDGET_MS: Cell<f64> = const { Cell::new(250.0) };
    static DIAGS: std::cell::RefCell<Vec<ShaderDiag>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Sets the scene size for `ox.resolution` and `ox.screen`; the graph
/// calls this before it builds a draw list.
pub(crate) fn set_scene_size(size: (f64, f64)) {
    SCENE.with(|s| s.set((size, s.get().1)));
}

/// Frames per second, for `ox.frame` in custom shaders. The engine sets it
/// before it draws.
#[doc(hidden)]
pub fn set_frame_rate(fps: f64) {
    SCENE.with(|s| s.set((s.get().0, fps.max(1.0))));
}

pub(crate) fn clear_diags() {
    DIAGS.with(|d| d.borrow_mut().clear());
}

pub(crate) fn push_diag(diag: ShaderDiag) {
    DIAGS.with(|d| d.borrow_mut().push(diag));
}

pub(crate) fn take_diags() -> Vec<ShaderDiag> {
    DIAGS.with(|d| std::mem::take(&mut *d.borrow_mut()))
}

/// Restores the previous draft flag when dropped.
pub struct ShaderDraft {
    prev: bool,
}

impl Drop for ShaderDraft {
    fn drop(&mut self) {
        DRAFT.with(|d| d.set(self.prev));
    }
}

/// Paints shader nodes at half resolution until dropped.
///
/// Timings and positions stay the same. `oxframe check` uses this so a
/// shader layer does not dominate the budget.
pub fn draft_shaders() -> ShaderDraft {
    let prev = DRAFT.with(Cell::get);
    DRAFT.with(|d| d.set(true));
    ShaderDraft { prev }
}

/// Milliseconds a shader raster may take before `shader_slow`.
pub fn set_shader_budget_ms(ms: f64) {
    BUDGET_MS.with(|b| b.set(ms));
}

fn draft() -> bool {
    DRAFT.with(Cell::get)
}

fn budget_ms() -> f64 {
    BUDGET_MS.with(Cell::get)
}

/// Compile and parameter problems for this node, and a video that could
/// not be opened. Empty for other nodes.
pub(crate) fn problems(n: &NodeData) -> Vec<ShaderDiag> {
    if let NodeKind::Video { data, .. } = &n.kind {
        return data
            .problem
            .iter()
            .map(|p| ShaderDiag {
                node: label(n),
                source: n.source.clone(),
                rule: "video_unavailable",
                hint: format!("{}: {p}", data.path.display()),
            })
            .collect();
    }
    let NodeKind::Shader {
        code,
        uniforms,
        preset,
        inputs,
        ..
    } = &n.kind
    else {
        return Vec::new();
    };
    let label = label(n);
    let source = n.source.clone();
    let diag = |hint: String| {
        vec![ShaderDiag {
            node: label.clone(),
            source: source.clone(),
            rule: "param_unknown",
            hint,
        }]
    };
    let known: &[&str] = match preset {
        Preset::LensDrops => &LENS_INPUTS,
        Preset::Waves => &[],
        Preset::Custom => &[],
    };
    if *preset == Preset::Custom {
        for (name, _) in inputs {
            if !is_ident(name) {
                return diag(format!(
                    "input `{name}` is read as `sample_{name}(uv)`, so its name must be letters, digits and `_`."
                ));
            }
        }
    } else {
        for (name, _) in inputs {
            if !known.contains(&name.as_str()) {
                return diag(unknown("input", name, known));
            }
        }
    }
    if *preset == Preset::LensDrops {
        for u in uniforms {
            if !LENS_PARAMS.contains(&u.name.as_str()) {
                return diag(unknown("parameter", &u.name, &LENS_PARAMS));
            }
            if !matches!(u.value, UniformValue::Num(_)) {
                return diag(format!("`{}` of lens_drops is a number.", u.name));
            }
        }
        return Vec::new();
    }
    let text = match load(code) {
        Ok(text) => text,
        Err(hint) => {
            return vec![ShaderDiag {
                node: label,
                source,
                rule: "shader_error",
                hint,
            }];
        }
    };
    let names: Vec<&str> = inputs.iter().map(|(n, _)| n.as_str()).collect();
    match check_wgsl(&text, uniforms, &names) {
        Ok(()) => Vec::new(),
        Err(diag) => vec![ShaderDiag {
            node: label,
            source,
            rule: diag.0,
            hint: diag.1,
        }],
    }
}

/// Raster of a waves node, or nothing when this node is not the preset.
pub(crate) fn raster(
    n: &NodeData,
    size: (f64, f64),
    tf: Affine,
    time: f64,
    opacity: f64,
) -> Option<Primitive> {
    let NodeKind::Shader {
        preset,
        uniforms,
        code,
        ..
    } = &n.kind
    else {
        return None;
    };
    let (w, h) = (size.0.max(1.0), size.1.max(1.0));
    let scale = if draft() { 0.5 } else { 1.0 };
    let iw = ((w * scale).round() as u32).clamp(1, 1920);
    let ih = ((h * scale).round() as u32).clamp(1, 1080);
    let started = Stopwatch::start();
    let rgba = match preset {
        Preset::Waves => {
            let (amp, speed, deep, foam) = wave_uniforms(uniforms);
            paint(
                iw,
                ih,
                |uv, local| {
                    let world = tf * Point::new(local.0, local.1);
                    let crest = (world.x * 0.008 + time * speed).sin() * amp;
                    let t = smoothstep(crest - 0.02, crest + 0.02, uv.1 - 0.5);
                    Ok(mix(foam, deep, t, opacity))
                },
                (w, h),
            )
        }
        Preset::Custom if !reads_frame_input(n) => {
            custom(n, code, uniforms, (w, h), tf, time, opacity, (iw, ih))
        }
        _ => {
            let _ = code;
            return None;
        }
    };
    let rgba = match rgba {
        Ok(rgba) => rgba,
        Err(hint) => {
            push_diag(ShaderDiag {
                node: label(n),
                source: n.source.clone(),
                rule: "shader_error",
                hint,
            });
            return None;
        }
    };
    let ms = started.millis();
    if ms > budget_ms() {
        push_diag(ShaderDiag {
            node: label(n),
            source: n.source.clone(),
            rule: "shader_slow",
            hint: format!(
                "This shader took {ms:.0} ms in draft. The budget is {:.0} ms.",
                budget_ms()
            ),
        });
    }
    let rect = kurbo::Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0);
    Some(Primitive::Image {
        image: std::rc::Rc::new(RasterImage {
            width: iw,
            height: ih,
            id: hash_bytes(&rgba),
            rgba,
        }),
        rect,
        opacity: 1.0,
    })
}

/// The frame marker of a lens_drops node, or nothing for other shaders.
///
/// `area` is the node's box in scene coordinates. The node reads the frame
/// drawn before it whatever its input is named: a backdrop outside a layer
/// is the frame.
pub(crate) fn read_frame(
    nodes: &[NodeData],
    n: &NodeData,
    size: (f64, f64),
    tf: Affine,
    time: f64,
    opacity: f64,
) -> Option<crate::post::Shade> {
    if reads_frame_input(n) {
        return custom_read_frame(nodes, n, size, tf, time, opacity);
    }
    let NodeKind::Shader {
        preset: Preset::LensDrops,
        uniforms,
        ..
    } = &n.kind
    else {
        return None;
    };
    let num = |name: &str, default: f64| {
        uniforms
            .iter()
            .find_map(|u| match (&u.name[..], &u.value) {
                (m, UniformValue::Num(v)) if m == name => Some(v.get()),
                _ => None,
            })
            .unwrap_or(default)
    };
    let (w, h) = (size.0.max(0.0), size.1.max(0.0));
    let area = tf.transform_rect_bbox(kurbo::Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0));
    Some(crate::post::Shade {
        op: crate::post::ShadeOp::LensDrops {
            wetness: num("wetness", 0.6),
            seed: num("seed", 0.0).max(0.0) as u64,
            scale: num("drop_size", 1.0),
            time,
        },
        opacity,
        area: Some(area),
    })
}

/// The frame-reading pass of a custom shader, or nothing (with a
/// `shader_error`) when it does not compile.
fn custom_read_frame(
    nodes: &[NodeData],
    n: &NodeData,
    size: (f64, f64),
    tf: Affine,
    time: f64,
    opacity: f64,
) -> Option<crate::post::Shade> {
    let NodeKind::Shader {
        code,
        uniforms,
        inputs,
        ..
    } = &n.kind
    else {
        return None;
    };
    let (program, globals) = match prepare(n, code, uniforms, size, time) {
        Ok(ok) => ok,
        Err(hint) => {
            push_diag(ShaderDiag {
                node: label(n),
                source: n.source.clone(),
                rule: "shader_error",
                hint,
            });
            return None;
        }
    };
    let frame = inputs
        .iter()
        .filter(|(_, i)| matches!(i, crate::post::Input::Frame | crate::post::Input::Backdrop))
        .map(|(name, _)| name.clone())
        .collect();
    let offscreen = inputs
        .iter()
        .filter_map(|(name, i)| match i {
            crate::post::Input::Node(id) if (id.0 as usize) < nodes.len() => {
                let (items, bounds) = crate::graph::offscreen_items(nodes, *id, time);
                Some(NodeInput {
                    name: name.clone(),
                    items,
                    bounds,
                })
            }
            _ => None,
        })
        .collect();
    let ((sw, sh), _) = SCENE.with(Cell::get);
    let (w, h) = (size.0.max(0.0), size.1.max(0.0));
    let area = tf.transform_rect_bbox(kurbo::Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0));
    Some(crate::post::Shade {
        op: crate::post::ShadeOp::Custom(Box::new(CustomShade {
            program,
            globals,
            frame,
            offscreen,
            tf,
            size: (w, h),
            scene: (sw, sh),
        })),
        opacity,
        area: Some(area),
    })
}

/// `no input `x`. Did you mean `y`? Available: …` for a preset.
fn unknown(what: &str, name: &str, known: &[&str]) -> String {
    let list = if known.is_empty() {
        "none".to_owned()
    } else {
        known.join(", ")
    };
    match closest(name, known) {
        Some(n) => format!("no {what} `{name}`. Did you mean `{n}`? Available: {list}."),
        None => format!("no {what} `{name}`. Available: {list}."),
    }
}

/// Wall clock for `shader_slow`.
///
/// `Instant::now()` panics on `wasm32-unknown-unknown`, which ended the
/// playground's waves example in "unreachable executed". A check never runs
/// in the browser, so there the watch reads 0 and `shader_slow` stays quiet.
#[cfg(not(target_arch = "wasm32"))]
struct Stopwatch(std::time::Instant);

#[cfg(target_arch = "wasm32")]
struct Stopwatch;

impl Stopwatch {
    fn start() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self(std::time::Instant::now())
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self
        }
    }

    fn millis(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.0.elapsed().as_secs_f64() * 1000.0
        }
        #[cfg(target_arch = "wasm32")]
        {
            0.0
        }
    }
}

/// RGBA bytes of an `iw` × `ih` raster of a `size` box: `pixel` gets the
/// uv (0..1, from the top left) and the local point (from the center).
fn paint(
    iw: u32,
    ih: u32,
    mut pixel: impl FnMut((f64, f64), (f64, f64)) -> Result<Color, String>,
    size: (f64, f64),
) -> Result<Vec<u8>, String> {
    let (w, h) = size;
    let mut rgba = vec![0u8; iw as usize * ih as usize * 4];
    for py in 0..ih {
        for px in 0..iw {
            let u = (f64::from(px) + 0.5) / f64::from(iw);
            let v = (f64::from(py) + 0.5) / f64::from(ih);
            let c = pixel((u, v), (u * w - w / 2.0, v * h - h / 2.0))?;
            let i = (py as usize * iw as usize + px as usize) * 4;
            rgba[i] = byte(c.r);
            rgba[i + 1] = byte(c.g);
            rgba[i + 2] = byte(c.b);
            rgba[i + 3] = byte(c.a);
        }
    }
    Ok(rgba)
}

/// A custom `shade`, run per pixel of its own box.
#[allow(clippy::too_many_arguments)]
fn custom(
    n: &NodeData,
    code: &ShaderCode,
    uniforms: &[Uniform],
    size: (f64, f64),
    tf: Affine,
    time: f64,
    opacity: f64,
    raster: (u32, u32),
) -> Result<Vec<u8>, String> {
    let (program, globals) = prepare(n, code, uniforms, size, time)?;
    let ((sw, sh), _) = SCENE.with(Cell::get);
    let (iw, ih) = raster;
    let a = opacity.clamp(0.0, 1.0) as f32;
    let (w, h) = size;
    let total = iw as usize * ih as usize;
    let mut rgba = vec![0u8; total * 4];
    run_tiles(
        &program,
        &globals,
        total,
        |i| {
            let (px, py) = ((i % iw as usize) as f64, (i / iw as usize) as f64);
            let u = (px + 0.5) / f64::from(iw);
            let v = (py + 0.5) / f64::from(ih);
            let local = (u * w - w / 2.0, v * h - h / 2.0);
            let world = tf * Point::new(local.0, local.1);
            [
                u,
                v,
                local.0,
                local.1,
                world.x,
                world.y,
                world.x + sw / 2.0,
                world.y + sh / 2.0,
            ]
        },
        |i, [r, g, b, alpha]| {
            let o = i * 4;
            rgba[o] = byte(r);
            rgba[o + 1] = byte(g);
            rgba[o + 2] = byte(b);
            rgba[o + 3] = byte(alpha * a);
        },
    )?;
    Ok(rgba)
}

/// The compiled shader of `n` and the values it reads: `ox`, the `Params`
/// fields, and its image inputs.
fn prepare(
    n: &NodeData,
    code: &ShaderCode,
    uniforms: &[Uniform],
    size: (f64, f64),
    time: f64,
) -> Result<
    (
        std::rc::Rc<crate::wgsl_cpu::Program>,
        crate::wgsl_cpu::Globals,
    ),
    String,
> {
    use crate::wgsl_cpu::{Globals, Value};
    let inputs = match &n.kind {
        NodeKind::Shader { inputs, .. } => inputs.as_slice(),
        _ => &[],
    };
    let names: Vec<&str> = inputs.iter().map(|(n, _)| n.as_str()).collect();
    let text = load(code)?;
    let program = compiled(&text, &names).map_err(|(_, hint)| hint)?;
    let ((sw, sh), fps) = SCENE.with(Cell::get);
    let identity = Value::C(
        (0..4)
            .map(|c| {
                let mut col = [0.0; 4];
                col[c] = 1.0;
                Value::fv(&col)
            })
            .collect(),
    );
    let ox = Value::C(vec![
        Value::f(time as f32),
        Value::f((time * fps + 1e-6).floor() as f32),
        Value::fv(&[sw as f32, sh as f32]),
        Value::fv(&[size.0 as f32, size.1 as f32]),
        Value::f(0.0),
        identity,
        Value::f(0.0),
    ]);
    let params = program
        .params()
        .into_iter()
        .map(|name| {
            uniforms
                .iter()
                .find(|u| u.name == name)
                .map_or(Value::f(0.0), |u| match &u.value {
                    UniformValue::Num(v) => Value::f(v.get() as f32),
                    UniformValue::Vec2(v) => {
                        let v = v.get();
                        Value::fv(&[v.x as f32, v.y as f32])
                    }
                    UniformValue::Color(v) => {
                        let c = v.get();
                        Value::fv(&[c.r, c.g, c.b, c.a])
                    }
                    UniformValue::Bool(v) => Value::B(1, [v.get() > 0.5, false, false, false]),
                })
        })
        .collect();
    let globals = Globals {
        ox,
        params: Value::C(params),
        inputs: inputs
            .iter()
            .filter_map(|(name, input)| match input {
                crate::post::Input::Image(img) => Some((name.clone(), img.clone())),
                _ => None,
            })
            .collect(),
    };
    Ok((program, globals))
}

/// Runs `shade` for pixels `0..total`, a tile at a time. `at` gives a
/// pixel's `ShadeIn` (uv, local, world, screen); `put` takes its color.
fn run_tiles(
    program: &crate::wgsl_cpu::Program,
    globals: &crate::wgsl_cpu::Globals,
    total: usize,
    at: impl Fn(usize) -> [f64; 8],
    mut put: impl FnMut(usize, [f32; 4]),
) -> Result<(), String> {
    use crate::wgsl_cpu::{LANES, V};
    let mut runner = program.runner(globals);
    let mut start = 0;
    while start < total {
        let count = (total - start).min(LANES);
        let mut comps = vec![[0.0f32; LANES]; 8];
        for k in 0..count {
            for (c, x) in comps.iter_mut().zip(at(start + k)) {
                c[k] = x as f32;
            }
        }
        let input = V::C(comps.chunks(2).map(|pair| V::F(pair.to_vec())).collect());
        let out = runner
            .run(&input, count)
            .map_err(|e| format!("{e} (when oxframe runs the shader on the CPU)"))?;
        for k in 0..count {
            let rgba = out
                .lane(k)
                .floats()
                .ok_or("`shade` must return vec4<f32>")?;
            put(start + k, rgba);
        }
        start += count;
    }
    Ok(())
}

/// A custom `shade` with a frame or backdrop input. It runs between the
/// parts of the draw list, over the pixels drawn before it (see
/// [`crate::post`]), and is painted over them inside its box.
#[derive(Clone)]
pub struct CustomShade {
    program: std::rc::Rc<crate::wgsl_cpu::Program>,
    globals: crate::wgsl_cpu::Globals,
    /// Inputs that read the frame.
    frame: Vec<String>,
    /// Inputs that read a node drawn alone.
    offscreen: Vec<NodeInput>,
    tf: Affine,
    size: (f64, f64),
    scene: (f64, f64),
}

/// A node a custom shader reads: its draw list and its box in scene
/// coordinates.
#[derive(Clone)]
struct NodeInput {
    name: String,
    items: Vec<crate::DrawItem>,
    bounds: kurbo::Rect,
}

impl PartialEq for NodeInput {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.bounds == other.bounds
            && self.items.len() == other.items.len()
            && self.items.iter().zip(&other.items).all(|(a, b)| {
                a.node == b.node
                    && a.transform == b.transform
                    && crate::graph::prim_same(&a.prim, &b.prim)
            })
    }
}

impl std::fmt::Debug for CustomShade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomShade")
            .field("frame", &self.frame)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl PartialEq for CustomShade {
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.program, &other.program)
            && self.globals.ox == other.globals.ox
            && self.globals.params == other.globals.params
            && self.globals.inputs.len() == other.globals.inputs.len()
            && self
                .globals
                .inputs
                .iter()
                .zip(&other.globals.inputs)
                .all(|(a, b)| a.0 == b.0 && a.1.id == b.1.id)
            && self.frame == other.frame
            && self.offscreen == other.offscreen
            && self.tf == other.tf
            && self.size == other.size
            && self.scene == other.scene
    }
}

impl CustomShade {
    /// Draw lists of the nodes this shader reads, in order. The engine
    /// draws each alone on a clear target of the frame's size and passes
    /// the images to [`crate::post::apply_with`].
    pub fn offscreen_lists(&self) -> Vec<&[crate::DrawItem]> {
        self.offscreen.iter().map(|n| n.items.as_slice()).collect()
    }

    /// Runs the shader over `img` inside the node's box. `to_px` maps
    /// scene coordinates to the pixels of `img`; `drawn` holds the images
    /// of [`offscreen_lists`](Self::offscreen_lists). A shader that fails
    /// while running leaves the pixels as they were; `check` reports it.
    pub(crate) fn apply(
        &self,
        img: &mut oxframe_core::Image,
        to_px: Affine,
        opacity: f32,
        drawn: &[oxframe_core::Image],
    ) {
        let (w, h) = self.size;
        if img.width == 0 || img.height == 0 || w <= 0.0 || h <= 0.0 {
            return;
        }
        let snapshot = std::rc::Rc::new(RasterImage {
            width: img.width,
            height: img.height,
            rgba: img.data.clone(),
            id: 0,
        });
        let mut globals = self.globals.clone();
        for name in &self.frame {
            globals.inputs.push((name.clone(), snapshot.clone()));
        }
        for (input, pixels) in self.offscreen.iter().zip(drawn) {
            globals.inputs.push((
                input.name.clone(),
                std::rc::Rc::new(crop(pixels, to_px, input.bounds)),
            ));
        }
        let node_px = to_px * self.tf;
        let Some(inv) = (node_px.determinant().abs() > 1e-12).then(|| node_px.inverse()) else {
            return;
        };
        let r = node_px.transform_rect_bbox(kurbo::Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0));
        let x0 = r.x0.floor().clamp(0.0, f64::from(img.width)) as usize;
        let y0 = r.y0.floor().clamp(0.0, f64::from(img.height)) as usize;
        let x1 = r.x1.ceil().clamp(0.0, f64::from(img.width)) as usize;
        let y1 = r.y1.ceil().clamp(0.0, f64::from(img.height)) as usize;
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let cols = x1 - x0;
        let total = cols * (y1 - y0);
        let (sw, sh) = self.scene;
        let tf = self.tf;
        let width = img.width as usize;
        let pixel = |i: usize| (x0 + i % cols, y0 + i / cols);
        let local = |i: usize| {
            let (x, y) = pixel(i);
            inv * Point::new(x as f64 + 0.5, y as f64 + 0.5)
        };
        let src = &snapshot.rgba;
        let data = &mut img.data;
        let _ = run_tiles(
            &self.program,
            &globals,
            total,
            |i| {
                let l = local(i);
                let world = tf * l;
                [
                    (l.x + w / 2.0) / w,
                    (l.y + h / 2.0) / h,
                    l.x,
                    l.y,
                    world.x,
                    world.y,
                    world.x + sw / 2.0,
                    world.y + sh / 2.0,
                ]
            },
            |i, [r, g, b, a]| {
                let l = local(i);
                if l.x.abs() > w / 2.0 || l.y.abs() > h / 2.0 {
                    return;
                }
                let (x, y) = pixel(i);
                let o = (y * width + x) * 4;
                let a = (a.clamp(0.0, 1.0) * opacity).clamp(0.0, 1.0);
                let under = f32::from(src[o + 3]) / 255.0;
                let out_a = a + under * (1.0 - a);
                for (c, v) in [r, g, b].into_iter().enumerate() {
                    let below = f32::from(src[o + c]) / 255.0;
                    let mixed = if out_a > 0.0 {
                        (v.clamp(0.0, 1.0) * a + below * under * (1.0 - a)) / out_a
                    } else {
                        0.0
                    };
                    data[o + c] = byte(mixed);
                }
                data[o + 3] = byte(out_a);
            },
        );
    }
}

/// The part of `img` under `bounds` (scene coordinates), at least a pixel.
fn crop(img: &oxframe_core::Image, to_px: Affine, bounds: kurbo::Rect) -> RasterImage {
    let r = to_px.transform_rect_bbox(bounds);
    let clampx = |v: f64| v.clamp(0.0, f64::from(img.width)) as u32;
    let clampy = |v: f64| v.clamp(0.0, f64::from(img.height)) as u32;
    let (x0, y0) = (clampx(r.x0.floor()), clampy(r.y0.floor()));
    let (x1, y1) = (
        clampx(r.x1.ceil()).max(x0 + 1),
        clampy(r.y1.ceil()).max(y0 + 1),
    );
    let (w, h) = (x1 - x0, y1 - y0);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = (x0 + x, y0 + y);
            if sx < img.width && sy < img.height {
                let s = ((sy * img.width + sx) * 4) as usize;
                let d = ((y * w + x) * 4) as usize;
                rgba[d..d + 4].copy_from_slice(&img.data[s..s + 4]);
            }
        }
    }
    RasterImage {
        width: w,
        height: h,
        rgba,
        id: 0,
    }
}

/// True when a custom shader reads the frame, the backdrop or a node.
pub(crate) fn reads_frame_input(n: &NodeData) -> bool {
    matches!(&n.kind, NodeKind::Shader { preset: Preset::Custom, inputs, .. }
        if inputs.iter().any(|(_, i)| matches!(i, crate::post::Input::Frame | crate::post::Input::Backdrop | crate::post::Input::Node(_))))
}

fn wave_uniforms(uniforms: &[Uniform]) -> (f64, f64, Color, Color) {
    let mut amp = 0.05;
    let mut speed = 1.0;
    let mut deep = Color::BLACK;
    let mut foam = Color::WHITE;
    for u in uniforms {
        match (&u.name[..], &u.value) {
            ("amp", UniformValue::Num(v)) => amp = v.get(),
            ("speed", UniformValue::Num(v)) => speed = v.get(),
            ("deep", UniformValue::Color(v)) => deep = v.get(),
            ("foam", UniformValue::Color(v)) => foam = v.get(),
            _ => {}
        }
    }
    (amp, speed, deep, foam)
}

fn load(code: &ShaderCode) -> Result<String, String> {
    match code {
        ShaderCode::Wgsl(text) => Ok(text.clone()),
        ShaderCode::File(path) => read_file(path),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_file(path: &std::path::Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(target_arch = "wasm32")]
fn read_file(path: &std::path::Path) -> Result<String, String> {
    let _ = path;
    Err("a shader file is read by the native renderer".into())
}

fn label(n: &NodeData) -> String {
    n.name
        .clone()
        .unwrap_or_else(|| format!("{}#{}", n.kind.name(), n.id.0))
}

type Compiled = Result<std::rc::Rc<crate::wgsl_cpu::Program>, (&'static str, String)>;

thread_local! {
    /// Parsed and validated shaders by the hash of their text: a file is
    /// read on every draw, but parsed once per change.
    static PROGRAMS: std::cell::RefCell<std::collections::HashMap<u64, Compiled>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The shader for `text`, parsed and validated once.
fn compiled(text: &str, inputs: &[&str]) -> Compiled {
    let key = hash_bytes(format!("{text}\0{}", inputs.join(",")).as_bytes());
    if let Some(hit) = PROGRAMS.with(|p| p.borrow().get(&key).cloned()) {
        return hit;
    }
    let result = compile(text, inputs);
    PROGRAMS.with(|p| {
        let mut p = p.borrow_mut();
        if p.len() > 64 {
            p.clear();
        }
        p.insert(key, result.clone());
    });
    result
}

fn compile(text: &str, inputs: &[&str]) -> Compiled {
    // After the user's text, so naga's line numbers still point into it.
    let samplers: String = inputs
        .iter()
        .map(|name| {
            format!("\nfn sample_{name}(uv: vec2<f32>) -> vec4<f32> {{ return vec4<f32>(0.0); }}\n")
        })
        .collect();
    let full = format!("{PRELUDE}{text}{samplers}{EPILOGUE}");
    let module = naga::front::wgsl::parse_str(&full).map_err(|e| {
        let mut hint = shift_lines(&e.emit_to_string(&full), prelude_lines());
        if let Some(name) = missing_sampler(&hint) {
            hint.push_str(&format!(
                "\nRead an image with `.input(\"{name}\", Input::file(\"…png\"))` on the shader."
            ));
        }
        ("shader_error", hint)
    })?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|e| ("shader_error", shift_lines(&e.to_string(), prelude_lines())))?;
    crate::wgsl_cpu::Program::new(module)
        .map(std::rc::Rc::new)
        .ok_or(("shader_error", "the shader has no `fn shade`".to_string()))
}

fn check_wgsl(
    text: &str,
    uniforms: &[Uniform],
    inputs: &[&str],
) -> Result<(), (&'static str, String)> {
    let program = compiled(text, inputs)?;
    let fields = params_of(program.module()).ok_or((
        "shader_error",
        "the shader has no `struct Params`".to_string(),
    ))?;
    match params(uniforms, &fields) {
        Ok(()) => Ok(()),
        Err(hint) => Err(("param_unknown", hint)),
    }
}

/// The input name in naga's error for a `sample_<name>` that no input
/// declares.
fn missing_sampler(message: &str) -> Option<String> {
    let at = message.find("sample_")?;
    let name: String = message[at + 7..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (message.contains("no definition in scope") && !name.is_empty()).then_some(name)
}

/// A WGSL identifier: a letter or `_`, then letters, digits and `_`.
fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn prelude_lines() -> usize {
    PRELUDE.lines().count()
}

/// naga reports lines in the wrapped source. Subtract the prelude so the
/// number points at the user's text.
fn shift_lines(message: &str, by: usize) -> String {
    let chars: Vec<char> = message.chars().collect();
    let mut out = String::with_capacity(message.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ':' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            let digits: String = chars[start..end].iter().collect();
            let line: usize = digits.parse().unwrap_or(0);
            out.push(':');
            out.push_str(&line.saturating_sub(by).to_string());
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

enum WgslTy {
    F32,
    Vec2,
    Vec4,
    Bool,
    Other,
}

fn params_of(module: &naga::Module) -> Option<Vec<(String, WgslTy)>> {
    for (_, ty) in module.types.iter() {
        if ty.name.as_deref() != Some("Params") {
            continue;
        }
        let naga::TypeInner::Struct { members, .. } = &ty.inner else {
            continue;
        };
        return Some(
            members
                .iter()
                .map(|m| {
                    let name = m.name.clone().unwrap_or_default();
                    (name, wgsl_ty(&module.types[m.ty].inner))
                })
                .collect(),
        );
    }
    None
}

fn wgsl_ty(inner: &naga::TypeInner) -> WgslTy {
    match inner {
        naga::TypeInner::Scalar(s) if s.kind == naga::ScalarKind::Float && s.width == 4 => {
            WgslTy::F32
        }
        naga::TypeInner::Scalar(s) if s.kind == naga::ScalarKind::Bool => WgslTy::Bool,
        naga::TypeInner::Vector { size, scalar }
            if scalar.kind == naga::ScalarKind::Float
                && scalar.width == 4
                && *size == naga::VectorSize::Bi =>
        {
            WgslTy::Vec2
        }
        naga::TypeInner::Vector { size, scalar }
            if scalar.kind == naga::ScalarKind::Float
                && scalar.width == 4
                && *size == naga::VectorSize::Quad =>
        {
            WgslTy::Vec4
        }
        _ => WgslTy::Other,
    }
}

fn params(uniforms: &[Uniform], fields: &[(String, WgslTy)]) -> Result<(), String> {
    let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
    let list = names.join(", ");
    for u in uniforms {
        let Some((_, ty)) = fields.iter().find(|(n, _)| n == &u.name) else {
            let near = closest(&u.name, &names);
            let hint = match near {
                Some(n) => format!(
                    "no parameter `{}`. Did you mean `{n}`? Available: {list}.",
                    u.name
                ),
                None => format!("no parameter `{}`. Available: {list}.", u.name),
            };
            return Err(hint);
        };
        if !type_matches(&u.value, ty) {
            return Err(format!(
                "`{}` is {} in Params, but the scene sets {}.",
                u.name,
                ty_name(ty),
                value_name(&u.value)
            ));
        }
    }
    for (name, _) in fields {
        if !uniforms.iter().any(|u| u.name == *name) {
            return Err(format!(
                "Params.{name} has no value. Set it with `.uniform(\"{name}\", ...)`."
            ));
        }
    }
    Ok(())
}

fn type_matches(value: &UniformValue, ty: &WgslTy) -> bool {
    matches!(
        (value, ty),
        (UniformValue::Num(_), WgslTy::F32)
            | (UniformValue::Vec2(_), WgslTy::Vec2)
            | (UniformValue::Color(_), WgslTy::Vec4)
            | (UniformValue::Bool(_), WgslTy::Bool)
    )
}

fn ty_name(ty: &WgslTy) -> &'static str {
    match ty {
        WgslTy::F32 => "f32",
        WgslTy::Vec2 => "vec2<f32>",
        WgslTy::Vec4 => "vec4<f32>",
        WgslTy::Bool => "bool",
        WgslTy::Other => "another type",
    }
}

fn value_name(value: &UniformValue) -> &'static str {
    match value {
        UniformValue::Num(_) => "a number",
        UniformValue::Vec2(_) => "a point",
        UniformValue::Color(_) => "a color",
        UniformValue::Bool(_) => "a bool",
    }
}

fn closest<'a>(name: &str, options: &[&'a str]) -> Option<&'a str> {
    options
        .iter()
        .copied()
        .filter(|o| edit(name, o) <= 3 && *o != name)
        .min_by_key(|o| edit(name, o))
}

fn edit(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = if (e1 - e0).abs() < 1e-9 {
        if x < e0 { 0.0 } else { 1.0 }
    } else {
        ((x - e0) / (e1 - e0)).clamp(0.0, 1.0)
    };
    t * t * (3.0 - 2.0 * t)
}

fn mix(foam: Color, deep: Color, t: f64, opacity: f64) -> Color {
    let t = t as f32;
    let a = opacity.clamp(0.0, 1.0) as f32;
    Color {
        r: foam.r + (deep.r - foam.r) * t,
        g: foam.g + (deep.g - foam.g) * t,
        b: foam.b + (deep.b - foam.b) * t,
        a: (foam.a + (deep.a - foam.a) * t) * a,
    }
}

fn byte(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::NodeBuilder;
    use crate::{SceneGraph, View};
    use std::rc::Rc;

    fn paint(f: impl FnOnce(View)) -> Rc<SceneGraph> {
        let graph = SceneGraph::new(320, 180);
        graph.enter(|| f(View::current()));
        graph
    }

    fn image_of(graph: &SceneGraph) -> Rc<RasterImage> {
        let items = graph.draw_list();
        for item in &items {
            if let Primitive::Image { image, .. } = &item.prim {
                return image.clone();
            }
        }
        panic!("waves did not paint, diags: {:?}", graph.shader_diags());
    }

    #[test]
    fn waves_put_foam_above_deep_water() {
        let graph = paint(|view| {
            view.add(crate::waves().width(200.0).height(80.0));
        });
        let image = image_of(&graph);
        let px = |x: u32, y: u32| {
            let i = (y * image.width + x) as usize * 4;
            image.rgba[i + 2]
        };
        let mid = image.width / 2;
        assert!(
            px(mid, 2) > px(mid, image.height - 3),
            "the crest is the lighter foam"
        );
        assert!(
            graph.shader_diags().is_empty(),
            "{:?}",
            graph.shader_diags()
        );
    }

    #[test]
    fn custom_wgsl_runs_like_the_hand_written_preset() {
        // The waves preset's own WGSL, run by the interpreter, against the
        // preset's Rust code: the same picture up to f32 rounding.
        let custom = paint(|view| {
            view.add(
                crate::Shader::source(waves_wgsl())
                    .uniform("amp", 0.2)
                    .uniform("speed", 1.0)
                    .uniform_color("deep", Color::rgba(0.1, 0.3, 0.5, 1.0))
                    .uniform_color("foam", Color::rgba(0.9, 0.95, 1.0, 1.0))
                    .width(200.0)
                    .height(80.0),
            );
        });
        let preset = paint(|view| {
            view.add(
                crate::waves()
                    .amp(0.2)
                    .deep(Color::rgba(0.1, 0.3, 0.5, 1.0))
                    .foam(Color::rgba(0.9, 0.95, 1.0, 1.0))
                    .width(200.0)
                    .height(80.0),
            );
        });
        for t in [0.0, 1.3] {
            custom.set_time(t);
            preset.set_time(t);
            let (a, b) = (image_of(&custom), image_of(&preset));
            assert_eq!((a.width, a.height), (b.width, b.height));
            let worst = a
                .rgba
                .iter()
                .zip(&b.rgba)
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap_or(0);
            assert!(worst <= 2, "differs by {worst}/255 at t={t}");
        }
        assert!(
            custom.shader_diags().is_empty(),
            "{:?}",
            custom.shader_diags()
        );
    }

    fn two_pixels() -> crate::Input {
        crate::Input::Image(Rc::new(RasterImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
            id: 9_001,
        }))
    }

    const SAMPLE_PHOTO: &str = "struct Params { k: f32 }
@group(1) @binding(0) var<uniform> p: Params;
fn shade(i: ShadeIn) -> vec4<f32> {
    return sample_photo(i.uv);
}";

    #[test]
    fn a_custom_shade_reads_an_image_input() {
        let graph = paint(|view| {
            view.add(
                crate::Shader::source(SAMPLE_PHOTO)
                    .uniform("k", 0.0)
                    .input("photo", two_pixels())
                    .width(40.0)
                    .height(4.0),
            );
        });
        let img = image_of(&graph);
        let px = |x: u32| {
            let o = ((img.height / 2 * img.width + x) * 4) as usize;
            [img.rgba[o], img.rgba[o + 1], img.rgba[o + 2]]
        };
        assert_eq!(px(0), [255, 0, 0]);
        assert_eq!(px(img.width - 1), [0, 0, 255]);
        let mid = px(img.width / 2);
        assert!(mid[0] > 60 && mid[2] > 60, "bilinear between: {mid:?}");
        assert!(rules(&graph).is_empty(), "{:?}", graph.shader_diags());
    }

    #[test]
    fn a_custom_shade_with_a_frame_input_reads_what_was_drawn() {
        // A mirror over the right half of a 320x180 frame whose left half
        // is red and right half blue: the right half turns red.
        let graph = paint(|view| {
            view.add(
                crate::Shader::source(
                    "struct Params { k: f32 }
@group(1) @binding(0) var<uniform> p: Params;
fn shade(i: ShadeIn) -> vec4<f32> {
    let s = i.screen / ox.resolution;
    return sample_scene(vec2<f32>(1.0 - s.x, s.y));
}",
                )
                .uniform("k", 0.0)
                .input("scene", crate::Input::frame())
                .width(160.0)
                .height(180.0)
                .x(80.0),
            );
        });
        assert!(reads_frame(&graph));
        assert!(rules(&graph).is_empty(), "{:?}", graph.shader_diags());
        let shade = graph
            .draw_list()
            .iter()
            .find_map(|i| match &i.prim {
                Primitive::ReadFrame(s) => Some(s.clone()),
                _ => None,
            })
            .expect("a frame pass");
        assert_eq!(shade.op.name(), "custom");
        let mut img = oxframe_core::Image {
            width: 320,
            height: 180,
            data: (0..320 * 180)
                .flat_map(|i| {
                    if i % 320 < 160 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 255]
                    }
                })
                .collect(),
        };
        let to_px = Affine::translate((160.0, 90.0));
        crate::post::apply(&shade, &mut img, to_px);
        let px = |x: usize| &img.data[(90 * 320 + x) * 4..(90 * 320 + x) * 4 + 3];
        assert_eq!(px(10), [255, 0, 0], "left half is not touched");
        assert_eq!(px(300), [255, 0, 0], "right half mirrors the left");
    }

    #[test]
    fn an_offscreen_node_is_not_drawn_and_check_skips_it() {
        let graph = paint(|view| {
            let card = view.add(crate::Rect::new().size(20.0).name("card").offscreen());
            view.add(
                crate::Shader::source(SAMPLE_PHOTO.replace("photo", "card"))
                    .uniform("k", 0.0)
                    .input("card", crate::Input::node(card))
                    .width(40.0)
                    .height(40.0),
            );
        });
        let list = graph.draw_list();
        assert!(
            list.iter().all(|i| i.prim.is_marker()),
            "only the shader's pass is on screen"
        );
        let shade = list
            .iter()
            .find_map(|i| match &i.prim {
                Primitive::ReadFrame(s) => Some(s),
                _ => None,
            })
            .expect("a node input makes a frame pass");
        assert_eq!(shade.offscreen_lists().len(), 1);
        assert!(!shade.offscreen_lists()[0].is_empty());
        let card = graph
            .inspect_nodes()
            .into_iter()
            .find(|n| n.name == "card")
            .expect("card");
        assert_eq!(card.opacity, 0.0);
        assert!(graph.has_post());
    }

    #[test]
    fn sampling_an_input_that_is_not_declared_says_how_to_add_it() {
        let graph = paint(|view| {
            view.add(
                crate::Shader::source(SAMPLE_PHOTO)
                    .uniform("k", 0.0)
                    .width(4.0)
                    .height(4.0),
            );
        });
        let _ = graph.draw_list();
        let diags = graph.shader_diags();
        let d = diags
            .iter()
            .find(|d| d.rule == "shader_error")
            .expect("shader_error");
        assert!(d.hint.contains(".input(\"photo\""), "{}", d.hint);
    }

    #[test]
    fn an_input_name_must_be_a_wgsl_name() {
        let graph = paint(|view| {
            view.add(
                crate::Shader::source(SAMPLE_PHOTO)
                    .uniform("k", 0.0)
                    .input("my photo", two_pixels())
                    .width(4.0)
                    .height(4.0),
            );
        });
        assert!(rules(&graph).contains(&"param_unknown"));
    }

    #[test]
    fn a_shader_that_fails_while_running_is_a_shader_error() {
        let graph = paint(|view| {
            view.add(
                crate::Shader::source(
                    "struct Params { k: f32 }
@group(1) @binding(0) var<uniform> p: Params;
fn shade(i: ShadeIn) -> vec4<f32> {
    var x = 0.0;
    loop { x += p.k; }
    return vec4<f32>(x);
}",
                )
                .uniform("k", 1.0)
                .width(4.0)
                .height(4.0),
            );
        });
        let items = graph.draw_list();
        assert!(
            !items
                .iter()
                .any(|i| matches!(i.prim, Primitive::Image { .. }))
        );
        let diags = graph.shader_diags();
        assert!(
            diags
                .iter()
                .any(|d| d.rule == "shader_error" && d.hint.contains("loops")),
            "{diags:?}"
        );
    }

    #[test]
    fn the_crest_moves_along_the_wave() {
        let graph = paint(|view| {
            view.add(crate::waves().amp(0.2).width(200.0).height(80.0));
        });
        let a = image_of(&graph).rgba.clone();
        graph.set_time(1.0);
        let b = image_of(&graph).rgba.clone();
        assert_ne!(a, b);
    }

    #[test]
    fn a_misspelled_uniform_names_the_real_one() {
        let graph = paint(|view| {
            view.add(
                crate::Shader::source(waves_wgsl())
                    .uniform("ampp", 0.1)
                    .uniform("speed", 1.0)
                    .uniform_color("deep", Color::BLACK)
                    .uniform_color("foam", Color::WHITE)
                    .width(40.0)
                    .height(20.0)
                    .name("sea"),
            );
        });
        let _ = graph.draw_list();
        let diag = graph
            .shader_diags()
            .into_iter()
            .find(|d| d.rule == "param_unknown")
            .expect("param_unknown");
        assert_eq!(diag.node, "sea");
        assert!(diag.hint.contains("amp"), "{diag:?}");
    }

    #[test]
    fn a_broken_shader_is_an_error() {
        let graph = paint(|view| {
            view.add(
                crate::Shader::source("this is not wgsl")
                    .width(40.0)
                    .height(20.0),
            );
        });
        let _ = graph.draw_list();
        assert!(
            graph
                .shader_diags()
                .iter()
                .any(|d| d.rule == "shader_error"),
            "{:?}",
            graph.shader_diags()
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn editing_the_file_is_seen_on_the_next_draw() {
        let dir = std::env::temp_dir().join(format!("oxframe-shader-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sea.wgsl");
        std::fs::write(&path, "this is not wgsl").unwrap();
        let graph = paint(|view| {
            view.add(crate::Shader::file(&path).width(40.0).height(20.0));
        });
        let _ = graph.draw_list();
        assert!(
            graph
                .shader_diags()
                .iter()
                .any(|d| d.rule == "shader_error"),
            "{:?}",
            graph.shader_diags()
        );
        std::fs::write(&path, waves_wgsl()).unwrap();
        let _ = graph.draw_list();
        let diags = graph.shader_diags();
        assert!(diags.iter().all(|d| d.rule != "shader_error"), "{diags:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn rules(graph: &SceneGraph) -> Vec<&'static str> {
        let _ = graph.draw_list();
        graph.shader_diags().iter().map(|d| d.rule).collect()
    }

    fn reads_frame(graph: &SceneGraph) -> bool {
        graph
            .draw_list()
            .iter()
            .any(|i| matches!(i.prim, Primitive::ReadFrame(_)))
    }

    #[test]
    fn lens_drops_read_the_frame() {
        let graph = paint(|view| {
            view.add(
                crate::lens_drops()
                    .fullscreen()
                    .input("scene", crate::Input::frame()),
            );
        });
        assert!(graph.has_post());
        assert!(reads_frame(&graph));
        assert!(rules(&graph).is_empty(), "{:?}", graph.shader_diags());
        let passes = crate::pass_graph(&graph.draw_list());
        assert!(
            passes
                .iter()
                .any(|p| p.kind == crate::PassKind::Effect && p.shader == Some("lens_drops"))
        );
    }

    #[test]
    fn a_misspelled_input_or_parameter_names_the_real_one() {
        let graph = paint(|view| {
            view.add(
                crate::lens_drops()
                    .input("scen", crate::Input::frame())
                    .name("lens"),
            );
        });
        let _ = graph.draw_list();
        let diag = &graph.shader_diags()[0];
        assert_eq!((diag.rule, diag.node.as_str()), ("param_unknown", "lens"));
        assert!(diag.hint.contains("`scene`"), "{diag:?}");

        let graph = paint(|view| {
            view.add(crate::lens_drops().uniform("wetnes", 1.0));
        });
        let _ = graph.draw_list();
        let diag = &graph.shader_diags()[0];
        assert!(diag.hint.contains("`wetness`"), "{diag:?}");

        let graph = paint(|view| {
            view.add(crate::waves().input("scene", crate::Input::frame()));
        });
        assert_eq!(rules(&graph), ["param_unknown"]);
    }

    #[test]
    fn a_lens_inside_a_fading_group_reads_the_layer() {
        let graph = paint(|view| {
            let group = view.add(crate::Group::new().opacity(0.5));
            group.add(crate::Rect::new().size(20.0));
            group.add(crate::lens_drops().fullscreen());
        });
        assert!(rules(&graph).is_empty(), "{:?}", graph.shader_diags());
        assert!(reads_frame(&graph));
    }

    #[test]
    fn an_effect_inside_an_effect_runs_both() {
        let graph = paint(|view| {
            let outer = view.add(crate::Group::new());
            let inner = outer.add(crate::Rect::new().size(20.0));
            outer.effect(crate::ripple());
            inner.effect(crate::ripple());
        });
        assert!(rules(&graph).is_empty(), "{:?}", graph.shader_diags());
        let items = graph.draw_list();
        let begins = items
            .iter()
            .filter(|i| matches!(i.prim, Primitive::BeginEffect(_)))
            .count();
        assert_eq!(begins, 2);
        let passes = crate::pass_graph(&items);
        assert!(
            passes
                .iter()
                .any(|p| p.kind == crate::PassKind::Effect && p.shader == Some("ripple"))
        );
    }

    #[test]
    fn effect_signals_animate_the_marker() {
        let graph = SceneGraph::new(320, 180);
        let effect = graph.enter(|| {
            View::current()
                .add(crate::Rect::new().size(40.0))
                .effect(crate::ripple().strength(0.0))
        });
        let strength = |graph: &SceneGraph| {
            graph.draw_list().iter().find_map(|i| match &i.prim {
                Primitive::BeginEffect(s) => match s.op {
                    crate::post::ShadeOp::Ripple { strength, .. } => Some(strength),
                    _ => None,
                },
                _ => None,
            })
        };
        assert_eq!(strength(&graph), Some(0.0));
        graph.enter(|| effect.strength().set(0.7));
        assert_eq!(strength(&graph), Some(0.7));
    }

    #[test]
    fn a_slow_raster_is_reported() {
        let graph = paint(|view| {
            view.add(crate::waves().width(180.0).height(80.0).name("sea"));
        });
        set_shader_budget_ms(0.0);
        let _ = graph.draw_list();
        set_shader_budget_ms(250.0);
        assert!(
            graph.shader_diags().iter().any(|d| d.rule == "shader_slow"),
            "{:?}",
            graph.shader_diags()
        );
    }
}

#[cfg(test)]
mod speed {
    use super::*;

    #[test]
    #[ignore = "timing, run by hand in release"]
    fn custom_waves_full_frame() {
        let graph = crate::SceneGraph::new(1920, 1080);
        graph.enter(|| {
            crate::View::current().add(
                crate::Shader::source(waves_wgsl())
                    .uniform("amp", 0.2)
                    .uniform("speed", 1.0)
                    .uniform_color("deep", Color::BLACK)
                    .uniform_color("foam", Color::WHITE)
                    .width(1920.0)
                    .height(1080.0),
            );
        });
        let t = std::time::Instant::now();
        let _ = graph.draw_list();
        eprintln!("custom waves 1920x1080: {:?}", t.elapsed());
        let graph = crate::SceneGraph::new(1920, 1080);
        graph.enter(|| {
            crate::View::current().add(crate::waves().width(1920.0).height(1080.0));
        });
        let t = std::time::Instant::now();
        let _ = graph.draw_list();
        eprintln!("preset waves 1920x1080: {:?}", t.elapsed());
    }
}
