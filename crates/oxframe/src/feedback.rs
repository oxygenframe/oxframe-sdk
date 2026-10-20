//! `check` and `inspect`: a storyboard, a visual-lint report, and a scene dump.
//!
//! The project binary serves these; `oxframe-cli` forwards to them.

use std::path::PathBuf;
use std::time::Instant;

use oxframe_core::{Color, Image};
use oxframe_protocol::MarkerInfo;
use oxframe_render::CpuRenderer;
use serde::Serialize;

use crate::lint::{self, Issue, Limits, Sample, Severity};
use crate::{Engine, EngineError, Project};

/// What `check` writes and how strict it is.
#[derive(Debug, Clone)]
pub struct Options {
    /// Storyboard frames at least this far apart, plus one per marker.
    pub interval: f64,
    /// Width of a storyboard cell. Height follows the project aspect.
    pub cell_width: u32,
    /// Also write each storyboard frame at full size.
    pub full_frames: bool,
    /// Lint thresholds.
    pub limits: Limits,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            interval: 1.0,
            cell_width: 320,
            full_frames: false,
            limits: Limits::default(),
        }
    }
}

/// `out/check/report.json`.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Project length in seconds.
    pub duration: f64,
    /// Problems, sorted by time.
    pub issues: Vec<Issue>,
}

/// Storyboard plus the report. `frames` is empty unless [`Options::full_frames`].
pub struct Checked {
    /// The lint report.
    pub report: Report,
    /// Grid of the sampled frames.
    pub storyboard: Image,
    /// `(relative path, image)` for full-size frames.
    pub frames: Vec<(String, Image)>,
}

/// Samples the project and lints it, without drawing.
///
/// # Errors
///
/// If the project has no scenes or a meta file is invalid.
pub fn report(project: Project, opts: &Options) -> Result<Report, EngineError> {
    Ok(pass(&mut Engine::new(project)?, opts, false)?.0)
}

/// Samples, lints and draws the storyboard (CPU, storyboard resolution).
///
/// # Errors
///
/// If the project has no scenes or a meta file is invalid.
pub fn check(project: Project, opts: &Options) -> Result<Checked, EngineError> {
    let mut engine = Engine::new(project)?;
    let (report, cells) = pass(&mut engine, opts, true)?;
    let storyboard = compose(&cells, engine.project().background);
    let frames = if opts.full_frames {
        let size = engine.size();
        let mut cpu = CpuRenderer::new();
        cells
            .iter()
            .map(|(t, marker, _)| {
                let frame = frame_at(*t, engine.fps(), engine.total_frames());
                let name = frame_file(*t, marker.as_deref());
                (name, engine.render(frame, &mut cpu, size))
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(Checked {
        report,
        storyboard,
        frames,
    })
}

/// JSON tree of the scene at `seconds` (global time).
///
/// # Errors
///
/// If the project has no scenes or a meta file is invalid.
pub fn inspect(project: Project, seconds: f64) -> Result<serde_json::Value, EngineError> {
    let mut engine = Engine::new(project)?;
    let frame = frame_at(seconds, engine.fps(), engine.total_frames());
    let nodes = engine.seek(frame).inspect_nodes();
    let (w, h) = engine.size();
    Ok(serde_json::json!({
        "time": seconds,
        "frame": frame,
        "size": [w, h],
        "nodes": nodes.iter().map(|n| serde_json::json!({
            "id": n.id,
            "parent": n.parent,
            "kind": n.kind,
            "name": n.name,
            "source": n.source,
            "bounds": n.bounds,
            "opacity": n.opacity,
            "text": n.text,
            "order": n.order,
        })).collect::<Vec<_>>(),
    }))
}

struct When {
    t: f64,
    marker: Option<String>,
    story: bool,
}

type StoryCell = (f64, Option<String>, Image);

fn pass(
    engine: &mut Engine,
    opts: &Options,
    draw: bool,
) -> Result<(Report, Vec<StoryCell>), EngineError> {
    let fps = f64::from(engine.fps());
    let last = engine.total_frames().saturating_sub(1);
    let duration = if fps > 0.0 {
        f64::from(last) / fps
    } else {
        0.0
    };
    let schedule = schedule(duration, fps, &marker_times(engine), opts);
    let mut samples = Vec::with_capacity(schedule.len());
    let mut cells = Vec::new();
    let mut cpu = CpuRenderer::new();
    let cell = cell_size(engine.size(), opts.cell_width);
    for when in &schedule {
        let frame = frame_at(when.t, engine.fps(), engine.total_frames());
        let nodes = engine.seek(frame).inspect_nodes();
        samples.push(Sample {
            time: when.t,
            nodes,
        });
        if draw && when.story {
            cells.push((
                when.t,
                when.marker.clone(),
                engine.render(frame, &mut cpu, cell),
            ));
        }
    }
    let has_audio = engine.audio().is_some();
    let issues = lint::lint(
        &samples,
        engine.size(),
        engine.project().background,
        has_audio,
        &opts.limits,
    );
    Ok((Report { duration, issues }, cells))
}

fn marker_times(engine: &Engine) -> Vec<(f64, String)> {
    let fps = f64::from(engine.fps());
    let scenes = engine.scenes();
    engine
        .markers()
        .into_iter()
        .filter_map(|MarkerInfo { scene, name, time }| {
            let start = scenes.iter().find(|s| s.name == scene)?.start;
            Some((time + f64::from(start) / fps, name))
        })
        .collect()
}

fn schedule(duration: f64, fps: f64, markers: &[(f64, String)], opts: &Options) -> Vec<When> {
    let mut raw = vec![When {
        t: 0.0,
        marker: None,
        story: true,
    }];
    let step = opts.limits.sample_step.max(1.0 / 120.0);
    let mut t = 0.0;
    while t + step < duration - 1e-9 {
        t += step;
        raw.push(When {
            t,
            marker: None,
            story: false,
        });
    }
    if duration > 1e-9 {
        raw.push(When {
            t: duration,
            marker: None,
            story: true,
        });
    }
    if opts.interval > 0.0 {
        let mut t = 0.0;
        while t + opts.interval < duration - 1e-9 {
            t += opts.interval;
            raw.push(When {
                t,
                marker: None,
                story: true,
            });
        }
    }
    for (t, name) in markers {
        if *t >= -1e-6 && *t <= duration + 1e-6 {
            raw.push(When {
                t: t.clamp(0.0, duration),
                marker: Some(name.clone()),
                story: true,
            });
        }
    }
    raw.sort_by(|a, b| a.t.total_cmp(&b.t));
    let tol = 0.5 / fps.max(1.0);
    let mut out: Vec<When> = Vec::new();
    for w in raw {
        if let Some(prev) = out.last_mut()
            && (w.t - prev.t).abs() <= tol
        {
            prev.story |= w.story;
            if prev.marker.is_none() {
                prev.marker = w.marker;
            }
        } else {
            out.push(w);
        }
    }
    out
}

fn frame_at(t: f64, fps: u32, total: u32) -> u32 {
    let last = total.saturating_sub(1);
    ((t.max(0.0) * f64::from(fps)).round() as u32).min(last)
}

fn cell_size(project: (u32, u32), width: u32) -> (u32, u32) {
    let width = width.max(16);
    let height =
        (f64::from(width) * f64::from(project.1) / f64::from(project.0.max(1))).round() as u32;
    (width, height.max(16))
}

fn compose(cells: &[StoryCell], _background: Color) -> Image {
    if cells.is_empty() {
        return Image {
            width: 1,
            height: 1,
            data: vec![0, 0, 0, 255],
        };
    }
    let (cw, ch) = (cells[0].2.width, cells[0].2.height);
    let cols = if cells.len() <= 4 {
        cells.len() as u32
    } else {
        4
    };
    let rows = cells.len().div_ceil(cols as usize) as u32;
    let pad = 8u32;
    let label = 18u32;
    let width = cols * cw + (cols + 1) * pad;
    let height = rows * (ch + label) + (rows + 1) * pad;
    let mut img = Image {
        width,
        height,
        data: [0x11, 0x11, 0x14, 0xff].repeat((width as usize) * (height as usize)),
    };
    for (i, (t, marker, cell)) in cells.iter().enumerate() {
        let col = i as u32 % cols;
        let row = i as u32 / cols;
        let x = pad + col * (cw + pad);
        let y = pad + row * (ch + label + pad);
        blit(&mut img, x, y, cell);
        let caption = match marker {
            Some(name) => format!("{t:.2} {name}"),
            None => format!("{t:.2}"),
        };
        draw_text(&mut img, x, y + ch + 4, &caption, [0xee, 0xee, 0xee, 0xff]);
    }
    img
}

fn blit(dst: &mut Image, x: u32, y: u32, src: &Image) {
    for row in 0..src.height {
        let dy = y + row;
        if dy >= dst.height {
            break;
        }
        let copy = src.width.min(dst.width.saturating_sub(x));
        let s = (row * src.width) as usize * 4;
        let d = ((dy * dst.width + x) as usize) * 4;
        let n = copy as usize * 4;
        dst.data[d..d + n].copy_from_slice(&src.data[s..s + n]);
    }
}

fn draw_text(img: &mut Image, x: u32, y: u32, text: &str, color: [u8; 4]) {
    let mut pen = x;
    for ch in text.chars() {
        if pen + 8 > img.width {
            break;
        }
        for (row, bits) in glyph(ch).iter().enumerate() {
            for col in 0..5 {
                if bits & (1u8 << (4 - col)) == 0 {
                    continue;
                }
                put(img, pen + col, y + row as u32, color);
            }
        }
        pen += 6;
    }
}

fn put(img: &mut Image, x: u32, y: u32, color: [u8; 4]) {
    if x >= img.width || y >= img.height {
        return;
    }
    let i = ((y * img.width + x) as usize) * 4;
    img.data[i..i + 4].copy_from_slice(&color);
}

/// 5×7 glyphs, top row first, bit 4 is the left pixel.
fn glyph(c: char) -> [u8; 7] {
    match c.to_ascii_lowercase() {
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        'a' => [
            0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b10001, 0b01111,
        ],
        'b' => [
            0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'c' => [
            0b00000, 0b01110, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'd' => [
            0b00001, 0b00001, 0b01111, 0b10001, 0b10001, 0b10001, 0b01111,
        ],
        'e' => [
            0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b10001, 0b01110,
        ],
        'f' => [
            0b00110, 0b01001, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000,
        ],
        'g' => [
            0b01111, 0b10001, 0b10001, 0b01111, 0b00001, 0b10001, 0b01110,
        ],
        'h' => [
            0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        'i' => [
            0b00100, 0b00000, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'j' => [
            0b00010, 0b00000, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        'k' => [
            0b10000, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'l' => [
            0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'm' => [
            0b00000, 0b11010, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'n' => [
            0b00000, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        'o' => [
            0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'p' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'q' => [
            0b01111, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b00001,
        ],
        'r' => [
            0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000, 0b10000,
        ],
        's' => [
            0b00000, 0b01111, 0b10000, 0b01110, 0b00001, 0b10001, 0b11110,
        ],
        't' => [
            0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b01001, 0b00110,
        ],
        'u' => [
            0b00000, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01111,
        ],
        'v' => [
            0b00000, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'w' => [
            0b00000, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'x' => [
            0b00000, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'y' => [
            0b10001, 0b10001, 0b10001, 0b01111, 0b00001, 0b10001, 0b01110,
        ],
        'z' => [
            0b00000, 0b11111, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        ':' => [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0],
        '-' | '_' => [0, 0, 0, 0b11111, 0, 0, 0],
        ' ' => [0; 7],
        _ => [
            0b11111, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11111,
        ],
    }
}

fn frame_file(t: f64, marker: Option<&str>) -> String {
    match marker {
        Some(name) => format!("frames/{t:.2}-{name}.png"),
        None => format!("frames/{t:.2}.png"),
    }
}

/// `check` subcommand of a project binary.
pub fn command_check(project: Project, args: impl Iterator<Item = String>) -> i32 {
    let opts = match parse_check(args) {
        Ok(o) => o,
        Err(e) => return crate::cli::usage_error(&e),
    };
    let started = Instant::now();
    let checked = match check(project, &opts.spec) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return crate::cli::EXIT_ERROR;
        }
    };
    if let Err(e) = write_output(&opts.out, &checked) {
        eprintln!("error: {e}");
        return crate::cli::EXIT_ENV;
    }
    let errors = checked
        .report
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    for issue in &checked.report.issues {
        eprintln!("{}", format_issue(issue));
    }
    eprintln!(
        "check: {} issue(s), {} error(s), {:.2} s → {}/storyboard.png",
        checked.report.issues.len(),
        errors,
        started.elapsed().as_secs_f64(),
        opts.out.display()
    );
    println!("{}/report.json", opts.out.display());
    println!("{}/storyboard.png", opts.out.display());
    if errors > 0 {
        crate::cli::EXIT_ERROR
    } else {
        crate::cli::EXIT_OK
    }
}

/// `inspect` subcommand of a project binary.
pub fn command_inspect(project: Project, args: impl Iterator<Item = String>) -> i32 {
    let mut at = None;
    let mut scene = None;
    let mut args = args;
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--at" => {
                let v = match value() {
                    Ok(v) => v,
                    Err(e) => return crate::cli::usage_error(&e),
                };
                at = Some(match v.parse::<f64>() {
                    Ok(t) => t,
                    Err(_) => return crate::cli::usage_error(&format!("bad time `{v}`")),
                });
            }
            "--scene" => match value() {
                Ok(v) => scene = Some(v),
                Err(e) => return crate::cli::usage_error(&e),
            },
            other => return crate::cli::usage_error(&format!("unknown option `{other}`")),
        }
    }
    let Some(mut at) = at else {
        return crate::cli::usage_error("inspect needs --at <seconds>");
    };
    let engine = match Engine::new(project) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return crate::cli::EXIT_ERROR;
        }
    };
    if let Some(name) = &scene {
        let Some(info) = engine.scenes().into_iter().find(|s| &s.name == name) else {
            eprintln!("error: no scene named `{name}`");
            return crate::cli::EXIT_ERROR;
        };
        at += f64::from(info.start) / f64::from(engine.fps());
    }
    match inspect_engine(engine, at) {
        Ok(json) => {
            println!("{json}");
            crate::cli::EXIT_OK
        }
        Err(e) => {
            eprintln!("error: {e}");
            crate::cli::EXIT_ERROR
        }
    }
}

fn inspect_engine(engine: Engine, seconds: f64) -> Result<String, EngineError> {
    // Re-enter through the project is unnecessary: seek on this engine.
    let mut engine = engine;
    let frame = frame_at(seconds, engine.fps(), engine.total_frames());
    let nodes = engine.seek(frame).inspect_nodes();
    let (w, h) = engine.size();
    let value = serde_json::json!({
        "time": seconds,
        "frame": frame,
        "size": [w, h],
        "nodes": nodes.iter().map(|n| serde_json::json!({
            "id": n.id,
            "parent": n.parent,
            "kind": n.kind,
            "name": n.name,
            "source": n.source,
            "bounds": n.bounds,
            "opacity": n.opacity,
            "text": n.text,
            "order": n.order,
        })).collect::<Vec<_>>(),
    });
    Ok(serde_json::to_string_pretty(&value).expect("json"))
}

struct CheckArgs {
    out: PathBuf,
    spec: Options,
}

fn parse_check(mut args: impl Iterator<Item = String>) -> Result<CheckArgs, String> {
    let mut out = PathBuf::from("out/check");
    let mut spec = Options::default();
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--out" => out = value()?.into(),
            "--interval" => spec.interval = num(&value()?, "interval")?,
            "--safe-inset" => spec.limits.safe_inset = num(&value()?, "safe-inset")?,
            "--min-contrast" => spec.limits.min_contrast = num(&value()?, "min-contrast")?,
            "--min-motion" => spec.limits.min_motion = num(&value()?, "min-motion")?,
            "--max-still" => spec.limits.max_still = num(&value()?, "max-still")?,
            "--chars-per-second" => {
                spec.limits.chars_per_second = num(&value()?, "chars-per-second")?;
            }
            "--min-chars" => {
                spec.limits.min_chars = num(&value()?, "min-chars")? as usize;
            }
            "--cell-width" => spec.cell_width = num(&value()?, "cell-width")? as u32,
            "--frames" => spec.full_frames = true,
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    Ok(CheckArgs { out, spec })
}

fn num(v: &str, name: &str) -> Result<f64, String> {
    v.parse().map_err(|_| format!("bad {name} `{v}`"))
}

fn write_output(dir: &std::path::Path, checked: &Checked) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        dir.join("report.json"),
        serde_json::to_string_pretty(&checked.report).expect("json"),
    )?;
    std::fs::write(dir.join("storyboard.png"), checked.storyboard.encode_png())?;
    for (rel, image) in &checked.frames {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, image.encode_png())?;
    }
    Ok(())
}

fn format_issue(issue: &Issue) -> String {
    let where_ = issue
        .source
        .as_deref()
        .map(|s| format!(" ({s})"))
        .unwrap_or_default();
    format!(
        "{} {} {:.2}..{:.2} {}{where_}: {}",
        match issue.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        },
        issue.rule,
        issue.time[0],
        issue.time[1],
        issue.node,
        issue.hint
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BoxFuture, NodeBuilder, SceneDef, SourceLocation, View};
    use oxframe_core::Color;
    use oxframe_protocol::Meta;
    use std::collections::BTreeMap;

    fn scene(name: &'static str, factory: fn(View) -> BoxFuture) -> SceneDef {
        SceneDef::new(
            name,
            SourceLocation {
                file: "src/scene.rs",
                manifest_dir: env!("CARGO_MANIFEST_DIR"),
            },
            factory,
        )
    }

    fn project(factory: fn(View) -> BoxFuture) -> Project {
        Project::new("check")
            .scene(scene("main", factory))
            .write_meta(false)
            .inline_meta(BTreeMap::<String, Meta>::new())
    }

    fn hold(view: View) -> BoxFuture {
        Box::pin(async move {
            let _ = view.add(
                crate::Rect::new()
                    .size(120.0)
                    .fill(Color::WHITE)
                    .name("box"),
            );
            crate::wait_for(0.5).await;
        })
    }

    fn parked(view: View) -> BoxFuture {
        Box::pin(async move {
            let _ = view.add(
                crate::Rect::new()
                    .size(80.0)
                    .x(1400.0)
                    .fill(Color::WHITE)
                    .name("gone"),
            );
            crate::wait_for(1.0).await;
        })
    }

    fn long(view: View) -> BoxFuture {
        Box::pin(async move {
            let r = view.add(crate::Rect::new().size(80.0).fill(Color::WHITE).name("box"));
            r.x().to(200.0, 30.0).await;
        })
    }

    #[test]
    fn clean_scene_has_no_errors_and_a_storyboard() {
        let checked = check(project(hold), &Options::default()).unwrap();
        assert!(
            checked
                .report
                .issues
                .iter()
                .all(|i| i.severity != Severity::Error)
        );
        assert!(checked.storyboard.width > 100 && checked.storyboard.data.len() > 1000);
    }

    #[test]
    fn parked_node_is_an_error() {
        let report = report(project(parked), &Options::default()).unwrap();
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.rule == "out_of_frame" && i.node == "gone")
        );
    }

    #[test]
    fn thirty_seconds_stays_under_five() {
        let started = Instant::now();
        let checked = check(project(long), &Options::default()).unwrap();
        let elapsed = started.elapsed().as_secs_f64();
        assert!(checked.storyboard.width > 0);
        assert!(elapsed < 5.0, "check of a 30 s scene took {elapsed:.2} s");
    }

    #[test]
    fn inspect_names_the_node() {
        let json = inspect(project(hold), 0.1).unwrap();
        let nodes = json["nodes"].as_array().unwrap();
        assert!(
            nodes
                .iter()
                .any(|n| n["name"] == "box" && n["kind"] == "Rect")
        );
    }
}
