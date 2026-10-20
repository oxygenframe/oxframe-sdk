//! Text: typewriter effect, animated weight and size, glyph-by-glyph reveal.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn text(view: View) {
    let title = view.add(Txt::new().text("").font_size(96.0).weight(300.0).y(-120.0));
    // Strings tween like a typewriter.
    title
        .text()
        .to("Hello, oxframe", 1.2)
        .ease(presets::LINEAR)
        .await;
    all((
        title.weight().to(800.0, 0.8),
        title.font_size().to(120.0, 0.8),
    ))
    .await;
    // Retype: deletes back to the common prefix, then types the rest.
    title
        .text()
        .to("Hello, Rust", 1.0)
        .ease(presets::LINEAR)
        .await;

    let code = view.add(
        Txt::new()
            .text("async fn intro(view: View) { .. }")
            .family(MONO)
            .font_size(44.0)
            .fill(color!("#68abdf"))
            .y(60.0)
            .end(0.0),
    );
    // `end` reveals glyph by glyph.
    code.end().to(1.0, 1.5).ease(presets::LINEAR).await;

    let note = view.add(
        Txt::new()
            .text("Кириллица тоже работает — встроенный Inter")
            .font_size(40.0)
            .fill(color!("#f5c242"))
            .y(180.0)
            .opacity(0.0),
    );
    note.opacity().to(1.0, 0.6).await;
    wait_for(1.0).await;
}

pub fn project() -> Project {
    Project::new("text").scene(text())
}
