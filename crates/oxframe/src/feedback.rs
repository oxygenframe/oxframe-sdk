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
    let mut issues = Vec::new();
    let mut duration = 0.0_f64;
    for (label, project) in variants(project) {
        let mut report = pass(&mut Engine::new(project)?, opts, false)?.0;
        stamp(&mut report.issues, label);
        duration = duration.max(report.duration);
        issues.append(&mut report.issues);
    }
    Ok(Report { duration, issues })
}

/// Samples, lints and draws the storyboard (CPU, storyboard resolution).
///
/// # Errors
///
/// If the project has no scenes or a meta file is invalid.
pub fn check(project: Project, opts: &Options) -> Result<Checked, EngineError> {
    let mut issues = Vec::new();
    let mut duration = 0.0_f64;
    let mut storyboard = None;
    let mut frames = Vec::new();
    let jobs = variants(project);
    let multi = jobs.len() > 1;
    for (label, project) in jobs {
        let mut engine = Engine::new(project)?;
        let (mut report, cells) = pass(&mut engine, opts, true)?;
        stamp(&mut report.issues, label);
        duration = duration.max(report.duration);
        issues.append(&mut report.issues);
        let board = compose(&cells, engine.project().background);
        if multi {
            let tag = label.unwrap_or("frame").replace(':', "x");
            frames.push((format!("storyboard-{tag}.png"), board.clone()));
        }
        if storyboard.is_none() {
            storyboard = Some(board);
        }
        if opts.full_frames {
            let size = engine.size();
            let mut cpu = CpuRenderer::new();
            let prefix = label
                .map(|l| format!("{}/", l.replace(':', "x")))
                .unwrap_or_default();
            for (t, marker, _) in &cells {
                let frame = frame_at(*t, engine.fps(), engine.total_frames());
                let name = format!("{prefix}{}", frame_file(*t, marker.as_deref()));
                frames.push((name, engine.render(frame, &mut cpu, size)));
            }
        }
    }
    Ok(Checked {
        report: Report { duration, issues },
        storyboard: storyboard.expect("at least one variant"),
        frames,
    })
}

/// One project per declared aspect. No aspects means the project's own size.
fn variants(project: Project) -> Vec<(Option<&'static str>, Project)> {
    if project.aspects.is_empty() {
        return vec![(None, project)];
    }
    project
        .aspects
        .clone()
        .into_iter()
        .map(|aspect| {
            let mut sized = project.clone();
            sized.size = aspect.frame(project.size);
            (Some(aspect.label()), sized)
        })
        .collect()
}

fn stamp(issues: &mut [Issue], aspect: Option<&str>) {
    if let Some(aspect) = aspect {
        for issue in issues {
            issue.aspect = Some(aspect.to_owned());
        }
    }
}

/// JSON tree of the scene at `seconds` (global time).
///
/// # Errors
///
/// If the project has no scenes or a meta file is invalid.
pub fn inspect(project: Project, seconds: f64) -> Result<serde_json::Value, EngineError> {
    inspect_value(project, seconds, false)
}

fn inspect_value(
    project: Project,
    seconds: f64,
    graph: bool,
) -> Result<serde_json::Value, EngineError> {
    let mut engine = Engine::new(project)?;
    let frame = frame_at(seconds, engine.fps(), engine.total_frames());
    let nodes = engine.seek(frame).inspect_nodes();
    let passes = graph.then(|| oxframe_2d::pass_graph(&engine.seek(frame).draw_list()));
    Ok(scene_json(
        &engine,
        seconds,
        frame,
        &nodes,
        passes.as_deref(),
    ))
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
    let mut schedule = schedule(duration, fps, &marker_times(engine), opts);
    for span in engine.transition_spans() {
        if span.frames == 0 {
            continue;
        }
        let mid = span.start + span.frames / 2;
        schedule.push(When {
            t: f64::from(mid) / fps,
            marker: Some(span.kind.to_owned()),
            story: true,
        });
    }
    schedule.sort_by(|a, b| a.t.total_cmp(&b.t));
    schedule = dedup(schedule, fps);
    let mut samples = Vec::with_capacity(schedule.len());
    let mut cells = Vec::new();
    let mut cpu = CpuRenderer::new();
    let cell = cell_size(engine.size(), opts.cell_width);
    let mut drift = Vec::new();
    for when in &schedule {
        let frame = frame_at(when.t, engine.fps(), engine.total_frames());
        if let Some((node, source)) = engine.seek(frame).draw_drift()
            && !drift.iter().any(|(_, name, _)| name == &node)
        {
            drift.push((when.t, node, source));
        }
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
    let mut issues = lint::lint(
        &samples,
        engine.size(),
        engine.project().background,
        has_audio,
        &opts.limits,
    );
    for (t, node, source) in drift {
        issues.push(lint::Issue {
            rule: "nondeterministic",
            severity: lint::Severity::Error,
            time: [t, t],
            node,
            source,
            hint: "This frame differs when it is drawn twice. Draw from the time and ctx.rng(seed), not from saved numbers.".into(),
            aspect: None,
        });
    }
    issues.sort_by(|a, b| {
        a.time[0]
            .total_cmp(&b.time[0])
            .then(a.rule.cmp(b.rule))
            .then(a.node.cmp(&b.node))
    });
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
    dedup(raw, fps)
}

fn dedup(raw: Vec<When>, fps: f64) -> Vec<When> {
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

fn scene_json(
    engine: &Engine,
    seconds: f64,
    frame: u32,
    nodes: &[oxframe_2d::InspectNode],
    passes: Option<&[oxframe_2d::Pass]>,
) -> serde_json::Value {
    let (w, h) = engine.size();
    let mut value = serde_json::json!({
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
            "in_camera": n.in_camera,
        })).collect::<Vec<_>>(),
    });
    if let Some(tr) = engine.transition_at(frame) {
        value["transition"] = serde_json::json!({
            "kind": tr.kind,
            "dir": tr.dir,
            "progress": tr.progress,
            "overlap": tr.overlap,
        });
    }
    if let Some(passes) = passes {
        value["passes"] = serde_json::json!(
            passes
                .iter()
                .enumerate()
                .map(|(order, pass)| {
                    let name = nodes
                        .iter()
                        .find(|n| n.id == pass.node.0)
                        .map(|n| n.name.as_str());
                    serde_json::json!({
                        "order": order,
                        "kind": pass.kind.name(),
                        "node": pass.node.0,
                        "name": name,
                        "items": pass.items,
                        "depth": pass.depth,
                        "opacity": pass.opacity,
                        "blend": pass.blend,
                        "filtered": pass.filtered,
                    })
                })
                .collect::<Vec<_>>()
        );
    }
    value
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
    let mut graph = false;
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
            "--graph" => graph = true,
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
    match inspect_engine(engine, at, graph) {
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

fn inspect_engine(engine: Engine, seconds: f64, graph: bool) -> Result<String, EngineError> {
    let mut engine = engine;
    let frame = frame_at(seconds, engine.fps(), engine.total_frames());
    let nodes = engine.seek(frame).inspect_nodes();
    let passes = graph.then(|| oxframe_2d::pass_graph(&engine.seek(frame).draw_list()));
    let value = scene_json(&engine, seconds, frame, &nodes, passes.as_deref());
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
    let aspect = issue
        .aspect
        .as_deref()
        .map(|a| format!(" [{a}]"))
        .unwrap_or_default();
    format!(
        "{} {} {:.2}..{:.2} {}{where_}: {}{aspect}",
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
    use crate::{BoxFuture, Custom, NodeBuilder, Particles, SceneDef, SourceLocation, View};
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

    fn tokens(view: View) -> BoxFuture {
        Box::pin(async move {
            let _ = view.add(crate::Txt::new().text("Hi").name("title"));
            let _ = view.add(
                crate::Rect::new()
                    .size(80.0)
                    .fill(crate::Accent)
                    .name("mark"),
            );
            crate::wait_for(0.2).await;
        })
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
    fn check_runs_every_declared_aspect() {
        fn wide(view: View) -> BoxFuture {
            Box::pin(async move {
                let _ = view.add(
                    crate::Rect::new()
                        .size(40.0)
                        .x(700.0)
                        .fill(Color::WHITE)
                        .name("side"),
                );
                crate::wait_for(0.5).await;
            })
        }
        let report = report(
            project(wide).aspects([crate::Aspect::Landscape, crate::Aspect::Portrait]),
            &Options::default(),
        )
        .unwrap();
        let portrait = report
            .issues
            .iter()
            .any(|i| i.rule == "out_of_frame" && i.aspect.as_deref() == Some("9:16"));
        let landscape = report
            .issues
            .iter()
            .any(|i| i.rule == "out_of_frame" && i.aspect.as_deref() == Some("16:9"));
        assert!(portrait, "{:?}", report.issues);
        assert!(!landscape, "{:?}", report.issues);
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
    fn custom_that_keeps_a_counter_fails_check() {
        fn drift(view: View) -> BoxFuture {
            Box::pin(async move {
                let n = std::cell::Cell::new(0.0);
                let _ = view.add(
                    Custom::new(move |ctx, _| {
                        let x = n.get();
                        n.set(x + 1.0);
                        ctx.circle((x, 0.0), 6.0).fill(Color::WHITE);
                    })
                    .size(40.0, 40.0)
                    .name("drift"),
                );
                crate::wait_for(0.2).await;
            })
        }
        let report = report(project(drift), &Options::default()).unwrap();
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.rule == "nondeterministic" && i.node == "drift"),
            "{:?}",
            report.issues
        );
    }

    #[test]
    fn spray_seeks_back_to_the_same_frame() {
        fn spray(view: View) -> BoxFuture {
            Box::pin(async move {
                let _ = view.add(
                    Custom::new(|ctx, t| {
                        let mut rng = ctx.rng(3);
                        let y = (rng.range(0.0..40.0) + t * 20.0) % 40.0;
                        ctx.circle((0.0, y), 5.0).fill(Color::WHITE);
                    })
                    .size(20.0, 80.0)
                    .name("rain"),
                );
                let p = view.add(
                    Particles::new()
                        .seed(7)
                        .gravity(400.0)
                        .life(1.0)
                        .fill(Color::WHITE)
                        .toward_camera()
                        .name("spray"),
                );
                p.burst((0.0, 0.0), 6).await;
                crate::wait_for(0.4).await;
            })
        }
        let mut engine = crate::Engine::new(project(spray)).unwrap();
        let frame = engine.total_frames() / 2;
        let paint = |engine: &mut crate::Engine, frame: u32| {
            engine
                .seek(frame)
                .draw_list()
                .iter()
                .map(|item| format!("{:?}", item.prim))
                .collect::<Vec<_>>()
        };
        let first = paint(&mut engine, frame);
        engine.seek(0);
        let second = paint(&mut engine, frame);
        assert_eq!(first, second);
        assert!(engine.seek(frame).draw_drift().is_none());
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn custom_circle_matches_on_cpu_and_gpu() {
        fn dot(view: View) -> BoxFuture {
            Box::pin(async move {
                let _ = view.add(
                    Custom::new(|ctx, _| {
                        ctx.circle((0.0, 0.0), 30.0).fill(Color::WHITE);
                    })
                    .size(80.0, 80.0),
                );
                crate::wait_for(0.05).await;
            })
        }
        let mut engine = crate::Engine::new(project(dot)).unwrap();
        let size = engine.size();
        let cpu = engine.render(0, &mut crate::render::CpuRenderer::new(), size);
        let mut gpu = match crate::render::GpuRenderer::new() {
            Ok(g) if g.is_software() && std::env::var_os("OXFRAME_GPU_TESTS").is_none() => return,
            Ok(g) => g,
            Err(_) => return,
        };
        let gpu_img = engine.render(0, &mut gpu, size);
        let px = |img: &crate::render::Image, x, y| {
            let i = ((y * img.width + x) * 4) as usize;
            [
                img.data[i],
                img.data[i + 1],
                img.data[i + 2],
                img.data[i + 3],
            ]
        };
        let (cx, cy) = (size.0 / 2, size.1 / 2);
        assert_eq!(px(&cpu, cx, cy), px(&gpu_img, cx, cy));
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
    fn theme_recolors_tokens_and_text() {
        use crate::{Engine, Theme};
        let sample = |theme: Theme| {
            let mut engine = Engine::new(project(tokens).theme(theme)).unwrap();
            let nodes = engine.seek(0).inspect_nodes();
            let fill = |name: &str| {
                nodes
                    .iter()
                    .find(|n| n.name == name)
                    .unwrap()
                    .fill
                    .to_rgba8()
            };
            (fill("title"), fill("mark"))
        };
        let dark = sample(Theme::dark());
        let light = sample(Theme::light());
        assert_eq!(dark.0, Color::WHITE.to_rgba8());
        assert_ne!(dark.0, light.0, "text follows the theme");
        assert_ne!(dark.1, light.1, "Accent follows the theme");
        assert_eq!(dark.1, Theme::dark().accent.to_rgba8());
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
        assert!(json.get("passes").is_none());
    }

    #[test]
    fn inspect_graph_lists_passes_only_when_asked() {
        fn soft(view: View) -> BoxFuture {
            Box::pin(async move {
                let _ = view.add(
                    crate::Rect::new()
                        .size(40.0)
                        .fill(Color::WHITE)
                        .blur(2.0)
                        .name("soft"),
                );
                crate::wait_for(0.2).await;
            })
        }
        let plain = inspect(project(hold), 0.1).unwrap();
        assert!(plain.get("passes").is_none());
        let json = inspect_value(project(soft), 0.1, true).unwrap();
        let passes = json["passes"].as_array().unwrap();
        assert!(passes.iter().any(|p| p["kind"] == "vector"));
        assert!(
            passes
                .iter()
                .any(|p| p["kind"] == "composite" && p["filtered"] == true)
        );
    }
}
