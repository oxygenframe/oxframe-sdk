//! Lesson 4 — Easing: how an animation speeds up and slows down.
//!
//! By default animations start slowly, speed up and end slowly. `.ease(..)`
//! picks another curve. Watch the three balls: same distance, same time,
//! different feel.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn easing(view: View) {
    let lane = |y: f64, name: &str| {
        view.add(
            Txt::new()
                .text(name)
                .family(MONO)
                .font_size(28.0)
                .position((-620.0, y))
                .fill(Color::WHITE.with_alpha(0.6)),
        );
        view.add(
            Circle::new()
                .size(70.0)
                .position((-400.0, y))
                .fill(color!("#68abdf")),
        )
    };
    let linear = lane(-150.0, "LINEAR");
    let smooth = lane(0.0, "default");
    let bouncy = lane(150.0, "OUT_BACK");

    all((
        linear.x().to(500.0, 1.5).ease(presets::LINEAR),
        smooth.x().to(500.0, 1.5),
        bouncy.x().to(500.0, 1.5).ease(presets::OUT_BACK),
    ))
    .await;
    wait_for(0.5).await;
}

// Try: `presets::IN_OUT_EXPO`, `presets::OUT_CUBIC`, or the functions
// `bounce_out` and `elastic_out` (as `.ease(oxframe::bounce_out)`).

pub fn project() -> Project {
    Project::new("easing").scene(easing())
}
