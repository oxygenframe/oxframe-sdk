//! A 3D node inside a 2D diagram: a lit cube turns in a `View3d`, and 2D
//! labels follow points on its faces.

use oxframe::prelude::*;

/// A label and a leader line pinned to a point on the cube.
fn pin(world: &Scene3d, cube: &Object3d, frame: View3d, text: &str, local: Vec3, offset: Vec2) {
    let (w, c, to_view) = (world.clone(), cube.clone(), frame.projector());
    let at = move || to_view(w.world_point(&c, local));
    let label = frame.add(Txt::new().text(text).caption().opacity(0.0));
    let line = frame.add(
        Line::new()
            .points([(0.0, 0.0), (0.0, 0.0)])
            .stroke(Muted)
            .line_width(2.0)
            .opacity(0.0),
    );
    let a = at.clone();
    label.x().bind(move || a().x + offset.x);
    let a = at.clone();
    label.y().bind(move || a().y + offset.y);
    line.points().bind(move || {
        let p = at();
        vec![p + offset * 0.8, p]
    });
    spawn(async move {
        all((label.fade_in(), line.fade_in())).await;
    });
}

#[oxframe::scene]
pub async fn diagram(view: View) {
    view.add(
        Txt::new()
            .text("A 3D node in a 2D diagram")
            .h1()
            .to_edge(Edge::Top),
    );
    let world = Scene3d::new();
    let cube = world.add(Mesh::cube().color(Accent).roughness(0.35));
    let frame = view.add(View3d::new(&world).size(760.0, 520.0).y(40.0));

    // The diagram around it is plain 2D.
    let code = view.add(
        Rect::new()
            .width(340.0)
            .height(90.0)
            .radius(Lg)
            .fill(Surface)
            .x(-740.0)
            .y(40.0),
    );
    code.add(Txt::new().text("Scene3d").body());
    let out = view.add(
        Rect::new()
            .width(340.0)
            .height(90.0)
            .radius(Lg)
            .fill(Surface)
            .x(740.0)
            .y(40.0),
    );
    out.add(Txt::new().text("MP4 · PNG · check").body());
    for (a, b) in [
        ((-560.0, 40.0), (-400.0, 40.0)),
        ((400.0, 40.0), (560.0, 40.0)),
    ] {
        view.add(
            Line::new()
                .points([a, b])
                .end_arrow(true)
                .stroke(Muted)
                .line_width(4.0),
        );
    }

    pin(
        &world,
        &cube,
        frame,
        "top",
        Vec3::new(0.0, 0.5, 0.0),
        Vec2::new(-120.0, -110.0),
    );
    pin(
        &world,
        &cube,
        frame,
        "front",
        Vec3::new(0.0, 0.0, 0.5),
        Vec2::new(150.0, 90.0),
    );

    // One full turn while the camera moves in.
    all((
        cube.rotation().to(Vec3::new(0.0, 360.0, 0.0), 6.0),
        world.camera().position().to(Vec3::new(2.2, 1.8, 3.0), 6.0),
    ))
    .await;
    cube.color().to(Success, 0.6).await;
    wait_for(0.5).await;
}

pub fn project() -> Project {
    Project::new("cube_3d").scene(diagram())
}
