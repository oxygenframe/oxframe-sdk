//! Shapes and layout: polygons, stars, arcs, smooth curves and a flexbox
//! row whose items grow and wrap.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn shapes(view: View) {
    let poly = view.add(
        Polygon::new()
            .sides(3.0)
            .size(220.0)
            .position((-600.0, -250.0))
            .fill(color!("#68abdf")),
    );
    let star = view.add(
        Star::new()
            .points(5.0)
            .size(240.0)
            .position((-250.0, -250.0))
            .fill(color!("#f5c242")),
    );
    let ring = view.add(
        Circle::new()
            .size(220.0)
            .position((100.0, -250.0))
            .end_angle(0.0)
            .fill(Color::TRANSPARENT)
            .stroke(color!("#e13238"))
            .line_width(18.0)
            .line_cap(LineCap::Round),
    );
    let pie = view.add(
        Circle::new()
            .size(220.0)
            .position((450.0, -250.0))
            .pie(true)
            .end_angle(0.0)
            .fill(color!("#2ecc71")),
    );
    let wave = view.add(
        Line::new()
            .points([
                (-700.0, 60.0),
                (-450.0, -40.0),
                (-200.0, 120.0),
                (50.0, 0.0),
                (300.0, 110.0),
                (600.0, 20.0),
            ])
            .smooth(true)
            .stroke(color!("#68abdf"))
            .line_width(8.0)
            .end(0.0),
    );
    all((
        poly.sides().to(8.0, 2.0),
        star.points().to(9.0, 2.0),
        star.inner().to(0.7, 2.0),
        ring.end_angle().to(300.0, 2.0),
        pie.end_angle().to(290.0, 2.0),
        wave.end().to(1.0, 2.0),
    ))
    .await;

    // Flexbox: the middle card grows, the row wraps when it gets narrow.
    let row = view.add(
        Layout::new()
            .width(1200.0)
            .gap(20.0)
            .padding(20.0)
            .radius(24.0)
            .fill(color!("#26282e"))
            .wrap(true)
            .y(300.0),
    );
    let colors = ["#e13238", "#f5c242", "#68abdf", "#2ecc71", "#9b59b6"];
    let mut cards = Vec::new();
    for (i, c) in colors.iter().enumerate() {
        let card = row.add(
            Rect::new()
                .width(160.0)
                .height(100.0)
                .radius(16.0)
                .fill(Color::from_hex(c).unwrap()),
        );
        if i == 1 {
            card.grow().set(1.0);
        }
        cards.push(card);
    }
    wait_for(0.6).await;
    row.width().to(Some(600.0), 1.5).await;
    wait_for(0.6).await;
    row.width().to(Some(1200.0), 1.5).await;
    wait_for(0.6).await;
}

pub fn project() -> Project {
    Project::new("shapes").scene(shapes())
}
