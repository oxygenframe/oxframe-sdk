//! Lesson 13 — Shaders and effects.
//!
//! Presets paint pixels with a shader: water as a node, drops on the lens
//! over everything drawn before them, rings on a node, and water rising
//! into the next scene. Their parameters are signals, so you animate them
//! like any other property.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn sea(view: View) {
    let water = view.add(
        shaders::waves()
            .deep(Accent)
            .foam(Surface)
            .width(pct(100))
            .height(pct(40))
            .to_edge(Edge::Bottom),
    );
    let title = view.add(Txt::new().text("Shaders").display().to_edge(Edge::Top));
    // Added last: it bends everything drawn before it.
    let lens = view.add(shaders::lens_drops().wetness(0.0).fullscreen());

    title.fade_in().await;
    water.amp().to(0.12, 1.0).await;

    // An effect changes one node's own pixels.
    let rings = water.effect(shaders::ripple().strength(0.0));
    rings.strength().to(1.0, 0.4).await;
    rings.strength().to(0.0, 0.8).await;

    lens.wetness().to(1.0, 0.5).await;
    wait_for(0.4).await;
}

#[oxframe::scene]
pub async fn land(view: View) {
    // Added before the transition, so the rising water uncovers it.
    view.add(Txt::new().text("Dry land").display());
    transition(Transition::shader(shaders::water_wipe())).await;
    wait_for(0.6).await;
}

// Try: `.wetness(0.3)` for a light drizzle, or `water_wipe().wave(0.1)`.

pub fn project() -> Project {
    Project::new("shaders").scene(sea()).scene(land())
}
