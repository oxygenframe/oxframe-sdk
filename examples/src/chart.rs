//! A function plot: grid, axes with arrows, a curve drawn on with `end`,
//! a dot following it — timed by markers that can be moved in the editor.

use oxframe::prelude::*;

const W: f64 = 1400.0;
const H: f64 = 700.0;

fn curve(phase: f64) -> Vec<Vec2> {
    (0..=200)
        .map(|i| {
            let t = i as f64 / 200.0;
            let x = -W / 2.0 + t * W;
            let y = -(t * std::f64::consts::TAU * 2.0 + phase).sin() * H * 0.3;
            Vec2::new(x, y)
        })
        .collect()
}

#[oxframe::scene]
pub async fn chart(view: View) {
    view.add(Grid::new().width(W).height(H).spacing(70.0).opacity(0.0));
    let grid = view.children()[0];
    let axes = [
        view.add(
            Line::new()
                .points([(-W / 2.0 - 20.0, 0.0), (W / 2.0 + 30.0, 0.0)])
                .end_arrow(true)
                .end(0.0),
        ),
        view.add(
            Line::new()
                .points([(-W / 2.0, H / 2.0), (-W / 2.0, -H / 2.0 - 30.0)])
                .end_arrow(true)
                .end(0.0),
        ),
    ];
    let plot = view.add(
        Line::new()
            .points(curve(0.0))
            .stroke(color!("#68abdf"))
            .line_width(6.0)
            .end(0.0),
    );
    let label = view.add(
        Txt::new()
            .text("y = sin(4πx)")
            .font_size(44.0)
            .position((0.0, -H / 2.0 - 60.0))
            .opacity(0.0),
    );

    all((
        grid.opacity().to(1.0, 0.6),
        axes[0].end().to(1.0, 0.6),
        axes[1].end().to(1.0, 0.6),
    ))
    .await;

    // Marker: move it in the timeline to retime everything after it.
    wait_until("draw").await;
    all((
        plot.end()
            .to(1.0, 2.0)
            .ease(ease!("draw", presets::IN_OUT_SINE)),
        label.opacity().to(1.0, 1.0),
    ))
    .await;

    let dot = view.add(
        Circle::new()
            .size(28.0)
            .fill(color!("#f5c242"))
            .position(curve(0.0)[0]),
    );
    let pts = curve(0.0);
    let t = Signal::new(0.0f64);
    {
        // Bind the dot's position to a progress signal.
        let (t, pts) = (t.clone(), pts.clone());
        let pos = Signal::derived(move || {
            let u = t.get().clamp(0.0, 1.0) * (pts.len() - 1) as f64;
            let i = (u as usize).min(pts.len() - 2);
            pts[i].lerp(pts[i + 1], u - i as f64)
        });
        let p = pos.clone();
        dot.x().bind(move || p.get().x);
        dot.y().bind(move || pos.get().y);
    }
    wait_until("dot").await;
    t.to(1.0, tune!("dot_seconds", 2.5)).await;

    // Shift the phase: points tween element-wise.
    plot.points().to(curve(std::f64::consts::PI), 1.2).await;
    wait_for(0.5).await;
}

pub fn project() -> Project {
    Project::new("chart").scene(chart())
}
