//! A 3D world: objects, a camera and lights, all made of signals.

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use oxframe_core::{Color, Signal};

use crate::math::{Mat4, Quat, Vec3};
use crate::mesh::{Geometry, SharedGeometry};
use crate::raster::{self, Frame, LightDraw, Look, ObjectDraw};

/// What an object looks like. Metallic and roughness follow the glTF
/// metal-rough model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Material {
    pub color: Color,
    pub metallic: f64,
    pub roughness: f64,
    pub emissive: Color,
    pub opacity: f64,
}

/// A mesh with its initial transform and material, before it is added
/// with [`Scene3d::add`] or [`Object3d::add`].
#[must_use = "a mesh does nothing until it is added to a scene"]
pub struct Mesh {
    geometry: Option<SharedGeometry>,
    position: Vec3,
    rotation: Vec3,
    scale: Vec3,
    material: Material,
    name: String,
}

impl Mesh {
    fn with(geometry: Option<Geometry>, name: &str) -> Mesh {
        Mesh {
            geometry: geometry.map(Rc::new),
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
            material: Material {
                color: Color::rgb(0.85, 0.85, 0.88),
                metallic: 0.0,
                roughness: 0.5,
                emissive: Color::BLACK,
                opacity: 1.0,
            },
            name: name.to_owned(),
        }
    }

    /// A cube, 1 unit on each side, centered on its position.
    pub fn cube() -> Mesh {
        Mesh::with(Some(Geometry::cube(1.0)), "cube")
    }

    /// A sphere of radius 0.5 (1 unit across).
    pub fn sphere() -> Mesh {
        Mesh::with(Some(Geometry::sphere(0.5, 24)), "sphere")
    }

    /// A flat square, 1 unit wide, lying on the floor (facing +Y).
    pub fn plane() -> Mesh {
        Mesh::with(Some(Geometry::plane(1.0)), "plane")
    }

    /// A cylinder along Y, 1 unit across and 1 unit tall.
    pub fn cylinder() -> Mesh {
        Mesh::with(Some(Geometry::cylinder(0.5, 1.0, 32)), "cylinder")
    }

    /// Nothing to draw: a parent that moves its children together.
    pub fn group() -> Mesh {
        Mesh::with(None, "group")
    }

    /// Triangles of your own.
    pub fn geometry(geometry: Geometry) -> Mesh {
        Mesh::with(Some(geometry), "mesh")
    }

    /// A model from a binary glTF (`.glb`), fitted into a 1-unit box. Its
    /// base color becomes the mesh color; textures are not read.
    ///
    /// # Panics
    ///
    /// If the bytes are not a glTF with triangles.
    #[cfg(feature = "gltf")]
    pub fn glb(bytes: &[u8]) -> Mesh {
        let (geometry, color) =
            crate::mesh::load_glb(bytes).unwrap_or_else(|e| panic!("cannot read the model: {e}"));
        let mut m = Mesh::with(Some(geometry.fit(1.0)), "model");
        if let Some([r, g, b, a]) = color {
            m.material.color = Color::rgba(r as f32, g as f32, b as f32, a as f32);
        }
        m
    }

    /// Reads a `.glb` file (native only).
    ///
    /// # Panics
    ///
    /// If the file cannot be read or is not a glTF with triangles.
    #[cfg(all(feature = "gltf", not(target_arch = "wasm32")))]
    pub fn glb_file(path: impl AsRef<std::path::Path>) -> Mesh {
        let path = path.as_ref();
        let bytes =
            std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        Mesh::glb(&bytes)
    }

    /// Where it is, relative to its parent.
    pub fn position(mut self, p: impl Into<Vec3>) -> Self {
        self.position = p.into();
        self
    }

    /// Turn in degrees around X, then Y, then Z.
    pub fn rotation(mut self, degrees: impl Into<Vec3>) -> Self {
        self.rotation = degrees.into();
        self
    }

    /// The same scale on every axis.
    pub fn scale(mut self, k: f64) -> Self {
        self.scale = Vec3::splat(k);
        self
    }

    /// A scale per axis.
    pub fn scale3(mut self, k: impl Into<Vec3>) -> Self {
        self.scale = k.into();
        self
    }

    /// Surface color. A theme role such as `Accent` works too.
    pub fn color(mut self, c: impl Into<Color>) -> Self {
        self.material.color = c.into();
        self
    }

    /// 0 is plastic or paint, 1 is metal.
    pub fn metallic(mut self, v: f64) -> Self {
        self.material.metallic = v.clamp(0.0, 1.0);
        self
    }

    /// 0 is a mirror finish, 1 is matte. Default 0.5.
    pub fn roughness(mut self, v: f64) -> Self {
        self.material.roughness = v.clamp(0.02, 1.0);
        self
    }

    /// Light the surface gives off by itself.
    pub fn emissive(mut self, c: impl Into<Color>) -> Self {
        self.material.emissive = c.into();
        self
    }

    /// 1 is solid, 0 is invisible.
    pub fn opacity(mut self, v: f64) -> Self {
        self.material.opacity = v.clamp(0.0, 1.0);
        self
    }

    /// Name shown in `inspect`.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

pub(crate) struct ObjectData {
    pub geometry: Option<SharedGeometry>,
    pub parent: Option<usize>,
    pub name: String,
    pub props: Props,
}

#[derive(Clone)]
pub(crate) struct Props {
    pub position: Signal<Vec3>,
    pub rotation: Signal<Vec3>,
    pub orientation: Signal<Quat>,
    pub scale: Signal<Vec3>,
    pub color: Signal<Color>,
    pub metallic: Signal<f64>,
    pub roughness: Signal<f64>,
    pub emissive: Signal<Color>,
    pub opacity: Signal<f64>,
}

/// Handle to an object in a [`Scene3d`]. Every property is a signal, so
/// `cube.rotation().to(Vec3::new(0.0, 360.0, 0.0), 2.0)` turns it.
#[derive(Clone)]
pub struct Object3d {
    scene: Scene3d,
    index: usize,
    props: Props,
}

impl Object3d {
    /// Position relative to the parent.
    pub fn position(&self) -> Signal<Vec3> {
        self.props.position.clone()
    }
    /// Turn in degrees around X, then Y, then Z. Animating past 360 turns
    /// more than once.
    pub fn rotation(&self) -> Signal<Vec3> {
        self.props.rotation.clone()
    }
    /// A rotation applied after [`rotation`](Self::rotation); animates
    /// along the shortest arc.
    pub fn orientation(&self) -> Signal<Quat> {
        self.props.orientation.clone()
    }
    /// Scale per axis.
    pub fn scale(&self) -> Signal<Vec3> {
        self.props.scale.clone()
    }
    /// Surface color.
    pub fn color(&self) -> Signal<Color> {
        self.props.color.clone()
    }
    /// 0 is plastic, 1 is metal.
    pub fn metallic(&self) -> Signal<f64> {
        self.props.metallic.clone()
    }
    /// 0 is a mirror finish, 1 is matte.
    pub fn roughness(&self) -> Signal<f64> {
        self.props.roughness.clone()
    }
    /// Light given off by the surface.
    pub fn emissive(&self) -> Signal<Color> {
        self.props.emissive.clone()
    }
    /// 1 is solid, 0 is invisible; children multiply it.
    pub fn opacity(&self) -> Signal<f64> {
        self.props.opacity.clone()
    }

    /// Adds `mesh` as a child: it moves, turns and scales with this one.
    pub fn add(&self, mesh: Mesh) -> Object3d {
        self.scene.insert(mesh, Some(self.index))
    }
}

/// The camera of a [`Scene3d`]: where it is, what it looks at, and how wide
/// it sees.
#[derive(Clone)]
pub struct Camera3d {
    position: Signal<Vec3>,
    target: Signal<Vec3>,
    fov: Signal<f64>,
}

impl Camera3d {
    /// Where the camera is. Default (3, 2.5, 4).
    pub fn position(&self) -> Signal<Vec3> {
        self.position.clone()
    }
    /// The point it looks at. Default the origin.
    pub fn target(&self) -> Signal<Vec3> {
        self.target.clone()
    }
    /// Vertical field of view in degrees. Default 40.
    pub fn fov(&self) -> Signal<f64> {
        self.fov.clone()
    }
}

/// A light, before it is added with [`Scene3d::light`].
#[must_use = "a light does nothing until it is added to a scene"]
pub struct Light {
    point: bool,
    vector: Vec3,
    color: Color,
    intensity: f64,
}

impl Light {
    /// Sunlight shining in `direction`, the same everywhere.
    pub fn directional(direction: impl Into<Vec3>) -> Light {
        Light {
            point: false,
            vector: direction.into(),
            color: Color::WHITE,
            intensity: 1.0,
        }
    }

    /// A bulb at `position` that fades with distance.
    pub fn point(position: impl Into<Vec3>) -> Light {
        Light {
            point: true,
            vector: position.into(),
            color: Color::WHITE,
            intensity: 4.0,
        }
    }

    /// Light color. Default white.
    pub fn color(mut self, c: impl Into<Color>) -> Self {
        self.color = c.into();
        self
    }

    /// Strength. Default 1 for sunlight, 4 for a bulb.
    pub fn intensity(mut self, v: f64) -> Self {
        self.intensity = v.max(0.0);
        self
    }
}

/// Handle to a light; its properties are signals.
#[derive(Clone)]
pub struct Light3d {
    vector: Signal<Vec3>,
    color: Signal<Color>,
    intensity: Signal<f64>,
}

impl Light3d {
    /// Direction for sunlight, position for a bulb.
    pub fn vector(&self) -> Signal<Vec3> {
        self.vector.clone()
    }
    /// Light color.
    pub fn color(&self) -> Signal<Color> {
        self.color.clone()
    }
    /// Strength.
    pub fn intensity(&self) -> Signal<f64> {
        self.intensity.clone()
    }
}

struct LightData {
    point: bool,
    handle: Light3d,
}

struct Inner {
    objects: RefCell<Vec<ObjectData>>,
    lights: RefCell<Vec<LightData>>,
    camera: Camera3d,
    ambient: Signal<Color>,
    background: Signal<Color>,
}

/// A 3D world. Show it in a 2D scene with `View3d` (feature `3d` of
/// `oxframe`).
///
/// ```ignore
/// let world = Scene3d::new();
/// let cube = world.add(Mesh::cube().color(Accent));
/// view.add(View3d::new(&world).size(600.0, 400.0));
/// cube.rotation().to(Vec3::new(0.0, 360.0, 0.0), 2.0).await;
/// ```
///
/// With no light added, sunlight from above and to the side is used, so
/// each face of a box has its own shade.
#[derive(Clone)]
pub struct Scene3d(Rc<Inner>);

impl Default for Scene3d {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene3d {
    /// An empty world with a camera at (3, 2.5, 4) looking at the origin.
    pub fn new() -> Scene3d {
        Scene3d(Rc::new(Inner {
            objects: RefCell::new(Vec::new()),
            lights: RefCell::new(Vec::new()),
            camera: Camera3d {
                position: Signal::new(Vec3::new(3.0, 2.5, 4.0)).with_name("Camera3d.position"),
                target: Signal::new(Vec3::ZERO).with_name("Camera3d.target"),
                fov: Signal::new(40.0).with_name("Camera3d.fov"),
            },
            ambient: Signal::new(Color::rgb(0.42, 0.44, 0.5)).with_name("Scene3d.ambient"),
            background: Signal::new(Color::TRANSPARENT).with_name("Scene3d.background"),
        }))
    }

    /// Adds `mesh` at the top level.
    pub fn add(&self, mesh: Mesh) -> Object3d {
        self.insert(mesh, None)
    }

    fn insert(&self, mesh: Mesh, parent: Option<usize>) -> Object3d {
        let m = mesh.material;
        let props = Props {
            position: Signal::new(mesh.position),
            rotation: Signal::new(mesh.rotation),
            orientation: Signal::new(Quat::IDENTITY),
            scale: Signal::new(mesh.scale),
            color: Signal::new(m.color),
            metallic: Signal::new(m.metallic),
            roughness: Signal::new(m.roughness),
            emissive: Signal::new(m.emissive),
            opacity: Signal::new(m.opacity),
        };
        let mut objects = self.0.objects.borrow_mut();
        objects.push(ObjectData {
            geometry: mesh.geometry,
            parent,
            name: mesh.name,
            props: props.clone(),
        });
        Object3d {
            scene: self.clone(),
            index: objects.len() - 1,
            props,
        }
    }

    /// Adds a light. The first one replaces the default sunlight.
    pub fn light(&self, light: Light) -> Light3d {
        let handle = Light3d {
            vector: Signal::new(light.vector),
            color: Signal::new(light.color),
            intensity: Signal::new(light.intensity),
        };
        self.0.lights.borrow_mut().push(LightData {
            point: light.point,
            handle: handle.clone(),
        });
        handle
    }

    /// The camera.
    pub fn camera(&self) -> Camera3d {
        self.0.camera.clone()
    }

    /// Light that reaches every surface, so shadowed sides are not black.
    pub fn ambient(&self) -> Signal<Color> {
        self.0.ambient.clone()
    }

    /// Behind the objects. Transparent by default, so the 2D scene shows
    /// through.
    pub fn background(&self) -> Signal<Color> {
        self.0.background.clone()
    }

    /// Names of the objects, in the order they were added.
    pub fn names(&self) -> Vec<String> {
        self.0
            .objects
            .borrow()
            .iter()
            .map(|o| o.name.clone())
            .collect()
    }

    /// Everything the renderer needs, read from the signals now.
    pub(crate) fn look(&self) -> Look {
        let objects = self.0.objects.borrow();
        let mut worlds: Vec<(Mat4, f64)> = Vec::with_capacity(objects.len());
        let mut draws = Vec::new();
        for o in objects.iter() {
            let p = &o.props;
            let local = Mat4::trs(
                p.position.get(),
                p.orientation.get() * Quat::from_euler(p.rotation.get()),
                p.scale.get(),
            );
            let (parent, parent_opacity) = o.parent.map_or((Mat4::IDENTITY, 1.0), |i| worlds[i]);
            let world = parent.mul(&local);
            let opacity = parent_opacity * p.opacity.get().clamp(0.0, 1.0);
            worlds.push((world, opacity));
            if let Some(g) = &o.geometry {
                draws.push(ObjectDraw {
                    geometry: g.clone(),
                    world,
                    material: Material {
                        color: p.color.get(),
                        metallic: p.metallic.get().clamp(0.0, 1.0),
                        roughness: p.roughness.get().clamp(0.02, 1.0),
                        emissive: p.emissive.get(),
                        opacity,
                    },
                });
            }
        }
        let lights = self.0.lights.borrow();
        let lights = if lights.is_empty() {
            vec![LightDraw {
                point: false,
                vector: Vec3::new(-2.0, -4.0, -1.0),
                color: Color::WHITE,
                intensity: 1.6,
            }]
        } else {
            lights
                .iter()
                .map(|l| LightDraw {
                    point: l.point,
                    vector: l.handle.vector.get(),
                    color: l.handle.color.get(),
                    intensity: l.handle.intensity.get(),
                })
                .collect()
        };
        let cam = &self.0.camera;
        Look {
            objects: draws,
            lights,
            eye: cam.position.get(),
            target: cam.target.get(),
            fov: cam.fov.get().clamp(1.0, 170.0),
            ambient: self.0.ambient.get(),
            background: self.0.background.get(),
        }
    }

    /// Renders the world at `width` × `height` pixels with `samples` ×
    /// `samples` supersampling. Straight-alpha sRGB RGBA, row by row.
    pub fn render(&self, width: u32, height: u32, samples: u32) -> Frame {
        raster::render(&self.look(), width, height, samples)
    }

    /// Where world point `p` lands on a picture with this `aspect` (width
    /// over height): x and y from -1 to 1 across the picture, y up. `None`
    /// behind the camera.
    pub fn project(&self, p: Vec3, aspect: f64) -> Option<(f64, f64)> {
        let cam = &self.0.camera;
        let view = Mat4::look_at(cam.position.get(), cam.target.get(), Vec3::Y);
        let proj = Mat4::perspective(
            cam.fov.get().clamp(1.0, 170.0),
            aspect.max(1e-6),
            0.01,
            1000.0,
        );
        let [x, y, _, w] = proj.mul(&view).project(p);
        (w > 1e-6).then(|| (x / w, y / w))
    }

    /// The world position of point `local` of `object`, through its
    /// parents, now.
    pub fn world_point(&self, object: &Object3d, local: Vec3) -> Vec3 {
        let objects = self.0.objects.borrow();
        let mut m = Mat4::IDENTITY;
        let mut at = Some(object.index);
        let mut chain = Vec::new();
        while let Some(i) = at {
            chain.push(i);
            at = objects[i].parent;
        }
        for i in chain.into_iter().rev() {
            let p = &objects[i].props;
            m = m.mul(&Mat4::trs(
                p.position.get(),
                p.orientation.get() * Quat::from_euler(p.rotation.get()),
                p.scale.get(),
            ));
        }
        m.point(local)
    }

    /// A hash of everything that changes the picture, for caches.
    pub fn fingerprint(&self, width: u32, height: u32, samples: u32) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (width, height, samples).hash(&mut h);
        self.look().hash_into(&mut h);
        h.finish()
    }
}
