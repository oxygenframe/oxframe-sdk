//! Lesson 9 — Drawing and morphing shapes.
//!
//! `start` and `end` choose which part of an outline is drawn: animate `end`
//! from 0 to 1 to draw a shape on. A `Path` can morph into any other path.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn drawing(view: View) {
    let star = BezPath::from_svg(
        "M0 -150 L44 -46 L150 -46 L64 18 L96 130 L0 64 L-96 130 L-64 18 L-150 -46 L-44 -46 Z",
    )
    .unwrap();
    let heart = BezPath::from_svg(
        "M0 140 C-60 80 -160 30 -160 -50 C-160 -110 -110 -150 -60 -150 C-30 -150 -10 -130 0 -110 \
         C10 -130 30 -150 60 -150 C110 -150 160 -110 160 -50 C160 30 60 80 0 140 Z",
    )
    .unwrap();

    let shape = view.add(
        Path::new()
            .data(star)
            .fill(Color::TRANSPARENT)
            .stroke(color!("#f5c242"))
            .line_width(8.0)
            .end(0.0),
    );
    // Draw the outline on, then fill it.
    shape.end().to(1.0, 1.2).await;
    shape.fill().to(color!("#f5c242"), 0.4).await;
    // Morph into a heart, changing color on the way.
    all((
        shape.data().to(heart, 1.2),
        shape.fill().to(color!("#e13238"), 1.2),
        shape.stroke().to(color!("#e13238"), 1.2),
    ))
    .await;
    wait_for(0.5).await;
}

// Try: morph back into the star, or animate `start` to 1 to erase the outline.

pub fn project() -> Project {
    Project::new("drawing").scene(drawing())
}
