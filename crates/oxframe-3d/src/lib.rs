//! 3D for oxframe: a world of meshes, a camera and lights, rendered into a
//! picture that the 2D scene shows like any other node (`View3d`, feature
//! `3d` of `oxframe`).
//!
//! Every property is a signal, so `.to()` animates a position, a rotation,
//! a color or the camera. This crate depends only on `oxframe-core`; the
//! 2D crates never depend on it.
//!
//! The renderer is a software rasterizer with a depth buffer and
//! supersampling: the same scene gives the same pixels in a render, in
//! `check`, in CI without a GPU and in the browser.

#![warn(missing_docs)]

mod math;
mod mesh;
mod raster;
mod scene;

pub use math::{Quat, Vec3};
pub use mesh::Geometry;
#[cfg(feature = "gltf")]
pub use mesh::load_glb;
pub use raster::Frame;
pub use scene::{Camera3d, Light, Light3d, Mesh, Object3d, Scene3d};
