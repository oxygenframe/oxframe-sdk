//! Lesson 10 — Randomness and background tasks.
//!
//! `random()` gives numbers that look random but are the same on every run:
//! they come from the project's seed, so renders are reproducible. `spawn`
//! starts an animation in the background while the scene continues.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn random_lesson(view: View) {
    // Confetti in the background.
    let rain = spawn(async move {
        for _ in 0..60 {
            let x = random_range(-800.0, 800.0);
            let c = Color::rgb(random() as f32, 0.6, 1.0 - random() as f32);
            let piece = view.add(
                Rect::new()
                    .size(random_range(12.0, 28.0))
                    .position((x, -600.0))
                    .fill(c),
            );
            // Each piece falls on its own, also in the background.
            spawn(async move {
                all((
                    piece.y().to(600.0, 2.0).ease(presets::LINEAR),
                    piece.rotation().to(360.0, 2.0),
                ))
                .await;
                piece.remove();
            });
            wait_for(0.05).await;
        }
    });

    // Meanwhile, in the foreground:
    let title = view.add(
        Txt::new()
            .text("seed = 7")
            .font_size(110.0)
            .weight(800.0)
            .scale(0.0),
    );
    title
        .scale()
        .to(Vec2::new(1.0, 1.0), 0.8)
        .ease(presets::OUT_BACK)
        .await;
    rain.await;
    wait_for(2.0).await;
}

// Try: change `.seed(7)` below to another number — a different, but again
// repeatable, confetti.

pub fn project() -> Project {
    Project::new("random").seed(7).scene(random_lesson())
}
