//! Command line of a project binary. `oxframe-cli` drives these commands;
//! they also work directly via `cargo run -- <command>`.

use std::path::PathBuf;
use std::time::Instant;

use oxframe_export::{Audio, ExportOptions, Exporter, Format};
use oxframe_render::{CpuRenderer, Renderer};

use oxframe_core::FrameSource;

use crate::{Aspect, Engine, PreviewAdapter, Project};

pub(crate) const EXIT_OK: i32 = 0;
pub(crate) const EXIT_ERROR: i32 = 1;
pub(crate) const EXIT_ENV: i32 = 2;

const HELP: &str = "\
usage: <project> <command> [options]

commands:
  render   render the project to a video or PNG sequence
             --out <path>        output file or directory (default: out/<project>.mp4)
             --format <f>        mp4 | webm | prores | png (default: from --out extension)
             --scene <name>      render only this scene
             --range <a>..<b>    time range in seconds, e.g. 0.5..2 (either side optional)
             --at <seconds>      only this frame, as PNG (repeatable); with one --at and
                                 --out file.png writes that file, else out/snapshots/
             --size <w>x<h>      output size (default: project size)
             --aspect <shape>    16:9, 9:16, 1:1, or all (every Project::aspects format)
             --gpu               render on the GPU (vello); the default is the CPU (tiny-skia)
             --ffmpeg <path>     ffmpeg binary (default: from PATH)
             --no-audio          do not mux the project's soundtrack
             --subtitles <path>  also write the spoken lines as .srt or .vtt
             --slides <path>     instead, a PDF with the frame of every stop
                                 (slide_break and scene ends), no ffmpeg
             --jobs <n>          render 2-second segments in n processes at once
                                 (default: cores, at most 8); the output is the
                                 same for any n
  check    write out/check/storyboard.png and report.json (CPU, exits 1 on errors)
             --out <dir>         output directory (default: out/check)
             --frames            also write each storyboard frame at full size
             --against <dir>     compare the frames with an earlier --frames output;
                                 writes diff/ and `comparison` in report.json
             --interval <s>      seconds between storyboard frames, plus every marker
             --safe-inset <px>   safe area; 0 disables that warning
             --min-contrast <n>  WCAG ratio for text (default 4.5)
             --min-motion <s>    large moves shorter than this are too_fast
             --max-still <s>     pause with nothing changing (needs no soundtrack)
             --chars-per-second <n>
             --min-chars <n>     too_fast ignores shorter text
             --cell-width <px>   storyboard cell width (default 320)
  inspect  print the scene tree at a time as JSON
             --at <seconds>      required
             --scene <name>      --at is measured from the start of this scene
             --graph             also list render passes (vector, then composite)
  preview  run the preview server for the editor
             --port <n>          port on 127.0.0.1 (0 = pick a free one; printed on stdout)
  info     print the project timeline as JSON

  --theme <name>     override the project theme: dark, light, midnight, paper, vivid
  --accent <#hex>    rebuild that theme from one brand color
";

pub(crate) fn run(mut project: Project, args: Vec<String>) -> i32 {
    let args = match apply_theme_flags(&mut project, args) {
        Ok(a) => a,
        Err(e) => return usage_error(&e),
    };
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("render") => match RenderArgs::parse(args) {
            Ok(a) => render(project, a),
            Err(e) => usage_error(&e),
        },
        Some("check") => crate::feedback::command_check(project, args),
        Some("inspect") => crate::feedback::command_inspect(project, args),
        Some("preview") => crate::preview::run(project, args.collect()),
        Some("info") => info(project),
        Some("-h" | "--help" | "help") | None => {
            print!("{HELP}");
            EXIT_OK
        }
        Some(other) => usage_error(&format!("unknown command `{other}`")),
    }
}

/// `--theme <preset>` and `--accent <#hex>` may appear anywhere. They override
/// the theme compiled into the project (what `oxframe.toml` `[theme]` forwards).
fn apply_theme_flags(project: &mut Project, args: Vec<String>) -> Result<Vec<String>, String> {
    let mut kept = Vec::with_capacity(args.len());
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--theme" => {
                let name = args
                    .next()
                    .ok_or_else(|| "--theme needs a name".to_owned())?;
                project.apply_preset(&name)?;
            }
            "--accent" => {
                let hex = args
                    .next()
                    .ok_or_else(|| "--accent needs a #rrggbb color".to_owned())?;
                project.apply_accent(&hex)?;
            }
            _ => kept.push(flag),
        }
    }
    Ok(kept)
}

pub(crate) fn usage_error(msg: &str) -> i32 {
    eprintln!("error: {msg}\n\n{HELP}");
    EXIT_ENV
}

#[derive(Debug, Default, Clone)]
struct RenderArgs {
    out: Option<PathBuf>,
    format: Option<Format>,
    scene: Option<String>,
    range: Option<(f64, f64)>,
    /// Single frames to write as PNG (seconds); see `snapshot`.
    at: Vec<f64>,
    size: Option<(u32, u32)>,
    /// One shape, or every shape the project declares (`all`).
    aspect: Option<AspectPick>,
    cpu: bool,
    gpu: bool,
    ffmpeg: Option<PathBuf>,
    no_audio: bool,
    subtitles: Option<PathBuf>,
    /// `--slides out.pdf`: a page per presentation stop.
    slides: Option<PathBuf>,
    /// Segments rendered at once.
    jobs: Option<usize>,
    /// `--frames a..b`: global frames `a` to `b` (exclusive). Set by the
    /// parent process for one segment of a parallel render.
    frames: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AspectPick {
    One(Aspect),
    All,
}

impl RenderArgs {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut a = Self::default();
        while let Some(flag) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
            match flag.as_str() {
                "--out" => a.out = Some(value()?.into()),
                "--scene" => a.scene = Some(value()?),
                "--format" => {
                    let f = value()?;
                    a.format = Some(Format::parse(&f).ok_or_else(|| {
                        format!("unknown format `{f}`; use mp4, webm, prores or png")
                    })?);
                }
                "--at" => {
                    let v = value()?;
                    a.at.push(
                        v.parse()
                            .map_err(|_| format!("bad time `{v}`, expected seconds"))?,
                    );
                }
                "--range" => {
                    let v = value()?;
                    let (s, e) = v
                        .split_once("..")
                        .ok_or_else(|| format!("bad range `{v}`, expected a..b"))?;
                    let parse = |x: &str, default: f64| {
                        if x.is_empty() {
                            Ok(default)
                        } else {
                            x.parse::<f64>().map_err(|_| format!("bad range `{v}`"))
                        }
                    };
                    a.range = Some((parse(s, 0.0)?, parse(e, f64::INFINITY)?));
                }
                "--size" => {
                    let v = value()?;
                    let (w, h) = v
                        .split_once('x')
                        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                        .ok_or_else(|| format!("bad size `{v}`, expected WxH"))?;
                    a.size = Some((w, h));
                }
                "--aspect" => {
                    let v = value()?;
                    a.aspect =
                        Some(if v == "all" {
                            AspectPick::All
                        } else {
                            AspectPick::One(Aspect::parse(&v).ok_or_else(|| {
                                format!("bad aspect `{v}` (16:9, 9:16, 1:1, or all)")
                            })?)
                        });
                }
                "--cpu" => a.cpu = true,
                "--gpu" => a.gpu = true,
                "--ffmpeg" => a.ffmpeg = Some(value()?.into()),
                "--no-audio" => a.no_audio = true,
                "--subtitles" => a.subtitles = Some(value()?.into()),
                "--slides" => a.slides = Some(value()?.into()),
                "--jobs" => {
                    let v = value()?;
                    a.jobs =
                        Some(v.parse::<usize>().ok().filter(|n| *n > 0).ok_or_else(|| {
                            format!("bad --jobs `{v}`, expected a positive number")
                        })?);
                }
                "--frames" => {
                    let v = value()?;
                    let (s, e) = v
                        .split_once("..")
                        .and_then(|(s, e)| Some((s.parse().ok()?, e.parse().ok()?)))
                        .ok_or_else(|| format!("bad --frames `{v}`, expected a..b"))?;
                    a.frames = Some((s, e));
                }
                other => return Err(format!("unknown option `{other}`")),
            }
        }
        Ok(a)
    }
}

/// Length of a segment of a parallel render. Every segment starts with a
/// key frame, so shorter ones cost bitrate; longer ones leave cores idle on
/// short videos.
const SEGMENT_SECONDS: u32 = 2;

/// Segments rendered at once by default: one per core, at most 8 (each
/// process holds a full engine).
fn default_jobs() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(8)
}

struct SegmentJob {
    frames: std::ops::Range<u32>,
    segment: u32,
    fps: u32,
    size: (u32, u32),
    format: Format,
    out: PathBuf,
    audio: Option<Audio>,
    ffmpeg: Option<PathBuf>,
    seconds: f64,
    jobs: usize,
}

struct RemoveDirOnDrop(PathBuf);

impl Drop for RemoveDirOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The arguments this process was started with, for one segment: the same
/// project and settings (theme, size, renderer), with its own frames and
/// output and no sound (the sound is added once, after joining).
fn segment_args(
    range: &std::ops::Range<u32>,
    out: &std::path::Path,
    format: Format,
) -> Vec<String> {
    let replaced = [
        "--out",
        "--range",
        "--jobs",
        "--subtitles",
        "--slides",
        "--frames",
        "--format",
    ];
    let mut args = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if replaced.contains(&a.as_str()) {
            it.next();
        } else if a != "--no-audio" {
            args.push(a);
        }
    }
    let name = match format {
        Format::Png => "png",
        f => f.extension(),
    };
    args.extend([
        "--frames".to_owned(),
        format!("{}..{}", range.start, range.end),
        "--out".to_owned(),
        out.to_string_lossy().into_owned(),
        "--format".to_owned(),
        name.to_owned(),
        "--no-audio".to_owned(),
        "--jobs".to_owned(),
        "1".to_owned(),
    ]);
    args
}

/// Renders `job.frames` in segments, each in a child process of this
/// binary, at most `job.jobs` at once, then joins them.
fn render_segments(job: &SegmentJob) -> i32 {
    use std::process::{Command, Stdio};

    let segments: Vec<std::ops::Range<u32>> = (job.frames.start..job.frames.end)
        .step_by(job.segment as usize)
        .map(|a| a..(a + job.segment).min(job.frames.end))
        .collect();
    let parent = job
        .out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);
    let tmp = parent.join(format!(".oxframe-render-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&tmp) {
        eprintln!("error: cannot create {}: {e}", tmp.display());
        return EXIT_ENV;
    }
    let _cleanup = RemoveDirOnDrop(tmp.clone());
    let seg_out = |i: usize| match job.format {
        Format::Png => tmp.join(format!("seg{i:05}")),
        f => tmp.join(format!("seg{i:05}.{}", f.extension())),
    };
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot find this program to start segments: {e}");
            return EXIT_ENV;
        }
    };
    let jobs = job.jobs.clamp(1, segments.len());
    eprintln!(
        "rendering {} frames at {}x{} @ {} fps in {} segments, {jobs} at a time → {}",
        job.frames.len(),
        job.size.0,
        job.size.1,
        job.fps,
        segments.len(),
        job.out.display()
    );
    let started = Instant::now();
    let mut next = 0;
    let mut done = 0;
    let mut running: Vec<(usize, std::process::Child, std::thread::JoinHandle<String>)> =
        Vec::new();
    let failure = 'run: loop {
        while running.len() < jobs && next < segments.len() {
            let spawned = Command::new(&exe)
                .args(segment_args(&segments[next], &seg_out(next), job.format))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn();
            let mut child = match spawned {
                Ok(c) => c,
                Err(e) => {
                    break 'run Some(break_with(
                        &mut running,
                        &format!("cannot start a segment: {e}"),
                    ));
                }
            };
            let mut stderr = child.stderr.take().expect("piped stderr");
            let log = std::thread::spawn(move || {
                let mut s = String::new();
                let _ = std::io::Read::read_to_string(&mut stderr, &mut s);
                s
            });
            running.push((next, child, log));
            next += 1;
        }
        if running.is_empty() {
            break 'run None;
        }
        let mut finished = None;
        for (k, (_, child, _)) in running.iter_mut().enumerate() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    finished = Some((k, status));
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    finished = None;
                    eprintln!("error: {e}");
                }
            }
        }
        let Some((k, status)) = finished else {
            std::thread::sleep(std::time::Duration::from_millis(20));
            continue;
        };
        let (i, _, log) = running.swap_remove(k);
        let log = log.join().unwrap_or_default();
        if !status.success() {
            let tail: Vec<&str> = log.lines().rev().take(12).collect();
            let msg = format!(
                "segment {} (frames {}..{}) failed:\n{}",
                i + 1,
                segments[i].start,
                segments[i].end,
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            );
            break 'run Some(break_with(&mut running, &msg));
        }
        done += 1;
        eprintln!("  segment {done}/{}", segments.len());
    };
    if let Some(code) = failure {
        return code;
    }

    if job.format == Format::Png {
        if let Err(e) = std::fs::create_dir_all(&job.out) {
            eprintln!("error: cannot create {}: {e}", job.out.display());
            return EXIT_ENV;
        }
        for (i, range) in segments.iter().enumerate() {
            for k in 0..range.len() {
                let from = seg_out(i).join(format!("{k:06}.png"));
                let n = range.start - job.frames.start + k as u32;
                let to = job.out.join(format!("{n:06}.png"));
                if let Err(e) = std::fs::rename(&from, &to) {
                    eprintln!("error: cannot move {}: {e}", from.display());
                    return EXIT_ENV;
                }
            }
        }
    } else {
        let files: Vec<PathBuf> = (0..segments.len()).map(seg_out).collect();
        let opts = ExportOptions {
            out: job.out.clone(),
            format: job.format,
            width: job.size.0,
            height: job.size.1,
            fps: job.fps,
            audio: job.audio.clone(),
            ffmpeg: job.ffmpeg.clone(),
            duration: Some(job.seconds),
        };
        if let Err(e) = oxframe_export::concat(&files, &opts) {
            eprintln!("error: {e}");
            return EXIT_ENV;
        }
    }
    let secs = started.elapsed().as_secs_f64();
    eprintln!(
        "done in {secs:.2}s ({:.1} frames/s, {jobs} jobs): {}",
        job.frames.len() as f64 / secs.max(1e-9),
        job.out.display()
    );
    EXIT_OK
}

/// Stops the other segments and reports `message`.
fn break_with(
    running: &mut Vec<(usize, std::process::Child, std::thread::JoinHandle<String>)>,
    message: &str,
) -> i32 {
    for (_, child, _) in running.iter_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    running.clear();
    eprintln!("error: {message}");
    EXIT_ENV
}

/// GPU renderer if compiled in and an adapter exists, else CPU.
pub(crate) fn make_renderer(gpu: bool) -> (Box<dyn Renderer>, &'static str) {
    #[cfg(feature = "gpu")]
    if gpu {
        match oxframe_render::GpuRenderer::new() {
            Ok(r) => return (Box::new(r), "gpu (vello)"),
            Err(e) => eprintln!("warning: {e}; falling back to the CPU renderer"),
        }
    }
    let _ = gpu;
    (Box::new(CpuRenderer::new()), "cpu (tiny-skia)")
}

struct RemoveOnDrop(Option<PathBuf>);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn render(project: Project, args: RenderArgs) -> i32 {
    if let Some(path) = &args.slides {
        return slides(project, &args, path);
    }
    if !args.at.is_empty() {
        return snapshot(project, &args);
    }
    if let Some(jobs) = expand_aspects(&project, &args) {
        if jobs.len() == 1 {
            let mut project = project;
            project.size = jobs[0].1;
            return render_video(project, args);
        }
        let format = args
            .format
            .or_else(|| args.out.as_deref().map(Format::from_path))
            .unwrap_or(Format::Mp4);
        for (aspect, size) in jobs {
            let mut sized = project.clone();
            sized.size = size;
            let mut job = args.clone();
            job.aspect = None;
            job.out = Some(with_tag(
                output_path(&project, &args, format),
                &aspect.label().replace(':', "x"),
            ));
            let code = render_video(sized, job);
            if code != EXIT_OK {
                return code;
            }
        }
        return EXIT_OK;
    }
    render_video(project, args)
}

fn expand_aspects(project: &Project, args: &RenderArgs) -> Option<Vec<(Aspect, (u32, u32))>> {
    if args.size.is_some() {
        return None;
    }
    let pick = args.aspect?;
    let aspects = match pick {
        AspectPick::One(aspect) => vec![aspect],
        AspectPick::All => {
            if project.aspects.is_empty() {
                vec![Aspect::Landscape, Aspect::Portrait, Aspect::Square]
            } else {
                project.aspects.clone()
            }
        }
    };
    Some(
        aspects
            .into_iter()
            .map(|aspect| {
                let size = aspect.frame(project.size);
                (aspect, size)
            })
            .collect(),
    )
}

fn output_path(project: &Project, args: &RenderArgs, format: Format) -> PathBuf {
    args.out.clone().unwrap_or_else(|| {
        let base = PathBuf::from("out").join(project.name());
        match format {
            Format::Png => base,
            f => base.with_extension(f.extension()),
        }
    })
}

fn with_tag(path: PathBuf, tag: &str) -> PathBuf {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("out");
    let tagged = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
            format!("{stem}-{tag}.{ext}")
        }
        None => format!("{name}-{tag}"),
    };
    path.with_file_name(tagged)
}

fn render_video(project: Project, args: RenderArgs) -> i32 {
    let format = args
        .format
        .or_else(|| args.out.as_deref().map(Format::from_path))
        .unwrap_or(Format::Mp4);
    let out = args.out.clone().unwrap_or_else(|| {
        let base = PathBuf::from("out").join(project.name());
        match format {
            Format::Png => base,
            f => base.with_extension(f.extension()),
        }
    });
    if format != Format::Png
        && let Err(e) = oxframe_export::find_ffmpeg(args.ffmpeg.as_deref())
    {
        eprintln!("error: {e}");
        return EXIT_ENV;
    }

    let mut engine = match Engine::new(project) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_ERROR;
        }
    };
    // One scene, on its own: no blend with the scene before it.
    if args.scene.is_some() {
        engine.skip_transitions(true);
    }
    let fps = engine.fps();
    let mut frames = 0..engine.total_frames();
    if let Some(name) = &args.scene {
        let Some(s) = engine.scenes().into_iter().find(|s| &s.name == name) else {
            eprintln!("error: no scene named `{name}`");
            return EXIT_ERROR;
        };
        frames = s.start..s.start + s.frames;
    }
    if let Some((a, b)) = args.range {
        let first = frames.start + (a * f64::from(fps)).round() as u32;
        let last = if b.is_finite() {
            frames.start + (b * f64::from(fps)).round() as u32
        } else {
            frames.end
        };
        frames = first.max(frames.start)..last.min(frames.end);
    }
    if let Some((a, b)) = args.frames {
        frames = a.min(engine.total_frames())..b.min(engine.total_frames());
    }
    if frames.is_empty() {
        eprintln!("error: nothing to render (empty range)");
        return EXIT_ERROR;
    }

    let size = args.size.unwrap_or(engine.size());
    let origin = f64::from(frames.start) / f64::from(fps);
    let seconds = frames.len() as f64 / f64::from(fps);
    if let Some(path) = &args.subtitles {
        let mut cues = engine.voices();
        for cue in &mut cues {
            cue.start -= origin;
            cue.end -= origin;
        }
        cues.retain(|cue| cue.end > 0.0 && cue.start < seconds);
        for cue in &mut cues {
            cue.start = cue.start.max(0.0);
            cue.end = cue.end.min(seconds);
        }
        let vtt = path.extension().and_then(|ext| ext.to_str()) == Some("vtt");
        if let Err(e) = std::fs::write(path, crate::voice::subtitles(&cues, vtt)) {
            eprintln!("error: {e}");
            return EXIT_ERROR;
        }
    }
    let mixed = if !args.no_audio && format != Format::Png {
        match crate::audio::mix_range(&engine, origin, seconds) {
            Ok(mixed) => mixed,
            Err(e) => {
                eprintln!("error: {e}");
                return EXIT_ERROR;
            }
        }
    } else {
        None
    };
    let mix_path = if let Some(mixed) = mixed {
        let path = std::env::temp_dir().join(format!("oxframe-mix-{}.wav", std::process::id()));
        if let Err(e) = oxframe_export::write_wav(&path, &mixed) {
            eprintln!("error: {e}");
            return EXIT_ERROR;
        }
        Some(path)
    } else {
        None
    };
    let _remove_mix = RemoveOnDrop(mix_path.clone());
    let audio = mix_path.as_ref().map(|file| Audio {
        file: file.clone(),
        offset: 0.0,
    });
    // A video longer than one segment is rendered in segments, whatever
    // `--jobs` is, so the file is the same for any number of jobs.
    let segment = fps * SEGMENT_SECONDS;
    if args.frames.is_none() && frames.len() > segment as usize {
        let job = SegmentJob {
            frames: frames.clone(),
            segment,
            fps,
            size,
            format,
            out: out.clone(),
            audio,
            ffmpeg: args.ffmpeg.clone(),
            seconds,
            jobs: args.jobs.unwrap_or_else(default_jobs),
        };
        return render_segments(&job);
    }
    let opts = ExportOptions {
        out: out.clone(),
        format,
        width: size.0,
        height: size.1,
        fps,
        audio,
        ffmpeg: args.ffmpeg.clone(),
        duration: Some(seconds),
    };
    let mut exporter = match Exporter::start(&opts) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_ENV;
        }
    };
    let (renderer, backend) = make_renderer(args.gpu && !args.cpu);
    // From here on frames come through the generic `FrameSource`.
    let mut source = PreviewAdapter::new(engine, renderer);
    eprintln!(
        "rendering {} frames at {}x{} @ {fps} fps with {backend} → {}",
        frames.len(),
        size.0,
        size.1,
        out.display()
    );

    let started = Instant::now();
    let count = frames.len();
    let mut last_report = 0;
    let (mut t_render, mut t_queue) = (0.0, 0.0);
    for (n, frame) in frames.enumerate() {
        let t0 = Instant::now();
        let image = source.render(frame, size);
        let t1 = Instant::now();
        t_render += (t1 - t0).as_secs_f64();
        let pushed = exporter.push(image);
        t_queue += t1.elapsed().as_secs_f64();
        if let Err(e) = pushed {
            eprintln!("error: {e}");
            return EXIT_ENV;
        }
        let pct = (n + 1) * 100 / count;
        if pct >= last_report + 10 || n + 1 == count {
            last_report = pct - pct % 10;
            eprintln!("  {pct:3}%  frame {}/{count}", n + 1);
        }
    }
    if let Err(e) = exporter.finish() {
        eprintln!("error: {e}");
        return EXIT_ENV;
    }
    let secs = started.elapsed().as_secs_f64();
    eprintln!(
        "done in {secs:.2}s ({:.1} frames/s; per frame: simulate+render {:.1} ms, waiting for encoder {:.1} ms): {}",
        count as f64 / secs.max(1e-9),
        1000.0 * t_render / count as f64,
        1000.0 * t_queue / count as f64,
        out.display()
    );
    EXIT_OK
}

fn info(project: Project) -> i32 {
    let engine = match Engine::new(project) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_ERROR;
        }
    };
    let json = serde_json::json!({
        "protocol_version": oxframe_protocol::PROTOCOL_VERSION,
        "engine_version": env!("CARGO_PKG_VERSION"),
        "fps": engine.fps(),
        "size": [engine.size().0, engine.size().1],
        "frames": engine.total_frames(),
        "scenes": engine.scenes(),
        "markers": engine.markers(),
    });
    println!("{}", serde_json::to_string_pretty(&json).expect("json"));
    EXIT_OK
}

/// `render --slides out.pdf`: the frame of every presentation stop, one
/// per page.
fn slides(project: Project, args: &RenderArgs, path: &std::path::Path) -> i32 {
    let engine = match Engine::new(project) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_ERROR;
        }
    };
    let stops = engine.slides();
    let size = args.size.unwrap_or(engine.size());
    let (renderer, _) = make_renderer(args.gpu && !args.cpu);
    let mut source = PreviewAdapter::new(engine, renderer);
    let frames: Vec<_> = stops.iter().map(|s| source.render(s.frame, size)).collect();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(path, crate::pdf::pages(&frames)) {
        eprintln!("error: cannot write {}: {e}", path.display());
        return EXIT_ENV;
    }
    println!("{} pages={}", path.display(), frames.len());
    EXIT_OK
}

/// `render --at <s> [--at <s> …]`: single frames as PNG, no ffmpeg. One
/// time and an `--out` ending in `.png` writes exactly that file; otherwise
/// files `<project>-<time>.png` go into `--out` (default `out/snapshots`).
fn snapshot(project: Project, args: &RenderArgs) -> i32 {
    let name = project.name().to_owned();
    let engine = match Engine::new(project) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_ERROR;
        }
    };
    let fps = f64::from(engine.fps());
    let offset = match &args.scene {
        Some(scene) => match engine.scenes().into_iter().find(|s| &s.name == scene) {
            Some(s) => s.start,
            None => {
                eprintln!("error: no scene named `{scene}`");
                return EXIT_ERROR;
            }
        },
        None => 0,
    };
    let single_file = args.at.len() == 1
        && args
            .out
            .as_ref()
            .is_some_and(|o| o.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")));
    let dir = if single_file {
        None
    } else {
        Some(
            args.out
                .clone()
                .unwrap_or_else(|| PathBuf::from("out/snapshots")),
        )
    };
    if let Some(d) = &dir
        && let Err(e) = std::fs::create_dir_all(d)
    {
        eprintln!("error: cannot create {}: {e}", d.display());
        return EXIT_ENV;
    }
    let size = args.size.unwrap_or(engine.size());
    let (renderer, _) = make_renderer(args.gpu && !args.cpu);
    let mut source = PreviewAdapter::new(engine, renderer);
    let last = source.total_frames().saturating_sub(1);
    for &t in &args.at {
        let frame = (offset + (t.max(0.0) * fps).round() as u32).min(last);
        let path = match &dir {
            Some(d) => d.join(format!("{name}-{t:.2}.png")),
            None => args.out.clone().expect("single file has --out"),
        };
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, source.render(frame, size).encode_png()) {
            eprintln!("error: cannot write {}: {e}", path.display());
            return EXIT_ENV;
        }
        // Machine-readable for tools and agents: one line per file.
        println!("{} frame={frame} t={t:.3}", path.display());
    }
    EXIT_OK
}
