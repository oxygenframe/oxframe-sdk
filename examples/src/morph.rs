//! Morphing between arbitrary paths and drawing outlines on with `end`.

use oxframe::prelude::*;

const STAR: &str =
    "M0 -150 L44 -46 L150 -46 L64 18 L96 130 L0 64 L-96 130 L-64 18 L-150 -46 L-44 -46 Z";
const HEART: &str = "M0 140 C-60 80 -160 30 -160 -50 C-160 -110 -110 -150 -60 -150 C-30 -150 -10 -130 0 -110 \
                     C10 -130 30 -150 60 -150 C110 -150 160 -110 160 -50 C160 30 60 80 0 140 Z";

fn circle(r: f64) -> BezPath {
    use oxframe::core::kurbo::Shape;
    oxframe::core::kurbo::Circle::new((0.0, 0.0), r).to_path(0.1)
}

fn svg(d: &str) -> BezPath {
    BezPath::from_svg(d).expect("valid path")
}

#[oxframe::scene]
pub async fn morph(view: View) {
    // Draw the outline on, then fill it.
    let shape = view.add(
        Path::new()
            .svg(STAR)
            .fill(Color::TRANSPARENT)
            .stroke(color!("#f5c242"))
            .line_width(8.0)
            .end(0.0),
    );
    shape.end().to(1.0, 1.2).await;
    shape.fill().to(color!("#f5c242"), 0.5).await;

    // Star → circle → heart → star: alignment picks start points and
    // directions so the shapes do not twist.
    shape.data().to(circle(140.0), 1.0).await;
    all((
        shape.data().to(svg(HEART), 1.0),
        shape.fill().to(color!("#e13238"), 1.0),
        shape.stroke().to(color!("#e13238"), 1.0),
    ))
    .await;
    wait_for(0.3).await;
    all((
        shape.data().to(svg(STAR), 1.0).ease(presets::OUT_BACK),
        shape.rotation().to(144.0, 1.0),
        shape.fill().to(color!("#68abdf"), 1.0),
        shape.stroke().to(color!("#68abdf"), 1.0),
    ))
    .await;
    // Erase from the start.
    shape.fill().to(Color::TRANSPARENT, 0.4).await;
    shape.start().to(1.0, 0.8).await;
}

pub fn project() -> Project {
    Project::new("morph").scene(morph())
}
