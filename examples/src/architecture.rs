//! An architecture diagram with icons: `icon!` names an icon from Lucide,
//! Tabler or Simple Icons, and only those end up in the binary.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn request(view: View) {
    view.add(
        Txt::new()
            .text("How a request is served")
            .h1()
            .to_edge(Edge::Top),
    );
    let flow = view.add(
        FlowDiagram::new()
            .edge("browser", "edge")
            .edge("edge", "api")
            .edge("api", "cache")
            .edge("api", "database")
            .icon("browser", icon!("lucide:globe"))
            .icon("edge", icon!("lucide:cloud"))
            .icon("api", icon!("lucide:server"))
            .icon("cache", icon!("tabler:bolt"))
            .icon("database", icon!("lucide:database"))
            .opacity(0.0),
    );
    flow.enter().await;
    for _ in 0..flow.len() {
        flow.next().await;
    }
    // A slow query: the database turns red and back.
    if let Some(db) = flow.icon("database") {
        db.color().to(Danger, 0.3).await;
        db.stroke_width().to(3.0, 0.3).await;
        wait_for(0.4).await;
        all((db.color().to(Text, 0.4), db.stroke_width().to(2.0, 0.4))).await;
    }

    let logos = view.add(Layout::new().gap(56.0).y(300.0).opacity(0.0));
    for name in [
        icon!("simple:github"),
        icon!("simple:rust"),
        icon!("simple:cloudflare"),
    ] {
        logos.add(name.size(56.0).color(Muted));
    }
    logos.fade_in().await;
    wait_for(0.6).await;
}

pub fn project() -> Project {
    Project::new("architecture").scene(request())
}
