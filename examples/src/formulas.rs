//! Formulas set by Typst: write one on glyph by glyph, then recolor it.
//!
//! Needs the `math` feature (`oxframe = { features = ["math"] }`), so it is
//! not in the playground: run it with
//! `cargo run -p oxframe-examples --features math --bin formulas -- preview`.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn formulas(view: View) {
    let title = view.add(Txt::new().text("Set by Typst").h2().y(-300.0));
    title.fade_in().await;

    // Typst math syntax, without the `$`.
    let euler = view.add(Math::new("e^(i pi) + 1 = 0").size(140.0).y(-60.0));
    // Every glyph is a path, in reading order.
    stagger(0.08, euler.paths(), |glyph| glyph.pop_in()).await;
    euler.color().to(Accent, 0.6).await;
    wait_for(0.4).await;

    // `.display()` sets it on its own line: limits above and below the sum.
    let sum = view.add(
        Math::new("sum_(k=1)^n k = (n(n + 1)) / 2")
            .display()
            .size(72.0)
            .y(220.0),
    );
    sum.fade_in().await;
    wait_for(1.0).await;
}

pub fn project() -> Project {
    Project::new("formulas").scene(formulas())
}
