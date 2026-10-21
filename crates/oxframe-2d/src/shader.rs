//! A shader node: WGSL `Params` plus `shade`, checked with naga.
//!
//! The waves preset is rasterized on the CPU, so a check without a GPU
//! still paints it. Both backends then draw that image, which keeps the
//! preset the same on either one. A custom `shade` is type-checked on
//! every draw (a changed file is picked up without a rebuild) and is not
//! executed yet: inputs and shader transitions come later.

use std::cell::Cell;
use std::time::Instant;

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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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
    static DRAFT: Cell<bool> = const { Cell::new(false) };
    static BUDGET_MS: Cell<f64> = const { Cell::new(250.0) };
    static DIAGS: std::cell::RefCell<Vec<ShaderDiag>> = const { std::cell::RefCell::new(Vec::new()) };
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

/// Compile and parameter problems for this node. Empty when it is not a shader.
pub(crate) fn problems(n: &NodeData) -> Vec<ShaderDiag> {
    let NodeKind::Shader { code, uniforms, .. } = &n.kind else {
        return Vec::new();
    };
    let label = label(n);
    let source = n.source.clone();
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
    match check_wgsl(&text, uniforms) {
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
        waves, uniforms, ..
    } = &n.kind
    else {
        return None;
    };
    if !waves {
        return None;
    }
    let (w, h) = (size.0.max(1.0), size.1.max(1.0));
    let scale = if draft() { 0.5 } else { 1.0 };
    let iw = ((w * scale).round() as u32).clamp(1, 1920);
    let ih = ((h * scale).round() as u32).clamp(1, 1080);
    let (amp, speed, deep, foam) = wave_uniforms(uniforms);
    let started = Instant::now();
    let mut rgba = vec![0u8; iw as usize * ih as usize * 4];
    for py in 0..ih {
        for px in 0..iw {
            let lx = (f64::from(px) + 0.5) / f64::from(iw) * w - w / 2.0;
            let ly = (f64::from(py) + 0.5) / f64::from(ih) * h - h / 2.0;
            let world = tf * Point::new(lx, ly);
            let uv_y = (f64::from(py) + 0.5) / f64::from(ih);
            let crest = (world.x * 0.008 + time * speed).sin() * amp;
            let t = smoothstep(crest - 0.02, crest + 0.02, uv_y - 0.5);
            let c = mix(foam, deep, t, opacity);
            let i = (py as usize * iw as usize + px as usize) * 4;
            rgba[i] = byte(c.r);
            rgba[i + 1] = byte(c.g);
            rgba[i + 2] = byte(c.b);
            rgba[i + 3] = byte(c.a);
        }
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0;
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
        .unwrap_or_else(|| format!("Shader#{}", n.id.0))
}

#[cfg(target_arch = "wasm32")]
fn check_wgsl(_text: &str, _uniforms: &[Uniform]) -> Result<(), (&'static str, String)> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn check_wgsl(text: &str, uniforms: &[Uniform]) -> Result<(), (&'static str, String)> {
    let full = format!("{PRELUDE}{text}{EPILOGUE}");
    let module = naga::front::wgsl::parse_str(&full).map_err(|e| {
        (
            "shader_error",
            shift_lines(&e.emit_to_string(&full), prelude_lines()),
        )
    })?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|e| ("shader_error", shift_lines(&e.to_string(), prelude_lines())))?;
    let fields = params_of(&module).ok_or((
        "shader_error",
        "the shader has no `struct Params`".to_string(),
    ))?;
    match params(uniforms, &fields) {
        Ok(()) => Ok(()),
        Err(hint) => Err(("param_unknown", hint)),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn prelude_lines() -> usize {
    PRELUDE.lines().count()
}

/// naga reports lines in the wrapped source. Subtract the prelude so the
/// number points at the user's text.
#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
enum WgslTy {
    F32,
    Vec2,
    Vec4,
    Bool,
    Other,
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn type_matches(value: &UniformValue, ty: &WgslTy) -> bool {
    matches!(
        (value, ty),
        (UniformValue::Num(_), WgslTy::F32)
            | (UniformValue::Vec2(_), WgslTy::Vec2)
            | (UniformValue::Color(_), WgslTy::Vec4)
            | (UniformValue::Bool(_), WgslTy::Bool)
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn ty_name(ty: &WgslTy) -> &'static str {
    match ty {
        WgslTy::F32 => "f32",
        WgslTy::Vec2 => "vec2<f32>",
        WgslTy::Vec4 => "vec4<f32>",
        WgslTy::Bool => "bool",
        WgslTy::Other => "another type",
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn value_name(value: &UniformValue) -> &'static str {
    match value {
        UniformValue::Num(_) => "a number",
        UniformValue::Vec2(_) => "a point",
        UniformValue::Color(_) => "a color",
        UniformValue::Bool(_) => "a bool",
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn closest<'a>(name: &str, options: &[&'a str]) -> Option<&'a str> {
    options
        .iter()
        .copied()
        .filter(|o| edit(name, o) <= 3 && *o != name)
        .min_by_key(|o| edit(name, o))
}

#[cfg(not(target_arch = "wasm32"))]
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
