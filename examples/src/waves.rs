//! A wave that grows. The crest is a shader preset, so it renders without a GPU.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn sea(view: View) {
    let water = view.add(
        shaders::waves()
            .amp(0.05)
            .deep(Accent)
            .foam(Surface)
            .width(pct(100))
            .height(pct(40))
            .to_edge(Edge::Bottom),
    );
    view.add(Txt::new().text("Waves").display().to_edge(Edge::Top));
    water.amp().to(0.12, 1.2).await;
}

pub fn project() -> Project {
    Project::new("waves").size(960, 540).fps(30).scene(sea())
}
