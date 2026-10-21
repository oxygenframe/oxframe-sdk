//! Lesson 14 — Charts and graphs.
//!
//! `Axes` maps data coordinates into a box. A plot is a function of x,
//! sampled again every frame, so it follows when the ranges move. Values
//! that change are signals, like everything else in oxframe.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn charts_lesson(view: View) {
    let axes = view.add(Axes::new().x_range(-4.0..4.0).y_range(-1.5..1.5));

    // A curve draws itself on, like any path.
    let curve = axes.plot(|x| (x * 1.5).sin());
    curve.draw_on().await;

    // A dot rides on the curve: its x is a signal.
    let x = Signal::new(-4.0);
    axes.point_on(&curve, x.clone());
    x.to(4.0, 2.0).await;

    // Morph into another function, then zoom out.
    curve
        .to_fn(|x| x.sin() * 0.5 + (3.0 * x).sin() * 0.4, 1.0)
        .await;
    axes.x_range().to(-8.0..8.0, 1.0).await;
    wait_for(0.4).await;
}

// Try: plot `|x| x.tan()` — the gaps stay gaps, no line jumps across.

pub fn project() -> Project {
    Project::new("charts_lesson").scene(charts_lesson())
}
