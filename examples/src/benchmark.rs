//! Reference scene for performance targets (spec §8): 200 nodes, 2 text
//! blocks and 1 morph, animated for 60 s at 60 fps.

use oxframe::prelude::*;

pub const NODES: usize = 200;

#[oxframe::scene]
pub async fn benchmark(view: View) {
    let (w, h) = view.size();
    let (w, h) = (f64::from(w), f64::from(h));
    let mut shapes = Vec::with_capacity(NODES);
    for i in 0..NODES {
        let col = (i % 20) as f64;
        let row = (i / 20) as f64;
        let x = -w / 2.0 + 60.0 + col * (w - 120.0) / 19.0;
        let y = -h / 2.0 + 60.0 + row * (h - 120.0) / 9.0;
        let hue = i as f32 / NODES as f32;
        let fill = Color::rgb(0.9 * hue + 0.1, 0.4, 1.0 - 0.8 * hue);
        shapes.push(view.add(Rect::new().size(40.0).position((x, y)).fill(fill)));
    }
    let speed = tune!("speed", 1.0);
    // Two text blocks and one morphing path run in the background.
    let caption = view.add(
        Txt::new()
            .text("benchmark")
            .font_size(64.0)
            .weight(800.0)
            .y(-20.0),
    );
    let counter = view.add(Txt::new().text("0").family(MONO).font_size(48.0).y(40.0));
    let blob = view.add(
        Path::new()
            .data(star())
            .fill(color!("#ffffff40"))
            .scale(1.5),
    );
    spawn(async move {
        use oxframe::core::kurbo::Shape;
        let circle = oxframe::core::kurbo::Circle::new((0.0, 0.0), 60.0).to_path(0.1);
        for i in 0..30 {
            let target = if i % 2 == 0 { circle.clone() } else { star() };
            all((
                blob.data().to(target, 2.0),
                counter.text().to(format!("{}", i + 1), 0.5),
            ))
            .await;
        }
    });
    let _ = caption;
    for round in 0..15 {
        let dir = if round % 2 == 0 { 1.0 } else { -1.0 };
        stagger(0.01, &shapes, |s| {
            all((
                s.rotation()
                    .to(dir * 90.0 * f64::from(round + 1), 2.0 / speed),
                s.radius()
                    .to(if round % 2 == 0 { 20.0 } else { 0.0 }, 2.0 / speed),
                s.y().to(s.y().get() + dir * 20.0, 2.0 / speed),
            ))
        })
        .await;
        wait_for(4.0 - 2.0 - (NODES - 1) as f64 * 0.01).await;
    }
}

fn star() -> BezPath {
    BezPath::from_svg(
        "M0 -80 L24 -24 L80 -24 L36 10 L50 70 L0 34 L-50 70 L-36 10 L-80 -24 L-24 -24 Z",
    )
    .expect("valid path")
}

pub fn project() -> Project {
    Project::new("benchmark").scene(benchmark())
}
