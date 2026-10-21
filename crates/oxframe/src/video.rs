//! Video frames through ffmpeg, for [`Video`](crate::Video) nodes.
//!
//! `ffprobe` reads the size, frame rate, length and streams once. Frames
//! come from an `ffmpeg` process that writes raw RGBA: while a render asks
//! for frame after frame, the same process keeps going; a seek starts a new
//! one with an accurate `-ss`. Recent frames stay in a small cache, so the
//! editor scrubbing back and forth decodes little.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::rc::Rc;

use oxframe_2d::{RasterImage, VideoInfo, VideoSource};

/// Frames kept, at most about this many bytes.
const CACHE_BYTES: usize = 256 << 20;
/// A forward jump up to this many frames is read through instead of
/// starting a new process.
const READ_AHEAD: u64 = 48;
/// Wider clips are scaled down to this width as they are decoded.
const MAX_WIDTH: u32 = 1920;

/// Makes `Video` nodes decode with ffmpeg on this thread.
pub(crate) fn install() {
    oxframe_2d::set_video_opener(Rc::new(|path: &Path| {
        FfmpegVideo::open(path).map(|v| Rc::new(v) as Rc<dyn VideoSource>)
    }));
}

struct Pipe {
    child: Child,
    out: ChildStdout,
    /// Index of the frame the next read returns.
    next: u64,
}

impl Drop for Pipe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct FfmpegVideo {
    path: PathBuf,
    ffmpeg: PathBuf,
    info: VideoInfo,
    /// Decoded size (scaled down from the clip if it is wider than 1920).
    size: (u32, u32),
    /// The VP9 decoder that keeps alpha (libvpx) must be asked for.
    vp9_alpha: bool,
    cache: RefCell<VecDeque<(u64, Rc<RasterImage>)>>,
    pipe: RefCell<Option<Pipe>>,
}

impl FfmpegVideo {
    fn open(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Err("no such file".into());
        }
        let ffmpeg = oxframe_export::find_ffmpeg(None)
            .map_err(|_| "ffmpeg is not installed (it decodes videos)".to_string())?;
        let ffprobe = ffmpeg.with_file_name(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        });
        let out = Command::new(&ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,codec_name,width,height,avg_frame_rate,r_frame_rate,pix_fmt:stream_tags=alpha_mode:format=duration",
                "-of",
                "json",
            ])
            .arg(path)
            .output()
            .map_err(|e| format!("cannot run ffprobe: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        let probe: serde_json::Value =
            serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe output: {e}"))?;
        let streams = probe["streams"].as_array().cloned().unwrap_or_default();
        let video = streams
            .iter()
            .find(|s| s["codec_type"] == "video")
            .ok_or("the file has no video stream")?;
        let has_audio = streams.iter().any(|s| s["codec_type"] == "audio");
        let width = video["width"].as_u64().unwrap_or(0) as u32;
        let height = video["height"].as_u64().unwrap_or(0) as u32;
        let fps = rate(video["avg_frame_rate"].as_str())
            .or_else(|| rate(video["r_frame_rate"].as_str()))
            .ok_or("unknown frame rate")?;
        let duration = probe["format"]["duration"]
            .as_str()
            .and_then(|d| d.parse::<f64>().ok())
            .unwrap_or(0.0);
        if width == 0 || height == 0 {
            return Err("unknown frame size".into());
        }
        let vp9_alpha = video["codec_name"] == "vp9" && video["tags"]["alpha_mode"] == "1";
        let size = if width > MAX_WIDTH {
            let h = (f64::from(height) * f64::from(MAX_WIDTH) / f64::from(width)).round() as u32;
            (MAX_WIDTH, h.max(2) & !1)
        } else {
            (width, height)
        };
        Ok(Self {
            path: path.to_path_buf(),
            ffmpeg,
            info: VideoInfo {
                width,
                height,
                fps,
                duration,
                has_audio,
            },
            size,
            vp9_alpha,
            cache: RefCell::new(VecDeque::new()),
            pipe: RefCell::new(None),
        })
    }

    fn frame_bytes(&self) -> usize {
        self.size.0 as usize * self.size.1 as usize * 4
    }

    /// A process that writes frames from `index` on.
    fn start(&self, index: u64) -> Option<Pipe> {
        // Half a frame early, so the first frame out is `index` even when
        // its timestamp is rounded down.
        let at = ((index as f64 - 0.5) / self.info.fps).max(0.0);
        let mut cmd = Command::new(&self.ffmpeg);
        cmd.args(["-v", "error", "-nostdin"]);
        if self.vp9_alpha {
            cmd.args(["-c:v", "libvpx-vp9"]);
        }
        cmd.args(["-ss", &format!("{at:.6}"), "-i"])
            .arg(&self.path)
            .args(["-an", "-sn", "-map", "0:v:0"]);
        if self.size != (self.info.width, self.info.height) {
            cmd.args(["-vf", &format!("scale={}:{}", self.size.0, self.size.1)]);
        }
        cmd.args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn().ok()?;
        let out = child.stdout.take()?;
        Some(Pipe {
            child,
            out,
            next: index,
        })
    }

    fn remember(&self, index: u64, image: Rc<RasterImage>) {
        let mut cache = self.cache.borrow_mut();
        let keep = (CACHE_BYTES / self.frame_bytes().max(1)).max(8);
        while cache.len() >= keep {
            cache.pop_front();
        }
        cache.push_back((index, image));
    }
}

impl VideoSource for FfmpegVideo {
    fn info(&self) -> VideoInfo {
        self.info
    }

    fn frame(&self, index: u64) -> Option<Rc<RasterImage>> {
        if let Some((_, img)) = self.cache.borrow().iter().find(|(i, _)| *i == index) {
            return Some(img.clone());
        }
        let mut pipe = self.pipe.borrow_mut();
        let reuse = matches!(&*pipe, Some(p) if p.next <= index && index - p.next <= READ_AHEAD);
        if !reuse {
            *pipe = self.start(index);
        }
        let p = pipe.as_mut()?;
        let mut buf = vec![0u8; self.frame_bytes()];
        loop {
            if p.out.read_exact(&mut buf).is_err() {
                *pipe = None;
                return None;
            }
            let i = p.next;
            p.next += 1;
            let image = Rc::new(RasterImage {
                width: self.size.0,
                height: self.size.1,
                rgba: buf.clone(),
                id: id(&self.path, i),
            });
            self.remember(i, image.clone());
            if i == index {
                return Some(image);
            }
        }
    }
}

/// `"30000/1001"` as frames per second.
fn rate(s: Option<&str>) -> Option<f64> {
    let (n, d) = s?.split_once('/')?;
    let (n, d) = (n.parse::<f64>().ok()?, d.parse::<f64>().ok()?);
    (n > 0.0 && d > 0.0).then(|| n / d)
}

/// Stable identity of a frame for renderer caches.
fn id(path: &Path, index: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    index.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use oxframe_core::BoxFuture;
    use oxframe_protocol::Meta;

    use super::*;
    use crate::{Engine, Project, SceneDef, SourceLocation, View};

    thread_local! {
        static CLIP: RefCell<PathBuf> = const { RefCell::new(PathBuf::new()) };
    }

    /// A fresh directory, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static N: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "oxframe-video-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn ffmpeg() -> Option<PathBuf> {
        oxframe_export::find_ffmpeg(None).ok()
    }

    /// A clip whose frames all differ (testsrc draws a counter).
    fn make_clip(dir: &Path, fps: u32, seconds: f64, audio: bool) -> PathBuf {
        let out = dir.join(format!("clip{fps}.mp4"));
        let mut cmd = Command::new(ffmpeg().unwrap());
        cmd.args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg(format!("testsrc=size=160x90:rate={fps}:duration={seconds}"));
        if audio {
            cmd.args(["-f", "lavfi", "-i"])
                .arg(format!("sine=frequency=440:duration={seconds}"));
        }
        cmd.args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g"])
            .arg(fps.to_string());
        if audio {
            cmd.args(["-c:a", "aac", "-shortest"]);
        }
        assert!(cmd.arg(&out).status().unwrap().success());
        out
    }

    /// Frame `n` as ffmpeg extracts it on its own.
    fn extract(path: &Path, n: u64) -> Vec<u8> {
        let out = Command::new(ffmpeg().unwrap())
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-vf", &format!("select=eq(n\\,{n})"), "-vframes", "1"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .output()
            .unwrap();
        out.stdout
    }

    #[test]
    fn frames_match_what_ffmpeg_extracts() {
        if ffmpeg().is_none() {
            return;
        }
        let dir = TempDir::new();
        for fps in [24, 30, 60] {
            let path = make_clip(dir.path(), fps, 2.0, false);
            let video = FfmpegVideo::open(&path).unwrap();
            assert!((video.info.fps - f64::from(fps)).abs() < 1e-6);
            // In order, then a jump back and a jump ahead, as an editor does.
            for n in [
                0,
                1,
                2,
                3,
                17,
                5,
                u64::from(fps) + 7,
                u64::from(fps * 2) - 1,
            ] {
                let frame = video
                    .frame(n)
                    .unwrap_or_else(|| panic!("{fps} fps frame {n}"));
                assert_eq!(frame.rgba, extract(&path, n), "{fps} fps frame {n}");
            }
        }
    }

    fn playing(view: View) -> BoxFuture {
        Box::pin(async move {
            let path = CLIP.with(|c| c.borrow().clone());
            let clip = view.add(crate::Video::new().file(&path).width(320.0));
            clip.play();
            crate::wait_for(0.5).await;
            clip.pause();
            crate::wait_for(0.25).await;
            clip.seek(1.0);
            clip.play();
            crate::wait_for(0.5).await;
        })
    }

    fn engine(fps: u32) -> Engine {
        let project = Project::new("video")
            .size(320, 180)
            .fps(fps)
            .write_meta(false)
            .inline_meta(BTreeMap::<String, Meta>::new())
            .scene(SceneDef::new(
                "main",
                SourceLocation {
                    file: "src/video.rs",
                    manifest_dir: env!("CARGO_MANIFEST_DIR"),
                },
                playing,
            ));
        Engine::new(project).unwrap()
    }

    fn shown(engine: &mut Engine, frame: u32) -> u64 {
        engine
            .seek(frame)
            .draw_list()
            .iter()
            .find_map(|i| match &i.prim {
                oxframe_2d::Primitive::Image { image, .. } => Some(image.id),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn a_60_fps_scene_shows_the_clip_frame_of_its_time() {
        if ffmpeg().is_none() {
            return;
        }
        let dir = TempDir::new();
        for fps in [24u32, 30, 60] {
            let path = make_clip(dir.path(), fps, 3.0, true);
            CLIP.with(|c| *c.borrow_mut() = path.clone());
            let mut e = engine(60);
            for scene_frame in [0u32, 1, 7, 29] {
                let t = f64::from(scene_frame) / 60.0;
                let want = (t * f64::from(fps) + 1e-6).floor() as u64;
                assert_eq!(
                    shown(&mut e, scene_frame),
                    id(&path, want),
                    "{fps} fps at {t}"
                );
            }
            // Paused at 0.5 s for a quarter second: the same frame.
            let held = shown(&mut e, 31);
            assert_eq!(shown(&mut e, 40), held);
            // Seeking back gives the same frame as playing to it.
            let a = shown(&mut e, 70);
            e.seek(0);
            assert_eq!(shown(&mut e, 70), a);
        }
    }

    #[test]
    fn the_sound_follows_play_pause_and_seek() {
        if ffmpeg().is_none() {
            return;
        }
        let dir = TempDir::new();
        let path = make_clip(dir.path(), 30, 3.0, true);
        CLIP.with(|c| *c.borrow_mut() = path.clone());
        let e = engine(60);
        let clips = e.scheduled();
        assert_eq!(clips.len(), 2, "{clips:?}");
        // Played 0..0.5 s of the file from scene time 0.
        assert!(clips[0].start.abs() < 1e-9);
        assert!(clips[0].trim_start.abs() < 1e-9);
        assert!(
            (clips[0].trim_end.unwrap() - 0.5).abs() < 0.02,
            "{:?}",
            clips[0]
        );
        // After the pause and the seek, from 1.0 s in the file at scene 0.75 s.
        assert!((clips[1].start - 0.75).abs() < 0.02, "{:?}", clips[1]);
        assert!((clips[1].trim_start - 1.0).abs() < 0.02, "{:?}", clips[1]);
    }

    /// Spec: seeking in a five-minute clip takes under 200 ms. Slow (it
    /// encodes the clip first), so it runs on demand:
    /// `cargo test -p oxframe --features video --release -- --ignored seek_in`.
    #[test]
    #[ignore = "encodes a five-minute 1080p clip"]
    fn seek_in_a_five_minute_clip_is_fast() {
        if ffmpeg().is_none() {
            return;
        }
        let dir = TempDir::new();
        let path = dir.path().join("long.mp4");
        assert!(
            Command::new(ffmpeg().unwrap())
                .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
                .arg("testsrc=size=1920x1080:rate=30:duration=300")
                .args([
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-pix_fmt",
                    "yuv420p"
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let video = FfmpegVideo::open(&path).unwrap();
        let mut times = Vec::new();
        // Jumps an editor makes: across the clip, back and forth.
        for n in [4500u64, 120, 8000, 2999, 6011, 333, 8990, 4501] {
            let started = std::time::Instant::now();
            assert!(video.frame(n).is_some(), "frame {n}");
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        let worst = times[times.len() - 1];
        eprintln!(
            "seek ms: median {:.0}, worst {worst:.0}",
            times[times.len() / 2]
        );
        assert!(worst < 200.0, "a seek took {worst:.0} ms");
    }

    #[test]
    fn webm_vp9_and_prores_4444_keep_their_alpha() {
        if ffmpeg().is_none() {
            return;
        }
        let dir = TempDir::new();
        for (name, args) in [
            (
                "alpha.webm",
                vec!["-c:v", "libvpx-vp9", "-pix_fmt", "yuva420p"],
            ),
            (
                "alpha.mov",
                vec![
                    "-c:v",
                    "prores_ks",
                    "-profile:v",
                    "4444",
                    "-pix_fmt",
                    "yuva444p10le",
                ],
            ),
        ] {
            let path = dir.path().join(name);
            assert!(
                Command::new(ffmpeg().unwrap())
                    .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
                    .arg("color=c=red@0.5:size=64x36:rate=30:d=1,format=yuva444p")
                    .args(&args)
                    .arg(&path)
                    .status()
                    .unwrap()
                    .success()
            );
            let video = FfmpegVideo::open(&path).unwrap();
            let frame = video.frame(3).unwrap();
            let a = frame.rgba[3];
            assert!(
                (100..=160).contains(&a),
                "{name}: alpha {a}, wanted about 128"
            );
            assert!(frame.rgba[0] > 200, "{name}: red {}", frame.rgba[0]);
        }
    }
}
