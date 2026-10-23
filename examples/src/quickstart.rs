//! The README's quickstart as a short film: install, init, render in a
//! terminal, then the rendered scene plays. The command output is what
//! oxframe-cli 0.21.0 prints. Rendered to docs/quickstart.gif with
//! `scripts/readme-gif.sh`.

use oxframe::prelude::*;

const W: f64 = 1180.0;
const LINE: f64 = 38.0;

#[oxframe::scene]
pub async fn quickstart(view: View) {
    let term = view.add(
        Rect::new()
            .width(W)
            .height(600.0)
            .radius(14.0)
            .fill(color!("#1b1b21"))
            .stroke(color!("#33333d"))
            .line_width(2.0),
    );
    let dots = [color!("#ff5f57"), color!("#febc2e"), color!("#28c840")];
    for (i, c) in dots.into_iter().enumerate() {
        let _ = term.add(
            Circle::new()
                .size(14.0)
                .fill(c)
                .x(-W / 2.0 + 28.0 + 24.0 * i as f64)
                .y(-274.0),
        );
    }
    let left = -W / 2.0 + 36.0;
    let mut y = -225.0;
    let mut row = || {
        let at = y;
        y += LINE;
        at
    };

    for (command, output) in [
        (
            "cargo install oxframe-cli",
            vec!["  Installed package `oxframe-cli v0.21.0` (executable `oxframe`)"],
        ),
        ("oxframe init my-video", vec!["oxframe: created `my-video`"]),
        (
            "cd my-video && oxframe render",
            vec![
                "    Finished `release` profile [optimized] target(s) in 42.57s",
                "rendering 181 frames at 1920x1080 @ 60 fps in 2 segments → out/my-video.mp4",
                "done in 1.33s (135.7 frames/s, 2 jobs): out/my-video.mp4",
            ],
        ),
    ] {
        let at = row();
        let _ = term.add(prompt().x(left).y(at));
        let typed = term.add(mono("").x(left + 28.0).y(at));
        typed
            .text()
            .to(command, 0.04 * command.len() as f64)
            .ease(presets::LINEAR)
            .await;
        wait_for(0.25).await;
        for text in output {
            let line = term.add(mono(text).fill(Muted).x(left).y(row()).opacity(0.0));
            line.fade_in().await;
        }
        wait_for(0.35).await;
    }

    // out/my-video.mp4: the scene `oxframe init` starts with.
    let player = view.add(
        Rect::new()
            .width(640.0)
            .height(360.0)
            .radius(12.0)
            .fill(color!("#111114"))
            .stroke(color!("#44444f"))
            .line_width(2.0)
            .y(20.0)
            .clip(true),
    );
    let name = view.add(
        mono("out/my-video.mp4")
            .fill(Muted)
            .x(-320.0)
            .y(226.0)
            .opacity(0.0),
    );
    all((
        term.opacity().to(0.25, 0.5),
        player.slide_in(Side::Bottom),
        name.fade_in(),
    ))
    .await;
    let square = player.add(Rect::new().size(90.0).x(-150.0).fill(color!("#e13238")));
    all((
        square.x().to(150.0, 1.0),
        square.radius().to(45.0, 1.0),
        square.fill().to(color!("#68abdf"), 1.0),
    ))
    .await;
    wait_for(1.2).await;
}

fn prompt() -> oxframe::nodes::TxtBuilder {
    mono("$").fill(Success)
}

fn mono(text: &str) -> oxframe::nodes::TxtBuilder {
    Txt::new()
        .text(text)
        .family(MONO)
        .font_size(22.0)
        .anchor((-1.0, 0.0))
}

pub fn project() -> Project {
    Project::new("quickstart")
        .size(1280, 720)
        .fps(30)
        .scene(quickstart())
}
