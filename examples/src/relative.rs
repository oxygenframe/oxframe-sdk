//! A title, a card and a caption with no pixel coordinates.
//! `Project::aspects` is what makes the same scene 16:9, 9:16 and 1:1.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn relative(view: View) {
    let title = view.add(
        Txt::new()
            .text("Binary search")
            .h1()
            .to_edge(Edge::Top)
            .name("title"),
    );
    let card = view.add(
        Rect::new()
            .width(pct(80))
            .height(280.0)
            .radius(Lg)
            .fill(Surface)
            .name("card"),
    );
    view.add(
        Txt::new()
            .text("mid")
            .caption()
            .fill(Muted)
            .next_to(card, Side::Bottom)
            .name("label"),
    );
    title.fade_in().await;
    // The label is tied to the card, so it moves with it.
    card.y().to(-40.0, 1.0).await;
}

pub fn project() -> Project {
    Project::new("relative")
        .aspects([Aspect::Landscape, Aspect::Portrait, Aspect::Square])
        .scene(relative())
}
