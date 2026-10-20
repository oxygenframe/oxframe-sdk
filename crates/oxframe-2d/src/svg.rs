//! Turns an SVG file into ordinary oxframe nodes.
//!
//! `usvg` normalizes the file (styles, transforms, text outlines). Each
//! group, path and embedded image becomes a node, so a logo can be drawn
//! on path by path and one element recolored by its `id`.

use std::cell::RefCell;

use kurbo::{Affine, Point, Shape};
use oxframe_core::{BlendMode, Color, Gradient};

use crate::builder::NodeBuilder;
use crate::draw::{LineCap, LineJoin};
use crate::graph::{NodeId, SceneGraph};
use crate::handles::{Group, Img, Path};
use crate::image::to_bez;

/// How the SVG's viewport maps onto the node's centered box.
pub(crate) struct Fit {
    pub(crate) nw: f64,
    pub(crate) nh: f64,
    pub(crate) sx: f64,
    pub(crate) sy: f64,
}

impl Fit {
    fn affine(&self) -> Affine {
        Affine::scale_non_uniform(self.sx, self.sy)
            * Affine::translate((-self.nw / 2.0, -self.nh / 2.0))
    }
}

thread_local! {
    static WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[allow(dead_code)]
pub(crate) fn clear_warnings() {
    WARNINGS.with(|w| w.borrow_mut().clear());
}

#[allow(dead_code)]
pub(crate) fn warnings() -> Vec<String> {
    WARNINGS.with(|w| w.borrow().clone())
}

fn warn(msg: impl Into<String>) {
    let msg = msg.into();
    eprintln!("warning: svg: {msg}");
    #[cfg(target_arch = "wasm32")]
    web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(&format!("svg: {msg}")));
    WARNINGS.with(|w| w.borrow_mut().push(msg));
}

/// Builds the children of `parent` from an SVG `usvg` has already parsed.
pub(crate) fn inflate(graph: &SceneGraph, parent: NodeId, root: &usvg::Group, fit: &Fit) {
    note_limits(root);
    for child in root.children() {
        add_node(graph, parent, child, fit);
    }
}

pub(crate) fn note_limits(group: &usvg::Group) {
    let id = label(group.id());
    if group.clip_path().is_some() {
        warn(format!("{id}: clip path skipped"));
    }
    if group.mask().is_some() {
        warn(format!("{id}: mask skipped"));
    }
    if !group.filters().is_empty() {
        warn(format!("{id}: filter skipped"));
    }
}

fn label(id: &str) -> String {
    if id.is_empty() {
        "svg".to_string()
    } else {
        format!("#{id}")
    }
}

fn add_node(graph: &SceneGraph, parent: NodeId, node: &usvg::Node, fit: &Fit) {
    match node {
        usvg::Node::Group(g) => add_group(graph, parent, g, fit),
        usvg::Node::Path(p) => add_path(graph, parent, p, fit),
        usvg::Node::Image(img) => add_image(graph, parent, img, fit),
        usvg::Node::Text(t) => warn(format!(
            "{}: text was not converted to paths and was skipped",
            label(t.id())
        )),
    }
}

fn add_group(graph: &SceneGraph, parent: NodeId, group: &usvg::Group, fit: &Fit) {
    note_limits(group);
    let opacity = f64::from(group.opacity().get());
    if opacity <= 0.0 {
        return;
    }
    let mut builder = Group::new()
        .opacity(opacity)
        .blend(blend(group.blend_mode()));
    if let Some(name) = named(group.id()) {
        builder = builder.name(name);
    }
    let id = builder.build(graph, parent).id();
    for child in group.children() {
        add_node(graph, id, child, fit);
    }
}

fn add_path(graph: &SceneGraph, parent: NodeId, path: &usvg::Path, fit: &Fit) {
    if !path.is_visible() {
        return;
    }
    if path
        .fill()
        .is_some_and(|f| matches!(f.rule(), usvg::FillRule::EvenOdd))
    {
        warn(format!(
            "{}: even-odd fill is drawn with the non-zero rule",
            label(path.id())
        ));
    }
    let abs = affine(path.abs_transform());
    let placed = fit.affine() * (abs * to_bez(path.data()));
    let bb = placed.bounding_box();
    if !bb.width().is_finite() || (bb.width() == 0.0 && bb.height() == 0.0) {
        return;
    }
    let center = bb.center();
    let local = Affine::translate((-center.x, -center.y)) * placed;
    let scale = stroke_scale(path.abs_transform(), fit);
    let mut builder = Path::new()
        .data(local)
        .x(center.x)
        .y(center.y)
        .fill(Color::TRANSPARENT);
    if let Some(name) = named(path.id()) {
        builder = builder.name(name);
    }
    if let Some(fill) = path.fill() {
        builder = apply_paint(builder, path, fill.paint(), fill.opacity().get(), fit, true);
    }
    if let Some(stroke) = path.stroke() {
        let width = f64::from(stroke.width().get()) * scale;
        builder = builder
            .line_width(width)
            .line_cap(cap(stroke.linecap()))
            .line_join(join(stroke.linejoin()));
        if let Some(dash) = stroke.dasharray() {
            builder = builder.dash(
                dash.iter()
                    .map(|d| f64::from(*d) * scale)
                    .collect::<Vec<_>>(),
            );
            builder = builder.dash_offset(f64::from(stroke.dashoffset()) * scale);
        }
        builder = apply_paint(
            builder,
            path,
            stroke.paint(),
            stroke.opacity().get(),
            fit,
            false,
        );
        if matches!(path.paint_order(), usvg::PaintOrder::StrokeAndFill) {
            warn(format!(
                "{}: stroke is painted under the fill",
                label(path.id())
            ));
        }
    }
    builder.build(graph, parent);
}

fn apply_paint(
    builder: crate::builder::PathBuilder,
    path: &usvg::Path,
    paint: &usvg::Paint,
    opacity: f32,
    fit: &Fit,
    fill: bool,
) -> crate::builder::PathBuilder {
    match paint {
        usvg::Paint::Color(c) => {
            let color = rgba(*c, opacity);
            if fill {
                builder.fill(color)
            } else {
                builder.stroke(color)
            }
        }
        usvg::Paint::LinearGradient(g) => {
            let grad = linear(path, g, opacity, fit);
            if fill {
                builder.fill_gradient(grad)
            } else {
                builder.stroke_gradient(grad)
            }
        }
        usvg::Paint::RadialGradient(g) => {
            let grad = radial(path, g, opacity, fit);
            if fill {
                builder.fill_gradient(grad)
            } else {
                builder.stroke_gradient(grad)
            }
        }
        usvg::Paint::Pattern(_) => {
            warn(format!("{}: pattern paint skipped", label(path.id())));
            builder
        }
    }
}

fn linear(path: &usvg::Path, g: &usvg::LinearGradient, opacity: f32, fit: &Fit) -> Gradient {
    note_spread(g.spread_method(), g.id());
    let from = grad_point(path, g, g.x1(), g.y1(), fit);
    let to = grad_point(path, g, g.x2(), g.y2(), fit);
    stops(Gradient::linear(from, to), g.stops(), opacity)
}

fn radial(path: &usvg::Path, g: &usvg::RadialGradient, opacity: f32, fit: &Fit) -> Gradient {
    note_spread(g.spread_method(), g.id());
    if (g.fx() - g.cx()).abs() > 1e-3 || (g.fy() - g.cy()).abs() > 1e-3 || g.fr().get() > 1e-3 {
        warn(format!(
            "#{}: radial focal point is drawn from the center",
            g.id()
        ));
    }
    let center = grad_point(path, g, g.cx(), g.cy(), fit);
    let edge = grad_point(path, g, g.cx() + g.r().get(), g.cy(), fit);
    let down = grad_point(path, g, g.cx(), g.cy() + g.r().get(), fit);
    let rx = (edge - center).hypot();
    let ry = (down - center).hypot();
    if (rx - ry).abs() > 0.02 * rx.max(ry).max(1e-6) {
        warn(format!(
            "#{}: elliptical radial gradient is drawn as a circle",
            g.id()
        ));
    }
    stops(Gradient::radial(center, rx), g.stops(), opacity)
}

fn stops(mut grad: Gradient, stops: &[usvg::Stop], opacity: f32) -> Gradient {
    for s in stops {
        grad = grad.stop(
            f64::from(s.offset().get()),
            rgba(s.color(), opacity * s.opacity().get()),
        );
    }
    grad
}

fn note_spread(spread: usvg::SpreadMethod, id: &str) {
    if spread != usvg::SpreadMethod::Pad {
        warn(format!(
            "#{id}: gradient spread '{spread:?}' is drawn as pad"
        ));
    }
}

/// Gradient endpoint in the path node's local coordinates.
fn grad_point(path: &usvg::Path, base: &usvg::BaseGradient, x: f32, y: f32, fit: &Fit) -> Point {
    // `usvg` folds `gradientUnits` and `gradientTransform` into this transform,
    // so the raw 0..1 (or user-space) point is mapped by it alone.
    let (x, y) = apply(base.transform(), f64::from(x), f64::from(y));
    let canvas = affine(path.abs_transform()) * Point::new(x, y);
    let placed = fit.affine() * canvas;
    let center = (fit.affine() * (affine(path.abs_transform()) * to_bez(path.data())))
        .bounding_box()
        .center();
    Point::new(placed.x - center.x, placed.y - center.y)
}

fn add_image(graph: &SceneGraph, parent: NodeId, image: &usvg::Image, fit: &Fit) {
    if !image.is_visible() {
        return;
    }
    let bytes = match image.kind() {
        usvg::ImageKind::PNG(b) | usvg::ImageKind::JPEG(b) => b.as_slice(),
        usvg::ImageKind::GIF(_) | usvg::ImageKind::WEBP(_) => {
            warn(format!(
                "{}: only PNG and JPEG images are imported",
                label(image.id())
            ));
            return;
        }
        usvg::ImageKind::SVG(_) => {
            warn(format!("{}: nested SVG image skipped", label(image.id())));
            return;
        }
    };
    if image.abs_transform().has_skew() {
        warn(format!(
            "{}: skewed image is placed in its bounding box",
            label(image.id())
        ));
    }
    let w = f64::from(image.size().width());
    let h = f64::from(image.size().height());
    let tf = affine(image.abs_transform());
    let corner = |x: f64, y: f64| fit.affine() * (tf * Point::new(x, y));
    let origin = corner(0.0, 0.0);
    let right = corner(w, 0.0);
    let down = corner(0.0, h);
    let across = right - origin;
    let along = down - origin;
    let center = origin + across * 0.5 + along * 0.5;
    let mut builder = Img::new()
        .src(bytes)
        .width(across.hypot())
        .height(along.hypot())
        .x(center.x)
        .y(center.y)
        .rotation(across.y.atan2(across.x).to_degrees());
    if let Some(name) = named(image.id()) {
        builder = builder.name(name);
    }
    builder.build(graph, parent);
}

fn named(id: &str) -> Option<String> {
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

fn rgba(c: usvg::Color, opacity: f32) -> Color {
    Color::from_rgba8(
        c.red,
        c.green,
        c.blue,
        (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn affine(t: usvg::Transform) -> Affine {
    Affine::new([t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(f64::from))
}

fn apply(t: usvg::Transform, x: f64, y: f64) -> (f64, f64) {
    let p = affine(t) * Point::new(x, y);
    (p.x, p.y)
}

fn stroke_scale(t: usvg::Transform, fit: &Fit) -> f64 {
    let a = fit.sx * f64::from(t.sx);
    let b = fit.sx * f64::from(t.kx);
    let c = fit.sy * f64::from(t.ky);
    let d = fit.sy * f64::from(t.sy);
    (a * d - b * c).abs().sqrt().max(1e-9)
}

fn cap(c: usvg::LineCap) -> LineCap {
    match c {
        usvg::LineCap::Butt => LineCap::Butt,
        usvg::LineCap::Round => LineCap::Round,
        usvg::LineCap::Square => LineCap::Square,
    }
}

fn join(j: usvg::LineJoin) -> LineJoin {
    match j {
        usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter,
        usvg::LineJoin::Round => LineJoin::Round,
        usvg::LineJoin::Bevel => LineJoin::Bevel,
    }
}

pub(crate) fn blend(b: usvg::BlendMode) -> BlendMode {
    match b {
        usvg::BlendMode::Normal => BlendMode::Normal,
        usvg::BlendMode::Multiply => BlendMode::Multiply,
        usvg::BlendMode::Screen => BlendMode::Screen,
        usvg::BlendMode::Overlay => BlendMode::Overlay,
        usvg::BlendMode::Darken => BlendMode::Darken,
        usvg::BlendMode::Lighten => BlendMode::Lighten,
        usvg::BlendMode::ColorDodge => BlendMode::ColorDodge,
        usvg::BlendMode::ColorBurn => BlendMode::ColorBurn,
        usvg::BlendMode::HardLight => BlendMode::HardLight,
        usvg::BlendMode::SoftLight => BlendMode::SoftLight,
        usvg::BlendMode::Difference => BlendMode::Difference,
        usvg::BlendMode::Exclusion => BlendMode::Exclusion,
        usvg::BlendMode::Hue => BlendMode::Hue,
        usvg::BlendMode::Saturation => BlendMode::Saturation,
        usvg::BlendMode::Color => BlendMode::Color,
        usvg::BlendMode::Luminosity => BlendMode::Luminosity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SceneGraph;
    use crate::handles::View;
    use kurbo::{BezPath, Shape};

    const MARK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40">
        <g id="mark" transform="translate(8,4)">
            <rect id="bar" x="0" y="0" width="20" height="20" fill="#e13238"/>
            <circle id="dot" cx="40" cy="12" r="6" fill="#f5c242"/>
        </g>
    </svg>"##;

    #[test]
    fn finds_ids_and_lists_paths_in_order() {
        let g = SceneGraph::new(200, 100);
        g.enter(|| {
            let svg = View::current().add(crate::Svg::new().src(MARK.as_bytes()).width(160.0));
            assert!((svg.width().get() - 160.0).abs() < 1e-6);
            assert!((svg.height().get() - 80.0).abs() < 1e-6);
            assert_eq!(svg.paths().len(), 2);
            let dot = svg.find("dot");
            assert_eq!(dot.fill().get().to_rgba8(), [0xf5, 0xc2, 0x42, 255]);
            let bar = svg.find("bar");
            assert_eq!(bar.fill().get().to_rgba8(), [0xe1, 0x32, 0x38, 255]);
        });
    }

    #[test]
    fn an_imported_path_morphs_through_its_data_signal() {
        let g = SceneGraph::new(200, 100);
        g.enter(|| {
            let svg = View::current().add(crate::Svg::new().src(MARK.as_bytes()));
            let path = svg.paths()[0];
            let next = BezPath::from_svg("M0 0 L30 0 L30 10 Z").unwrap();
            let tween = path.data().to(next.clone(), 0.1);
            let mut ex = oxframe_core::Executor::new(
                60.0,
                oxframe_protocol::Meta::default(),
                0,
                std::pin::Pin::from(Box::new(async move {
                    tween.await;
                })),
            );
            let mut widths = vec![path.data().get().bounding_box().width()];
            while !ex.step() {
                widths.push(path.data().get().bounding_box().width());
            }
            let end = path.data().get().bounding_box().width();
            assert!(
                widths.len() > 2 && (end - 30.0).abs() < 1.0,
                "widths {widths:?} end {end}"
            );
            assert!(
                widths.windows(2).any(|w| (w[0] - w[1]).abs() > 0.5),
                "the path data did not move: {widths:?}"
            );
        });
    }

    #[test]
    fn gradient_crosses_the_shape() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="60">
            <linearGradient id="g" x1="0" y1="0" x2="1" y2="0">
              <stop offset="0" stop-color="#e13238"/>
              <stop offset="1" stop-color="#68abdf"/>
            </linearGradient>
            <g transform="translate(16,10) rotate(8)">
              <rect id="bar" x="0" y="0" width="36" height="36" fill="url(#g)"/>
            </g>
          </svg>"##;
        let g = SceneGraph::new(120, 60);
        g.enter(|| {
            let svg = View::current().add(crate::Svg::new().src(svg.as_bytes()));
            let bar = svg.paths()[0];
            let grad = bar.fill_gradient().get().expect("gradient");
            let data = bar.data().get().bounding_box();
            let oxframe_core::GradientKind::Linear { from, to } = grad.kind else {
                panic!("expected a linear gradient");
            };
            let span = (to.x - from.x).hypot(to.y - from.y);
            assert!(
                span > data.width() * 0.5 && span < data.width() * 2.0,
                "gradient span {span} path {data:?} from {from:?} to {to:?}"
            );
        });
    }

    #[test]
    fn filters_are_skipped_with_a_warning() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <filter id="b"><feGaussianBlur stdDeviation="2"/></filter>
            <rect id="box" x="0" y="0" width="10" height="10" fill="#000" filter="url(#b)"/>
        </svg>"##;
        clear_warnings();
        let g = SceneGraph::new(40, 40);
        g.enter(|| {
            let node = View::current().add(crate::Svg::new().src(svg));
            assert_eq!(node.paths().len(), 1);
        });
        assert!(
            warnings().iter().any(|w| w.contains("filter")),
            "{:?}",
            warnings()
        );
    }
}
