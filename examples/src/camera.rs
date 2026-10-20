//! A camera tour of a system diagram: see all of it, zoom onto one step,
//! follow a request as it travels, then back out. The title stays put: it
//! is added to the view, not to the camera.

use oxframe::prelude::*;

const STEPS: [&str; 6] = ["Browser", "CDN", "API", "Queue", "Worker", "Database"];

/// Where step `i` sits in the world the camera looks at (wider than a frame).
fn spot(i: usize) -> (f64, f64) {
    let x = -1500.0 + i as f64 * 600.0;
    let y = if i.is_multiple_of(2) { -140.0 } else { 140.0 };
    (x, y)
}

#[oxframe::scene]
pub async fn camera(view: View) {
    let cam = view.add(Camera::new().name("camera"));
    let world = cam.add(Group::new().name("diagram"));
    for i in 1..STEPS.len() {
        let (a, b) = (spot(i - 1), spot(i));
        world.add(
            Line::new()
                .points([(a.0 + 180.0, a.1), (b.0 - 180.0, b.1)])
                .stroke(Muted)
                .line_width(6.0)
                .end_arrow(true),
        );
    }
    let boxes: Vec<Rect> = STEPS
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let (x, y) = spot(i);
            world.add(
                Rect::new()
                    .width(360.0)
                    .height(160.0)
                    .radius(Md)
                    .fill(Surface)
                    .stroke(Surface)
                    .line_width(6.0)
                    .position((x, y))
                    .name(*name)
                    .child(Txt::new().text(*name).h2()),
            )
        })
        .collect();
    let title = view.add(
        Txt::new()
            .text("One request, end to end")
            .h2()
            .to_edge(Edge::Top)
            .name("title"),
    );

    // The whole diagram, then the steps appear one by one.
    cam.fit(world, 140.0, 0.0).await;
    title.fade_in().await;
    stagger(0.12, &boxes, |b| b.pop_in()).await;
    wait_for(0.4).await;

    // A closer look at the API.
    all((cam.center_on(boxes[2], 1.2), cam.zoom().to(1.6, 1.2))).await;
    wait_for(0.6).await;

    // A request travels on; the camera follows it.
    let (x0, y0) = spot(2);
    let packet = world.add(
        Circle::new()
            .size(44.0)
            .fill(Accent)
            .position((x0, y0 + 110.0)),
    );
    packet.pop_in().await;
    let (px, py) = (packet.x(), packet.y());
    cam.x().bind(move || px.get());
    cam.y().bind(move || py.get() - 110.0);
    // Each step it reaches lights up (an outline: text on a solid accent
    // fill would be hard to read).
    for (i, step) in boxes.iter().enumerate().skip(3) {
        let (x, y) = spot(i);
        all((packet.x().to(x, 1.0), packet.y().to(y + 110.0, 1.0))).await;
        step.stroke().to(Accent, 0.3).await;
    }

    // Back out to the whole picture.
    cam.fit(world, 140.0, 1.4).await;
    wait_for(1.0).await;
}

pub fn project() -> Project {
    Project::new("camera").scene(camera())
}
