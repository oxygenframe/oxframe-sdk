//! Runs a shader's `shade` function on the CPU, a tile of pixels at a time.
//!
//! The WGSL is parsed and validated by naga; this module walks naga's IR.
//! Float math is IEEE single precision and the transcendental functions
//! come from `libm`, not the platform, so a frame is the same bytes on
//! every operating system, like the rest of the CPU renderer.
//!
//! What a fragment shader cannot do here: textures and samplers (an input
//! is read with a `sample_<name>(uv)` function instead), derivatives,
//! atomics, barriers, subgroups. Using one is a `shader_error` naming it.

// Vector math indexes components of several arrays at once.
#![allow(clippy::needless_range_loop)]

use std::rc::Rc;

use crate::image::RasterImage;
use naga::{
    BinaryOperator, Expression, Function, Handle, Literal, MathFunction, Module, ScalarKind,
    Statement, TypeInner, UnaryOperator,
};

/// Iterations of all loops together, per pixel, before the shader is
/// stopped with an error: a loop that never ends must not hang a render.
const MAX_STEPS: u32 = 1 << 20;

/// A value: a scalar or vector (the count is the first field), or a
/// struct, array or matrix (columns) as a list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    F(u8, [f32; 4]),
    I(u8, [i32; 4]),
    U(u8, [u32; 4]),
    B(u8, [bool; 4]),
    C(Vec<Value>),
    P(Ptr),
}

/// A place a value is stored: a variable and the path into it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ptr {
    root: Root,
    path: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Root {
    Local(usize),
    Global(usize),
}

impl Value {
    /// A float scalar.
    pub(crate) fn f(v: f32) -> Self {
        Self::F(1, [v, 0.0, 0.0, 0.0])
    }

    /// A float vector of 2 to 4 components.
    pub(crate) fn fv(v: &[f32]) -> Self {
        let mut a = [0.0; 4];
        a[..v.len()].copy_from_slice(v);
        Self::F(v.len() as u8, a)
    }

    fn len(&self) -> usize {
        match self {
            Self::F(n, _) | Self::I(n, _) | Self::U(n, _) | Self::B(n, _) => usize::from(*n),
            Self::C(v) => v.len(),
            Self::P(_) => 1,
        }
    }

    fn component(&self, i: usize) -> Result<Value, String> {
        let bad = || format!("index {i} is out of range");
        Ok(match self {
            Self::F(n, a) if i < usize::from(*n) => Self::F(1, [a[i], 0.0, 0.0, 0.0]),
            Self::I(n, a) if i < usize::from(*n) => Self::I(1, [a[i], 0, 0, 0]),
            Self::U(n, a) if i < usize::from(*n) => Self::U(1, [a[i], 0, 0, 0]),
            Self::B(n, a) if i < usize::from(*n) => Self::B(1, [a[i], false, false, false]),
            Self::C(v) => v.get(i).cloned().ok_or_else(bad)?,
            _ => return Err(bad()),
        })
    }

    fn set_component(&mut self, i: usize, value: Value) -> Result<(), String> {
        let bad = || format!("index {i} is out of range");
        match (self, value) {
            (Self::F(n, a), Self::F(1, b)) if i < usize::from(*n) => a[i] = b[0],
            (Self::I(n, a), Self::I(1, b)) if i < usize::from(*n) => a[i] = b[0],
            (Self::U(n, a), Self::U(1, b)) if i < usize::from(*n) => a[i] = b[0],
            (Self::B(n, a), Self::B(1, b)) if i < usize::from(*n) => a[i] = b[0],
            (Self::C(v), value) => *v.get_mut(i).ok_or_else(bad)? = value,
            _ => return Err(bad()),
        }
        Ok(())
    }

    fn as_bool(&self) -> Result<bool, String> {
        match self {
            Self::B(1, a) => Ok(a[0]),
            _ => Err("expected a bool".into()),
        }
    }

    fn as_index(&self) -> Result<usize, String> {
        match self {
            Self::I(1, a) => Ok(a[0].max(0) as usize),
            Self::U(1, a) => Ok(a[0] as usize),
            _ => Err("an index must be an integer".into()),
        }
    }

    fn as_switch(&self) -> Result<i64, String> {
        match self {
            Self::I(1, a) => Ok(i64::from(a[0])),
            Self::U(1, a) => Ok(i64::from(a[0])),
            _ => Err("a switch needs an integer".into()),
        }
    }

    /// Float components, for the result of `shade`.
    pub(crate) fn floats(&self) -> Option<[f32; 4]> {
        match self {
            Self::F(_, a) => Some(*a),
            _ => None,
        }
    }
}

/// Pixels run together: every expression is computed for a tile of this
/// many pixels at once, and a value that is the same for all of them (a
/// parameter, `ox.time`, a constant) is computed once.
pub(crate) const LANES: usize = 64;

/// One bit per lane: which pixels a statement runs for.
type Mask = u64;

type Lanes<T> = [T; LANES];

/// A value across the lanes of a tile.
#[derive(Clone, Debug)]
pub(crate) enum V {
    /// The same in every lane (also every pointer).
    Uni(Value),
    /// Float scalar or vector: one array per component.
    F(Vec<Lanes<f32>>),
    I(Vec<Lanes<i32>>),
    U(Vec<Lanes<u32>>),
    B(Vec<Lanes<bool>>),
    /// A struct, array or matrix with a part that differs per lane.
    C(Vec<V>),
}

impl V {
    /// The value in lane `k`.
    pub(crate) fn lane(&self, k: usize) -> Value {
        fn pick<T: Copy + Default>(c: &[Lanes<T>], k: usize) -> (u8, [T; 4]) {
            let mut a = [T::default(); 4];
            for (i, comp) in c.iter().enumerate() {
                a[i] = comp[k];
            }
            (c.len() as u8, a)
        }
        match self {
            Self::Uni(v) => v.clone(),
            Self::F(c) => {
                let (n, a) = pick(c, k);
                Value::F(n, a)
            }
            Self::I(c) => {
                let (n, a) = pick(c, k);
                Value::I(n, a)
            }
            Self::U(c) => {
                let (n, a) = pick(c, k);
                Value::U(n, a)
            }
            Self::B(c) => {
                let (n, a) = pick(c, k);
                Value::B(n, a)
            }
            Self::C(items) => Value::C(items.iter().map(|v| v.lane(k)).collect()),
        }
    }

    /// Float components across lanes, for a float scalar or vector.
    fn floats(&self) -> Option<Vec<Lanes<f32>>> {
        match self {
            Self::F(c) => Some(c.clone()),
            Self::Uni(Value::F(n, a)) => {
                Some((0..usize::from(*n)).map(|i| [a[i]; LANES]).collect())
            }
            _ => None,
        }
    }

    fn is_uniform(&self) -> bool {
        matches!(self, Self::Uni(_))
    }
}

/// A varying value from one value per lane, all of one shape.
fn from_lanes(vals: &[Value]) -> V {
    fn gather<T: Copy + Default>(
        vals: &[Value],
        n: u8,
        get: impl Fn(&Value) -> [T; 4],
    ) -> Vec<Lanes<T>> {
        let mut out = vec![[T::default(); LANES]; usize::from(n)];
        for (k, v) in vals.iter().enumerate().take(LANES) {
            let a = get(v);
            for (i, comp) in out.iter_mut().enumerate() {
                comp[k] = a[i];
            }
        }
        out
    }
    if vals.iter().all(|v| v == &vals[0]) {
        return V::Uni(vals[0].clone());
    }
    match &vals[0] {
        Value::F(n, _) => V::F(gather(vals, *n, |v| match v {
            Value::F(_, a) => *a,
            _ => [0.0; 4],
        })),
        Value::I(n, _) => V::I(gather(vals, *n, |v| match v {
            Value::I(_, a) => *a,
            _ => [0; 4],
        })),
        Value::U(n, _) => V::U(gather(vals, *n, |v| match v {
            Value::U(_, a) => *a,
            _ => [0; 4],
        })),
        Value::B(n, _) => V::B(gather(vals, *n, |v| match v {
            Value::B(_, a) => *a,
            _ => [false; 4],
        })),
        Value::C(items) => V::C(
            (0..items.len())
                .map(|i| {
                    let parts: Vec<Value> = vals
                        .iter()
                        .map(|v| match v {
                            Value::C(x) => x.get(i).cloned().unwrap_or(Value::f(0.0)),
                            other => other.clone(),
                        })
                        .collect();
                    from_lanes(&parts)
                })
                .collect(),
        ),
        Value::P(_) => V::Uni(vals[0].clone()),
    }
}

/// Runs `f` on the scalar values of every active lane (the others copy the
/// first active one), or once when every argument is uniform.
fn per_lane(
    active: Mask,
    args: &[&V],
    f: impl Fn(Vec<Value>) -> Result<Value, String>,
) -> Result<V, String> {
    if args.iter().all(|a| a.is_uniform()) {
        return Ok(V::Uni(f(args.iter().map(|a| a.lane(0)).collect())?));
    }
    let first = if active == 0 {
        0
    } else {
        active.trailing_zeros() as usize
    };
    let base = f(args.iter().map(|a| a.lane(first)).collect())?;
    let mut out = Vec::with_capacity(LANES);
    for k in 0..LANES {
        if k != first && active & (1 << k) != 0 {
            out.push(f(args.iter().map(|a| a.lane(k)).collect())?);
        } else {
            out.push(base.clone());
        }
    }
    Ok(from_lanes(&out))
}

/// `new` in the lanes of `mask`, `old` in the rest.
fn merge(old: V, new: V, mask: Mask, full: Mask) -> V {
    if mask & full == full {
        return new;
    }
    if mask & full == 0 {
        return old;
    }
    if let (V::Uni(a), V::Uni(b)) = (&old, &new)
        && a == b
    {
        return new;
    }
    if let (Some(a), Some(b)) = (old.floats(), new.floats())
        && a.len() == b.len()
    {
        let mut out = a;
        for (o, n) in out.iter_mut().zip(&b) {
            for k in 0..LANES {
                if mask & (1 << k) != 0 {
                    o[k] = n[k];
                }
            }
        }
        return V::F(out);
    }
    let vals: Vec<Value> = (0..LANES)
        .map(|k| {
            if mask & (1 << k) != 0 {
                new.lane(k)
            } else {
                old.lane(k)
            }
        })
        .collect();
    from_lanes(&vals)
}

fn zip2(a: Vec<Lanes<f32>>, b: Vec<Lanes<f32>>, f: impl Fn(f32, f32) -> f32) -> Option<V> {
    let n = a.len().max(b.len());
    if (a.len() != n && a.len() != 1) || (b.len() != n && b.len() != 1) {
        return None;
    }
    let mut out = vec![[0.0f32; LANES]; n];
    for (i, o) in out.iter_mut().enumerate() {
        let (x, y) = (&a[i.min(a.len() - 1)], &b[i.min(b.len() - 1)]);
        for k in 0..LANES {
            o[k] = f(x[k], y[k]);
        }
    }
    Some(V::F(out))
}

fn zip3(
    a: Vec<Lanes<f32>>,
    b: Vec<Lanes<f32>>,
    c: Vec<Lanes<f32>>,
    f: impl Fn(f32, f32, f32) -> f32,
) -> Option<V> {
    let n = a.len().max(b.len()).max(c.len());
    if [a.len(), b.len(), c.len()]
        .iter()
        .any(|&l| l != n && l != 1)
    {
        return None;
    }
    let mut out = vec![[0.0f32; LANES]; n];
    for (i, o) in out.iter_mut().enumerate() {
        let x = &a[i.min(a.len() - 1)];
        let y = &b[i.min(b.len() - 1)];
        let z = &c[i.min(c.len() - 1)];
        for k in 0..LANES {
            o[k] = f(x[k], y[k], z[k]);
        }
    }
    Some(V::F(out))
}

fn map1(a: Vec<Lanes<f32>>, f: impl Fn(f32) -> f32) -> V {
    V::F(
        a.into_iter()
            .map(|c| {
                let mut o = c;
                for x in &mut o {
                    *x = f(*x);
                }
                o
            })
            .collect(),
    )
}

fn binary_v(op: BinaryOperator, a: V, b: V, active: Mask) -> Result<V, String> {
    use BinaryOperator as B;
    if let (V::Uni(x), V::Uni(y)) = (&a, &b) {
        return Ok(V::Uni(binary(op, x.clone(), y.clone())?));
    }
    if let (Some(x), Some(y)) = (a.floats(), b.floats()) {
        let arith: Option<fn(f32, f32) -> f32> = match op {
            B::Add => Some(|p, q| p + q),
            B::Subtract => Some(|p, q| p - q),
            B::Multiply => Some(|p, q| p * q),
            B::Divide => Some(|p, q| p / q),
            B::Modulo => Some(|p, q| p - q * (p / q).trunc()),
            _ => None,
        };
        if let Some(f) = arith
            && let Some(v) = zip2(x.clone(), y.clone(), f)
        {
            return Ok(v);
        }
        let cmp: Option<fn(f32, f32) -> bool> = match op {
            B::Less => Some(|p, q| p < q),
            B::LessEqual => Some(|p, q| p <= q),
            B::Greater => Some(|p, q| p > q),
            B::GreaterEqual => Some(|p, q| p >= q),
            B::Equal => Some(|p, q| p == q),
            B::NotEqual => Some(|p, q| p != q),
            _ => None,
        };
        if let Some(f) = cmp
            && x.len() == y.len()
        {
            let out = x
                .iter()
                .zip(&y)
                .map(|(p, q)| {
                    let mut o = [false; LANES];
                    for k in 0..LANES {
                        o[k] = f(p[k], q[k]);
                    }
                    o
                })
                .collect();
            return Ok(V::B(out));
        }
    }
    if let (V::B(x), V::B(y)) = (&a, &b)
        && x.len() == y.len()
        && matches!(op, B::LogicalAnd | B::LogicalOr | B::And | B::InclusiveOr)
    {
        let and = matches!(op, B::LogicalAnd | B::And);
        let out = x
            .iter()
            .zip(y)
            .map(|(p, q)| {
                let mut o = [false; LANES];
                for k in 0..LANES {
                    o[k] = if and { p[k] && q[k] } else { p[k] || q[k] };
                }
                o
            })
            .collect();
        return Ok(V::B(out));
    }
    per_lane(active, &[&a, &b], |v| {
        let mut v = v.into_iter();
        binary(
            op,
            v.next().unwrap_or(Value::f(0.0)),
            v.next().unwrap_or(Value::f(0.0)),
        )
    })
}

fn math_v(fun: MathFunction, a: V, b: Option<V>, c: Option<V>, active: Mask) -> Result<V, String> {
    use MathFunction as M;
    let all_uniform = a.is_uniform()
        && b.as_ref().is_none_or(V::is_uniform)
        && c.as_ref().is_none_or(V::is_uniform);
    if all_uniform {
        return Ok(V::Uni(math(
            fun,
            a.lane(0),
            b.map(|v| v.lane(0)),
            c.map(|v| v.lane(0)),
        )?));
    }
    let fa = a.floats();
    let fb = b.as_ref().and_then(V::floats);
    let fc = c.as_ref().and_then(V::floats);
    let unary: Option<fn(f32) -> f32> = match fun {
        M::Sin => Some(libm::sinf),
        M::Cos => Some(libm::cosf),
        M::Tan => Some(libm::tanf),
        M::Exp => Some(libm::expf),
        M::Exp2 => Some(libm::exp2f),
        M::Log => Some(libm::logf),
        M::Log2 => Some(libm::log2f),
        M::Sqrt => Some(f32::sqrt),
        M::InverseSqrt => Some(|x| 1.0 / x.sqrt()),
        M::Abs => Some(f32::abs),
        M::Floor => Some(f32::floor),
        M::Ceil => Some(f32::ceil),
        M::Fract => Some(|x| x - x.floor()),
        M::Trunc => Some(f32::trunc),
        M::Round => Some(round_even),
        M::Saturate => Some(|x| x.clamp(0.0, 1.0)),
        M::Atan => Some(libm::atanf),
        M::Asin => Some(libm::asinf),
        M::Acos => Some(libm::acosf),
        M::Tanh => Some(libm::tanhf),
        _ => None,
    };
    if let (Some(f), Some(x), None) = (unary, fa.clone(), b.as_ref()) {
        return Ok(map1(x, f));
    }
    let binary2: Option<fn(f32, f32) -> f32> = match fun {
        M::Min => Some(f32::min),
        M::Max => Some(f32::max),
        M::Pow => Some(libm::powf),
        M::Atan2 => Some(libm::atan2f),
        M::Step => Some(|edge, x| if x < edge { 0.0 } else { 1.0 }),
        _ => None,
    };
    if let (Some(f), Some(x), Some(y)) = (binary2, fa.clone(), fb.clone())
        && let Some(v) = zip2(x, y, f)
    {
        return Ok(v);
    }
    let ternary: Option<fn(f32, f32, f32) -> f32> = match fun {
        M::Mix => Some(|x, y, t| x * (1.0 - t) + y * t),
        M::SmoothStep => Some(smoothstep),
        M::Clamp => Some(|x, l, h| x.max(l).min(h)),
        M::Fma => Some(|x, y, z| x * y + z),
        _ => None,
    };
    if let (Some(f), Some(x), Some(y), Some(z)) = (ternary, fa.clone(), fb.clone(), fc)
        && let Some(v) = zip3(x, y, z, f)
    {
        return Ok(v);
    }
    match (fun, fa, fb) {
        (M::Dot, Some(x), Some(y)) if x.len() == y.len() => {
            let mut o = [0.0f32; LANES];
            for (p, q) in x.iter().zip(&y) {
                for k in 0..LANES {
                    o[k] += p[k] * q[k];
                }
            }
            return Ok(V::F(vec![o]));
        }
        (M::Length, Some(x), None) => {
            let mut o = [0.0f32; LANES];
            for p in &x {
                for k in 0..LANES {
                    o[k] += p[k] * p[k];
                }
            }
            return Ok(map1(vec![o], f32::sqrt));
        }
        (M::Distance, Some(x), Some(y)) if x.len() == y.len() => {
            let mut o = [0.0f32; LANES];
            for (p, q) in x.iter().zip(&y) {
                for k in 0..LANES {
                    let d = p[k] - q[k];
                    o[k] += d * d;
                }
            }
            return Ok(map1(vec![o], f32::sqrt));
        }
        (M::Normalize, Some(x), None) => {
            let mut l = [0.0f32; LANES];
            for p in &x {
                for k in 0..LANES {
                    l[k] += p[k] * p[k];
                }
            }
            let out = x
                .into_iter()
                .map(|mut p| {
                    for k in 0..LANES {
                        p[k] /= l[k].sqrt();
                    }
                    p
                })
                .collect();
            return Ok(V::F(out));
        }
        _ => {}
    }
    let args: Vec<&V> = [Some(&a), b.as_ref(), c.as_ref()]
        .into_iter()
        .flatten()
        .collect();
    per_lane(active, &args, |v| {
        let mut v = v.into_iter();
        let a = v.next().unwrap_or(Value::f(0.0));
        math(fun, a, v.next(), v.next())
    })
}

fn select_v(c: V, accept: V, reject: V, active: Mask) -> Result<V, String> {
    if let V::Uni(Value::B(1, x)) = &c {
        return Ok(if x[0] { accept } else { reject });
    }
    if let V::B(m) = &c
        && m.len() == 1
    {
        let mut mask: Mask = 0;
        for k in 0..LANES {
            if m[0][k] {
                mask |= 1 << k;
            }
        }
        return Ok(merge(reject, accept, mask, Mask::MAX));
    }
    per_lane(active, &[&c, &accept, &reject], |v| {
        let mut v = v.into_iter();
        let (c, a, r) = (
            v.next().unwrap_or(Value::f(0.0)),
            v.next().unwrap_or(Value::f(0.0)),
            v.next().unwrap_or(Value::f(0.0)),
        );
        select(c, a, r)
    })
}

/// Component or member `i` of a value (or a pointer to it).
fn access_v(base: V, i: usize) -> Result<V, String> {
    Ok(match base {
        V::Uni(v) => V::Uni(access(v, i)?),
        V::C(items) => items.into_iter().nth(i).ok_or("index out of range")?,
        V::F(c) => V::F(vec![*c.get(i).ok_or("index out of range")?]),
        V::I(c) => V::I(vec![*c.get(i).ok_or("index out of range")?]),
        V::U(c) => V::U(vec![*c.get(i).ok_or("index out of range")?]),
        V::B(c) => V::B(vec![*c.get(i).ok_or("index out of range")?]),
    })
}

fn swizzle_v(v: V, pattern: &[naga::SwizzleComponent]) -> Result<V, String> {
    let pick = |len: usize| -> Result<Vec<usize>, String> {
        pattern
            .iter()
            .map(|c| {
                let i = *c as usize;
                if i < len {
                    Ok(i)
                } else {
                    Err("swizzle out of range".to_string())
                }
            })
            .collect()
    };
    Ok(match v {
        V::Uni(x) => {
            let parts = pattern
                .iter()
                .map(|c| x.component(*c as usize))
                .collect::<Result<Vec<_>, _>>()?;
            V::Uni(vector_of(&parts)?)
        }
        V::F(c) => V::F(pick(c.len())?.into_iter().map(|i| c[i]).collect()),
        V::I(c) => V::I(pick(c.len())?.into_iter().map(|i| c[i]).collect()),
        V::U(c) => V::U(pick(c.len())?.into_iter().map(|i| c[i]).collect()),
        V::B(c) => V::B(pick(c.len())?.into_iter().map(|i| c[i]).collect()),
        V::C(_) => return Err("swizzle of a value that is not a vector".into()),
    })
}

fn splat_v(v: V, n: usize) -> Result<V, String> {
    Ok(match v {
        V::Uni(x) => V::Uni(splat(x, n)?),
        V::F(c) if c.len() == 1 => V::F(vec![c[0]; n]),
        V::I(c) if c.len() == 1 => V::I(vec![c[0]; n]),
        V::U(c) if c.len() == 1 => V::U(vec![c[0]; n]),
        V::B(c) if c.len() == 1 => V::B(vec![c[0]; n]),
        _ => return Err("splat needs a scalar".into()),
    })
}

fn compose_v(
    module: &Module,
    ty: Handle<naga::Type>,
    parts: Vec<V>,
    active: Mask,
) -> Result<V, String> {
    if parts.iter().all(V::is_uniform) {
        return Ok(V::Uni(compose(
            module,
            ty,
            parts.iter().map(|p| p.lane(0)).collect(),
        )?));
    }
    match &module.types[ty].inner {
        TypeInner::Vector { scalar, .. } if scalar.kind == ScalarKind::Float => {
            let mut comps = Vec::with_capacity(4);
            for p in &parts {
                comps.extend(p.floats().ok_or("a vector mixes number kinds")?);
            }
            Ok(V::F(comps))
        }
        TypeInner::Vector { .. } | TypeInner::Scalar(_) => {
            let refs: Vec<&V> = parts.iter().collect();
            per_lane(active, &refs, |v| vector_of(&v))
        }
        _ => Ok(V::C(parts)),
    }
}

fn unary_v(op: UnaryOperator, v: V, active: Mask) -> Result<V, String> {
    match (op, v) {
        (_, V::Uni(x)) => Ok(V::Uni(unary(op, x)?)),
        (UnaryOperator::Negate, V::F(c)) => Ok(map1(c, |x| -x)),
        (UnaryOperator::LogicalNot, V::B(c)) => Ok(V::B(
            c.into_iter()
                .map(|mut l| {
                    for x in &mut l {
                        *x = !*x;
                    }
                    l
                })
                .collect(),
        )),
        (op, v) => per_lane(active, &[&v], |a| {
            unary(op, a.into_iter().next().unwrap_or(Value::f(0.0)))
        }),
    }
}

fn cast_v(v: V, kind: ScalarKind, convert: bool, active: Mask) -> Result<V, String> {
    match (&v, kind, convert) {
        (V::Uni(x), _, _) => Ok(V::Uni(cast(x.clone(), kind, convert)?)),
        (V::F(_), ScalarKind::Float, true) => Ok(v),
        (V::I(c), ScalarKind::Float, true) => {
            Ok(V::F(c.iter().map(|l| l.map(|x| x as f32)).collect()))
        }
        (V::U(c), ScalarKind::Float, true) => {
            Ok(V::F(c.iter().map(|l| l.map(|x| x as f32)).collect()))
        }
        (V::F(c), ScalarKind::Sint, true) => {
            Ok(V::I(c.iter().map(|l| l.map(|x| x as i32)).collect()))
        }
        (V::F(c), ScalarKind::Uint, true) => {
            Ok(V::U(c.iter().map(|l| l.map(|x| x as u32)).collect()))
        }
        _ => per_lane(active, &[&v], |a| {
            cast(a.into_iter().next().unwrap_or(Value::f(0.0)), kind, convert)
        }),
    }
}

/// A compiled shader, ready to run on tiles of pixels.
pub(crate) struct Program {
    module: Module,
    shade: Handle<Function>,
    /// Global variables by index: `ox`, `p`.
    globals: Vec<Option<&'static str>>,
    /// `sample_<name>` functions and the input name each one reads.
    samplers: Vec<(Handle<Function>, String)>,
}

/// What [`Program::runner`] reads besides the pixels: the `ox` struct, the
/// `Params` fields in declaration order, and the images behind
/// `sample_<name>`.
#[derive(Clone)]
pub(crate) struct Globals {
    pub(crate) ox: Value,
    pub(crate) params: Value,
    pub(crate) inputs: Vec<(String, Rc<RasterImage>)>,
}

/// Bilinear sample of `img` at `uv` (0 to 1 across the image, clamped at
/// the edges) in every lane, as straight-alpha sRGB from 0 to 1.
fn sample_v(img: &RasterImage, uv: &V) -> V {
    let (w, h) = (img.width.max(1) as usize, img.height.max(1) as usize);
    let at = |x: usize, y: usize, c: usize| -> f32 {
        img.rgba
            .get((y * w + x) * 4 + c)
            .map_or(0.0, |b| f32::from(*b) / 255.0)
    };
    let px = |u: f32, v: f32| -> [f32; 4] {
        let x = (u * w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
        let y = (v * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let mut out = [0.0; 4];
        for (c, o) in out.iter_mut().enumerate() {
            let top = at(x0, y0, c) * (1.0 - fx) + at(x1, y0, c) * fx;
            let bottom = at(x0, y1, c) * (1.0 - fx) + at(x1, y1, c) * fx;
            *o = top * (1.0 - fy) + bottom * fy;
        }
        out
    };
    if let V::Uni(Value::F(_, a)) = uv {
        return V::Uni(Value::fv(&px(a[0], a[1])));
    }
    let c = uv.floats().unwrap_or_default();
    let mut out = vec![[0.0f32; LANES]; 4];
    if c.len() >= 2 {
        for k in 0..LANES {
            let p = px(c[0][k], c[1][k]);
            for (o, v) in out.iter_mut().zip(p) {
                o[k] = v;
            }
        }
    }
    V::F(out)
}

impl Program {
    /// Compiles a validated module; `None` when it has no `shade` function.
    pub(crate) fn new(module: Module) -> Option<Self> {
        let shade = module
            .functions
            .iter()
            .find(|(_, f)| f.name.as_deref() == Some("shade"))
            .map(|(h, _)| h)?;
        let globals = module
            .global_variables
            .iter()
            .map(|(_, g)| match g.name.as_deref() {
                Some("ox") => Some("ox"),
                Some("p") => Some("p"),
                _ => None,
            })
            .collect();
        let samplers = module
            .functions
            .iter()
            .filter_map(|(h, f)| {
                let input = f.name.as_deref()?.strip_prefix("sample_")?;
                Some((h, input.to_string()))
            })
            .collect();
        Some(Self {
            module,
            shade,
            globals,
            samplers,
        })
    }

    /// The parsed module.
    pub(crate) fn module(&self) -> &Module {
        &self.module
    }

    /// Names of the fields of `struct Params`, in order.
    pub(crate) fn params(&self) -> Vec<String> {
        self.module
            .types
            .iter()
            .find(|(_, t)| t.name.as_deref() == Some("Params"))
            .and_then(|(_, t)| match &t.inner {
                TypeInner::Struct { members, .. } => Some(
                    members
                        .iter()
                        .map(|m| m.name.clone().unwrap_or_default())
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Runs `shade` for one pixel. `input` is the `ShadeIn` struct.
    #[cfg(test)]
    pub(crate) fn run(&self, globals: &Globals, input: Value) -> Result<Value, String> {
        let out = self.runner(globals).run(&V::Uni(input), 1)?;
        Ok(out.lane(0))
    }

    /// A runner for many tiles with the same globals.
    pub(crate) fn runner(&self, globals: &Globals) -> Runner<'_> {
        let module = &self.module;
        let store: Vec<V> = module
            .global_variables
            .iter()
            .zip(&self.globals)
            .map(|((_, g), name)| {
                V::Uni(match name {
                    Some("ox") => globals.ox.clone(),
                    Some("p") => globals.params.clone(),
                    _ => match g.init {
                        Some(init) => {
                            const_eval(module, init).unwrap_or_else(|_| zero(module, g.ty))
                        }
                        None => zero(module, g.ty),
                    },
                })
            })
            .collect();
        let private = module
            .global_variables
            .iter()
            .enumerate()
            .filter(|(_, (_, g))| g.space == naga::AddressSpace::Private)
            .map(|(i, _)| i)
            .collect();
        let textures = self
            .samplers
            .iter()
            .filter_map(|(h, name)| {
                let (_, img) = globals.inputs.iter().find(|(n, _)| n == name)?;
                Some((*h, img.clone()))
            })
            .collect();
        Runner {
            program: self,
            initial: store.clone(),
            store,
            private,
            textures,
        }
    }
}

/// Runs a program tile after tile; see [`Program::runner`].
pub(crate) struct Runner<'p> {
    program: &'p Program,
    store: Vec<V>,
    initial: Vec<V>,
    /// Indices of `var<private>` globals, reset before every tile.
    private: Vec<usize>,
    /// The image behind each `sample_<name>` function that has one.
    textures: Vec<(Handle<Function>, Rc<RasterImage>)>,
}

impl Runner<'_> {
    /// Runs `shade` for the first `count` lanes of `input` (the `ShadeIn`
    /// struct across a tile) and returns the result across the tile.
    pub(crate) fn run(&mut self, input: &V, count: usize) -> Result<V, String> {
        for &i in &self.private {
            self.store[i] = self.initial[i].clone();
        }
        let full: Mask = if count >= LANES {
            Mask::MAX
        } else {
            (1 << count) - 1
        };
        let mut steps = 0;
        let mut m = Machine {
            module: &self.program.module,
            globals: &mut self.store,
            steps: &mut steps,
            full,
            textures: &self.textures,
        };
        m.call(self.program.shade, vec![input.clone()], full)?
            .ok_or_else(|| "`shade` returned nothing".to_string())
    }
}

struct Machine<'a> {
    module: &'a Module,
    globals: &'a mut Vec<V>,
    steps: &'a mut u32,
    /// Lanes that hold pixels.
    full: Mask,
    textures: &'a [(Handle<Function>, Rc<RasterImage>)],
}

/// One function call across the tile.
struct Frame<'f> {
    fun: &'f Function,
    args: Vec<V>,
    locals: Vec<V>,
    vals: Vec<Option<V>>,
    ret: Option<V>,
    /// Lanes that returned, broke out of the innermost loop or switch, or
    /// continued it.
    returned: Mask,
    brk: Mask,
    cont: Mask,
}

impl Machine<'_> {
    fn call(
        &mut self,
        f: Handle<Function>,
        args: Vec<V>,
        active: Mask,
    ) -> Result<Option<V>, String> {
        if let Some((_, img)) = self.textures.iter().find(|(h, _)| *h == f) {
            let uv = args.first().ok_or("`sample_` takes the uv")?;
            return Ok(Some(sample_v(img, uv)));
        }
        let module = self.module;
        let fun = &module.functions[f];
        let mut frame = Frame {
            fun,
            args,
            locals: Vec::with_capacity(fun.local_variables.len()),
            vals: vec![None; fun.expressions.len()],
            ret: None,
            returned: 0,
            brk: 0,
            cont: 0,
        };
        for (_, local) in fun.local_variables.iter() {
            let v = match local.init {
                Some(init) => self.eval(&mut frame, init, active)?,
                None => V::Uni(zero(module, local.ty)),
            };
            frame.locals.push(v);
        }
        self.block(&mut frame, &fun.body, active)?;
        Ok(frame.ret)
    }

    fn block(
        &mut self,
        frame: &mut Frame,
        block: &naga::Block,
        active: Mask,
    ) -> Result<(), String> {
        let mut act = active;
        for stmt in block.iter() {
            act &= !(frame.returned | frame.brk | frame.cont);
            if act & self.full == 0 {
                break;
            }
            self.statement(frame, stmt, act)?;
        }
        Ok(())
    }

    fn mask_of(&self, v: &V) -> Result<Mask, String> {
        match v {
            V::Uni(x) => Ok(if x.as_bool()? { Mask::MAX } else { 0 }),
            V::B(c) if c.len() == 1 => {
                let mut m: Mask = 0;
                for k in 0..LANES {
                    if c[0][k] {
                        m |= 1 << k;
                    }
                }
                Ok(m)
            }
            _ => Err("expected a bool".into()),
        }
    }

    fn give_back(&mut self, frame: &mut Frame, v: V, act: Mask) {
        frame.ret = Some(match frame.ret.take() {
            None => v,
            Some(old) => merge(old, v, act, self.full),
        });
        frame.returned |= act;
    }

    fn statement(&mut self, frame: &mut Frame, stmt: &Statement, act: Mask) -> Result<(), String> {
        match stmt {
            Statement::Emit(range) => {
                for h in range.clone() {
                    let v = self.compute(frame, h, act)?;
                    frame.vals[h.index()] = Some(v);
                }
            }
            Statement::Block(b) => self.block(frame, b, act)?,
            Statement::If {
                condition,
                accept,
                reject,
            } => {
                let c = self.eval(frame, *condition, act)?;
                let c = self.mask_of(&c)?;
                if act & c != 0 {
                    self.block(frame, accept, act & c)?;
                }
                if act & !c != 0 {
                    self.block(frame, reject, act & !c)?;
                }
            }
            Statement::Switch { selector, cases } => {
                let s = self.eval(frame, *selector, act)?;
                let saved = frame.brk;
                frame.brk = 0;
                let mut taken: Mask = 0;
                let mut falling: Mask = 0;
                for case in cases {
                    let here = match case.value {
                        naga::SwitchValue::Default => {
                            let mut others: Mask = 0;
                            for c in cases {
                                if c.value != naga::SwitchValue::Default {
                                    others |= self.lanes_equal(&s, c.value)?;
                                }
                            }
                            act & !others
                        }
                        value => act & self.lanes_equal(&s, value)?,
                    };
                    let run = (here & !taken) | falling;
                    taken |= here;
                    if run != 0 {
                        self.block(frame, &case.body, run)?;
                    }
                    falling = if case.fall_through {
                        run & !(frame.brk | frame.returned)
                    } else {
                        0
                    };
                }
                frame.brk = saved;
            }
            Statement::Loop {
                body,
                continuing,
                break_if,
            } => {
                let (saved_brk, saved_cont) = (frame.brk, frame.cont);
                frame.brk = 0;
                let mut live = act;
                loop {
                    *self.steps += 1;
                    if *self.steps > MAX_STEPS {
                        return Err(format!(
                            "the loops ran more than {MAX_STEPS} times for one pixel"
                        ));
                    }
                    frame.cont = 0;
                    self.block(frame, body, live)?;
                    live &= !(frame.brk | frame.returned);
                    frame.cont = 0;
                    if live & self.full == 0 {
                        break;
                    }
                    self.block(frame, continuing, live)?;
                    live &= !frame.returned;
                    if let Some(b) = break_if {
                        let c = self.eval(frame, *b, live)?;
                        live &= !self.mask_of(&c)?;
                    }
                    if live & self.full == 0 {
                        break;
                    }
                }
                frame.brk = saved_brk;
                frame.cont = saved_cont;
            }
            Statement::Break => frame.brk |= act,
            Statement::Continue => frame.cont |= act,
            Statement::Return { value } => {
                let v = match value {
                    Some(h) => self.eval(frame, *h, act)?,
                    None => V::Uni(Value::C(Vec::new())),
                };
                self.give_back(frame, v, act);
            }
            Statement::Kill => {
                self.give_back(frame, V::Uni(Value::fv(&[0.0, 0.0, 0.0, 0.0])), act);
            }
            Statement::Store { pointer, value } => {
                let V::Uni(Value::P(ptr)) = self.eval(frame, *pointer, act)? else {
                    return Err("store through a pointer that differs per pixel".into());
                };
                let v = self.eval(frame, *value, act)?;
                self.store(frame, &ptr, v, act)?;
            }
            Statement::Call {
                function,
                arguments,
                result,
            } => {
                let args = arguments
                    .iter()
                    .map(|a| self.eval(frame, *a, act))
                    .collect::<Result<Vec<_>, _>>()?;
                let v = self.call(*function, args, act)?;
                if let (Some(r), Some(v)) = (result, v) {
                    frame.vals[r.index()] = Some(v);
                }
            }
            Statement::ControlBarrier(_) | Statement::MemoryBarrier(_) => {}
            other => return Err(format!("{} is not supported", statement_name(other))),
        }
        Ok(())
    }

    fn lanes_equal(&self, s: &V, value: naga::SwitchValue) -> Result<Mask, String> {
        let want = match value {
            naga::SwitchValue::I32(v) => i64::from(v),
            naga::SwitchValue::U32(v) => i64::from(v),
            naga::SwitchValue::Default => return Ok(0),
        };
        let mut m: Mask = 0;
        for k in 0..LANES {
            if s.lane(k).as_switch()? == want {
                m |= 1 << k;
            }
            if s.is_uniform() {
                return Ok(if m & 1 != 0 { Mask::MAX } else { 0 });
            }
        }
        Ok(m)
    }

    /// The value of `h`, computing it if no `Emit` has.
    fn eval(&mut self, frame: &mut Frame, h: Handle<Expression>, act: Mask) -> Result<V, String> {
        if let Some(v) = &frame.vals[h.index()] {
            return Ok(v.clone());
        }
        let v = self.compute(frame, h, act)?;
        if !matches!(frame.fun.expressions[h], Expression::Load { .. }) {
            frame.vals[h.index()] = Some(v.clone());
        }
        Ok(v)
    }

    fn compute(
        &mut self,
        frame: &mut Frame,
        h: Handle<Expression>,
        act: Mask,
    ) -> Result<V, String> {
        let module = self.module;
        Ok(match &frame.fun.expressions[h] {
            Expression::Literal(l) => V::Uni(literal(l)?),
            Expression::Constant(c) => V::Uni(const_eval(module, module.constants[*c].init)?),
            Expression::ZeroValue(ty) => V::Uni(zero(module, *ty)),
            Expression::Compose { ty, components } => {
                let parts = components
                    .iter()
                    .map(|c| self.eval(frame, *c, act))
                    .collect::<Result<Vec<_>, _>>()?;
                compose_v(module, *ty, parts, act)?
            }
            Expression::Access { base, index } => {
                let base = self.eval(frame, *base, act)?;
                match self.eval(frame, *index, act)? {
                    V::Uni(i) => access_v(base, i.as_index()?)?,
                    index => {
                        if matches!(base, V::Uni(Value::P(_))) {
                            return Err(
                                "indexing a variable with an index that differs per pixel is not supported"
                                    .into(),
                            );
                        }
                        per_lane(act, &[&base, &index], |v| {
                            let mut v = v.into_iter();
                            let base = v.next().unwrap_or(Value::f(0.0));
                            let i = v.next().map_or(Ok(0), |i| i.as_index())?;
                            // Lanes past the end read the last element.
                            let i = i.min(base.len().saturating_sub(1));
                            access(base, i)
                        })?
                    }
                }
            }
            Expression::AccessIndex { base, index } => {
                access_v(self.eval(frame, *base, act)?, *index as usize)?
            }
            Expression::Splat { size, value } => {
                splat_v(self.eval(frame, *value, act)?, *size as usize)?
            }
            Expression::Swizzle {
                size,
                vector,
                pattern,
            } => swizzle_v(self.eval(frame, *vector, act)?, &pattern[..*size as usize])?,
            Expression::FunctionArgument(i) => frame
                .args
                .get(*i as usize)
                .cloned()
                .ok_or("missing argument")?,
            Expression::GlobalVariable(g) => V::Uni(Value::P(Ptr {
                root: Root::Global(g.index()),
                path: Vec::new(),
            })),
            Expression::LocalVariable(l) => V::Uni(Value::P(Ptr {
                root: Root::Local(l.index()),
                path: Vec::new(),
            })),
            Expression::Load { pointer } => {
                let V::Uni(Value::P(ptr)) = self.eval(frame, *pointer, act)? else {
                    return Err("load through a pointer that differs per pixel".into());
                };
                self.load(frame, &ptr)?
            }
            Expression::Unary { op, expr } => unary_v(*op, self.eval(frame, *expr, act)?, act)?,
            Expression::Binary { op, left, right } => {
                let (a, b) = (
                    self.eval(frame, *left, act)?,
                    self.eval(frame, *right, act)?,
                );
                binary_v(*op, a, b, act)?
            }
            Expression::Select {
                condition,
                accept,
                reject,
            } => {
                let c = self.eval(frame, *condition, act)?;
                let (a, r) = (
                    self.eval(frame, *accept, act)?,
                    self.eval(frame, *reject, act)?,
                );
                select_v(c, a, r, act)?
            }
            Expression::Relational { fun, argument } => {
                let a = self.eval(frame, *argument, act)?;
                let fun = *fun;
                per_lane(act, &[&a], |v| {
                    relational(fun, v.into_iter().next().unwrap_or(Value::f(0.0)))
                })?
            }
            Expression::Math {
                fun,
                arg,
                arg1,
                arg2,
                arg3: _,
            } => {
                let a = self.eval(frame, *arg, act)?;
                let b = match arg1 {
                    Some(h) => Some(self.eval(frame, *h, act)?),
                    None => None,
                };
                let c = match arg2 {
                    Some(h) => Some(self.eval(frame, *h, act)?),
                    None => None,
                };
                math_v(*fun, a, b, c, act)?
            }
            Expression::As {
                expr,
                kind,
                convert,
            } => cast_v(self.eval(frame, *expr, act)?, *kind, convert.is_some(), act)?,
            Expression::CallResult(_) => {
                return Err("a call result was read before the call".into());
            }
            other => return Err(format!("{} is not supported", expression_name(other))),
        })
    }

    fn slot<'s>(&'s mut self, frame: &'s mut Frame, root: Root) -> &'s mut V {
        match root {
            Root::Local(i) => &mut frame.locals[i],
            Root::Global(i) => &mut self.globals[i],
        }
    }

    fn load(&mut self, frame: &mut Frame, ptr: &Ptr) -> Result<V, String> {
        let mut v: &V = self.slot(frame, ptr.root);
        for (k, &i) in ptr.path.iter().enumerate() {
            match v {
                V::C(items) => v = items.get(i as usize).ok_or("index out of range")?,
                V::Uni(x) => {
                    let mut x = x.component(i as usize)?;
                    for &j in &ptr.path[k + 1..] {
                        x = x.component(j as usize)?;
                    }
                    return Ok(V::Uni(x));
                }
                leaf => return access_v(leaf.clone(), i as usize),
            }
        }
        Ok(v.clone())
    }

    fn store(&mut self, frame: &mut Frame, ptr: &Ptr, value: V, act: Mask) -> Result<(), String> {
        let full = self.full;
        let old = self.load(frame, ptr)?;
        let new = merge(old, value, act, full);
        put_v(self.slot(frame, ptr.root), &ptr.path, new)
    }
}

/// Writes `value` at `path` inside `target`, splitting a uniform value
/// into parts where a part becomes varying.
fn put_v(target: &mut V, path: &[u32], value: V) -> Result<(), String> {
    let Some((&i, rest)) = path.split_first() else {
        *target = value;
        return Ok(());
    };
    let i = i as usize;
    if let (V::Uni(t), V::Uni(v)) = (&mut *target, &value) {
        let mut inner = t.component(i)?;
        if rest.is_empty() {
            inner = v.clone();
        } else {
            let mut w = V::Uni(inner);
            put_v(&mut w, rest, value)?;
            let V::Uni(x) = w else {
                return Err("a uniform value became varying".into());
            };
            inner = x;
        }
        return t.set_component(i, inner);
    }
    // Split a uniform target into lanes or parts.
    if let V::Uni(t) = &*target {
        *target = match t {
            Value::C(items) => V::C(items.iter().cloned().map(V::Uni).collect()),
            v => V::Uni(v.clone()).into_varying(),
        };
    }
    match target {
        V::C(items) => put_v(items.get_mut(i).ok_or("index out of range")?, rest, value),
        V::F(c) if rest.is_empty() => {
            let comp = value.floats().ok_or("expected a float")?;
            *c.get_mut(i).ok_or("index out of range")? = comp[0];
            Ok(())
        }
        V::I(_) | V::U(_) | V::B(_) if rest.is_empty() => {
            let mut vals: Vec<Value> = (0..LANES).map(|k| target.lane(k)).collect();
            for (k, v) in vals.iter_mut().enumerate() {
                v.set_component(i, value.lane(k))?;
            }
            *target = from_lanes(&vals).into_varying();
            Ok(())
        }
        _ => Err("index into a value that has no parts".into()),
    }
}

impl V {
    /// The same value with one array per component, even when all lanes
    /// are equal.
    fn into_varying(self) -> V {
        fn fill<T: Copy>(n: u8, a: [T; 4]) -> Vec<Lanes<T>> {
            (0..usize::from(n)).map(|i| [a[i]; LANES]).collect()
        }
        match self {
            V::Uni(Value::F(n, a)) => V::F(fill(n, a)),
            V::Uni(Value::I(n, a)) => V::I(fill(n, a)),
            V::Uni(Value::U(n, a)) => V::U(fill(n, a)),
            V::Uni(Value::B(n, a)) => V::B(fill(n, a)),
            V::Uni(Value::C(items)) => V::C(items.into_iter().map(V::Uni).collect()),
            other => other,
        }
    }
}

fn access(base: Value, i: usize) -> Result<Value, String> {
    match base {
        Value::P(mut ptr) => {
            ptr.path.push(i as u32);
            Ok(Value::P(ptr))
        }
        v => v.component(i),
    }
}

fn literal(l: &Literal) -> Result<Value, String> {
    Ok(match *l {
        Literal::F32(v) => Value::f(v),
        Literal::AbstractFloat(v) | Literal::F64(v) => Value::f(v as f32),
        Literal::I32(v) => Value::I(1, [v, 0, 0, 0]),
        Literal::AbstractInt(v) | Literal::I64(v) => Value::I(1, [v as i32, 0, 0, 0]),
        Literal::U32(v) => Value::U(1, [v, 0, 0, 0]),
        Literal::U64(v) => Value::U(1, [v as u32, 0, 0, 0]),
        Literal::Bool(v) => Value::B(1, [v, false, false, false]),
        _ => return Err("16-bit numbers are not supported".into()),
    })
}

/// A constant from the module's global expressions.
fn const_eval(module: &Module, h: Handle<Expression>) -> Result<Value, String> {
    Ok(match &module.global_expressions[h] {
        Expression::Literal(l) => literal(l)?,
        Expression::Constant(c) => const_eval(module, module.constants[*c].init)?,
        Expression::ZeroValue(ty) => zero(module, *ty),
        Expression::Compose { ty, components } => {
            let parts = components
                .iter()
                .map(|c| const_eval(module, *c))
                .collect::<Result<Vec<_>, _>>()?;
            compose(module, *ty, parts)?
        }
        Expression::Splat { size, value } => splat(const_eval(module, *value)?, *size as usize)?,
        other => {
            return Err(format!(
                "{} in a constant is not supported",
                expression_name(other)
            ));
        }
    })
}

fn scalar_zero(kind: ScalarKind, n: u8) -> Value {
    match kind {
        ScalarKind::Float | ScalarKind::AbstractFloat => Value::F(n, [0.0; 4]),
        ScalarKind::Sint | ScalarKind::AbstractInt => Value::I(n, [0; 4]),
        ScalarKind::Uint => Value::U(n, [0; 4]),
        ScalarKind::Bool => Value::B(n, [false; 4]),
    }
}

/// The zero value of a type.
pub(crate) fn zero(module: &Module, ty: Handle<naga::Type>) -> Value {
    match &module.types[ty].inner {
        TypeInner::Scalar(s) => scalar_zero(s.kind, 1),
        TypeInner::Vector { size, scalar } => scalar_zero(scalar.kind, *size as u8),
        TypeInner::Matrix {
            columns,
            rows,
            scalar,
        } => Value::C(vec![
            scalar_zero(scalar.kind, *rows as u8);
            *columns as usize
        ]),
        TypeInner::Struct { members, .. } => {
            Value::C(members.iter().map(|m| zero(module, m.ty)).collect())
        }
        TypeInner::Array {
            base,
            size: naga::ArraySize::Constant(n),
            ..
        } => Value::C(vec![zero(module, *base); n.get() as usize]),
        _ => Value::C(Vec::new()),
    }
}

/// A vector from scalars and smaller vectors, all of one kind.
fn vector_of(parts: &[Value]) -> Result<Value, String> {
    let mut out: Option<Value> = None;
    let mut n = 0usize;
    for part in parts {
        for i in 0..part.len() {
            let c = part.component(i)?;
            if n >= 4 {
                return Err("a vector has at most 4 components".into());
            }
            match (&mut out, c) {
                (None, Value::F(_, a)) => out = Some(Value::F(0, [a[0], 0.0, 0.0, 0.0])),
                (None, Value::I(_, a)) => out = Some(Value::I(0, [a[0], 0, 0, 0])),
                (None, Value::U(_, a)) => out = Some(Value::U(0, [a[0], 0, 0, 0])),
                (None, Value::B(_, a)) => out = Some(Value::B(0, [a[0], false, false, false])),
                (Some(Value::F(_, o)), Value::F(_, a)) => o[n] = a[0],
                (Some(Value::I(_, o)), Value::I(_, a)) => o[n] = a[0],
                (Some(Value::U(_, o)), Value::U(_, a)) => o[n] = a[0],
                (Some(Value::B(_, o)), Value::B(_, a)) => o[n] = a[0],
                _ => return Err("a vector mixes number kinds".into()),
            }
            n += 1;
        }
    }
    let mut v = out.ok_or("an empty vector")?;
    match &mut v {
        Value::F(len, _) | Value::I(len, _) | Value::U(len, _) | Value::B(len, _) => {
            *len = n as u8;
        }
        _ => {}
    }
    Ok(v)
}

fn compose(module: &Module, ty: Handle<naga::Type>, parts: Vec<Value>) -> Result<Value, String> {
    match &module.types[ty].inner {
        TypeInner::Vector { .. } | TypeInner::Scalar(_) => vector_of(&parts),
        TypeInner::Matrix { columns, rows, .. } => {
            if parts.len() == *columns as usize {
                Ok(Value::C(parts))
            } else {
                // From single numbers, column by column.
                let rows = *rows as usize;
                parts
                    .chunks(rows)
                    .map(vector_of)
                    .collect::<Result<Vec<_>, _>>()
                    .map(Value::C)
            }
        }
        _ => Ok(Value::C(parts)),
    }
}

fn splat(v: Value, n: usize) -> Result<Value, String> {
    Ok(match v {
        Value::F(1, a) => Value::F(n as u8, [a[0]; 4]),
        Value::I(1, a) => Value::I(n as u8, [a[0]; 4]),
        Value::U(1, a) => Value::U(n as u8, [a[0]; 4]),
        Value::B(1, a) => Value::B(n as u8, [a[0]; 4]),
        _ => return Err("splat needs a scalar".into()),
    })
}

/// Makes `a` and `b` the same length, repeating a scalar.
fn widen(a: Value, b: Value) -> Result<(Value, Value), String> {
    let (la, lb) = (a.len(), b.len());
    if la == lb {
        return Ok((a, b));
    }
    if la == 1 && !matches!(a, Value::C(_)) {
        return Ok((splat(a, lb)?, b));
    }
    if lb == 1 && !matches!(b, Value::C(_)) {
        return Ok((a, splat(b, la)?));
    }
    Err("the two sides have different sizes".into())
}

fn map_f(v: &Value, f: impl Fn(f32) -> f32) -> Result<Value, String> {
    match v {
        Value::F(n, a) => Ok(Value::F(*n, [f(a[0]), f(a[1]), f(a[2]), f(a[3])])),
        _ => Err("expected floats".into()),
    }
}

fn zip_f(a: Value, b: Value, f: impl Fn(f32, f32) -> f32) -> Result<Value, String> {
    match widen(a, b)? {
        (Value::F(n, x), Value::F(_, y)) => Ok(Value::F(
            n,
            [f(x[0], y[0]), f(x[1], y[1]), f(x[2], y[2]), f(x[3], y[3])],
        )),
        _ => Err("expected floats".into()),
    }
}

fn zip3_f(a: Value, b: Value, c: Value, f: impl Fn(f32, f32, f32) -> f32) -> Result<Value, String> {
    let n = a.len().max(b.len()).max(c.len());
    let w = |v: Value| -> Result<Value, String> { if v.len() == n { Ok(v) } else { splat(v, n) } };
    match (w(a)?, w(b)?, w(c)?) {
        (Value::F(n, x), Value::F(_, y), Value::F(_, z)) => Ok(Value::F(
            n,
            [
                f(x[0], y[0], z[0]),
                f(x[1], y[1], z[1]),
                f(x[2], y[2], z[2]),
                f(x[3], y[3], z[3]),
            ],
        )),
        _ => Err("expected floats".into()),
    }
}

fn unary(op: UnaryOperator, v: Value) -> Result<Value, String> {
    Ok(match (op, v) {
        (UnaryOperator::Negate, Value::F(n, a)) => Value::F(n, a.map(|x| -x)),
        (UnaryOperator::Negate, Value::I(n, a)) => Value::I(n, a.map(i32::wrapping_neg)),
        (UnaryOperator::Negate, Value::C(cols)) => Value::C(
            cols.into_iter()
                .map(|c| unary(op, c))
                .collect::<Result<_, _>>()?,
        ),
        (UnaryOperator::LogicalNot, Value::B(n, a)) => Value::B(n, a.map(|x| !x)),
        (UnaryOperator::BitwiseNot, Value::I(n, a)) => Value::I(n, a.map(|x| !x)),
        (UnaryOperator::BitwiseNot, Value::U(n, a)) => Value::U(n, a.map(|x| !x)),
        _ => return Err("this unary operator does not apply to this type".into()),
    })
}

fn dot(a: &Value, b: &Value) -> Result<f32, String> {
    match (a, b) {
        (Value::F(n, x), Value::F(m, y)) if n == m => {
            let mut s = 0.0;
            for i in 0..usize::from(*n) {
                s += x[i] * y[i];
            }
            Ok(s)
        }
        _ => Err("dot needs two float vectors of one size".into()),
    }
}

fn matrix_columns(m: &Value) -> Option<&[Value]> {
    match m {
        Value::C(cols) if cols.iter().all(|c| matches!(c, Value::F(_, _))) => Some(cols),
        _ => None,
    }
}

fn binary(op: BinaryOperator, a: Value, b: Value) -> Result<Value, String> {
    use BinaryOperator as B;
    // Matrices.
    if let (Some(cols), Value::F(n, v)) = (matrix_columns(&a), &b)
        && op == B::Multiply
        && cols.len() == usize::from(*n)
    {
        let mut out = Value::F(cols[0].len() as u8, [0.0; 4]);
        for (j, col) in cols.iter().enumerate() {
            out = zip_f(out, map_f(col, |x| x * v[j])?, |x, y| x + y)?;
        }
        return Ok(out);
    }
    if let (Value::F(_, _), Some(cols)) = (&a, matrix_columns(&b))
        && op == B::Multiply
    {
        let vals = cols
            .iter()
            .map(|c| dot(&a, c))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Value::fv(&vals));
    }
    if let (Some(x), Some(y)) = (matrix_columns(&a), matrix_columns(&b)) {
        if op == B::Multiply {
            let cols = y
                .iter()
                .map(|c| binary(B::Multiply, Value::C(x.to_vec()), c.clone()))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Value::C(cols));
        }
        if matches!(op, B::Add | B::Subtract) {
            let cols = x
                .iter()
                .zip(y)
                .map(|(p, q)| binary(op, p.clone(), q.clone()))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Value::C(cols));
        }
    }
    if let Value::C(cols) = &a
        && matches!(b, Value::F(1, _))
        && op == B::Multiply
    {
        let cols = cols
            .iter()
            .map(|c| binary(op, c.clone(), b.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Value::C(cols));
    }
    let (a, b) = widen(a, b)?;
    let cmp = |o: std::cmp::Ordering| match op {
        B::Equal => Some(o == std::cmp::Ordering::Equal),
        B::NotEqual => Some(o != std::cmp::Ordering::Equal),
        B::Less => Some(o == std::cmp::Ordering::Less),
        B::LessEqual => Some(o != std::cmp::Ordering::Greater),
        B::Greater => Some(o == std::cmp::Ordering::Greater),
        B::GreaterEqual => Some(o != std::cmp::Ordering::Less),
        _ => None,
    };
    let unsupported = || "this operator does not apply to this type".to_string();
    Ok(match (a, b) {
        (Value::F(n, x), Value::F(_, y)) => {
            if matches!(
                op,
                B::Equal | B::NotEqual | B::Less | B::LessEqual | B::Greater | B::GreaterEqual
            ) {
                let mut r = [false; 4];
                for i in 0..usize::from(n) {
                    r[i] = match op {
                        B::Equal => x[i] == y[i],
                        B::NotEqual => x[i] != y[i],
                        B::Less => x[i] < y[i],
                        B::LessEqual => x[i] <= y[i],
                        B::Greater => x[i] > y[i],
                        _ => x[i] >= y[i],
                    };
                }
                Value::B(n, r)
            } else {
                let f = |p: f32, q: f32| -> Option<f32> {
                    Some(match op {
                        B::Add => p + q,
                        B::Subtract => p - q,
                        B::Multiply => p * q,
                        B::Divide => p / q,
                        B::Modulo => p - q * (p / q).trunc(),
                        _ => return None,
                    })
                };
                let mut r = [0.0; 4];
                for i in 0..4 {
                    r[i] = f(x[i], y[i]).ok_or_else(unsupported)?;
                }
                Value::F(n, r)
            }
        }
        (Value::I(n, x), Value::I(_, y)) => {
            if cmp(std::cmp::Ordering::Equal).is_some() {
                let mut r = [false; 4];
                for i in 0..usize::from(n) {
                    r[i] = cmp(x[i].cmp(&y[i])).unwrap_or(false);
                }
                Value::B(n, r)
            } else {
                let mut r = [0; 4];
                for i in 0..4 {
                    let (p, q) = (x[i], y[i]);
                    r[i] = match op {
                        B::Add => p.wrapping_add(q),
                        B::Subtract => p.wrapping_sub(q),
                        B::Multiply => p.wrapping_mul(q),
                        B::Divide if q == 0 => p,
                        B::Divide => p.wrapping_div(q),
                        B::Modulo if q == 0 => 0,
                        B::Modulo => p.wrapping_rem(q),
                        B::And => p & q,
                        B::InclusiveOr => p | q,
                        B::ExclusiveOr => p ^ q,
                        B::ShiftLeft => p.wrapping_shl(q as u32),
                        B::ShiftRight => p.wrapping_shr(q as u32),
                        _ => return Err(unsupported()),
                    };
                }
                Value::I(n, r)
            }
        }
        (Value::I(n, x), Value::U(_, y)) if matches!(op, B::ShiftLeft | B::ShiftRight) => {
            let mut r = [0; 4];
            for i in 0..4 {
                r[i] = if op == B::ShiftLeft {
                    x[i].wrapping_shl(y[i])
                } else {
                    x[i].wrapping_shr(y[i])
                };
            }
            Value::I(n, r)
        }
        (Value::U(n, x), Value::U(_, y)) => {
            if cmp(std::cmp::Ordering::Equal).is_some() {
                let mut r = [false; 4];
                for i in 0..usize::from(n) {
                    r[i] = cmp(x[i].cmp(&y[i])).unwrap_or(false);
                }
                Value::B(n, r)
            } else {
                let mut r = [0; 4];
                for i in 0..4 {
                    let (p, q) = (x[i], y[i]);
                    r[i] = match op {
                        B::Add => p.wrapping_add(q),
                        B::Subtract => p.wrapping_sub(q),
                        B::Multiply => p.wrapping_mul(q),
                        B::Divide if q == 0 => p,
                        B::Divide => p / q,
                        B::Modulo if q == 0 => 0,
                        B::Modulo => p % q,
                        B::And => p & q,
                        B::InclusiveOr => p | q,
                        B::ExclusiveOr => p ^ q,
                        B::ShiftLeft => p.wrapping_shl(q),
                        B::ShiftRight => p.wrapping_shr(q),
                        _ => return Err(unsupported()),
                    };
                }
                Value::U(n, r)
            }
        }
        (Value::B(n, x), Value::B(_, y)) => {
            let mut r = [false; 4];
            for i in 0..4 {
                r[i] = match op {
                    B::Equal => x[i] == y[i],
                    B::NotEqual => x[i] != y[i],
                    B::LogicalAnd | B::And => x[i] && y[i],
                    B::LogicalOr | B::InclusiveOr => x[i] || y[i],
                    _ => return Err(unsupported()),
                };
            }
            Value::B(n, r)
        }
        _ => return Err(unsupported()),
    })
}

fn select(c: Value, accept: Value, reject: Value) -> Result<Value, String> {
    match c {
        Value::B(1, x) => Ok(if x[0] { accept } else { reject }),
        Value::B(n, x) => {
            let mut out = reject;
            for i in 0..usize::from(n) {
                if x[i] {
                    out.set_component(i, accept.component(i)?)?;
                }
            }
            Ok(out)
        }
        _ => Err("select needs a bool condition".into()),
    }
}

fn relational(fun: naga::RelationalFunction, v: Value) -> Result<Value, String> {
    use naga::RelationalFunction as R;
    let one = |b: bool| Value::B(1, [b, false, false, false]);
    Ok(match (fun, v) {
        (R::All, Value::B(n, a)) => one(a[..usize::from(n)].iter().all(|x| *x)),
        (R::Any, Value::B(n, a)) => one(a[..usize::from(n)].iter().any(|x| *x)),
        (R::IsNan, Value::F(n, a)) => Value::B(n, a.map(f32::is_nan)),
        (R::IsInf, Value::F(n, a)) => Value::B(n, a.map(f32::is_infinite)),
        _ => return Err("this relational function does not apply to this type".into()),
    })
}

fn length(v: &Value) -> Result<f32, String> {
    Ok(dot(v, v)?.sqrt())
}

/// `round` in WGSL rounds halves to even.
fn round_even(x: f32) -> f32 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 {
        2.0 * (x / 2.0).round()
    } else {
        r
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn math(fun: MathFunction, a: Value, b: Option<Value>, c: Option<Value>) -> Result<Value, String> {
    use MathFunction as M;
    let need = |v: Option<Value>| v.ok_or_else(|| "missing argument".to_string());
    Ok(match fun {
        M::Abs => match a {
            Value::I(n, x) => Value::I(n, x.map(i32::wrapping_abs)),
            Value::U(..) => a,
            _ => map_f(&a, f32::abs)?,
        },
        M::Min | M::Max => {
            let b = need(b)?;
            match widen(a, b)? {
                (Value::I(n, x), Value::I(_, y)) => {
                    let mut r = [0; 4];
                    for i in 0..4 {
                        r[i] = if fun == M::Min {
                            x[i].min(y[i])
                        } else {
                            x[i].max(y[i])
                        };
                    }
                    Value::I(n, r)
                }
                (Value::U(n, x), Value::U(_, y)) => {
                    let mut r = [0; 4];
                    for i in 0..4 {
                        r[i] = if fun == M::Min {
                            x[i].min(y[i])
                        } else {
                            x[i].max(y[i])
                        };
                    }
                    Value::U(n, r)
                }
                (a, b) if fun == M::Min => zip_f(a, b, f32::min)?,
                (a, b) => zip_f(a, b, f32::max)?,
            }
        }
        M::Clamp => match (a, need(b)?, need(c)?) {
            (Value::I(n, x), lo, hi) => {
                let (Value::I(_, l), Value::I(_, h)) = (splat_to(lo, n)?, splat_to(hi, n)?) else {
                    return Err("clamp mixes number kinds".into());
                };
                let mut r = [0; 4];
                for i in 0..4 {
                    r[i] = x[i].max(l[i]).min(h[i]);
                }
                Value::I(n, r)
            }
            (a, lo, hi) => zip3_f(a, lo, hi, |x, l, h| x.max(l).min(h))?,
        },
        M::Saturate => map_f(&a, |x| x.clamp(0.0, 1.0))?,
        M::Cos => map_f(&a, libm::cosf)?,
        M::Cosh => map_f(&a, libm::coshf)?,
        M::Sin => map_f(&a, libm::sinf)?,
        M::Sinh => map_f(&a, libm::sinhf)?,
        M::Tan => map_f(&a, libm::tanf)?,
        M::Tanh => map_f(&a, libm::tanhf)?,
        M::Acos => map_f(&a, libm::acosf)?,
        M::Asin => map_f(&a, libm::asinf)?,
        M::Atan => map_f(&a, libm::atanf)?,
        M::Atan2 => zip_f(a, need(b)?, libm::atan2f)?,
        M::Asinh => map_f(&a, libm::asinhf)?,
        M::Acosh => map_f(&a, libm::acoshf)?,
        M::Atanh => map_f(&a, libm::atanhf)?,
        M::Radians => map_f(&a, |x| x * (std::f32::consts::PI / 180.0))?,
        M::Degrees => map_f(&a, |x| x * (180.0 / std::f32::consts::PI))?,
        M::Ceil => map_f(&a, f32::ceil)?,
        M::Floor => map_f(&a, f32::floor)?,
        M::Round => map_f(&a, round_even)?,
        M::Fract => map_f(&a, |x| x - x.floor())?,
        M::Trunc => map_f(&a, f32::trunc)?,
        M::Exp => map_f(&a, libm::expf)?,
        M::Exp2 => map_f(&a, libm::exp2f)?,
        M::Log => map_f(&a, libm::logf)?,
        M::Log2 => map_f(&a, libm::log2f)?,
        M::Pow => zip_f(a, need(b)?, libm::powf)?,
        M::Sqrt => map_f(&a, f32::sqrt)?,
        M::InverseSqrt => map_f(&a, |x| 1.0 / x.sqrt())?,
        M::Sign => match a {
            Value::I(n, x) => Value::I(n, x.map(i32::signum)),
            _ => map_f(&a, |x| {
                if x > 0.0 {
                    1.0
                } else if x < 0.0 {
                    -1.0
                } else {
                    0.0
                }
            })?,
        },
        M::Fma => zip3_f(a, need(b)?, need(c)?, |x, y, z| x * y + z)?,
        M::Mix => zip3_f(a, need(b)?, need(c)?, |x, y, t| x * (1.0 - t) + y * t)?,
        M::Step => zip_f(a, need(b)?, |edge, x| if x < edge { 0.0 } else { 1.0 })?,
        M::SmoothStep => zip3_f(a, need(b)?, need(c)?, smoothstep)?,
        M::Dot => match (a, need(b)?) {
            (Value::I(n, x), Value::I(_, y)) => {
                let mut s = 0i32;
                for i in 0..usize::from(n) {
                    s = s.wrapping_add(x[i].wrapping_mul(y[i]));
                }
                Value::I(1, [s, 0, 0, 0])
            }
            (a, b) => Value::f(dot(&a, &b)?),
        },
        M::Cross => match (a, need(b)?) {
            (Value::F(3, x), Value::F(3, y)) => Value::fv(&[
                x[1] * y[2] - x[2] * y[1],
                x[2] * y[0] - x[0] * y[2],
                x[0] * y[1] - x[1] * y[0],
            ]),
            _ => return Err("cross needs two vec3<f32>".into()),
        },
        M::Length => Value::f(length(&a)?),
        M::Distance => Value::f(length(&zip_f(a, need(b)?, |x, y| x - y)?)?),
        M::Normalize => {
            let l = length(&a)?;
            map_f(&a, |x| x / l)?
        }
        M::FaceForward => {
            let (i, nref) = (need(b)?, need(c)?);
            if dot(&nref, &i)? < 0.0 {
                a
            } else {
                unary(UnaryOperator::Negate, a)?
            }
        }
        M::Reflect => {
            let n = need(b)?;
            let d = 2.0 * dot(&n, &a)?;
            zip_f(a, n, |i, n| i - d * n)?
        }
        M::Transpose => {
            let cols = matrix_columns(&a).ok_or("transpose needs a matrix")?;
            let rows = cols[0].len();
            let mut out = Vec::with_capacity(rows);
            for r in 0..rows {
                let parts = cols
                    .iter()
                    .map(|c| c.component(r))
                    .collect::<Result<Vec<_>, _>>()?;
                out.push(vector_of(&parts)?);
            }
            Value::C(out)
        }
        M::CountOneBits => match a {
            Value::U(n, x) => Value::U(n, x.map(u32::count_ones)),
            Value::I(n, x) => Value::I(n, x.map(|v| v.count_ones() as i32)),
            _ => return Err("countOneBits needs integers".into()),
        },
        M::ReverseBits => match a {
            Value::U(n, x) => Value::U(n, x.map(u32::reverse_bits)),
            Value::I(n, x) => Value::I(n, x.map(i32::reverse_bits)),
            _ => return Err("reverseBits needs integers".into()),
        },
        other => return Err(format!("{other:?} is not supported")),
    })
}

fn splat_to(v: Value, n: u8) -> Result<Value, String> {
    if v.len() == usize::from(n) {
        Ok(v)
    } else {
        splat(v, usize::from(n))
    }
}

fn cast(v: Value, kind: ScalarKind, convert: bool) -> Result<Value, String> {
    let (n, comps): (u8, [f64; 4]) = match &v {
        Value::F(n, a) if convert => (*n, a.map(f64::from)),
        Value::I(n, a) if convert => (*n, a.map(f64::from)),
        Value::U(n, a) if convert => (*n, a.map(f64::from)),
        Value::B(n, a) if convert => (*n, a.map(|b| if b { 1.0 } else { 0.0 })),
        _ => {
            // Bitcast.
            return Ok(match (v, kind) {
                (Value::F(n, a), ScalarKind::Uint) => Value::U(n, a.map(f32::to_bits)),
                (Value::F(n, a), ScalarKind::Sint) => Value::I(n, a.map(|x| x.to_bits() as i32)),
                (Value::U(n, a), ScalarKind::Float) => Value::F(n, a.map(f32::from_bits)),
                (Value::I(n, a), ScalarKind::Float) => {
                    Value::F(n, a.map(|x| f32::from_bits(x as u32)))
                }
                (Value::U(n, a), ScalarKind::Sint) => Value::I(n, a.map(|x| x as i32)),
                (Value::I(n, a), ScalarKind::Uint) => Value::U(n, a.map(|x| x as u32)),
                (v, _) => v,
            });
        }
    };
    Ok(match kind {
        ScalarKind::Float | ScalarKind::AbstractFloat => match &v {
            // f32 → f32 or int → f32 directly, to round once.
            Value::F(..) => v,
            Value::I(n, a) => Value::F(*n, a.map(|x| x as f32)),
            Value::U(n, a) => Value::F(*n, a.map(|x| x as f32)),
            _ => Value::F(n, comps.map(|x| x as f32)),
        },
        ScalarKind::Sint | ScalarKind::AbstractInt => match &v {
            Value::F(n, a) => Value::I(*n, a.map(|x| x as i32)),
            Value::U(n, a) => Value::I(*n, a.map(|x| x as i32)),
            _ => Value::I(n, comps.map(|x| x as i32)),
        },
        ScalarKind::Uint => match &v {
            Value::F(n, a) => Value::U(*n, a.map(|x| x as u32)),
            Value::I(n, a) => Value::U(*n, a.map(|x| x as u32)),
            _ => Value::U(n, comps.map(|x| x as u32)),
        },
        ScalarKind::Bool => Value::B(n, comps.map(|x| x != 0.0)),
    })
}

fn expression_name(e: &Expression) -> &'static str {
    match e {
        Expression::ImageSample { .. } => "textureSample",
        Expression::ImageLoad { .. } => "textureLoad",
        Expression::ImageQuery { .. } => "a texture query",
        Expression::Derivative { .. } => "dpdx/dpdy/fwidth",
        Expression::ArrayLength(_) => "arrayLength",
        Expression::Override(_) => "an override",
        Expression::AtomicResult { .. } => "an atomic",
        _ => "this expression",
    }
}

fn statement_name(s: &Statement) -> &'static str {
    match s {
        Statement::ImageStore { .. } => "textureStore",
        Statement::Atomic { .. } | Statement::ImageAtomic { .. } => "an atomic",
        Statement::WorkGroupUniformLoad { .. } => "workgroupUniformLoad",
        _ => "this statement",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(src: &str) -> Program {
        let module = naga::front::wgsl::parse_str(src).expect("parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("valid");
        Program::new(module).expect("has shade")
    }

    fn globals() -> Globals {
        Globals {
            inputs: Vec::new(),
            ox: Value::C(vec![Value::f(2.0)]),
            params: Value::C(vec![Value::f(0.5), Value::fv(&[1.0, 0.0, 0.0, 1.0])]),
        }
    }

    const HEAD: &str = "struct Ox { time: f32 }
@group(0) @binding(0) var<uniform> ox: Ox;
struct Params { amp: f32, tint: vec4<f32> }
@group(1) @binding(0) var<uniform> p: Params;
struct ShadeIn { uv: vec2<f32> }
";

    fn run(body: &str, uv: [f32; 2]) -> [f32; 4] {
        let p = program(&format!("{HEAD}{body}"));
        p.run(&globals(), Value::C(vec![Value::fv(&uv)]))
            .expect("runs")
            .floats()
            .expect("floats")
    }

    #[test]
    fn reads_inputs_params_and_ox() {
        let c = run(
            "fn shade(i: ShadeIn) -> vec4<f32> { return vec4<f32>(i.uv.x, p.amp, ox.time, 1.0); }",
            [0.25, 0.0],
        );
        assert_eq!(c, [0.25, 0.5, 2.0, 1.0]);
    }

    #[test]
    fn loops_ifs_calls_and_vars() {
        let c = run(
            "fn twice(x: f32) -> f32 { return x * 2.0; }
fn shade(i: ShadeIn) -> vec4<f32> {
    var s = 0.0;
    for (var k = 0; k < 4; k++) {
        if (k == 2) { continue; }
        s += twice(f32(k));
    }
    var c = p.tint;
    c.g = s / 10.0;
    return c;
}",
            [0.0, 0.0],
        );
        // 2 * (0 + 1 + 3) = 8
        assert_eq!(c, [1.0, 0.8, 0.0, 1.0]);
    }

    #[test]
    fn builtins_match_their_definitions() {
        let c = run(
            "fn shade(i: ShadeIn) -> vec4<f32> {
    let h = 0.5 + i.uv.x;
    let a = smoothstep(0.0, 1.0, h);
    let b = mix(vec2<f32>(0.0, 10.0), vec2<f32>(1.0, 20.0), h);
    let d = length(vec2<f32>(3.0, 4.0) + i.uv);
    return vec4<f32>(a, b.y, d, fract(i.uv.y - 0.25));
}",
            [0.0, 0.0],
        );
        assert_eq!(c, [0.5, 15.0, 5.0, 0.75]);
    }

    /// Runs `body` on a tile whose lane `k` has `uv = (k, 0)`, and returns
    /// the first component of each lane's result.
    fn tile(body: &str, count: usize) -> Vec<f32> {
        let p = program(&format!("{HEAD}{body}"));
        let mut x = [0.0f32; LANES];
        for (k, v) in x.iter_mut().enumerate() {
            *v = k as f32;
        }
        let input = V::C(vec![V::F(vec![x, [0.0; LANES]])]);
        let out = p.runner(&globals()).run(&input, count).expect("runs");
        (0..count)
            .map(|k| out.lane(k).floats().expect("floats")[0])
            .collect()
    }

    #[test]
    fn lanes_take_their_own_branches_loops_and_returns() {
        // Each pixel k: early return for k < 3; otherwise a loop that
        // skips odd i, stops at i == k, and a switch on k % 3.
        let got = tile(
            "fn shade(i: ShadeIn) -> vec4<f32> {
    let k = i32(i.uv.x);
    if (k < 3) { return vec4<f32>(-1.0); }
    var s = 0;
    for (var j = 0; j < 100; j++) {
        if (j == k) { break; }
        if (j % 2 == 1) { continue; }
        s += j;
    }
    var bonus = 0;
    switch (k % 3) {
        case 0: { bonus = 1000; }
        case 1: { bonus = 2000; }
        default: { bonus = 3000; }
    }
    return vec4<f32>(f32(s + bonus));
}",
            LANES,
        );
        for (k, v) in got.iter().enumerate() {
            let want = if k < 3 {
                -1.0
            } else {
                let s: usize = (0..k).filter(|j| j % 2 == 0).sum();
                (s + [1000, 2000, 3000][k % 3]) as f32
            };
            assert_eq!(*v, want, "lane {k}");
        }
    }

    #[test]
    fn a_partial_tile_runs_only_its_pixels() {
        // Lanes past `count` would loop forever; they must not run.
        let got = tile(
            "fn shade(i: ShadeIn) -> vec4<f32> {
    var x = 0.0;
    loop {
        if (i.uv.x < 5.0) { break; }
        x += 1.0;
    }
    return vec4<f32>(i.uv.x + x);
}",
            5,
        );
        assert_eq!(got, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn a_vector_component_written_in_a_branch_stays_per_lane() {
        let got = tile(
            "fn shade(i: ShadeIn) -> vec4<f32> {
    var c = vec4<f32>(0.0);
    if (i.uv.x > 1.5) { c.x = i.uv.x * 2.0; } else { c.x = 7.0; }
    return c;
}",
            4,
        );
        assert_eq!(got, vec![7.0, 7.0, 4.0, 6.0]);
    }

    #[test]
    fn a_loop_that_never_ends_is_an_error() {
        let p = program(&format!(
            "{HEAD}fn shade(i: ShadeIn) -> vec4<f32> {{ var x = 0.0; loop {{ x += 1.0; }} return vec4<f32>(x); }}"
        ));
        let err = p
            .run(&globals(), Value::C(vec![Value::fv(&[0.0, 0.0])]))
            .unwrap_err();
        assert!(err.contains("loops"), "{err}");
    }
}
