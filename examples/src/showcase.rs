//! Several scenes in one project, with markers, named eases and tunables
//! that the editor can adjust without recompiling.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn title(view: View) {
    let t = view.add(
        Txt::new()
            .text("oxframe")
            .font_size(180.0)
            .weight(900.0)
            .opacity(0.0)
            .y(40.0),
    );
    let sub = view.add(
        Txt::new()
            .text("procedural animation in Rust")
            .font_size(48.0)
            .y(140.0)
            .opacity(0.0),
    );
    all((
        t.opacity().to(1.0, 0.8),
        t.y()
            .to(-20.0, 0.8)
            .ease(ease!("title_in", presets::OUT_CUBIC)),
    ))
    .await;
    wait_until("subtitle").await;
    sub.opacity().to(1.0, 0.6).await;
    wait_for(tune!("title_hold", 1.0)).await;
    all((t.opacity().to(0.0, 0.5), sub.opacity().to(0.0, 0.5))).await;
}

#[oxframe::scene]
pub async fn shapes(view: View) {
    let n = tune!("count", 7.0) as usize;
    let spacing = 200.0;
    let start = -(n as f64 - 1.0) * spacing / 2.0;
    let items: Vec<_> = (0..n)
        .map(|i| {
            let hue = i as f32 / n as f32;
            view.add(
                Rect::new()
                    .size(120.0)
                    .x(start + i as f64 * spacing)
                    .fill(Color::rgb(0.3 + 0.7 * hue, 0.4, 1.0 - 0.6 * hue))
                    .scale(0.0),
            )
        })
        .collect();
    let pop = ease!("pop", presets::OUT_BACK);
    sequence(
        0.08,
        items
            .iter()
            .map(|r| -> oxframe::BoxFuture {
                Box::pin(r.scale().to(Vec2::new(1.0, 1.0), 0.5).ease(pop))
            })
            .collect(),
    )
    .await;
    wait_until("spin").await;
    sequence(
        0.05,
        items
            .iter()
            .map(|r| -> oxframe::BoxFuture {
                let r = *r;
                Box::pin(async move {
                    all((r.rotation().to(180.0, 0.8), r.radius().to(60.0, 0.8))).await;
                })
            })
            .collect(),
    )
    .await;
    wait_for(0.5).await;
}

pub fn project() -> Project {
    Project::new("showcase").scene(title()).scene(shapes())
}
