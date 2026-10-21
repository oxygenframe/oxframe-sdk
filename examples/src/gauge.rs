//! A node of your own: a gauge that draws an arc with `DrawCtx` and
//! animates its value like any built-in node.

use oxframe::prelude::*;

/// A dial from 0 to 1.
#[derive(Node)]
pub struct Gauge {
    /// 0 is empty, 1 is full.
    #[signal]
    value: f64,
    /// Diameter in pixels.
    #[signal]
    size: f64,
}

impl Draw for Gauge {
    fn draw(&self, ctx: &mut DrawCtx) {
        let r = self.size / 2.0 - 12.0;
        ctx.arc((0.0, 0.0), r, 135.0, 270.0).stroke(Surface, 18.0);
        ctx.arc((0.0, 0.0), r, 135.0, 270.0 * self.value.clamp(0.0, 1.0))
            .stroke(Accent, 18.0);
    }

    fn size(&self) -> (f64, f64) {
        (self.size, self.size)
    }
}

#[oxframe::scene]
pub async fn dials(view: View) {
    let row = view.add(Layout::new().gap(80.0));
    let gauges: Vec<GaugeHandle> = [0.35, 0.8, 0.55]
        .into_iter()
        .map(|_| row.add(Gauge::new().size(260.0)))
        .collect();
    view.add(
        Txt::new()
            .text("A node of your own")
            .h1()
            .to_edge(Edge::Top),
    );
    stagger(0.2, [0.35, 0.8, 0.55].into_iter().enumerate(), |(i, v)| {
        gauges[i].value().to(v, 1.0)
    })
    .await;
    wait_for(0.5).await;
}

pub fn project() -> Project {
    Project::new("gauge").scene(dials())
}
