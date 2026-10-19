//! Seeded randomness and background tasks: the same seed always gives the
//! same video.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn particles(view: View) {
    let (w, h) = view.size();
    let (w, h) = (f64::from(w), f64::from(h));
    let palette = [
        color!("#e13238"),
        color!("#f5c242"),
        color!("#68abdf"),
        color!("#2ecc71"),
    ];

    // A background task keeps emitting while the main script continues.
    let emitter = spawn(async move {
        for i in 0..120 {
            let x = random_range(-w / 2.0, w / 2.0);
            let size = random_range(10.0, 40.0);
            let c = palette[(random() * palette.len() as f64) as usize % palette.len()];
            let p = view.add(
                Circle::new()
                    .size(size)
                    .fill(c)
                    .position((x, h / 2.0 + 40.0)),
            );
            let rise = random_range(1.5, 3.0);
            spawn(async move {
                all((
                    p.y().to(-h / 2.0 - 40.0, rise).ease(presets::LINEAR),
                    p.opacity().to(0.0, rise),
                ))
                .await;
                p.remove();
            });
            if i % 3 == 0 {
                wait_for(0.05).await;
            }
        }
    });

    let title = view.add(
        Txt::new()
            .text("seed = 7")
            .font_size(120.0)
            .weight(800.0)
            .scale(0.0),
    );
    title
        .scale()
        .to(Vec2::new(1.0, 1.0), 0.8)
        .ease(presets::OUT_BACK)
        .await;
    emitter.await;
    wait_for(3.0).await;
}

pub fn project() -> Project {
    Project::new("particles").seed(7).scene(particles())
}
