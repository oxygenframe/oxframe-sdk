//! `check` and `inspect`: a storyboard, a visual-lint report, and a scene dump.
//!
//! The project binary serves these; `oxframe-cli` forwards to them.

use std::path::PathBuf;
use std::time::Instant;

use oxframe_2d::InspectNode;
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
    /// Nodes that travel: when they move and from where to where (see
    /// [`crate::motion`]). `check` also draws their paths per scene.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub motion: Vec<crate::motion::Motion>,
    /// With `check --against <dir>`: how the frames differ from that earlier
    /// `check --frames` output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison: Option<Comparison>,
}

/// Frames of this check against the same frames of an earlier one.
#[derive(Debug, Clone, Serialize)]
pub struct Comparison {
    /// The reference directory, as given.
    pub against: String,
    /// Frames that differ, in time order.
    pub changed: Vec<FrameChange>,
    /// How many frames are the same.
    pub unchanged: usize,
    /// Frames only in this check (the timing moved, or a new marker).
    pub added: Vec<String>,
    /// Frames only in the reference.
    pub removed: Vec<String>,
}

/// One frame that differs from the reference.
#[derive(Debug, Clone, Serialize)]
pub struct FrameChange {
    /// Its path under the check output, e.g. `frames/1.00.png`.
    pub frame: String,
    /// Share of pixels that changed, 0 to 1.
    pub changed: f64,
    /// The frame dimmed, with the changed pixels in red, e.g.
    /// `diff/1.00.png`; none when the sizes differ.
    pub diff: Option<String>,
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
    let mut motion = None;
    for (label, project) in variants(project) {
        let mut report = pass(&mut Engine::new(project)?, opts, false)?.0;
        stamp(&mut report.issues, label);
        duration = duration.max(report.duration);
        issues.append(&mut report.issues);
        // The first frame shape's; the others repeat it in other coordinates.
        motion.get_or_insert(report.motion);
    }
    Ok(Report {
        duration,
        issues,
        comparison: None,
        motion: motion.unwrap_or_default(),
    })
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
    let mut motion = None;
    let mut taken = 0;
    for (label, project) in jobs {
        let mut engine = Engine::new(project)?;
        let (mut report, cells) = pass(&mut engine, opts, true)?;
        stamp(&mut report.issues, label);
        let prefix = label
            .map(|l| format!("{}-", l.replace(':', "x")))
            .unwrap_or_default();
        frames.extend(crops(&mut engine, &mut report.issues, &prefix, &mut taken));
        frames.extend(trajectories(&mut engine, &report.motion, &prefix));
        motion.get_or_insert(std::mem::take(&mut report.motion));
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
        report: Report {
            duration,
            issues,
            comparison: None,
            motion: motion.unwrap_or_default(),
        },
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
    // A hard cut shows both sides on the storyboard: the last frame of a
    // scene and the first of the next. A blend already has its middle frame.
    let scenes = engine.scenes();
    for pair in scenes.windows(2) {
        let (prev, next) = (&pair[0], &pair[1]);
        if next.transition > 0 || next.start == 0 {
            continue;
        }
        schedule.push(When {
            t: f64::from(next.start - 1) / fps,
            marker: Some(format!("{}-end", prev.name)),
            story: true,
        });
        schedule.push(When {
            t: f64::from(next.start) / fps,
            marker: Some(next.name.clone()),
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
    let mut shader_notes = Vec::new();
    oxframe_2d::set_shader_budget_ms(opts.limits.shader_ms);
    let _draft = oxframe_2d::draft_shaders();
    for when in &schedule {
        let frame = frame_at(when.t, engine.fps(), engine.total_frames());
        if let Some((node, source)) = engine.seek(frame).draw_drift()
            && !drift.iter().any(|(_, name, _)| name == &node)
        {
            drift.push((when.t, node, source));
        }
        for diag in engine.seek(frame).shader_diags() {
            shader_notes.push((when.t, diag));
        }
        let nodes = engine.seek(frame).inspect_nodes();
        let contrast = pixel_contrast(engine, frame, &nodes, &mut cpu);
        samples.push(Sample {
            time: when.t,
            nodes,
            contrast,
        });
        if draw && when.story {
            cells.push((
                when.t,
                when.marker.clone(),
                engine.render(frame, &mut cpu, cell),
            ));
        }
    }
    let has_audio =
        engine.audio().is_some() || !engine.scheduled().is_empty() || !engine.voices().is_empty();
    let mut issues = lint::lint(
        &samples,
        engine.size(),
        engine.project().background,
        has_audio,
        &opts.limits,
    );
    issues.extend(lint::voice_lines(&engine.voices()));
    let mut seen_shader = std::collections::BTreeSet::new();
    for (t, diag) in shader_notes {
        if seen_shader.insert((diag.rule, diag.node.clone(), diag.hint.clone())) {
            let severity = if diag.rule == "shader_slow" {
                lint::Severity::Warning
            } else {
                lint::Severity::Error
            };
            issues.push(lint::Issue {
                rule: diag.rule,
                severity,
                time: [t, t],
                node: diag.node,
                source: diag.source,
                hint: diag.hint,
                aspect: None,
                bounds: None,
                crop: None,
            });
        }
    }
    for (t, node, source) in drift {
        issues.push(lint::Issue {
            rule: "nondeterministic",
            severity: lint::Severity::Error,
            time: [t, t],
            node,
            source,
            hint: "This frame differs when it is drawn twice. Draw from the time and ctx.rng(seed), not from saved numbers.".into(),
            aspect: None,
            bounds: None,
            crop: None,
        });
    }
    for issue in &mut issues {
        pin_bounds(issue, &samples);
    }
    issues.sort_by(|a, b| {
        a.time[0]
            .total_cmp(&b.time[0])
            .then(a.rule.cmp(b.rule))
            .then(a.node.cmp(&b.node))
    });
    let motion = crate::motion::motions(&samples, opts.limits.sample_step);
    Ok((
        Report {
            duration,
            issues,
            comparison: None,
            motion,
        },
        cells,
    ))
}

/// Node types drawn with one fill, where `low_contrast` reads the color
/// behind text from the fills. Anything else (a shader, an image, a video,
/// `Custom`, particles, 3D) is judged on drawn pixels.
const FLAT: &[&str] = &[
    "Rect", "Circle", "Polygon", "Star", "Path", "Line", "Group", "Layout", "Grid", "Camera",
    "Txt", "Code", "Math",
];

/// Width of the frame `low_contrast` draws to read the pixels behind text.
const CONTRAST_WIDTH: u32 = 480;

/// Contrast of each `Txt` that lies over a node in [`FLAT`]'s complement,
/// against the pixels behind it: the frame is drawn without text, and the
/// ratio is the median over the text's box (a thin line through the box
/// does not count, a light half of it does).
fn pixel_contrast(
    engine: &mut Engine,
    frame: u32,
    nodes: &[InspectNode],
    renderer: &mut CpuRenderer,
) -> Vec<(u32, f64)> {
    let drawn = |n: &InspectNode| n.id != 0 && n.opacity > 0.05 && n.order != u32::MAX;
    let meets = |a: [f64; 4], b: [f64; 4]| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
    let texts: Vec<&InspectNode> = nodes
        .iter()
        .filter(|t| {
            t.kind == "Txt"
                && drawn(t)
                && t.text.as_ref().is_some_and(|s| !s.trim().is_empty())
                && nodes.iter().any(|n| {
                    drawn(n)
                        && n.order < t.order
                        && !FLAT.contains(&n.kind)
                        && meets(n.bounds, t.bounds)
                })
        })
        .collect();
    if texts.is_empty() {
        return Vec::new();
    }
    let scene = engine.size();
    let size = (
        CONTRAST_WIDTH,
        (u64::from(CONTRAST_WIDTH) * u64::from(scene.1) / u64::from(scene.0.max(1))).max(1) as u32,
    );
    let hidden: Vec<u32> = nodes
        .iter()
        .filter(|n| n.kind == "Txt" || n.kind == "Code" || n.kind == "Math")
        .map(|n| n.id)
        .collect();
    let image = engine.render_without(frame, renderer, size, &hidden);
    texts
        .into_iter()
        .filter_map(|t| behind(&image, scene, t).map(|ratio| (t.id, ratio)))
        .collect()
}

/// The median contrast over `text`'s box on `image` (the frame at its own
/// size; `scene` is the scene size).
fn behind(image: &Image, scene: (u32, u32), text: &InspectNode) -> Option<f64> {
    let k = f64::from(image.width) / f64::from(scene.0.max(1));
    let (hw, hh) = (f64::from(scene.0) / 2.0, f64::from(scene.1) / 2.0);
    let col = |x: f64| ((x + hw) * k).clamp(0.0, f64::from(image.width));
    let row = |y: f64| ((y + hh) * k).clamp(0.0, f64::from(image.height));
    let b = text.bounds;
    let (x0, x1) = (col(b[0]).floor() as u32, col(b[2]).ceil() as u32);
    let (y0, y1) = (row(b[1]).floor() as u32, row(b[3]).ceil() as u32);
    let ink = Color::rgb(text.fill.r, text.fill.g, text.fill.b);
    let mut ratios = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * image.width + x) * 4) as usize;
            let px = &image.data[i..i + 3];
            let c = Color::rgb(
                f32::from(px[0]) / 255.0,
                f32::from(px[1]) / 255.0,
                f32::from(px[2]) / 255.0,
            );
            ratios.push(lint::contrast(ink, c));
        }
    }
    if ratios.is_empty() {
        return None;
    }
    ratios.sort_by(f64::total_cmp);
    Some(ratios[ratios.len() / 2])
}

/// A rule may report a node's rect from any sample in its time range; the
/// crop is drawn at `time[0]`, so take the rect from that sample. The same
/// name can be used twice (a loop), so the node closest to the reported
/// rect wins.
fn pin_bounds(issue: &mut Issue, samples: &[Sample]) {
    let Some(reported) = issue.bounds else {
        return;
    };
    let Some(sample) = samples.iter().min_by(|a, b| {
        (a.time - issue.time[0])
            .abs()
            .total_cmp(&(b.time - issue.time[0]).abs())
    }) else {
        return;
    };
    let center = |b: [f64; 4]| ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
    let (rx, ry) = center(reported);
    let found = sample
        .nodes
        .iter()
        .filter(|n| n.name == issue.node && n.source == issue.source)
        .filter(|n| n.bounds.iter().all(|v| v.is_finite()))
        .min_by(|a, b| {
            let d = |n: &oxframe_2d::InspectNode| {
                let (x, y) = center(n.bounds);
                (x - rx).powi(2) + (y - ry).powi(2)
            };
            d(a).total_cmp(&d(b))
        });
    if let Some(n) = found {
        issue.bounds = Some(n.bounds);
    }
}

/// At most this many crops per `check`; the rest of the issues keep their
/// `bounds` only.
const MAX_CROPS: usize = 24;

/// Full-resolution crops for the issues that have a node rect, under
/// `issues/`, named `<n>-<rule>.png`. One render per distinct frame.
fn crops(
    engine: &mut Engine,
    issues: &mut [Issue],
    prefix: &str,
    taken: &mut usize,
) -> Vec<(String, Image)> {
    let size = engine.size();
    let mut cpu = CpuRenderer::new();
    let mut frame: Option<(u32, Image)> = None;
    let mut out = Vec::new();
    for issue in issues.iter_mut() {
        if *taken >= MAX_CROPS {
            break;
        }
        let Some(scene) = issue.bounds else {
            continue;
        };
        let (cx, cy) = (f64::from(size.0) / 2.0, f64::from(size.1) / 2.0);
        let bounds = [scene[0] + cx, scene[1] + cy, scene[2] + cx, scene[3] + cy];
        let Some(rect) = crop_rect(bounds, size) else {
            continue;
        };
        let at = frame_at(issue.time[0], engine.fps(), engine.total_frames());
        if frame.as_ref().is_none_or(|(f, _)| *f != at) {
            frame = Some((at, engine.render(at, &mut cpu, size)));
        }
        let (_, full) = frame.as_ref().expect("rendered above");
        let mut img = cut(full, rect);
        outline(
            &mut img,
            [
                bounds[0] - f64::from(rect[0]),
                bounds[1] - f64::from(rect[1]),
                bounds[2] - f64::from(rect[0]),
                bounds[3] - f64::from(rect[1]),
            ],
        );
        *taken += 1;
        let name = format!("issues/{prefix}{:02}-{}.png", *taken, issue.rule);
        issue.crop = Some(name.clone());
        out.push((name, img));
    }
    out
}

const TRAIL_COLORS: [[u8; 4]; 6] = [
    [0xff, 0x5f, 0x57, 0xff],
    [0x68, 0xab, 0xdf, 0xff],
    [0xf5, 0xc2, 0x42, 0xff],
    [0x5c, 0xd6, 0x8a, 0xff],
    [0xc0, 0x84, 0xfc, 0xff],
    [0xff, 0x9f, 0x43, 0xff],
];

/// One picture per scene in which something travels: the scene's last frame
/// dimmed, each moving node's path in its own color from a dot (start) to a
/// ring (end), numbered as in `report.json`'s `motion` (1 is the first).
/// `trajectories/<scene>.png`.
fn trajectories(
    engine: &mut Engine,
    motion: &[crate::motion::Motion],
    prefix: &str,
) -> Vec<(String, Image)> {
    let fps = f64::from(engine.fps());
    let size = engine.size();
    let (cx, cy) = (f64::from(size.0) / 2.0, f64::from(size.1) / 2.0);
    let mut cpu = CpuRenderer::new();
    let mut out = Vec::new();
    for scene in engine.scenes() {
        let (t0, t1) = (
            f64::from(scene.start) / fps,
            f64::from(scene.start + scene.frames.saturating_sub(1)) / fps,
        );
        let inside: Vec<(usize, Vec<[f64; 2]>)> = motion
            .iter()
            .enumerate()
            .filter_map(|(i, m)| {
                let pts: Vec<[f64; 2]> = m
                    .path
                    .iter()
                    .filter(|p| p[0] >= t0 - 1e-9 && p[0] <= t1 + 1e-9)
                    .map(|p| [p[1] + cx, p[2] + cy])
                    .collect();
                let moved = pts
                    .windows(2)
                    .any(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]) > 0.5);
                moved.then_some((i, pts))
            })
            .collect();
        if inside.is_empty() {
            continue;
        }
        let last = scene.start + scene.frames.saturating_sub(1);
        let mut img = engine.render(last, &mut cpu, size);
        for px in img.data.chunks_exact_mut(4) {
            for c in &mut px[..3] {
                *c /= 3;
            }
        }
        for (n, (i, pts)) in inside.iter().enumerate() {
            let color = TRAIL_COLORS[n % TRAIL_COLORS.len()];
            for w in pts.windows(2) {
                stroke(&mut img, w[0], w[1], 3.0, color);
            }
            if let (Some(a), Some(b)) = (pts.first(), pts.last()) {
                disc(&mut img, *a, 7.0, color);
                ring(&mut img, *b, 11.0, 3.0, color);
                // Names are often call sites and collide; the number points
                // into report.json, which has the name and `file:line`.
                let label = (i + 1).to_string();
                let (x, y) = (b[0] + 16.0, b[1] - 30.0);
                let (w, h) = (label.len() as f64 * 18.0 + 10.0, 31.0);
                for yy in (y - 5.0).max(0.0) as u32..(y - 5.0 + h).max(0.0) as u32 {
                    for xx in (x - 5.0).max(0.0) as u32..(x - 5.0 + w).max(0.0) as u32 {
                        put(&mut img, xx, yy, [0x10, 0x10, 0x14, 0xff]);
                    }
                }
                draw_text_scaled(&mut img, x, y, &label, 3, color);
            }
        }
        out.push((format!("trajectories/{prefix}{}.png", scene.name), img));
    }
    out
}

fn disc(img: &mut Image, c: [f64; 2], r: f64, color: [u8; 4]) {
    let (x0, x1) = ((c[0] - r).floor() as i64, (c[0] + r).ceil() as i64);
    let (y0, y1) = ((c[1] - r).floor() as i64, (c[1] + r).ceil() as i64);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (dx, dy) = (x as f64 + 0.5 - c[0], y as f64 + 0.5 - c[1]);
            if dx * dx + dy * dy <= r * r && x >= 0 && y >= 0 {
                put(img, x as u32, y as u32, color);
            }
        }
    }
}

fn ring(img: &mut Image, c: [f64; 2], r: f64, width: f64, color: [u8; 4]) {
    let (x0, x1) = ((c[0] - r).floor() as i64, (c[0] + r).ceil() as i64);
    let (y0, y1) = ((c[1] - r).floor() as i64, (c[1] + r).ceil() as i64);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let d = (x as f64 + 0.5 - c[0]).hypot(y as f64 + 0.5 - c[1]);
            if d <= r && d >= r - width && x >= 0 && y >= 0 {
                put(img, x as u32, y as u32, color);
            }
        }
    }
}

/// A line `width` wide: discs stamped along it.
fn stroke(img: &mut Image, a: [f64; 2], b: [f64; 2], width: f64, color: [u8; 4]) {
    let len = (b[0] - a[0]).hypot(b[1] - a[1]);
    let steps = len.ceil().max(1.0) as usize;
    for k in 0..=steps {
        let t = k as f64 / steps as f64;
        disc(
            img,
            [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
            width / 2.0,
            color,
        );
    }
}

/// The storyboard's 5x7 font, `scale` pixels per dot.
fn draw_text_scaled(img: &mut Image, x: f64, y: f64, text: &str, scale: u32, color: [u8; 4]) {
    let mut pen = x.max(0.0) as u32;
    let y = y.max(0.0) as u32;
    for ch in text.chars() {
        if pen + 6 * scale > img.width {
            break;
        }
        for (row, bits) in glyph(ch).iter().enumerate() {
            for col in 0..5u32 {
                if bits & (1u8 << (4 - col)) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        put(
                            img,
                            pen + col * scale + dx,
                            y + row as u32 * scale + dy,
                            color,
                        );
                    }
                }
            }
        }
        pen += 6 * scale;
    }
}

/// The node's pixel rect plus a margin, clamped to the frame. A node wholly
/// outside the frame gets the strip along the edge it is past, so the crop
/// shows where it went missing.
fn crop_rect(b: [f64; 4], (w, h): (u32, u32)) -> Option<[u32; 4]> {
    if !b.iter().all(|v| v.is_finite()) || w == 0 || h == 0 {
        return None;
    }
    let (fw, fh) = (f64::from(w), f64::from(h));
    let margin = ((b[2] - b[0]).max(b[3] - b[1]) * 0.25).clamp(32.0, 160.0);
    let strip = 160.0_f64;
    let axis = |lo: f64, hi: f64, max: f64| -> (f64, f64) {
        let (a, z) = ((lo - margin).max(0.0), (hi + margin).min(max));
        if z - a >= 16.0 {
            (a, z)
        } else if lo >= max {
            ((max - strip).max(0.0), max)
        } else {
            (0.0, strip.min(max))
        }
    };
    let (x0, x1) = axis(b[0], b[2], fw);
    let (y0, y1) = axis(b[1], b[3], fh);
    Some([
        x0.floor() as u32,
        y0.floor() as u32,
        (x1.ceil() as u32).min(w),
        (y1.ceil() as u32).min(h),
    ])
}

fn cut(src: &Image, [x0, y0, x1, y1]: [u32; 4]) -> Image {
    let (w, h) = (x1 - x0, y1 - y0);
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in y0..y1 {
        let start = ((y * src.width + x0) * 4) as usize;
        data.extend_from_slice(&src.data[start..start + (w * 4) as usize]);
    }
    Image {
        width: w,
        height: h,
        data,
    }
}

/// A 2 px red frame around `b` (crop pixels); the parts outside the crop
/// are skipped.
fn outline(img: &mut Image, b: [f64; 4]) {
    let red = [0xff, 0x30, 0x30, 0xff];
    let clamp = |v: f64, max: u32| v.round().clamp(-1.0, f64::from(max)) as i64;
    let (x0, y0) = (clamp(b[0], img.width), clamp(b[1], img.height));
    let (x1, y1) = (clamp(b[2], img.width) - 1, clamp(b[3], img.height) - 1);
    let mut dot = |x: i64, y: i64| {
        if x >= 0 && y >= 0 {
            put(img, x as u32, y as u32, red);
        }
    };
    for t in 0..2 {
        for x in x0..=x1 {
            dot(x, y0 + t);
            dot(x, y1 - t);
        }
        for y in y0..=y1 {
            dot(x0 + t, y);
            dot(x1 - t, y);
        }
    }
}

/// A pixel counts as changed when a channel moved by more than this: below
/// it is noise no one sees.
const CHANGE_THRESHOLD: u8 = 8;

/// Share of changed pixels and a picture of them: the current frame at a
/// third of its brightness, changed pixels in red. `None` when the sizes
/// differ.
pub fn compare(reference: &Image, current: &Image) -> Option<(f64, Image)> {
    if (reference.width, reference.height) != (current.width, current.height) {
        return None;
    }
    let mut changed = 0usize;
    let mut out = current.clone();
    for (px, (a, b)) in out.data.chunks_exact_mut(4).zip(
        reference
            .data
            .chunks_exact(4)
            .zip(current.data.chunks_exact(4)),
    ) {
        if a.iter()
            .zip(b)
            .any(|(x, y)| x.abs_diff(*y) > CHANGE_THRESHOLD)
        {
            changed += 1;
            px.copy_from_slice(&[0xff, 0x30, 0x30, 0xff]);
        } else {
            for c in &mut px[..3] {
                *c /= 3;
            }
        }
    }
    let total = (current.width as usize * current.height as usize).max(1);
    Some((changed as f64 / total as f64, out))
}

/// Compares the full-size frames of `checked` with the PNGs of the same
/// names under `reference`, and returns the comparison plus the diff images
/// to write (`diff/<time>.png`).
fn against(reference: &std::path::Path, checked: &Checked) -> (Comparison, Vec<(String, Image)>) {
    let mut comparison = Comparison {
        against: reference.display().to_string(),
        changed: Vec::new(),
        unchanged: 0,
        added: Vec::new(),
        removed: Vec::new(),
    };
    let mut diffs = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (name, image) in checked
        .frames
        .iter()
        .filter(|(n, _)| n.starts_with("frames/"))
    {
        seen.insert(name.clone());
        let Some(old) = std::fs::read(reference.join(name))
            .ok()
            .and_then(|b| Image::decode_png(&b))
        else {
            comparison.added.push(name.clone());
            continue;
        };
        match compare(&old, image) {
            Some((0.0, _)) => comparison.unchanged += 1,
            Some((share, picture)) => {
                let diff = name.replacen("frames/", "diff/", 1);
                diffs.push((diff.clone(), picture));
                comparison.changed.push(FrameChange {
                    frame: name.clone(),
                    changed: share,
                    diff: Some(diff),
                });
            }
            None => comparison.changed.push(FrameChange {
                frame: name.clone(),
                changed: 1.0,
                diff: None,
            }),
        }
    }
    if let Ok(dir) = std::fs::read_dir(reference.join("frames")) {
        let mut gone: Vec<String> = dir
            .filter_map(Result::ok)
            .map(|e| format!("frames/{}", e.file_name().to_string_lossy()))
            .filter(|n| n.ends_with(".png") && !seen.contains(n))
            .collect();
        gone.sort();
        comparison.removed = gone;
    }
    (comparison, diffs)
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
            "shader": n.shader,
            "params": n.params.iter().cloned().collect::<std::collections::BTreeMap<String, f64>>(),
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
                        "shader": pass.shader,
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
    let mut checked = match check(project, &opts.spec) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return crate::cli::EXIT_ERROR;
        }
    };
    if let Some(reference) = &opts.against {
        let (comparison, diffs) = against(reference, &checked);
        checked.frames.extend(diffs);
        checked.report.comparison = Some(comparison);
    }
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
        if let Some(crop) = &issue.crop {
            eprintln!("  crop: {}", opts.out.join(crop).display());
        }
    }
    if let Some(c) = &checked.report.comparison {
        for change in &c.changed {
            let diff = change
                .diff
                .as_deref()
                .map(|d| format!(" → {}", opts.out.join(d).display()))
                .unwrap_or_else(|| " (size differs)".into());
            eprintln!(
                "changed {} {:.1}%{diff}",
                change.frame,
                change.changed * 100.0
            );
        }
        eprintln!(
            "against {}: {} changed, {} the same, {} new, {} gone",
            c.against,
            c.changed.len(),
            c.unchanged,
            c.added.len(),
            c.removed.len()
        );
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
    against: Option<PathBuf>,
}

fn parse_check(mut args: impl Iterator<Item = String>) -> Result<CheckArgs, String> {
    let mut out = PathBuf::from("out/check");
    let mut spec = Options::default();
    let mut against = None;
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
            // Comparing needs the full-size frames.
            "--against" => {
                against = Some(PathBuf::from(value()?));
                spec.full_frames = true;
            }
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    Ok(CheckArgs { out, spec, against })
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
    use crate::{
        BoxFuture, Custom, NodeBuilder, Particles, SceneDef, Shader, SourceLocation, View,
    };
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

    fn light_custom(view: View) -> BoxFuture {
        Box::pin(async move {
            let _ = view.add(
                Custom::new(|ctx, _| {
                    ctx.rect(oxframe_core::kurbo::Rect::new(-300.0, -100.0, 300.0, 100.0))
                        .fill(Color::rgb(0.95, 0.95, 0.95));
                })
                .size(600.0, 200.0)
                .name("panel"),
            );
            let _ = view.add(
                crate::Txt::new()
                    .text("Hard to read")
                    .fill(Color::WHITE)
                    .name("caption"),
            );
            crate::wait_for(0.5).await;
        })
    }

    #[test]
    fn low_contrast_reads_the_pixels_behind_text_over_custom() {
        // The fills say white on the black background; the pixels say white
        // on light gray.
        let report = report(project(light_custom), &Options::default()).unwrap();
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.rule == "low_contrast" && i.node == "caption"),
            "{:?}",
            report.issues
        );
    }

    #[test]
    fn low_contrast_over_pixels_passes_readable_text() {
        fn dark_custom(view: View) -> BoxFuture {
            Box::pin(async move {
                let _ = view.add(
                    Custom::new(|ctx, _| {
                        ctx.rect(oxframe_core::kurbo::Rect::new(-300.0, -100.0, 300.0, 100.0))
                            .fill(Color::rgb(0.1, 0.1, 0.12));
                    })
                    .size(600.0, 200.0),
                );
                let _ = view.add(
                    crate::Txt::new()
                        .text("Easy to read")
                        .fill(Color::WHITE)
                        .name("caption"),
                );
                crate::wait_for(0.5).await;
            })
        }
        let report = report(project(dark_custom), &Options::default()).unwrap();
        assert!(
            report.issues.iter().all(|i| i.rule != "low_contrast"),
            "{:?}",
            report.issues
        );
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
    fn an_issue_on_a_node_gets_a_crop_with_the_node_outlined() {
        fn edge(view: View) -> BoxFuture {
            Box::pin(async move {
                let _ = view.add(
                    crate::Rect::new()
                        .size(200.0)
                        .x(900.0)
                        .fill(Color::WHITE)
                        .name("edge"),
                );
                crate::wait_for(1.0).await;
            })
        }
        let checked = check(project(edge), &Options::default()).unwrap();
        let issue = checked
            .report
            .issues
            .iter()
            .find(|i| i.rule == "out_of_frame" && i.node == "edge")
            .expect("out_of_frame on edge");
        let crop = issue.crop.as_deref().expect("crop path");
        assert!(crop.starts_with("issues/01-out_of_frame"), "{crop}");
        let json = serde_json::to_value(issue).unwrap();
        assert_eq!(json["crop"], crop);
        assert_eq!(json["bounds"].as_array().map(Vec::len), Some(4));
        let (_, img) = checked
            .frames
            .iter()
            .find(|(name, _)| name == crop)
            .expect("crop image written");
        // The rect hangs off the right edge: the crop ends at the frame edge
        // and is smaller than the frame.
        assert!(
            img.width < 1920 && img.height < 1080,
            "{}x{}",
            img.width,
            img.height
        );
        let count = |rgba: [u8; 4]| img.data.chunks(4).filter(|p| *p == rgba).count();
        assert!(count([0xff, 0x30, 0x30, 0xff]) > 100, "outline drawn");
        // The visible part of the rect (40 x 200 px at the right edge) is in
        // the crop.
        let white = count([0xff, 0xff, 0xff, 0xff]);
        assert!(white > 30 * 190, "node in the crop ({white} px)");
    }

    #[test]
    fn compare_counts_changed_pixels_and_paints_them_red() {
        let a = Image {
            width: 2,
            height: 1,
            data: vec![10, 10, 10, 255, 200, 200, 200, 255],
        };
        let mut b = a.clone();
        assert_eq!(compare(&a, &b).unwrap().0, 0.0);
        b.data[4] = 100; // the second pixel moves; a step of 4 would not count
        let (share, picture) = compare(&a, &b).unwrap();
        assert!((share - 0.5).abs() < 1e-9);
        assert_eq!(&picture.data[4..8], &[0xff, 0x30, 0x30, 0xff]);
        assert_eq!(&picture.data[0..3], &[3, 3, 3]);
        let small = Image {
            width: 1,
            height: 1,
            data: vec![0, 0, 0, 255],
        };
        assert!(compare(&a, &small).is_none());
    }

    #[test]
    fn against_reports_changed_same_added_and_gone_frames() {
        let dir = std::env::temp_dir().join(format!("oxframe-against-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let opts = Options {
            full_frames: true,
            ..Options::default()
        };
        let before = check(project(long), &opts).unwrap();
        write_output(&dir, &before).unwrap();
        // One reference frame altered, one deleted, one that no check makes.
        let frames: Vec<&String> = before
            .frames
            .iter()
            .map(|(n, _)| n)
            .filter(|n| n.starts_with("frames/"))
            .collect();
        let mut altered = before
            .frames
            .iter()
            .find(|(n, _)| n == frames[1])
            .unwrap()
            .1
            .clone();
        for b in altered.data.iter_mut().step_by(4) {
            *b = b.wrapping_add(100);
        }
        std::fs::write(dir.join(frames[1]), altered.encode_png()).unwrap();
        std::fs::remove_file(dir.join(frames[2])).unwrap();
        std::fs::write(dir.join("frames/99.00.png"), altered.encode_png()).unwrap();

        let now = check(project(long), &opts).unwrap();
        let (c, diffs) = against(&dir, &now);
        assert_eq!(c.changed.len(), 1, "{c:?}");
        assert_eq!(&c.changed[0].frame, frames[1]);
        assert!(c.changed[0].changed > 0.9);
        assert_eq!(diffs.len(), 1);
        assert_eq!(c.added, vec![frames[2].clone()]);
        assert_eq!(c.removed, vec!["frames/99.00.png".to_owned()]);
        assert_eq!(c.unchanged, frames.len() - 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_node_that_travels_is_in_motion_and_drawn() {
        let checked = check(project(long), &Options::default()).unwrap();
        let m = checked
            .report
            .motion
            .iter()
            .find(|m| m.node == "box")
            .expect("box moves");
        assert!(m.distance > 150.0, "{m:?}");
        // Eased in and out over 30 s: it moves visibly through the middle.
        assert!(
            m.moves[0][0] < 10.0 && m.moves[0][1] > 20.0,
            "{:?}",
            m.moves
        );
        let json = serde_json::to_value(&checked.report).unwrap();
        assert_eq!(json["motion"][0]["node"], "box");
        assert!(json["motion"][0].get("path").is_none());
        assert!(
            checked
                .frames
                .iter()
                .any(|(n, _)| n == "trajectories/main.png")
        );
        // Nothing moves: no motion in the JSON, no picture.
        let still = check(project(hold), &Options::default()).unwrap();
        assert!(still.report.motion.is_empty());
        assert!(
            serde_json::to_value(&still.report)
                .unwrap()
                .get("motion")
                .is_none()
        );
        assert!(
            !still
                .frames
                .iter()
                .any(|(n, _)| n.starts_with("trajectories/"))
        );
    }

    #[test]
    fn a_hard_cut_puts_both_sides_on_the_storyboard() {
        let project = Project::new("cuts")
            .scene(scene("first", hold))
            .scene(scene("second", hold))
            .write_meta(false)
            .inline_meta(BTreeMap::<String, Meta>::new());
        let opts = Options {
            full_frames: true,
            ..Options::default()
        };
        let checked = check(project, &opts).unwrap();
        let names: Vec<&str> = checked.frames.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            names.iter().any(|n| n.ends_with("-first-end.png")),
            "{names:?}"
        );
        assert!(
            names.iter().any(|n| n.ends_with("-second.png")),
            "{names:?}"
        );
    }

    #[test]
    fn a_node_wholly_outside_gets_the_edge_strip() {
        assert_eq!(
            crop_rect([2000.0, 500.0, 2080.0, 580.0], (1920, 1080)),
            Some([1760, 468, 1920, 612])
        );
        assert_eq!(
            crop_rect([-300.0, -300.0, -200.0, -200.0], (1920, 1080)),
            Some([0, 0, 160, 160])
        );
        assert_eq!(
            crop_rect([100.0, 100.0, 300.0, 200.0], (1920, 1080)),
            Some([50, 50, 350, 250])
        );
        assert_eq!(crop_rect([f64::NAN, 0.0, 1.0, 1.0], (1920, 1080)), None);
    }

    #[test]
    fn report_without_check_has_no_crops() {
        let report = report(project(parked), &Options::default()).unwrap();
        assert!(report.issues.iter().all(|i| i.crop.is_none()));
        assert!(report.issues.iter().any(|i| i.bounds.is_some()));
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
    fn a_shader_that_does_not_compile_fails_check() {
        fn broken(view: View) -> BoxFuture {
            Box::pin(async move {
                let _ = view.add(
                    Shader::source("this is not wgsl")
                        .width(80.0)
                        .height(40.0)
                        .name("sea"),
                );
                crate::wait_for(0.2).await;
            })
        }
        let report = report(project(broken), &Options::default()).unwrap();
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.rule == "shader_error" && i.node == "sea"),
            "{:?}",
            report.issues
        );
    }

    #[test]
    fn a_lens_in_a_fading_group_passes_check() {
        fn fading(view: View) -> BoxFuture {
            Box::pin(async move {
                let group = view.add(crate::Group::new().opacity(0.5));
                group.add(crate::Rect::new().size(80.0));
                group.add(crate::shaders::lens_drops().fullscreen().name("lens"));
                crate::wait_for(0.2).await;
            })
        }
        let report = report(project(fading), &Options::default()).unwrap();
        assert!(
            report.issues.iter().all(|i| i.rule != "input_cycle"),
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
    fn waves_match_on_cpu_and_gpu() {
        fn sea(view: View) -> BoxFuture {
            Box::pin(async move {
                let _ = view.add(crate::waves().width(120.0).height(60.0));
                crate::wait_for(0.05).await;
            })
        }
        let mut engine = crate::Engine::new(project(sea)).unwrap();
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
        assert_eq!(px(&cpu, cx, cy - 20), px(&gpu_img, cx, cy - 20));
        assert_eq!(px(&cpu, cx, cy + 20), px(&gpu_img, cx, cy + 20));
        assert!(px(&cpu, cx, cy - 20)[2] > px(&cpu, cx, cy + 20)[2]);
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
