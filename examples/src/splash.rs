//! A drop rings the water, a splash wets the lens, and the water rises into
//! the next scene. All three shaders run on the CPU, so the example renders
//! without a GPU.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn sea(view: View) {
    let water = view.add(
        shaders::waves()
            .deep(Accent)
            .foam(Surface)
            .width(pct(100))
            .height(pct(45))
            .to_edge(Edge::Bottom),
    );
    let title = view.add(
        Txt::new()
            .text("Into the wave")
            .display()
            .to_edge(Edge::Top),
    );
    let lens = view.add(shaders::lens_drops().wetness(0.0).seed(7).fullscreen());

    title.fade_in().await;
    let rings = water.effect(shaders::ripple().strength(0.0));
    rings.strength().to(1.0, 0.4).await;
    all((water.amp().to(0.14, 1.0), rings.strength().to(0.0, 1.0))).await;
    lens.wetness().to(1.0, 0.4).await;
    wait_for(0.6).await;
}

#[oxframe::scene]
pub async fn shore(view: View) {
    // Added before the transition, so the rising water uncovers it.
    view.add(
        shaders::waves()
            .amp(0.02)
            .speed(0.3)
            .deep(Accent)
            .foam(Surface)
            .width(pct(100))
            .height(pct(20))
            .to_edge(Edge::Bottom),
    );
    transition(Transition::shader(shaders::water_wipe()).duration(1.0)).await;
    let end = view.add(EndCard::new("Made with oxframe"));
    end.enter().await;
}

pub fn project() -> Project {
    Project::new("splash")
        .size(960, 540)
        .fps(30)
        .scene(sea())
        .scene(shore())
}
