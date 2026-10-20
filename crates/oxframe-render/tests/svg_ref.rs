//! Imported SVGs against resvg. Mean ΔE ≤ 1, and at most 1% of pixels above ΔE 10.
//! Anti-aliasing can differ; the shapes, paints and transforms should not.

use oxframe_2d::{SceneGraph, Svg, View};
use oxframe_core::Color;
use oxframe_render::{CpuRenderer, Image, RenderOptions, Renderer};

const LUCIDE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#111111" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <path d="M20 6 L9 17 L4 12"/>
</svg>"##;

const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="60" viewBox="0 0 120 60">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="#e13238"/>
      <stop offset="1" stop-color="#68abdf"/>
    </linearGradient>
  </defs>
  <g id="mark" transform="translate(16,10) rotate(8)">
    <rect id="bar" x="0" y="0" width="36" height="36" rx="4" fill="url(#g)"/>
    <circle id="dot" cx="64" cy="18" r="10" fill="#f5c242"/>
  </g>
</svg>"##;

const GROUPS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80" viewBox="0 0 80 80">
  <g id="a" transform="translate(8,12)">
    <g id="b" transform="scale(1.5) rotate(-12)">
      <rect x="0" y="0" width="16" height="16" fill="#222222"/>
      <circle cx="28" cy="8" r="7" fill="#2ecc71"/>
    </g>
  </g>
</svg>"##;

#[test]
fn matches_resvg_within_delta_e() {
    for (name, svg) in [("lucide", LUCIDE), ("logo", LOGO), ("groups", GROUPS)] {
        let d = diff(&render_oxframe(svg), &render_resvg(svg));
        eprintln!("{name}: mean ΔE {:.3}, >10: {:.2}%", d.0, d.1 * 100.0);
        assert!(
            d.0 <= 1.0 && d.1 <= 0.01,
            "{name}: mean ΔE {:.3}, >10: {:.2}%",
            d.0,
            d.1 * 100.0
        );
    }
}

fn render_oxframe(svg: &str) -> Image {
    let (w, h) = size(svg);
    let graph = SceneGraph::new(w, h);
    graph.enter(|| {
        View::current().add(Svg::new().src(svg.as_bytes()));
    });
    CpuRenderer::new().render(
        &graph,
        &RenderOptions {
            width: w,
            height: h,
            background: Color::WHITE,
        },
    )
}

fn render_resvg(svg: &str) -> Image {
    let (w, h) = size(svg);
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
        .expect("resvg parse");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).expect("pixmap");
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for px in pixmap.pixels() {
        let c = px.demultiply();
        data.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Image {
        width: w,
        height: h,
        data,
    }
}

fn size(svg: &str) -> (u32, u32) {
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
        .expect("size");
    (
        tree.size().width().round() as u32,
        tree.size().height().round() as u32,
    )
}

fn oklab(px: &[u8]) -> [f32; 3] {
    let a = f32::from(px[3]) / 255.0;
    Color::rgb(
        f32::from(px[0]) / 255.0 * a,
        f32::from(px[1]) / 255.0 * a,
        f32::from(px[2]) / 255.0 * a,
    )
    .to_oklab()
}

fn diff(a: &Image, b: &Image) -> (f64, f64) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let mut sum = 0.0;
    let mut over = 0usize;
    let n = (a.width * a.height) as usize;
    for (p, q) in a.data.chunks_exact(4).zip(b.data.chunks_exact(4)) {
        let (x, y) = (oklab(p), oklab(q));
        let d = 100.0
            * (((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)) as f64)
                .sqrt();
        sum += d;
        if d > 10.0 {
            over += 1;
        }
    }
    (sum / n as f64, over as f64 / n as f64)
}
