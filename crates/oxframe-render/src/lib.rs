//! Renderers turn a [`SceneGraph`] into an RGBA image.
//!
//! > **Status:** oxframe is under very active development. The API is not stable yet and
//! > may change from version to version. Pin the exact version in your Cargo.toml and check
//! > the [changelog](https://github.com/oxygenframe/oxframe-sdk/blob/main/CHANGELOG.md)
//! > before upgrading.
//!
//! [`CpuRenderer`] (tiny-skia) is the reference backend and the `--cpu`
//! mode for machines without a GPU; `GpuRenderer` (vello, feature `gpu`) is
//! the fast path. Both consume the same [`oxframe_2d::DrawItem`] list and pass
//! the same golden tests within a ΔE tolerance.

#![warn(missing_docs)]

use kurbo::Affine;
use oxframe_2d::{DrawItem, Primitive, SceneGraph};
use oxframe_core::Color;
pub use oxframe_core::Image;

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
    /// Draws the current state of `graph` and returns the picture.
    ///
    /// A renderer that presents straight to a surface may return an empty
    /// image after showing the frame. Transitions need the pixels, so they
    /// call [`render_pixels`](Self::render_pixels) and then
    /// [`present_image`](Self::present_image).
    fn render(&mut self, graph: &SceneGraph, options: &RenderOptions) -> Image;

    /// Draws `items`, a part of `graph`'s draw list, and returns the pixels
    /// without presenting. `graph` gives the scene size.
    ///
    /// The engine calls this for a frame where a shader reads drawn pixels
    /// (`lens_drops`, an effect): it draws the list in parts and runs the
    /// shader between them.
    fn render_list(
        &mut self,
        graph: &SceneGraph,
        items: &[DrawItem],
        options: &RenderOptions,
    ) -> Image;

    /// Draws `graph` and returns the pixels, without presenting.
    ///
    /// The default is [`render`](Self::render), which already returns pixels.
    fn render_pixels(&mut self, graph: &SceneGraph, options: &RenderOptions) -> Image {
        self.render(graph, options)
    }

    /// Shows a frame that was blended from pixels. The default does nothing,
    /// because [`render`](Self::render) already showed the frame.
    fn present_image(&mut self, _image: &Image) {}
}

/// Ids of the images `items` draw. A renderer keeps its decoded copies of
/// these and drops the rest, so a shader that paints a new image every
/// frame does not grow the cache.
#[cfg(any(feature = "cpu", feature = "gpu"))]
pub(crate) fn image_ids(items: &[DrawItem]) -> std::collections::BTreeSet<u64> {
    items
        .iter()
        .filter_map(|item| match &item.prim {
            Primitive::Image { image, .. } => Some(image.id),
            _ => None,
        })
        .collect()
}

/// Maps scene coordinates (origin at the center, y down) to output pixels.
pub fn scene_to_pixels(scene: (u32, u32), out: (u32, u32)) -> Affine {
    let (sw, sh) = (f64::from(scene.0), f64::from(scene.1));
    let (ow, oh) = (f64::from(out.0), f64::from(out.1));
    let k = (ow / sw).min(oh / sh);
    Affine::translate((ow / 2.0, oh / 2.0)) * Affine::scale(k)
}
