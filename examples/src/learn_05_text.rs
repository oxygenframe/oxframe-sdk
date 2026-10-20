//! Lesson 5 — Text.
//!
//! `Txt` draws text with the built-in Inter font (or JetBrains Mono with
//! `.family(MONO)`). Text animates like anything else: tweening the text
//! types it out, `end` reveals it letter by letter, and `rich` mixes styles.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn text(view: View) {
    let title = view.add(Txt::new().text("").font_size(96.0).weight(800.0).y(-150.0));
    // Typewriter: from "" to the full text.
    title
        .text()
        .to("Hello, text", 1.0)
        .ease(presets::LINEAR)
        .await;

    let line = view.add(
        Txt::new()
            .font_size(44.0)
            .rich([
                span("Mix "),
                span("bold").bold(),
                span(", "),
                span("color").color(color!("#f5c242")),
                span(" and "),
                span("italic").italic(),
            ])
            .end(0.0),
    );
    // Reveal the line letter by letter.
    line.end().to(1.0, 1.0).ease(presets::LINEAR).await;

    let code = view.add(
        Code::new()
            .code("let x = 42; // code, highlighted")
            .font_size(36.0)
            .y(150.0)
            .opacity(0.0),
    );
    code.opacity().to(1.0, 0.5).await;
    wait_for(0.5).await;
}

// Try: `title.morph_to("Hello, world", 1.0).await;` — shared letters slide into place.

pub fn project() -> Project {
    Project::new("text").scene(text())
}
