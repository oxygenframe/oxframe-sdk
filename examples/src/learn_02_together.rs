//! Lesson 2 — One after another, or all at once.
//!
//! Each `.await` waits, so animations written one after another play in
//! order. `all((..))` plays several at the same time; `stagger` starts them
//! with a small delay between each.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn together(view: View) {
    let row = |y: f64, c: Color| view.add(Rect::new().size(90.0).position((-500.0, y)).fill(c));

    // 1. In order: the second square starts when the first one stops.
    let a = row(-250.0, color!("#e13238"));
    let b = row(-130.0, color!("#e13238"));
    a.x().to(500.0, 0.8).await;
    b.x().to(500.0, 0.8).await;

    // 2. At the same time: `all` waits for the longest one.
    let c = row(10.0, color!("#68abdf"));
    let d = row(130.0, color!("#68abdf"));
    all((c.x().to(500.0, 0.8), d.x().to(500.0, 1.6))).await;

    // 3. A pause, then a staggered wave of five squares.
    wait_for(0.3).await;
    let wave: Vec<_> = (0..5)
        .map(|i| {
            view.add(
                Circle::new()
                    .size(60.0)
                    .position((-500.0 + i as f64 * 70.0, 280.0))
                    .fill(color!("#2ecc71")),
            )
        })
        .collect();
    stagger(0.1, &wave, |c| c.y().to(200.0, 0.4)).await;
}

// Try: swap the two lines in step 1 into an `all((..))` and watch them move together.

pub fn project() -> Project {
    Project::new("together").scene(together())
}
