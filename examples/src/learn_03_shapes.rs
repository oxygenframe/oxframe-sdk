//! Lesson 3 — Shapes, colors and transforms.
//!
//! Every shape is made with a builder (`Rect::new().size(..)`) that sets its
//! starting look. After `view.add`, the same names without arguments give you
//! the property to animate: `.size(120.0)` sets it, `.size()` animates it.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn shapes(view: View) {
    let square = view.add(
        Rect::new()
            .size(140.0)
            .radius(16.0)
            .x(-450.0)
            .fill(color!("#e13238")),
    );
    let circle = view.add(Circle::new().size(140.0).x(-150.0).fill(color!("#f5c242")));
    let hexagon = view.add(
        Polygon::new()
            .sides(6.0)
            .size(150.0)
            .x(150.0)
            .fill(color!("#68abdf")),
    );
    let star = view.add(
        Star::new()
            .points(5.0)
            .size(160.0)
            .x(450.0)
            .fill(color!("#2ecc71")),
    );
    // Outlines: `stroke` is the line color, `line_width` its thickness.
    let arrow = view.add(
        Line::new()
            .points([(-450.0, 220.0), (450.0, 220.0)])
            .stroke(Color::WHITE)
            .line_width(6.0)
            .end_arrow(true),
    );

    // Transforms: rotation (degrees), scale, and position.
    all((
        square.rotation().to(45.0, 1.0),
        circle.scale().to(Vec2::new(1.4, 0.7), 1.0),
        hexagon.sides().to(3.0, 1.0),
        star.rotation().to(144.0, 1.0),
        arrow.y().to(-440.0, 1.0),
    ))
    .await;
    // Colors interpolate smoothly too.
    all((
        square.fill().to(color!("#9b59b6"), 0.8),
        circle.fill().to(color!("#e67e22"), 0.8),
    ))
    .await;
}

// Try: give the hexagon an outline with `.stroke(Color::WHITE).line_width(8.0)`.

pub fn project() -> Project {
    Project::new("shapes").scene(shapes())
}
