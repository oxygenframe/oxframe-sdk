//! Lesson 6 — Groups and layout.
//!
//! Nodes can have children. Children move, rotate and fade with their parent,
//! and their positions are relative to it. A `Layout` places its children in
//! a row or a column for you.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn groups(view: View) {
    // A tiny solar system: the moon orbits the planet, the planet the sun.
    let sun = view.add(
        Group::new()
            .x(-350.0)
            .child(Circle::new().size(120.0).fill(color!("#f5c242"))),
    );
    let planet = sun.add(Group::new().x(220.0));
    planet.add(Circle::new().size(50.0).fill(color!("#68abdf")));
    planet.add(Circle::new().size(18.0).x(60.0).fill(Color::WHITE));

    // A row of cards laid out automatically.
    let row = view.add(
        Layout::new()
            .gap(16.0)
            .padding(16.0)
            .radius(16.0)
            .fill(color!("#26282e"))
            .position((380.0, 0.0)),
    );
    for c in ["#e13238", "#f5c242", "#68abdf"] {
        row.add(
            Rect::new()
                .size(70.0)
                .radius(10.0)
                .fill(Color::from_hex(c).unwrap()),
        );
    }

    all((
        sun.rotation().to(360.0, 3.0).ease(presets::LINEAR),
        planet.rotation().to(720.0, 3.0).ease(presets::LINEAR),
        // Growing the gap re-flows the row.
        row.gap().to(40.0, 1.5),
    ))
    .await;
}

// Try: add a fourth card with `.grow(1.0)` and give the row `.width(500.0)`.

pub fn project() -> Project {
    Project::new("groups").scene(groups())
}
