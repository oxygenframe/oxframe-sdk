//! Command line of a project binary. `oxframe-cli` drives these commands;
//! they also work directly via `cargo run -- <command>`.

use std::path::PathBuf;
use std::time::Instant;

use oxframe_export::{Audio, ExportOptions, Exporter, Format};
use oxframe_render::{CpuRenderer, Renderer};

use oxframe_core::FrameSource;

use crate::{Engine, PreviewAdapter, Project};

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
             --cpu               use the CPU renderer (tiny-skia) instead of the GPU
             --ffmpeg <path>     ffmpeg binary (default: from PATH)
             --no-audio          do not mux the project's soundtrack
  check    write out/check/storyboard.png and report.json (CPU, exits 1 on errors)
             --out <dir>         output directory (default: out/check)
             --frames            also write each storyboard frame at full size
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
  preview  run the preview server for the editor
             --port <n>          port on 127.0.0.1 (0 = pick a free one; printed on stdout)
  info     print the project timeline as JSON
";

pub(crate) fn run(project: Project, args: Vec<String>) -> i32 {
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

pub(crate) fn usage_error(msg: &str) -> i32 {
    eprintln!("error: {msg}\n\n{HELP}");
    EXIT_ENV
}

#[derive(Debug, Default)]
struct RenderArgs {
    out: Option<PathBuf>,
    format: Option<Format>,
    scene: Option<String>,
    range: Option<(f64, f64)>,
    /// Single frames to write as PNG (seconds); see `snapshot`.
    at: Vec<f64>,
    size: Option<(u32, u32)>,
    cpu: bool,
    ffmpeg: Option<PathBuf>,
    no_audio: bool,
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
                "--cpu" => a.cpu = true,
                "--gpu" => a.cpu = false,
                "--ffmpeg" => a.ffmpeg = Some(value()?.into()),
                "--no-audio" => a.no_audio = true,
                other => return Err(format!("unknown option `{other}`")),
            }
        }
        Ok(a)
    }
}

/// GPU renderer if compiled in and an adapter exists, else CPU.
pub(crate) fn make_renderer(cpu: bool) -> (Box<dyn Renderer>, &'static str) {
    #[cfg(feature = "gpu")]
    if !cpu {
        match oxframe_render::GpuRenderer::new() {
            Ok(r) => return (Box::new(r), "gpu (vello)"),
            Err(e) => eprintln!("warning: {e}; falling back to the CPU renderer"),
        }
    }
    let _ = cpu;
    (Box::new(CpuRenderer::new()), "cpu (tiny-skia)")
}

fn render(project: Project, args: RenderArgs) -> i32 {
    if !args.at.is_empty() {
        return snapshot(project, &args);
    }
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

    let engine = match Engine::new(project) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_ERROR;
        }
    };
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
    if frames.is_empty() {
        eprintln!("error: nothing to render (empty range)");
        return EXIT_ERROR;
    }

    let size = args.size.unwrap_or(engine.size());
    let audio = (!args.no_audio && format != Format::Png)
        .then(|| engine.audio())
        .flatten()
        .map(|(file, offset)| Audio {
            file: file.into(),
            // The video starts at `frames.start`, so shift the audio too.
            offset: offset - f64::from(frames.start) / f64::from(fps),
        });
    let opts = ExportOptions {
        out: out.clone(),
        format,
        width: size.0,
        height: size.1,
        fps,
        audio,
        ffmpeg: args.ffmpeg.clone(),
    };
    let mut exporter = match Exporter::start(&opts) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_ENV;
        }
    };
    let (renderer, backend) = make_renderer(args.cpu);
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
    let (renderer, _) = make_renderer(args.cpu);
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
