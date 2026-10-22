//! A release's notes as a video: the version, then each section with its
//! changes appearing word by word, a halo over each one, and an end card.
//!
//! release-video-action generates scenes like these from a GitHub release.

use oxframe::nodes::Ink;
use oxframe::prelude::*;

/// Where each word of `text` ends, as a fraction of its drawn glyphs
/// (`end` counts glyphs, and spaces draw none).
fn word_ends(text: &str) -> Vec<f64> {
    let total = text.chars().filter(|c| !c.is_whitespace()).count().max(1) as f64;
    let mut seen = 0;
    text.split_whitespace()
        .map(|word| {
            seen += word.chars().count();
            seen as f64 / total
        })
        .collect()
}

/// The text color at rest: a step towards the background, so a halo can
/// brighten it.
fn rest() -> Color {
    let theme = oxframe::nodes::current_theme();
    theme.ink(Text).mix(theme.ink(Background), 0.18)
}

/// A short halo over `line`: its glow and brightness go up, then back.
async fn halo(line: Txt, glow: Color) {
    let peak = oxframe::nodes::current_theme().ink(Text);
    all((
        line.shadow_blur().to(16.0, 0.16),
        line.shadow_color().to(glow, 0.16),
        line.fill().to(peak, 0.16),
    ))
    .await;
    all((
        line.shadow_blur().to(0.0, 0.34),
        line.shadow_color().to(glow.with_alpha(0.0), 0.34),
        line.fill().to(rest(), 0.34),
    ))
    .await;
}

/// Writes `line` on word by word, then runs the halo over it.
async fn write_on(line: Txt, text: &str, glow: Color) {
    for end in word_ends(text) {
        line.end().to(end, 0.06).ease(presets::OUT_CUBIC).await;
    }
    halo(line, glow).await;
}

/// A calm sea along the bottom edge; it keeps moving while the text holds.
fn sea(view: View) {
    let h = f64::from(view.size().1);
    view.add(
        shaders::waves()
            .amp(0.06)
            .speed(0.5)
            .deep(Accent)
            .foam(Background)
            .width(pct(100))
            .height(h * 0.11)
            .y(h / 2.0 - h * 0.055)
            .opacity(0.35),
    );
}

/// One section: its heading in its color, then the changes one by one,
/// earlier ones stepping back.
async fn section(view: View, heading: &str, tone: Ink, items: &[&str]) {
    let (w, h) = view.size();
    let (w, h) = (f64::from(w), f64::from(h));
    let width = w - 280.0;
    let glow = oxframe::nodes::current_theme().ink(tone);
    // Added before the transition, so the heading comes in with the scene.
    let title = view.add(
        Txt::new()
            .text(heading)
            .font_size(72.0)
            .weight(800.0)
            .fill(tone)
            .to_edge(Edge::Left)
            .y(-h * 0.34),
    );
    transition(Transition::slide(Dir::Left)).await;
    let list = view.add(
        Layout::new()
            .column()
            .gap(44.0)
            .align(LayoutAlign::Start)
            .width(width)
            .to_edge(Edge::Left),
    );
    let mut rows = Vec::new();
    for &text in items {
        let row = list.add(
            Layout::new()
                .row()
                .gap(30.0)
                .align(LayoutAlign::Start)
                .justify(Justify::Start)
                .width(width),
        );
        let mark = row.add(
            Rect::new()
                .size(20.0)
                .radius(5.0)
                .fill(tone)
                .margin(20.0)
                .scale(0.0),
        );
        let line = row.add(
            Txt::new()
                .text(text)
                .font_size(56.0)
                .weight(600.0)
                .line_height(1.25)
                .max_width(width - 110.0)
                .fill(rest())
                .end(0.0)
                .glow(glow.with_alpha(0.0), 0.0),
        );
        rows.push((row, mark, line, text));
    }
    // The list hangs under the heading, so a long one grows downwards.
    let top = title.y().get() + title.size().1 / 2.0 + 56.0;
    list.y().set(top + list.size().1 / 2.0);
    for (i, &(_, mark, line, text)) in rows.iter().enumerate() {
        if i > 0 {
            rows[i - 1].0.opacity().to(0.45, 0.3).await;
        }
        mark.scale()
            .to(Vec2::new(1.0, 1.0), 0.25)
            .ease(presets::OUT_BACK)
            .await;
        write_on(line, text, glow).await;
        // Time to read the rest of it.
        wait_for((text.chars().count() as f64 / 30.0).clamp(1.0, 3.0)).await;
    }
}

#[oxframe::scene]
pub async fn version(view: View) {
    sea(view);
    let repo = view.add(
        Txt::new()
            .text("oxygenframe/oxframe-sdk")
            .h2()
            .fill(Muted)
            .y(-110.0)
            .opacity(0.0),
    );
    let version = view.add(
        Txt::new()
            .text("v0.20.0")
            .font_size(150.0)
            .weight(800.0)
            .y(20.0)
            .fill(rest())
            .opacity(0.0),
    );
    all((repo.fade_in(), version.slide_in(Side::Bottom))).await;
    halo(version, oxframe::nodes::current_theme().ink(Accent)).await;
    wait_for(1.2).await;
}

#[oxframe::scene]
pub async fn added(view: View) {
    sea(view);
    section(
        view,
        "Added",
        Success,
        &[
            "render --jobs renders a video in 2-second segments, several at once.",
            "The file does not depend on the number of jobs: --jobs 1 and --jobs 8 write the same bytes.",
        ],
    )
    .await;
}

#[oxframe::scene]
pub async fn changed(view: View) {
    sea(view);
    section(
        view,
        "Changed",
        Warning,
        &["A video longer than two seconds has a key frame every two seconds, where the segments meet."],
    )
    .await;
}

#[oxframe::scene]
pub async fn credits(view: View) {
    sea(view);
    transition(Transition::fade()).await;
    let version = view.add(
        Txt::new()
            .text("v0.20.0")
            .font_size(110.0)
            .weight(800.0)
            .y(-30.0)
            .fill(rest())
            .opacity(0.0),
    );
    let credit = view.add(
        Txt::new()
            .text("made with oxframe")
            .h2()
            .fill(Muted)
            .y(80.0)
            .opacity(0.0),
    );
    version.fade_in().await;
    halo(version, oxframe::nodes::current_theme().ink(Accent)).await;
    credit.fade_in().await;
    wait_for(1.2).await;
}

pub fn project() -> Project {
    Project::new("release")
        .scene(version())
        .scene(added())
        .scene(changed())
        .scene(credits())
}
