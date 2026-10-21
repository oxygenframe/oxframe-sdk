//! A node of your own behaves like a built-in one: same pixels, a box for
//! layout, its type name in check, and effects and cameras around it.

use oxframe::nodes::{NodeBuilder, SceneGraph, View};
use oxframe::prelude::*;
use oxframe::render::{CpuRenderer, RenderOptions, Renderer};

/// `Star` rewritten on the public node API.
#[derive(Node)]
struct MyStar {
    #[signal]
    points: f64,
    #[signal]
    size: f64,
    #[signal]
    inner: f64,
}

impl Draw for MyStar {
    fn draw(&self, ctx: &mut DrawCtx) {
        let n = self.points.round().max(2.0) as usize;
        let r = self.size.max(0.0) / 2.0;
        let mut path = BezPath::new();
        for i in 0..2 * n {
            let radius = if i % 2 == 1 {
                r * self.inner.clamp(0.0, 1.0)
            } else {
                r
            };
            let a =
                -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / (2 * n) as f64;
            let p = Point::new(a.cos() * radius, a.sin() * radius);
            if i == 0 {
                path.move_to(p);
            } else {
                path.line_to(p);
            }
        }
        path.close_path();
        ctx.path(path).fill(Color::rgb(1.0, 0.0, 0.0));
    }

    fn size(&self) -> (f64, f64) {
        (self.size, self.size)
    }
}

fn render(graph: &SceneGraph) -> Vec<u8> {
    CpuRenderer::new()
        .render(
            graph,
            &RenderOptions {
                width: 200,
                height: 200,
                background: Color::BLACK,
            },
        )
        .data
}

#[test]
fn a_star_on_the_public_api_paints_the_same_pixels_as_the_built_in_one() {
    let builtin = SceneGraph::new(200, 200);
    builtin.enter(|| {
        View::current().add(
            Star::new()
                .points(5.0)
                .size(160.0)
                .inner(0.45)
                .fill(Color::rgb(1.0, 0.0, 0.0)),
        );
    });
    let mine = SceneGraph::new(200, 200);
    mine.enter(|| {
        View::current().add(MyStar::new().points(5.0).size(160.0).inner(0.45));
    });
    assert_eq!(render(&builtin), render(&mine));
}

#[test]
fn signals_animate_and_the_box_follows_them() {
    let graph = SceneGraph::new(800, 400);
    let star = graph.enter(|| View::current().add(MyStar::new().points(5.0).size(100.0)));
    graph.enter(|| {
        assert_eq!(star.size().get(), 100.0);
        star.size().set(240.0);
    });
    assert_eq!(graph.measure(star.id()), (240.0, 240.0));
}

#[test]
fn layout_places_it_by_its_size_and_check_names_its_type() {
    let graph = SceneGraph::new(800, 400);
    graph.enter(|| {
        let row = View::current().add(Layout::new().gap(20.0));
        row.add(MyStar::new().size(100.0));
        row.add(MyStar::new().size(100.0));
    });
    let stars: Vec<_> = graph
        .inspect_nodes()
        .into_iter()
        .filter(|n| n.kind == "MyStar")
        .collect();
    assert_eq!(stars.len(), 2, "check and inspect use the type name");
    let gap = stars[1].bounds[0] - stars[0].bounds[2];
    assert!(
        (gap - 20.0).abs() < 0.5,
        "flex spaced them by their box: {gap}"
    );
}

#[test]
fn effects_and_a_camera_work_around_it() {
    let graph = SceneGraph::new(400, 400);
    graph.enter(|| {
        let cam = View::current().add(Camera::new());
        let star = cam.add(MyStar::new().size(160.0));
        star.effect(shaders::ripple());
        cam.zoom().set(2.0);
    });
    let items = graph.draw_list();
    assert!(
        items
            .iter()
            .any(|i| matches!(i.prim, oxframe::nodes::Primitive::BeginEffect(_)))
    );
    assert!(
        graph.shader_diags().is_empty(),
        "{:?}",
        graph.shader_diags()
    );
}
