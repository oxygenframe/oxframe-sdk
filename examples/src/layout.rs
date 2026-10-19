//! Layout: rows and columns size themselves to content; animating a
//! child's size re-flows its siblings.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn layout(view: View) {
    let colors = [
        color!("#e13238"),
        color!("#f5c242"),
        color!("#68abdf"),
        color!("#2ecc71"),
    ];
    let row = view.add(
        Layout::new()
            .gap(24.0)
            .padding(24.0)
            .radius(24.0)
            .fill(color!("#262626")),
    );
    let mut cards = Vec::new();
    for (i, c) in colors.into_iter().enumerate() {
        let card = row.add(
            Layout::new()
                .column()
                .gap(12.0)
                .padding(20.0)
                .radius(16.0)
                .fill(c)
                .opacity(0.0)
                .child(
                    Txt::new()
                        .text(format!("{}", i + 1))
                        .font_size(64.0)
                        .weight(800.0),
                )
                .child(Txt::new().text("card").font_size(28.0)),
        );
        cards.push(card);
    }
    // Staggered fade-in.
    sequence(
        0.15,
        cards
            .iter()
            .map(|c| -> oxframe::BoxFuture { Box::pin(c.opacity().to(1.0, 0.5)) })
            .collect(),
    )
    .await;

    // Growing the gap and one card's padding pushes the others aside.
    all((row.gap().to(60.0, 0.8), cards[1].padding().to(60.0, 0.8))).await;
    wait_for(0.4).await;
    all((row.gap().to(24.0, 0.8), cards[1].padding().to(20.0, 0.8))).await;

    // Removing a card re-flows the row.
    cards[2].opacity().to(0.0, 0.4).await;
    cards[2].remove();
    wait_for(0.6).await;
}

pub fn project() -> Project {
    Project::new("layout").scene(layout())
}
