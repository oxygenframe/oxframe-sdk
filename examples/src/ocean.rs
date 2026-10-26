//! The target scene of hybrid rendering: waves under a camera, a splash
//! toward the lens, a dive into the water, and the sea washing into the
//! shore. Every shader here runs on the CPU, so it renders without a GPU.
//!
//! Drag the `swell` and `splash` markers in the editor to retime the beats.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn ocean(view: View) {
    let cam = view.add(Camera::new());
    let sea = cam.add(
        shaders::waves()
            .amp(0.04)
            .deep(Accent)
            .foam(Surface)
            .width(pct(100))
            .height(pct(55))
            .to_edge(Edge::Bottom),
    );
    let title = view.add(
        Txt::new()
            .text("Into the wave")
            .display()
            .to_edge(Edge::Top),
    );
    let spray = view.add(
        Particles::new()
            .seed(7)
            .gravity(1400.0)
            .fill(Text) // white foam in the dark theme
            .toward_camera(),
    );
    // Added last, so the drops bend everything above.
    let lens = view.add(shaders::lens_drops().wetness(0.0).seed(3).fullscreen());

    all((title.fade_in(), cam.zoom().to(1.1, 2.0))).await;
    wait_for(0.4).await;
    wait_until("swell").await;
    sea.amp().to(0.14, 1.2).await;
    wait_until("splash").await;
    all((spray.burst((0.0, 140.0), 400), lens.wetness().to(1.0, 0.4))).await;
    cam.center_on(sea, 0.6).await;
    cam.zoom().to(4.0, 0.8).await; // into the wave
}

#[oxframe::scene]
pub async fn shore(view: View) {
    // Added before the transition, so the rising water uncovers calm sea.
    view.add(
        shaders::waves()
            .amp(0.01)
            .speed(0.3)
            .deep(Accent)
            .foam(Surface)
            .width(pct(100))
            .height(pct(12))
            .to_edge(Edge::Bottom),
    );
    transition(Transition::shader(shaders::water_wipe())).await;
    view.add(EndCard::new("").made_by()).enter().await;
}

pub fn project() -> Project {
    Project::new("ocean")
        .size(960, 540)
        .fps(30)
        .scene(ocean())
        .scene(shore())
}
