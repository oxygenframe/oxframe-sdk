//! Lesson 8 — Signals: properties that follow other properties.
//!
//! Every property is a `Signal`. Besides animating it, you can `bind` it to a
//! formula over other signals; it then updates by itself. Here the line and
//! the label follow the two dots — only the dots are animated.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn signals(view: View) {
    let a = view.add(
        Circle::new()
            .size(40.0)
            .position((-300.0, -100.0))
            .fill(color!("#e13238")),
    );
    let b = view.add(
        Circle::new()
            .size(40.0)
            .position((300.0, 100.0))
            .fill(color!("#68abdf")),
    );
    let link = view.add(
        Line::new()
            .stroke(Color::WHITE.with_alpha(0.6))
            .line_width(4.0)
            .z_index(-1.0),
    );
    let label = view.add(Txt::new().font_size(32.0).family(MONO));

    // The line's points are computed from the dots' positions.
    let (ax, ay, bx, by) = (a.x(), a.y(), b.x(), b.y());
    link.points()
        .bind(move || vec![Vec2::new(ax.get(), ay.get()), Vec2::new(bx.get(), by.get())]);
    // The label sits in the middle and shows the distance.
    let (ax, ay, bx, by) = (a.x(), a.y(), b.x(), b.y());
    label.text().bind(move || {
        format!(
            "{:.0} px",
            Vec2::new(bx.get() - ax.get(), by.get() - ay.get()).hypot()
        )
    });
    let (ax, bx) = (a.x(), b.x());
    label.x().bind(move || (ax.get() + bx.get()) / 2.0);
    let (ay, by) = (a.y(), b.y());
    label.y().bind(move || (ay.get() + by.get()) / 2.0 - 40.0);

    all((a.y().to(150.0, 1.5), b.x().to(-50.0, 1.5))).await;
    all((a.x().to(100.0, 1.0), b.y().to(-200.0, 1.0))).await;
}

// Try: bind `b.fill()` so the blue dot turns red when it gets close to the red one.

pub fn project() -> Project {
    Project::new("signals").scene(signals())
}
