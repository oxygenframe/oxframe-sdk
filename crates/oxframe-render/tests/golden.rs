//! Golden frames: every backend must match the reference PNGs in
//! `tests/golden/` within a perceptual tolerance.
//!
//! ΔE is the Euclidean distance in OKLab scaled by 100 (≈ CIE ΔE units).
//! Anti-aliasing differs between rasterizers along edges, so the criterion
//! is: mean ΔE ≤ 1.0 over the frame and ≤ 1 % of pixels above ΔE 10.
//!
//! Regenerate the references with `OXFRAME_UPDATE_GOLDEN=1 cargo test -p
//! oxframe-render --test golden` (CPU renderer is the reference).

use std::path::PathBuf;
use std::rc::Rc;

use kurbo::{BezPath, Shape};
use oxframe_2d::*;
use oxframe_core::{BlendMode, Color, Gradient, Lerp};
use oxframe_render::{CpuRenderer, Image, RenderOptions, Renderer};

const W: u32 = 320;
const H: u32 = 180;

fn hex(s: &str) -> Color {
    Color::from_hex(s).unwrap()
}

fn build(f: impl FnOnce(View)) -> Rc<SceneGraph> {
    let g = SceneGraph::new(640, 360);
    g.enter(|| f(View::current()));
    g
}

fn scenes() -> Vec<(&'static str, Rc<SceneGraph>)> {
    let png = {
        let mut v = Vec::new();
        let img = image::RgbaImage::from_fn(8, 8, |x, y| {
            if (x + y) % 2 == 0 {
                image::Rgba([255, 200, 0, 255])
            } else {
                image::Rgba([0, 80, 255, 255])
            }
        });
        img.write_to(&mut std::io::Cursor::new(&mut v), image::ImageFormat::Png)
            .unwrap();
        v
    };
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <circle cx="50" cy="50" r="40" fill="#22aa66"/>
        <path d="M30 50 L45 65 L72 35" fill="none" stroke="#ffffff" stroke-width="8"/>
    </svg>"##;
    vec![
        (
            "shapes",
            build(|v| {
                v.add(
                    Rect::new()
                        .width(160.0)
                        .height(100.0)
                        .x(-180.0)
                        .fill(hex("#e13238")),
                );
                v.add(
                    Rect::new()
                        .size(120.0)
                        .radius(30.0)
                        .fill(hex("#68abdf"))
                        .stroke(Color::WHITE)
                        .line_width(8.0),
                );
                v.add(
                    Circle::new()
                        .size(120.0)
                        .x(180.0)
                        .fill(hex("#f5c242"))
                        .opacity(0.6),
                );
                v.add(
                    Group::new()
                        .y(130.0)
                        .rotation(15.0)
                        .scale(0.8)
                        .child(Rect::new().width(400.0).height(20.0).fill(hex("#9b59b6"))),
                );
            }),
        ),
        (
            "text",
            build(|v| {
                v.add(
                    Txt::new()
                        .text("Oxframe 2D")
                        .font_size(72.0)
                        .weight(700.0)
                        .y(-60.0),
                );
                v.add(
                    Txt::new()
                        .text("Привет, мир — fn main() {}")
                        .family(MONO)
                        .font_size(32.0)
                        .y(40.0)
                        .fill(hex("#68abdf")),
                );
                v.add(
                    Txt::new()
                        .text("half drawn")
                        .font_size(40.0)
                        .y(120.0)
                        .end(0.5),
                );
            }),
        ),
        (
            "lines",
            build(|v| {
                v.add(Grid::new().width(640.0).height(360.0).spacing(40.0));
                v.add(
                    Line::new()
                        .points([(-250.0, 100.0), (-100.0, -100.0), (50.0, 100.0)])
                        .line_width(10.0)
                        .stroke(hex("#e13238"))
                        .end_arrow(true)
                        .start_arrow(true),
                );
                v.add(
                    Circle::new()
                        .size(160.0)
                        .x(180.0)
                        .fill(Color::TRANSPARENT)
                        .stroke(hex("#68abdf"))
                        .line_width(12.0)
                        .end(0.7),
                );
                let star = BezPath::from_svg(
                    "M0 -60 L18 -18 L60 -18 L26 8 L38 52 L0 26 L-38 52 L-26 8 L-60 -18 L-18 -18 Z",
                )
                .unwrap();
                let circle = kurbo::Circle::new((0.0, 0.0), 50.0).to_path(0.1);
                v.add(
                    Path::new()
                        .data(star.lerp(&circle, 0.5))
                        .fill(hex("#f5c242"))
                        .y(-100.0)
                        .x(180.0)
                        .scale(0.6),
                );
            }),
        ),
        (
            "effects",
            build(|v| {
                // Gradients and a dashed stroke.
                v.add(
                    Rect::new()
                        .width(160.0)
                        .height(100.0)
                        .position((-220.0, -90.0))
                        .radius(12.0)
                        .fill_gradient(
                            Gradient::linear((-80.0, 0.0), (80.0, 0.0))
                                .stops([hex("#e13238"), hex("#f5c242")]),
                        )
                        .stroke(Color::WHITE)
                        .line_width(4.0)
                        .dash(vec![12.0, 8.0]),
                );
                v.add(
                    Circle::new()
                        .size(110.0)
                        .position((-40.0, -90.0))
                        .fill_gradient(Gradient::radial((0.0, 0.0), 55.0).stops([
                            Color::WHITE,
                            hex("#68abdf"),
                            hex("#1b3a5c"),
                        ])),
                );
                // Group opacity: overlapping children must not show the overlap.
                v.add(
                    Group::new()
                        .position((120.0, -90.0))
                        .opacity(0.5)
                        .child(Circle::new().size(80.0).x(-25.0).fill(hex("#2ecc71")))
                        .child(Circle::new().size(80.0).x(25.0).fill(hex("#2ecc71"))),
                );
                // Clip: a tall rect inside a circle.
                v.add(
                    Circle::new()
                        .size(100.0)
                        .position((250.0, -90.0))
                        .fill(hex("#333333"))
                        .clip(true)
                        .child(
                            Rect::new()
                                .width(60.0)
                                .height(200.0)
                                .x(30.0)
                                .fill(hex("#e13238")),
                        ),
                );
                // Multiply blend over a stripe.
                v.add(
                    Rect::new()
                        .width(620.0)
                        .height(30.0)
                        .y(20.0)
                        .fill(hex("#68abdf")),
                );
                v.add(
                    Circle::new()
                        .size(80.0)
                        .position((-250.0, 20.0))
                        .fill(hex("#f5c242"))
                        .blend(BlendMode::Multiply),
                );
                // Blur, shadow, glow.
                v.add(
                    Rect::new()
                        .size(70.0)
                        .position((-120.0, 110.0))
                        .fill(hex("#f5c242"))
                        .blur(6.0),
                );
                v.add(
                    Rect::new()
                        .size(70.0)
                        .position((0.0, 110.0))
                        .radius(10.0)
                        .fill(Color::WHITE)
                        .shadow(Color::BLACK.with_alpha(0.8), 6.0, (8.0, 8.0)),
                );
                v.add(
                    Circle::new()
                        .size(60.0)
                        .position((120.0, 110.0))
                        .fill(hex("#68abdf"))
                        .glow(hex("#68abdf"), 10.0),
                );
                // z-index and anchor: the red square added later is drawn below.
                v.add(
                    Rect::new()
                        .size(60.0)
                        .position((240.0, 110.0))
                        .fill(hex("#2ecc71"))
                        .rotation(20.0),
                );
                v.add(
                    Rect::new()
                        .size(60.0)
                        .position((240.0, 110.0))
                        .fill(hex("#e13238"))
                        .z_index(-1.0)
                        .anchor((-1.0, -1.0)),
                );
            }),
        ),
        (
            "figures",
            build(|v| {
                v.add(
                    Polygon::new()
                        .sides(6.0)
                        .size(120.0)
                        .position((-230.0, -80.0))
                        .fill(hex("#68abdf")),
                );
                v.add(
                    Star::new()
                        .points(5.0)
                        .size(130.0)
                        .position((-80.0, -80.0))
                        .fill(hex("#f5c242")),
                );
                v.add(
                    Circle::new()
                        .size(120.0)
                        .position((70.0, -80.0))
                        .start_angle(0.0)
                        .end_angle(270.0)
                        .fill(Color::TRANSPARENT)
                        .stroke(hex("#e13238"))
                        .line_width(10.0)
                        .line_cap(LineCap::Butt),
                );
                v.add(
                    Circle::new()
                        .size(120.0)
                        .position((220.0, -80.0))
                        .start_angle(45.0)
                        .end_angle(315.0)
                        .pie(true)
                        .fill(hex("#2ecc71")),
                );
                v.add(
                    Line::new()
                        .points([
                            (-280.0, 100.0),
                            (-180.0, 30.0),
                            (-80.0, 130.0),
                            (20.0, 40.0),
                        ])
                        .smooth(true)
                        .stroke(hex("#68abdf"))
                        .line_width(6.0),
                );
                v.add(
                    Line::new()
                        .points([(80.0, 60.0), (200.0, 40.0), (260.0, 130.0), (120.0, 140.0)])
                        .smooth(true)
                        .closed(true)
                        .stroke(hex("#f5c242"))
                        .line_width(5.0)
                        .dash(vec![14.0, 10.0])
                        .line_cap(LineCap::Butt),
                );
            }),
        ),
        (
            "flex",
            build(|v| {
                let card = |c: &str| Rect::new().size(50.0).radius(8.0).fill(hex(c));
                // Row: fixed, growing, fixed.
                v.add(
                    Layout::new()
                        .width(560.0)
                        .gap(10.0)
                        .padding(10.0)
                        .radius(12.0)
                        .fill(hex("#2b2b2b"))
                        .y(-110.0)
                        .child(card("#e13238"))
                        .child(card("#f5c242").grow(1.0))
                        .child(card("#68abdf")),
                );
                // Wrapping row.
                let mut wrap = Layout::new()
                    .width(260.0)
                    .wrap(true)
                    .gap(8.0)
                    .padding(8.0)
                    .fill(hex("#2b2b2b"))
                    .justify(Justify::Start)
                    .position((-150.0, 40.0));
                for c in [
                    "#e13238", "#f5c242", "#68abdf", "#2ecc71", "#9b59b6", "#e67e22",
                ] {
                    wrap = wrap.child(card(c));
                }
                v.add(wrap);
                // Grid, stretched cells.
                let mut grid = Layout::new()
                    .grid(3)
                    .width(240.0)
                    .gap(8.0)
                    .padding(8.0)
                    .fill(hex("#2b2b2b"))
                    .align(LayoutAlign::Stretch)
                    .position((150.0, 40.0));
                for c in ["#e13238", "#f5c242", "#68abdf", "#2ecc71", "#9b59b6"] {
                    grid = grid.child(card(c).height(40.0));
                }
                v.add(grid);
            }),
        ),
        (
            "richtext",
            build(|v| {
                v.add(Txt::new().y(-120.0).font_size(30.0).rich([
                    span("Signals are "),
                    span("lazy").bold().color(hex("#f5c242")),
                    span(", tweens are "),
                    span("futures").italic().color(hex("#68abdf")),
                ]));
                v.add(Code::new().y(40.0).font_size(22.0).focus(2, 3).code(
                "use oxframe::prelude::*;\n#[oxframe::scene]\nasync fn intro(view: View) {\n    let r = view.add(Rect::new().size(200.0));\n    r.x().to(300.0, 1.0).await; // 1 s\n}",
            ));
            }),
        ),
        (
            "layout_images",
            build(|v| {
                v.add(
                    Layout::new()
                        .gap(20.0)
                        .padding(20.0)
                        .radius(16.0)
                        .fill(hex("#2b2b2b"))
                        .child(Img::new().src(&png).width(100.0))
                        .child(Img::new().src(svg).width(100.0))
                        .child(
                            Layout::new()
                                .column()
                                .gap(10.0)
                                .child(Rect::new().width(80.0).height(30.0).fill(hex("#e13238")))
                                .child(Rect::new().width(120.0).height(30.0).fill(hex("#68abdf"))),
                        ),
                );
            }),
        ),
    ]
}

fn oklab(px: &[u8]) -> [f32; 3] {
    // Composite over black so transparency differences count.
    let a = f32::from(px[3]) / 255.0;
    Color::rgb(
        f32::from(px[0]) / 255.0 * a,
        f32::from(px[1]) / 255.0 * a,
        f32::from(px[2]) / 255.0 * a,
    )
    .to_oklab()
}

struct Diff {
    mean: f64,
    over10: f64,
}

fn diff(a: &Image, b: &Image) -> Diff {
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
    Diff {
        mean: sum / n as f64,
        over10: over as f64 / n as f64,
    }
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn load(name: &str) -> Image {
    let path = golden_dir().join(format!("{name}.png"));
    let img = image::open(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (run with OXFRAME_UPDATE_GOLDEN=1)", path.display()))
        .to_rgba8();
    Image {
        width: img.width(),
        height: img.height(),
        data: img.into_raw(),
    }
}

fn check(backend: &str, renderer: &mut dyn Renderer) {
    let update = std::env::var_os("OXFRAME_UPDATE_GOLDEN").is_some() && backend == "cpu";
    let opts = RenderOptions {
        width: W,
        height: H,
        background: hex("#141414"),
    };
    let mut failures = Vec::new();
    for (name, graph) in scenes() {
        let img = renderer.render(&graph, &opts);
        if update {
            std::fs::create_dir_all(golden_dir()).unwrap();
            std::fs::write(golden_dir().join(format!("{name}.png")), img.encode_png()).unwrap();
            continue;
        }
        let d = diff(&img, &load(name));
        eprintln!(
            "{backend}/{name}: mean ΔE {:.3}, >10: {:.2}%",
            d.mean,
            d.over10 * 100.0
        );
        if d.mean > 1.0 || d.over10 > 0.01 {
            let out = std::env::temp_dir().join(format!("oxframe-golden-{backend}-{name}.png"));
            std::fs::write(&out, img.encode_png()).unwrap();
            failures.push(format!("{name} (got {})", out.display()));
        }
    }
    assert!(
        failures.is_empty(),
        "{backend} differs from golden: {failures:?}"
    );
}

#[test]
fn cpu_matches_golden() {
    check("cpu", &mut CpuRenderer::new());
}

#[test]
fn cpu_is_deterministic() {
    let opts = RenderOptions {
        width: W,
        height: H,
        background: Color::BLACK,
    };
    for (_, g) in scenes() {
        assert!(CpuRenderer::new().render(&g, &opts) == CpuRenderer::new().render(&g, &opts));
    }
}

#[cfg(feature = "gpu")]
#[test]
fn gpu_matches_golden() {
    match oxframe_render::GpuRenderer::new() {
        Ok(mut r) => {
            eprintln!("GPU: {}", r.adapter_name());
            check("gpu", &mut r);
        }
        // CI without a GPU: skip loudly rather than fail.
        Err(e) => eprintln!("SKIPPED gpu golden test: {e}"),
    }
}
