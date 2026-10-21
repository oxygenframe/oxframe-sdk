//! Triangle meshes: built-in shapes and glTF models.

use std::f64::consts::{PI, TAU};
use std::rc::Rc;

use crate::math::Vec3;

/// Triangles with a normal per vertex. Counterclockwise triangles face out.
#[derive(Debug, Clone, Default)]
pub struct Geometry {
    /// Vertex positions.
    pub positions: Vec<Vec3>,
    /// Vertex normals, one per position.
    pub normals: Vec<Vec3>,
    /// Three vertex indices per triangle.
    pub triangles: Vec<[u32; 3]>,
}

impl Geometry {
    fn push_vertex(&mut self, p: Vec3, n: Vec3) -> u32 {
        self.positions.push(p);
        self.normals.push(n.normalize());
        (self.positions.len() - 1) as u32
    }

    /// A box `size` wide in each direction, centered on the origin, with
    /// flat faces.
    pub fn cube(size: f64) -> Geometry {
        let h = size / 2.0;
        let mut g = Geometry::default();
        // (normal, right, up) per face.
        let faces = [
            (Vec3::X, -Vec3::Z, Vec3::Y),
            (-Vec3::X, Vec3::Z, Vec3::Y),
            (Vec3::Y, Vec3::X, -Vec3::Z),
            (-Vec3::Y, Vec3::X, Vec3::Z),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (-Vec3::Z, -Vec3::X, Vec3::Y),
        ];
        for (n, r, u) in faces {
            let c = n * h;
            let corners = [
                c - r * h - u * h,
                c + r * h - u * h,
                c + r * h + u * h,
                c - r * h + u * h,
            ];
            let ids: Vec<u32> = corners.iter().map(|&p| g.push_vertex(p, n)).collect();
            g.triangles.push([ids[0], ids[1], ids[2]]);
            g.triangles.push([ids[0], ids[2], ids[3]]);
        }
        g
    }

    /// A sphere of `radius` with `rings` bands from pole to pole, smooth.
    pub fn sphere(radius: f64, rings: u32) -> Geometry {
        let rings = rings.max(3);
        let segments = rings * 2;
        let mut g = Geometry::default();
        for i in 0..=rings {
            let v = f64::from(i) / f64::from(rings);
            let (sv, cv) = (v * PI).sin_cos();
            for j in 0..=segments {
                let u = f64::from(j) / f64::from(segments);
                let (su, cu) = (u * TAU).sin_cos();
                let n = Vec3::new(sv * cu, cv, -sv * su);
                g.push_vertex(n * radius, n);
            }
        }
        let row = segments + 1;
        for i in 0..rings {
            for j in 0..segments {
                let a = i * row + j;
                let b = a + row;
                if i != 0 {
                    g.triangles.push([a, b, a + 1]);
                }
                if i != rings - 1 {
                    g.triangles.push([a + 1, b, b + 1]);
                }
            }
        }
        g
    }

    /// A flat square `size` wide on the XZ plane, facing up.
    pub fn plane(size: f64) -> Geometry {
        let h = size / 2.0;
        let mut g = Geometry::default();
        let ids: Vec<u32> = [(-h, h), (h, h), (h, -h), (-h, -h)]
            .iter()
            .map(|&(x, z)| g.push_vertex(Vec3::new(x, 0.0, z), Vec3::Y))
            .collect();
        g.triangles.push([ids[0], ids[1], ids[2]]);
        g.triangles.push([ids[0], ids[2], ids[3]]);
        g
    }

    /// A cylinder along Y, `radius` around and `height` tall, with caps.
    pub fn cylinder(radius: f64, height: f64, segments: u32) -> Geometry {
        let segments = segments.max(3);
        let h = height / 2.0;
        let mut g = Geometry::default();
        let ring = |j: u32| {
            let (s, c) = (f64::from(j) / f64::from(segments) * TAU).sin_cos();
            Vec3::new(c, 0.0, -s)
        };
        for j in 0..segments {
            let (a, b) = (ring(j), ring(j + 1));
            let ids = [
                g.push_vertex(a * radius - Vec3::Y * h, a),
                g.push_vertex(b * radius - Vec3::Y * h, b),
                g.push_vertex(b * radius + Vec3::Y * h, b),
                g.push_vertex(a * radius + Vec3::Y * h, a),
            ];
            g.triangles.push([ids[0], ids[1], ids[2]]);
            g.triangles.push([ids[0], ids[2], ids[3]]);
            for (y, n) in [(h, Vec3::Y), (-h, -Vec3::Y)] {
                let c = g.push_vertex(Vec3::new(0.0, y, 0.0), n);
                let p = g.push_vertex(a * radius + Vec3::Y * y, n);
                let q = g.push_vertex(b * radius + Vec3::Y * y, n);
                g.triangles
                    .push(if y > 0.0 { [c, p, q] } else { [c, q, p] });
            }
        }
        g
    }

    /// The smallest box around the mesh, as (min, max).
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f64::INFINITY);
        let mut hi = Vec3::splat(f64::NEG_INFINITY);
        for p in &self.positions {
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        (lo, hi)
    }

    /// Moves and scales the mesh so it is centered on the origin and its
    /// largest side is `size`. Models come in any units; this makes them
    /// sit like the built-in shapes.
    pub fn fit(mut self, size: f64) -> Geometry {
        let (lo, hi) = self.bounds();
        if !lo.x.is_finite() {
            return self;
        }
        let center = (lo + hi) * 0.5;
        let extent = hi - lo;
        let largest = extent.x.max(extent.y).max(extent.z);
        let k = if largest > 0.0 { size / largest } else { 1.0 };
        for p in &mut self.positions {
            *p = (*p - center) * k;
        }
        self
    }
}

/// Reads a binary glTF (`.glb`): every mesh primitive of the default scene,
/// with its node transforms, as one geometry. Textures are not read; the
/// material's base color is (see [`load_glb`]'s second value).
///
/// # Errors
///
/// If the bytes are not a glTF this crate can read.
#[cfg(feature = "gltf")]
pub fn load_glb(bytes: &[u8]) -> Result<(Geometry, Option<[f64; 4]>), String> {
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| format!("not a glTF: {e}"))?;
    let blob = gltf.blob.clone();
    let buffers: Vec<Vec<u8>> = gltf
        .buffers()
        .map(|b| match b.source() {
            gltf::buffer::Source::Bin => Ok(blob.clone().unwrap_or_default()),
            gltf::buffer::Source::Uri(uri) => data_uri(uri)
                .ok_or_else(|| format!("buffer `{uri}` is a separate file; use a .glb")),
        })
        .collect::<Result<_, _>>()?;
    let mut out = Geometry::default();
    let mut color = None;
    let scene = gltf
        .default_scene()
        .or_else(|| gltf.scenes().next())
        .ok_or("the glTF has no scene")?;
    for node in scene.nodes() {
        walk(
            &node,
            crate::math::Mat4::IDENTITY,
            &buffers,
            &mut out,
            &mut color,
        );
    }
    if out.triangles.is_empty() {
        return Err("the glTF has no triangles".into());
    }
    // Missing normals: flat ones from the triangles.
    for t in &out.triangles {
        let [a, b, c] = t.map(|i| i as usize);
        if out.normals[a] == Vec3::ZERO
            || out.normals[b] == Vec3::ZERO
            || out.normals[c] == Vec3::ZERO
        {
            let n = (out.positions[b] - out.positions[a])
                .cross(out.positions[c] - out.positions[a])
                .normalize();
            for i in [a, b, c] {
                if out.normals[i] == Vec3::ZERO {
                    out.normals[i] = n;
                }
            }
        }
    }
    Ok((out, color))
}

#[cfg(feature = "gltf")]
fn walk(
    node: &gltf::Node<'_>,
    parent: crate::math::Mat4,
    buffers: &[Vec<u8>],
    out: &mut Geometry,
    color: &mut Option<[f64; 4]>,
) {
    let local = crate::math::Mat4(node.transform().matrix().map(|c| c.map(f64::from)));
    let world = parent.mul(&local);
    if let Some(mesh) = node.mesh() {
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = prim.reader(|b| buffers.get(b.index()).map(Vec::as_slice));
            let Some(positions) = reader.read_positions() else {
                continue;
            };
            let base = out.positions.len() as u32;
            let positions: Vec<Vec3> = positions
                .map(|[x, y, z]| world.point(Vec3::new(x.into(), y.into(), z.into())))
                .collect();
            let normals: Vec<Vec3> = match reader.read_normals() {
                Some(n) => n
                    .map(|[x, y, z]| {
                        world
                            .vector(Vec3::new(x.into(), y.into(), z.into()))
                            .normalize()
                    })
                    .collect(),
                None => vec![Vec3::ZERO; positions.len()],
            };
            let count = positions.len() as u32;
            out.positions.extend(positions);
            out.normals.extend(normals);
            let indices: Vec<u32> = match reader.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..count).collect(),
            };
            for t in indices.chunks_exact(3) {
                out.triangles.push([base + t[0], base + t[1], base + t[2]]);
            }
            if color.is_none() {
                let c = prim.material().pbr_metallic_roughness().base_color_factor();
                *color = Some(c.map(f64::from));
            }
        }
    }
    for child in node.children() {
        walk(&child, world, buffers, out, color);
    }
}

#[cfg(feature = "gltf")]
fn data_uri(uri: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let data = uri.strip_prefix("data:")?;
    let (_, b64) = data.split_once(";base64,")?;
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

/// A shared, read-only mesh.
pub type SharedGeometry = Rc<Geometry>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_are_closed_and_face_out() {
        for g in [
            Geometry::cube(2.0),
            Geometry::sphere(1.0, 8),
            Geometry::cylinder(1.0, 2.0, 12),
        ] {
            assert_eq!(g.positions.len(), g.normals.len());
            // Every triangle's winding agrees with its vertex normals.
            for t in &g.triangles {
                let [a, b, c] = t.map(|i| g.positions[i as usize]);
                let face = (b - a).cross(c - a);
                let n =
                    g.normals[t[0] as usize] + g.normals[t[1] as usize] + g.normals[t[2] as usize];
                assert!(face.dot(n) > 0.0, "{t:?}");
            }
        }
        let (lo, hi) = Geometry::cube(2.0).bounds();
        assert_eq!((lo, hi), (Vec3::splat(-1.0), Vec3::splat(1.0)));
    }

    #[test]
    fn fit_centers_and_scales() {
        let mut g = Geometry::cube(4.0);
        for p in &mut g.positions {
            *p = *p + Vec3::new(10.0, 0.0, 0.0);
        }
        let (lo, hi) = g.fit(1.0).bounds();
        assert!((lo + hi).length() < 1e-9 && (hi.x - lo.x - 1.0).abs() < 1e-9);
    }
}
