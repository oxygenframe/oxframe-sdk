use std::cell::RefCell;
use std::rc::Rc;

use kurbo::{Affine, BezPath};
use oxframe_core::Color;

/// Decoded raster image: sRGB RGBA8, straight alpha.
#[derive(Debug)]
pub struct RasterImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, row by row.
    pub rgba: Vec<u8>,
    /// Stable identity for renderer-side caches.
    pub id: u64,
}

/// SVG converted to filled/stroked paths (gradients become their first
/// stop color in v1).
#[derive(Debug)]
pub struct VectorImage {
    /// Width from the SVG's view box.
    pub width: f64,
    /// Height from the SVG's view box.
    pub height: f64,
    /// Shapes in painting order.
    pub shapes: Vec<VectorShape>,
}

/// One painted path of a [`VectorImage`].
#[derive(Debug, Clone)]
pub struct VectorShape {
    /// Outline in image coordinates.
    pub path: BezPath,
    /// Fill color, if filled.
    pub fill: Option<Color>,
    /// Stroke color and width, if stroked.
    pub stroke: Option<(Color, f64)>,
}

/// Decoded image data for an `Img` node.
#[derive(Debug, Clone)]
pub enum ImageSource {
    /// PNG or JPEG pixels.
    Raster(Rc<RasterImage>),
    /// SVG paths, drawn as vectors at any scale.
    Vector(Rc<VectorImage>),
}

impl ImageSource {
    /// Size in pixels (raster) or view-box units (SVG).
    pub fn natural_size(&self) -> (f64, f64) {
        match self {
            Self::Raster(r) => (f64::from(r.width), f64::from(r.height)),
            Self::Vector(v) => (v.width, v.height),
        }
    }

    /// Decodes PNG, JPEG or SVG bytes. Decoded images are cached, so
    /// re-running a scene (seek) does not decode again.
    ///
    /// # Errors
    ///
    /// A description of the problem if the data is not a supported image.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let key = fnv(bytes);
        if let Some(hit) = CACHE.with(|c| {
            c.borrow()
                .iter()
                .find(|(k, _)| *k == key)
                .map(|e| e.1.clone())
        }) {
            return Ok(hit);
        }
        let src = if looks_like_svg(bytes) {
            Self::Vector(Rc::new(decode_svg(bytes)?))
        } else {
            let img = image::load_from_memory(bytes)
                .map_err(|e| e.to_string())?
                .to_rgba8();
            Self::Raster(Rc::new(RasterImage {
                width: img.width(),
                height: img.height(),
                rgba: img.into_raw(),
                id: key,
            }))
        };
        CACHE.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() >= 64 {
                c.remove(0);
            }
            c.push((key, src.clone()));
        });
        Ok(src)
    }
}

thread_local! {
    static CACHE: RefCell<Vec<(u64, ImageSource)>> = const { RefCell::new(Vec::new()) };
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h = (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
    }
    h ^ bytes.len() as u64
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(512)];
    let text = String::from_utf8_lossy(head);
    let t = text.trim_start_matches('\u{feff}').trim_start();
    t.starts_with('<') && (t.contains("<svg") || t.starts_with("<?xml"))
}

fn decode_svg(bytes: &[u8]) -> Result<VectorImage, String> {
    let tree =
        usvg::Tree::from_data(bytes, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let size = tree.size();
    let mut shapes = Vec::new();
    collect(tree.root(), 1.0, &mut shapes);
    Ok(VectorImage {
        width: f64::from(size.width()),
        height: f64::from(size.height()),
        shapes,
    })
}

fn collect(group: &usvg::Group, opacity: f32, out: &mut Vec<VectorShape>) {
    let opacity = opacity * group.opacity().get();
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => collect(g, opacity, out),
            usvg::Node::Path(p) if p.is_visible() => {
                let t = p.abs_transform();
                let tf = Affine::new([t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(f64::from));
                let path = tf * to_bez(p.data());
                let paint = |paint: &usvg::Paint, a: f32| {
                    let c = match paint {
                        usvg::Paint::Color(c) => *c,
                        usvg::Paint::LinearGradient(g) => g.stops().first().map(|s| s.color())?,
                        usvg::Paint::RadialGradient(g) => g.stops().first().map(|s| s.color())?,
                        usvg::Paint::Pattern(_) => return None,
                    };
                    Some(Color::from_rgba8(
                        c.red,
                        c.green,
                        c.blue,
                        (a * opacity * 255.0).round() as u8,
                    ))
                };
                let fill = p.fill().and_then(|f| paint(f.paint(), f.opacity().get()));
                let scale = (tf.determinant().abs()).sqrt();
                let stroke = p.stroke().and_then(|s| {
                    paint(s.paint(), s.opacity().get())
                        .map(|c| (c, f64::from(s.width().get()) * scale))
                });
                out.push(VectorShape { path, fill, stroke });
            }
            _ => {}
        }
    }
}

fn to_bez(p: &usvg::tiny_skia_path::Path) -> BezPath {
    use usvg::tiny_skia_path::PathSegment;
    let pt = |p: usvg::tiny_skia_path::Point| (f64::from(p.x), f64::from(p.y));
    let mut out = BezPath::new();
    for seg in p.segments() {
        match seg {
            PathSegment::MoveTo(a) => out.move_to(pt(a)),
            PathSegment::LineTo(a) => out.line_to(pt(a)),
            PathSegment::QuadTo(a, b) => out.quad_to(pt(a), pt(b)),
            PathSegment::CubicTo(a, b, c) => out.curve_to(pt(a), pt(b), pt(c)),
            PathSegment::Close => out.close_path(),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_svg_to_paths() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
            <rect x="0" y="0" width="10" height="10" fill="#ff0000"/>
            <circle cx="15" cy="5" r="5" fill="none" stroke="#00ff00" stroke-width="2"/>
        </svg>"##;
        let ImageSource::Vector(v) = ImageSource::decode(svg).unwrap() else {
            panic!("expected vector")
        };
        assert_eq!((v.width, v.height), (20.0, 10.0));
        assert_eq!(v.shapes.len(), 2);
        assert_eq!(v.shapes[0].fill.unwrap().to_rgba8(), [255, 0, 0, 255]);
        assert!(v.shapes[1].fill.is_none());
        assert_eq!(v.shapes[1].stroke.unwrap().1, 2.0);
    }

    #[test]
    fn decodes_png() {
        let mut png = Vec::new();
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255]));
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let ImageSource::Raster(r) = ImageSource::decode(&png).unwrap() else {
            panic!("expected raster")
        };
        assert_eq!((r.width, r.height), (3, 2));
        assert_eq!(&r.rgba[..4], &[1, 2, 3, 255]);
    }
}
