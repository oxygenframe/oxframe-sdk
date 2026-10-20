//! Frames as pixels, and the one interface through which the editor, the
//! CLI and the exporter get them: [`FrameSource`]. None of them needs to
//! know what draws the frames (2D nodes today, other modules later).

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

impl Image {
    /// Encodes the image as PNG (8-bit RGBA, straight alpha).
    ///
    /// # Panics
    ///
    /// If `data` is not `width * height * 4` bytes — a bug in whoever
    /// built the image.
    #[must_use]
    pub fn encode_png(&self) -> Vec<u8> {
        assert_eq!(
            self.data.len(),
            self.width as usize * self.height as usize * 4,
            "Image data must be width * height * 4 bytes"
        );
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("in-memory PNG header");
        writer
            .write_image_data(&self.data)
            .expect("in-memory PNG data");
        writer.finish().expect("in-memory PNG");
        out
    }
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

/// A timeline that can draw any of its frames: "give me frame `n`".
///
/// Frames are addressed by index (time `n / fps`) so that every caller
/// gets exactly the same pixels for the same frame, in any order.
pub trait FrameSource {
    /// Frames per second.
    fn fps(&self) -> u32;
    /// Native size in pixels.
    fn size(&self) -> (u32, u32);
    /// Length in frames.
    fn total_frames(&self) -> u32;
    /// Draws `frame` at `size` pixels (scaled to fit and centered).
    fn render(&mut self, frame: u32, size: (u32, u32)) -> Image;
}
