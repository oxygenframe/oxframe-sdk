//! The oxframe promo: one scene, one camera, no cuts. The voice is committed
//! in `examples/tts/`; `--features elevenlabs` and `ELEVENLABS_API_KEY` make it again.

use std::future::Future;

use oxframe::prelude::*;

#[cfg(feature = "elevenlabs")]
mod tts;

fn asset(name: &str) -> String {
    format!("{}/assets/one_shot/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// Lines of Rust in this video: the scenes plus the voice provider.
fn rust_lines() -> usize {
    include_str!("one_shot.rs").lines().count() + include_str!("one_shot/tts.rs").lines().count()
}

/// A box in world space: center x, center y, width, height.
type Area = (f64, f64, f64, f64);

/// Zoom that fits `area` in the frame with `pad` pixels to spare.
fn zoom_for(view: &View, (_, _, w, h): Area, pad: f64) -> f64 {
    let (fw, fh) = view.size();
    ((fw as f64 - 2.0 * pad) / w).min((fh as f64 - 2.0 * pad) / h)
}

/// Pulls the camera back until `area` fills the frame. Zoom moves in log
/// space and the center follows it, so every level of the zoom feels the same.
fn pull_back(view: &View, cam: Camera, area: Area, pad: f64, secs: f64) -> impl Future<Output = ()> + 'static {
    let (z0, x0, y0) = (cam.zoom().get(), cam.x().get(), cam.y().get());
    let (x1, y1, z1) = (area.0, area.1, zoom_for(view, area, pad));
    let p = Signal::new(0.0f64);
    let zoom = {
        let p = p.clone();
        move || (z0.ln() + (z1.ln() - z0.ln()) * p.get()).exp()
    };
    // Share of the start offset that is left: 1 at the start, 0 at the end.
    let left = {
        let zoom = zoom.clone();
        move || {
            let d = 1.0 / z0 - 1.0 / z1;
            if d.abs() < 1e-9 { 0.0 } else { (1.0 / zoom() - 1.0 / z1) / d }
        }
    };
    cam.zoom().bind(zoom);
    let l = left.clone();
    cam.x().bind(move || x1 + (x0 - x1) * l());
    cam.y().bind(move || y1 + (y0 - y1) * left());
    p.to(1.0, secs).ease(presets::IN_OUT_CUBIC)
}

/// Outline of a flat sea `w` wide and `h` tall: a sine crest of `amp` pixels
/// on top, the box edges below. `phase` slides the crest sideways.
fn sea_outline(w: f64, h: f64, amp: f64, phase: f64) -> Vec<Vec2> {
    let n = 96;
    let mut pts: Vec<Vec2> = (0..=n)
        .map(|i| {
            let x = -w / 2.0 + w * i as f64 / n as f64;
            Vec2::new(x, -h / 2.0 + amp + amp * (x / w * 3.0 * std::f64::consts::TAU + phase).sin())
        })
        .collect();
    pts.push(Vec2::new(w / 2.0, h / 2.0));
    pts.push(Vec2::new(-w / 2.0, h / 2.0));
    pts
}

/// A flat vector sea; returns its crest height in pixels as a signal.
fn add_sea(parent: Group, w: f64, h: f64, y: f64, amp: f64, phase: Signal<f64>) -> Signal<f64> {
    let crest = Signal::new(amp);
    let sea = parent.add(Line::new().closed(true).fill(Accent).line_width(0.0).y(y).name("sea"));
    let (c, p) = (crest.clone(), phase);
    sea.points().bind(move || sea_outline(w, h, c.get(), p.get()));
    crest
}

fn click() {
    sound(asset("click.wav")).volume(0.5).play();
}

/// A small diagram node: rounded box, icon, label.
fn diagram_node(parent: Group, x: f64, y: f64, label: &str, icon: oxframe::nodes::IconBuilder) -> Group {
    let g = parent.add(Group::new().x(x).y(y).opacity(0.0).name(label));
    g.add(Rect::new().width(104.0).height(40.0).radius(10.0).fill(Background));
    g.add(icon.size(18.0).x(-32.0));
    g.add(Txt::new().text(label).font_size(14.0).x(12.0));
    g
}

/// The whole video is one pull-back through nested levels; the edit at 5 s
/// pays off in the sea at 28 s.
#[oxframe::scene]
pub async fn oner(view: View) {
    let cam = view.add(Camera::new());
    let shot = cam.add(Group::new().name("shot"));

    // Level 4: the frame with the sea.
    const FRAME: Area = (0.0, 0.0, 1920.0, 1080.0);
    shot.add(Rect::new().width(1920.0).height(1080.0).fill(Background).name("frame"));
    // The crest drifts slowly for the whole shot.
    let phase = Signal::new(0.0f64);
    spawn(phase.to(-24.0, 50.0).ease(presets::LINEAR));
    let sea = add_sea(shot, 1920.0, 432.0, 324.0, SEA_PX * 0.04, phase.clone());

    // Level 3: the dashboard, hovering over the sea.
    const DASH: Area = (0.0, -150.0, 1240.0, 720.0);
    shot.add(
        Rect::new()
            .width(DASH.2)
            .height(DASH.3)
            .radius(24.0)
            .fill(Background)
            .stroke(Muted)
            .line_width(2.0)
            .y(DASH.1)
            .elevation(Md)
            .name("dashboard"),
    );
    let panels = [(-305.0, -325.0), (305.0, -325.0), (-305.0, 25.0), (305.0, 25.0)];
    for (i, (x, y)) in panels.iter().enumerate() {
        shot.add(Rect::new().width(590.0).height(330.0).radius(16.0).fill(Surface).x(*x).y(*y).name(format!("panel-{i}")));
    }

    // Level 2: the diagram in the first panel.
    let (dx, dy) = panels[0];
    const DIAGRAM: Area = (-305.0, -325.0, 590.0, 330.0);
    let diagram = shot.add(Group::new().name("diagram"));
    let agent = diagram_node(diagram, dx - 225.0, dy, "agent", icon!("lucide:bot"));
    let check = diagram_node(diagram, dx + 225.0, dy - 80.0, "check", icon!("lucide:circle-check"));
    let render = diagram_node(diagram, dx + 225.0, dy + 80.0, "render", icon!("lucide:clapperboard"));
    let arrows: Vec<Line> = [
        ((dx - 173.0, dy), (dx - 125.0, dy)),
        ((dx + 115.0, dy - 12.0), (dx + 173.0, dy - 70.0)),
        ((dx + 115.0, dy + 12.0), (dx + 173.0, dy + 70.0)),
    ]
    .into_iter()
    .map(|(a, b)| diagram.add(Line::new().points([a, b]).end_arrow(true).arrow_size(8.0).stroke(Muted).line_width(2.5).end(0.0)))
    .collect();

    // Level 1: the line of code, as one node of the diagram.
    let card = diagram.add(
        Rect::new().width(230.0).height(64.0).radius(10.0).fill(Background).stroke(Accent).line_width(1.5).x(dx).y(dy).name("code-card"),
    );
    let code = diagram.add(
        Code::new()
            .code("let sea = view.add(\n    Sea::new().amp(0.04));")
            .language("rust")
            .font_size(10.0)
            .x(dx)
            .y(dy)
            .name("code"),
    );
    let (cw, ch) = code.size();
    const CARD: Area = (-305.0, -325.0, 230.0, 64.0);

    // Level 0: the last `;` of line 2, the longest line (26 monospace chars).
    let glyph = cw / 26.0;
    let semicolon: Area = (dx + cw / 2.0 - glyph / 2.0, dy + ch / 4.0, glyph, glyph * 1.6);

    // The other three panels: bars, a sine with a rider, a clip.
    let (bx, by) = panels[1];
    let bar_axes = shot.add(Axes::new().x_range(-0.5..3.5).y_range(0.0..10.0).no_grid().width(500.0).height(250.0).x(bx).y(by));
    let bars = bar_axes.bars([0.0, 0.0, 0.0, 0.0]).labels(["Q1", "Q2", "Q3", "Q4"]);
    let (sx, sy) = panels[2];
    let sine_axes = shot.add(
        Axes::new().x_range(0.0..std::f64::consts::TAU).y_range(-1.2..1.2).no_grid().width(500.0).height(250.0).x(sx).y(sy),
    );
    let sine = sine_axes.plot(|x| x.sin()).stroke(Accent).width(3.0);
    let rider_x = Signal::new(0.0f64);
    sine_axes.point_on(&sine, rider_x.clone());
    let (vx, vy) = panels[3];
    let clip = shot.add(Video::new().file(asset("clip.mp4")).width(540.0).radius(16.0).muted(true).x(vx).y(vy).name("clip"));

    // Level 5: the frame is one cell of the check storyboard.
    let step = (2120.0, 1280.0);
    let mut cells = Vec::new();
    for row in -1i32..=1 {
        for col in -1i32..=1 {
            let (x, y) = (col as f64 * step.0, row as f64 * step.1);
            let k = (row + 1) * 3 + col + 1;
            let stamp = cam.add(
                Txt::new()
                    .text(format!("{:.2}", k as f64 * 5.0))
                    .font_size(72.0)
                    .fill(Muted)
                    .x(x - 880.0)
                    .y(y + 610.0)
                    .opacity(0.0),
            );
            cells.push(stamp.node());
            if (row, col) == (0, 0) {
                continue;
            }
            let cell = cam.add(Group::new().x(x).y(y).opacity(0.0).name(format!("cell-{k}")));
            cell.add(Rect::new().width(1920.0).height(1080.0).fill(Background).stroke(Surface).line_width(8.0));
            match k % 3 {
                0 => {
                    for i in 0..5 {
                        cell.add(Rect::new().width(600.0 + 160.0 * (i % 3) as f64).height(56.0).radius(28.0).fill(Surface).x(-200.0).y(-280.0 + 130.0 * i as f64));
                    }
                }
                1 => {
                    for i in 0..4 {
                        let h = 200.0 + 120.0 * ((i * 7 + k) % 4) as f64;
                        cell.add(Rect::new().width(220.0).height(h).radius(16.0).fill(Accent).x(-480.0 + 320.0 * i as f64).y(350.0 - h / 2.0));
                    }
                }
                _ => {
                    cell.add(Circle::new().size(560.0).fill(Surface));
                    cell.add(Rect::new().width(1920.0).height(300.0).fill(Accent).y(390.0));
                }
            }
            cells.push(cell.node());
        }
    }
    let outline = cam.add(Rect::new().width(1920.0).height(1080.0).fill(Color::rgba(0.0, 0.0, 0.0, 0.0)).stroke(Success).line_width(10.0).opacity(0.0).name("frame-outline"));
    const GRID: Area = (0.0, 0.0, 3.0 * 1920.0 + 2.0 * 200.0, 3.0 * 1080.0 + 2.0 * 200.0 + 200.0);
    let badge = view.add(Txt::new().text("oxframe check · 0 errors").h2().fill(Success).to_edge(Edge::Top).opacity(0.0).name("badge"));

    // Start on the semicolon.
    let z = zoom_for(&view, semicolon, 0.0);
    cam.zoom().set(z);
    cam.x().set(semicolon.0);
    cam.y().set(semicolon.1);

    // 0–5 s: one glyph, then the pull-back starts.
    let line = voice("One scene. One camera. No cuts.").start();
    wait_for(0.8).await;
    pull_back(&view, cam, CARD, 120.0, 4.4).await;
    line.await;

    // 5–12 s: the edit. The number waits until 28 s to matter.
    let line = voice("This number shapes the sea. Change it, and remember it.").start();
    code.focus_lines(2, 2, 0.4).await;
    line.at_word("Change").await;
    click();
    code.replace(((2, 19), (2, 23)), "0.14", 0.6).flash().await;
    line.await;
    wait_for(1.2).await;

    // 12–20 s: the card is one node of a graph.
    let line = voice("The line is one node in a graph.").start();
    all((pull_back(&view, cam, DIAGRAM, 60.0, 2.4), code.focus_lines(1, 2, 0.6))).await;
    let callout = diagram.add(Callout::new(card).text("drives the sea"));
    stagger(0.25, &[agent, check, render], |n| n.fade_in().duration(0.3)).await;
    stagger(0.3, &arrows, |a| a.end().to(1.0, 0.6).ease(presets::OUT_CUBIC)).await;
    all((callout.enter(), callout.next())).await;
    line.await;
    wait_for(2.5).await;

    // 20–28 s: the graph is one panel of a dashboard.
    let line = voice("The graph sits in a dashboard. Bars, a curve, a clip.").start();
    clip.play();
    pull_back(&view, cam, DASH, 60.0, 2.4).await;
    line.at_word("Bars").await;
    all((
        bars.value(0).to(4.0, 0.8),
        bars.value(1).to(6.5, 0.9),
        bars.value(2).to(5.2, 1.0),
        bars.value(3).to(8.8, 1.1),
        rider_x.to(std::f64::consts::TAU, 3.0).ease(presets::IN_OUT_SINE),
    ))
    .await;
    line.await;
    wait_for(2.6).await;

    // 28–38 s: the dashboard hangs over the sea, and the edit fires.
    let line = voice("Remember that number? Here it goes.").start();
    pull_back(&view, cam, FRAME, 0.0, 2.6).await;
    line.at_word("goes").await;
    let amp = amplitude(&code.code().get());
    sea.to(SEA_PX * amp, 1.2).ease(presets::OUT_BACK).await;
    line.await;
    wait_for(5.0).await;

    // 38–46 s: the shot is one frame of the check storyboard.
    let line = voice("All of it is one frame of oxframe check. Zero errors. Three formats.").start();
    all((
        pull_back(&view, cam, GRID, 40.0, 3.0),
        sea.to(SEA_PX * 0.04, 3.0),
        all(cells.iter().map(|c| c.fade_in().duration(1.0))),
    ))
    .await;
    line.at_word("Zero").await;
    all((badge.fade_in(), outline.fade_in())).await;
    line.at_word("Three").await;

    // The grid falls apart into the three frame shapes.
    all((
        all(cells.iter().map(|c| c.fade_out().duration(0.6))),
        badge.fade_out(),
        outline.stroke().to(Surface, 0.6),
    ))
    .await;
    let formats: [(&str, f64, f64, f64); 3] = [("16:9", -1165.0, 1920.0, 1080.0), ("9:16", 220.0, 850.0, 1512.0), ("1:1", 1485.0, 1080.0, 1080.0)];
    all((
        shot.x().to(formats[0].1, 0.8).ease(presets::IN_OUT_CUBIC),
        outline.x().to(formats[0].1, 0.8).ease(presets::IN_OUT_CUBIC),
        outline.line_width().to(8.0, 0.8),
    ))
    .await;
    let mut minis = Vec::new();
    for (label, x, w, h) in &formats[1..] {
        let g = cam.add(Group::new().x(*x).opacity(0.0).name(format!("format-{label}")));
        g.add(Rect::new().width(*w).height(*h).fill(Background).stroke(Surface).line_width(8.0));
        add_sea(g, *w, h * 0.4, h * 0.3, SEA_PX * 0.08, phase.clone());
        g.add(Rect::new().width(w * 0.64).height(h * 0.36).radius(20.0).fill(Surface).y(-h * 0.12));
        minis.push(g);
    }
    let labels: Vec<Txt> = formats
        .iter()
        .map(|(label, x, _, h)| cam.add(Txt::new().text(*label).font_size(110.0).weight(700.0).x(*x).y(h / 2.0 + 120.0).opacity(0.0)))
        .collect();
    all((
        pull_back(&view, cam, (160.0, 60.0, 4250.0, 1800.0), 40.0, 1.0),
        stagger(0.2, &minis, |g| g.fade_in()),
        stagger(0.15, &labels, |t| t.fade_in()),
    ))
    .await;
    line.await;
    wait_for(2.6).await;
}

/// Crest height in pixels for an `amp` of 1.
const SEA_PX: f64 = 450.0;

/// The `amp(..)` value in the edited source: the sea obeys the code on screen.
fn amplitude(src: &str) -> f64 {
    let start = src.find("amp(").expect("amp( in code") + 4;
    let end = start + src[start..].find(')').expect("closing )");
    src[start..end].parse().expect("amp is a number")
}

/// 46–55 s: fade to the end card.
#[oxframe::scene]
pub async fn finale(view: View) {
    sound(asset("whoosh.wav")).volume(0.4).play();
    transition(Transition::fade()).await;
    let card = view.add(
        EndCard::new("One scene. One camera.")
            .note(format!("{} lines of Rust · oxframe.dev", rust_lines())),
    );
    let line = voice("Made with oxframe. One scene, one camera, no cuts.").start();
    card.enter().await;
    card.next().await;
    line.await;
    wait_for(2.2).await;
}

pub fn project() -> Project {
    let project = Project::new("one_shot")
        .aspects([Aspect::Landscape, Aspect::Portrait, Aspect::Square])
        .speaker("IHw7aBJxrIo1SxkG9px5")
        .audio(Audio::track(asset("music.mp3")).volume(0.5).duck_under_voice().master_fade(0.3, 1.0))
        .scene(oner())
        .scene(finale());
    #[cfg(feature = "elevenlabs")]
    let project = { let _ = dotenvy::dotenv(); project.tts(tts::ElevenLabs) };
    project
}
