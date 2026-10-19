//! Renderers turn a [`SceneGraph`] into an RGBA image.
//!
//! [`CpuRenderer`] (tiny-skia) is the reference backend and the `--cpu`
//! mode for machines without a GPU; `GpuRenderer` (vello, feature `gpu`) is
//! the fast path. Both consume the same [`oxframe_2d::DrawItem`] list and pass
//! the same golden tests within a ΔE tolerance.

#![warn(missing_docs)]

use kurbo::Affine;
use oxframe_2d::SceneGraph;
use oxframe_core::Color;

#[cfg(feature = "cpu")]
mod blur;
#[cfg(feature = "cpu")]
mod cpu;
#[cfg(feature = "cpu")]
pub use cpu::CpuRenderer;

#[cfg(feature = "gpu")]
mod gpu;
#[cfg(feature = "gpu")]
pub use gpu::{GpuError, GpuRenderer};

/// 8-bit sRGB RGBA, straight alpha, row-major, no padding.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

/// Size and background of one render call.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Output size; the scene is scaled uniformly to fit and centered.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// Color drawn under the scene.
    pub background: Color,
}

/// Draws a scene graph into an image.
pub trait Renderer {
    /// Draws the current state of `graph`.
    fn render(&mut self, graph: &SceneGraph, options: &RenderOptions) -> Image;
}

/// Maps scene coordinates (origin at the center, y down) to output pixels.
pub fn scene_to_pixels(scene: (u32, u32), out: (u32, u32)) -> Affine {
    let (sw, sh) = (f64::from(scene.0), f64::from(scene.1));
    let (ow, oh) = (f64::from(out.0), f64::from(out.1));
    let k = (ow / sw).min(oh / sh);
    Affine::translate((ow / 2.0, oh / 2.0)) * Affine::scale(k)
}
