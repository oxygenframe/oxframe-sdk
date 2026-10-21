//! Axes and plots: a curve with a dot riding on it, a morph into another
//! function, a zoom out, then a benchmark whose bars grow with their labels.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn functions(view: View) {
    let axes = view.add(Axes::new().x_range(-5.0..5.0).y_range(-2.0..2.0));
    let wave = axes.plot(|x| x.sin());
    wave.draw_on().await;

    // A signal drives the dot along the curve.
    let x = Signal::new(-5.0);
    axes.point_on(&wave, x.clone());
    x.to(5.0, 2.0).await;

    wave.to_fn(|x| (2.0 * x).sin() * 1.5, 1.0).await;
    // The grid, ticks, labels and the curve follow the range.
    axes.x_range().to(-10.0..10.0, 1.2).await;
    wait_for(0.4).await;
}

#[oxframe::scene]
pub async fn build_times(view: View) {
    transition(Transition::fade()).await;
    let title = view.add(Txt::new().text("Build time, s").h1().to_edge(Edge::Top));
    let axes = view.add(
        Axes::new()
            .x_range(-0.5..3.5)
            .y_range(0.0..10.0)
            .height(pct(55))
            .y(40.0),
    );
    let bars = axes
        .bars([0.0, 0.0, 0.0, 0.0])
        .labels(["rustc", "gcc", "clang", "zig"]);
    title.fade_in().await;
    stagger(
        0.15,
        [3.2, 7.5, 5.1, 9.4].into_iter().enumerate(),
        |(i, v)| bars.value(i).to(v, 1.0),
    )
    .await;
    wait_for(0.6).await;
}

pub fn project() -> Project {
    Project::new("charts")
        .scene(functions())
        .scene(build_times())
}
