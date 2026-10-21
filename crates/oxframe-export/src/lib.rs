//! Video export: raw RGBA frames are piped into `ffmpeg` (no intermediate
//! files) from a writer thread, so encoding overlaps with rendering.
//!
//! > **Status:** oxframe is under very active development. The API is not stable yet and
//! > may change from version to version. Pin the exact version in your Cargo.toml and check
//! > the [changelog](https://github.com/oxygenframe/oxframe-sdk/blob/main/CHANGELOG.md)
//! > before upgrading.
//!
//! `ffmpeg` is looked up in `PATH` (or an explicit path); if it is missing,
//! [`ExportError::FfmpegNotFound`] carries installation instructions.

#![warn(missing_docs)]

mod mix;

pub use mix::{
    MixClip, MixError, MixRequest, Mixed, PREVIEW_RATE, SAMPLE_RATE, downsample, envelope, mix,
    probe_duration, write_wav,
};

use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::JoinHandle;

pub use oxframe_core::Image;

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// H.264, yuv420p, AAC audio.
    Mp4,
    /// VP9 with alpha, Opus audio.
    Webm,
    /// ProRes 4444 with alpha in .mov, PCM audio.
    ProRes,
    /// Numbered PNG files in a directory.
    Png,
}

impl Format {
    /// From a name or extension (`"mp4"`, `"mov"`…).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "mp4" => Some(Self::Mp4),
            "webm" => Some(Self::Webm),
            "prores" | "mov" => Some(Self::ProRes),
            "png" => Some(Self::Png),
            _ => None,
        }
    }

    /// Guesses from the output path; a path without extension is a PNG
    /// directory.
    pub fn from_path(path: &Path) -> Self {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(Self::parse)
            .unwrap_or(Self::Png)
    }

    /// File extension; empty for PNG directories.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
            Self::ProRes => "mov",
            Self::Png => "",
        }
    }
}

/// Soundtrack to mux into the video.
#[derive(Debug, Clone)]
pub struct Audio {
    /// Audio file in any format ffmpeg reads.
    pub file: PathBuf,
    /// Seconds; positive delays the audio, negative skips into it.
    pub offset: f64,
}

/// What to export and where.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Output file, or directory for PNG.
    pub out: PathBuf,
    /// Output format.
    pub format: Format,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Frames per second.
    pub fps: u32,
    /// Optional soundtrack (ignored for PNG).
    pub audio: Option<Audio>,
    /// Explicit ffmpeg binary; otherwise found in `PATH`.
    pub ffmpeg: Option<PathBuf>,
    /// Stop after this many seconds. Without it, padding the soundtrack can
    /// run past the last frame.
    pub duration: Option<f64>,
}

/// Why an export failed.
#[derive(Debug)]
pub enum ExportError {
    /// No ffmpeg binary; the message explains how to install it.
    FfmpegNotFound,
    /// Writing output or talking to ffmpeg failed.
    Io(std::io::Error),
    /// ffmpeg exited with an error.
    Ffmpeg {
        /// Exit status.
        status: String,
        /// The end of ffmpeg's stderr.
        log: String,
    },
    /// MP4 needs even dimensions.
    BadSize {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
    },
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FfmpegNotFound => f.write_str(FFMPEG_HELP),
            Self::Io(e) => write!(f, "{e}"),
            Self::Ffmpeg { status, log } => write!(f, "ffmpeg failed ({status}):\n{log}"),
            Self::BadSize { width, height } => write!(
                f,
                "{width}x{height}: mp4 (yuv420p) needs even width and height"
            ),
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Installation hint shown when ffmpeg is missing.
pub const FFMPEG_HELP: &str = "\
ffmpeg was not found in PATH. Install it and try again:
  Linux:   sudo apt install ffmpeg      (or dnf/pacman install ffmpeg)
  macOS:   brew install ffmpeg
  Windows: winget install Gyan.FFmpeg   (then reopen the terminal)
or set `ffmpeg = \"/path/to/ffmpeg\"` in oxframe.toml / pass --ffmpeg <path>.
PNG sequences (--format png) do not need ffmpeg.";

/// Finds `ffmpeg`: the explicit path if given, else the first match in
/// `PATH` (with `.exe` on Windows).
///
/// # Errors
///
/// [`ExportError::FfmpegNotFound`] if there is no such binary.
pub fn find_ffmpeg(explicit: Option<&Path>) -> Result<PathBuf, ExportError> {
    if let Some(p) = explicit {
        return if p.is_file() {
            Ok(p.to_path_buf())
        } else {
            Err(ExportError::FfmpegNotFound)
        };
    }
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|d| d.join(name))
                .find(|p| p.is_file())
        })
        .ok_or(ExportError::FfmpegNotFound)
}

/// Receives frames in order and writes them out.
pub struct Exporter {
    tx: Option<SyncSender<Image>>,
    worker: Option<JoinHandle<Result<(), ExportError>>>,
}

enum Sink {
    Ffmpeg(Child),
    Png { dir: PathBuf, next: u32 },
}

impl Exporter {
    /// Starts ffmpeg (or creates the PNG directory).
    ///
    /// # Errors
    ///
    /// If ffmpeg is missing or cannot start, the output cannot be created,
    /// or the size is odd for MP4.
    pub fn start(opts: &ExportOptions) -> Result<Self, ExportError> {
        let sink = match opts.format {
            Format::Png => {
                std::fs::create_dir_all(&opts.out)?;
                Sink::Png {
                    dir: opts.out.clone(),
                    next: 0,
                }
            }
            _ => {
                if opts.format == Format::Mp4
                    && (!opts.width.is_multiple_of(2) || !opts.height.is_multiple_of(2))
                {
                    return Err(ExportError::BadSize {
                        width: opts.width,
                        height: opts.height,
                    });
                }
                let ffmpeg = find_ffmpeg(opts.ffmpeg.as_deref())?;
                if let Some(dir) = opts.out.parent().filter(|d| !d.as_os_str().is_empty()) {
                    std::fs::create_dir_all(dir)?;
                }
                let child = Command::new(ffmpeg)
                    .args(ffmpeg_args(opts))
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .spawn()?;
                Sink::Ffmpeg(child)
            }
        };
        // A small bounded queue: rendering runs ahead of encoding by at
        // most a few frames, keeping memory flat.
        let (tx, rx) = mpsc::sync_channel::<Image>(4);
        let worker = std::thread::Builder::new()
            .name("oxframe-export".into())
            .spawn(move || write_frames(sink, rx))?;
        Ok(Self {
            tx: Some(tx),
            worker: Some(worker),
        })
    }

    /// Queues a frame. Fails early if the writer has stopped (e.g. ffmpeg
    /// exited).
    ///
    /// # Errors
    ///
    /// If the writer stopped; [`finish`](Self::finish) then reports why.
    pub fn push(&mut self, frame: Image) -> Result<(), ExportError> {
        let tx = self.tx.as_ref().expect("push after finish");
        if tx.send(frame).is_err() {
            return self.finish();
        }
        Ok(())
    }

    /// Flushes, waits for ffmpeg and reports its errors.
    ///
    /// # Errors
    ///
    /// If writing failed or ffmpeg exited with an error (its log is included).
    pub fn finish(&mut self) -> Result<(), ExportError> {
        drop(self.tx.take());
        match self.worker.take() {
            Some(w) => w.join().unwrap_or_else(|_| {
                Err(ExportError::Io(std::io::Error::other(
                    "export thread panicked",
                )))
            }),
            None => Ok(()),
        }
    }
}

impl Drop for Exporter {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn write_frames(sink: Sink, rx: Receiver<Image>) -> Result<(), ExportError> {
    match sink {
        Sink::Png { dir, mut next } => {
            for frame in rx {
                std::fs::write(dir.join(format!("{next:06}.png")), frame.encode_png())?;
                next += 1;
            }
            Ok(())
        }
        Sink::Ffmpeg(mut child) => {
            let mut stdin = child.stdin.take().expect("piped stdin");
            let mut stderr = child.stderr.take().expect("piped stderr");
            // Drain stderr concurrently so ffmpeg never blocks on it.
            let log = std::thread::spawn(move || {
                let mut s = String::new();
                let _ = stderr.read_to_string(&mut s);
                s
            });
            let mut write_err = None;
            for frame in rx {
                if let Err(e) = stdin.write_all(&frame.data) {
                    write_err = Some(e);
                    break;
                }
            }
            drop(stdin);
            let status = child.wait()?;
            let log = log.join().unwrap_or_default();
            if !status.success() || write_err.is_some() {
                let tail: Vec<&str> = log.lines().rev().take(15).collect();
                return Err(ExportError::Ffmpeg {
                    status: status.to_string(),
                    log: tail.into_iter().rev().collect::<Vec<_>>().join("\n"),
                });
            }
            Ok(())
        }
    }
}

/// Command line for ffmpeg (without the program name).
pub fn ffmpeg_args(o: &ExportOptions) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-y"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    let size = format!("{}x{}", o.width, o.height);
    let fps = o.fps.to_string();
    push(&[
        "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &size, "-r", &fps, "-i", "-",
    ]);
    audio_input(&mut a, o.audio.as_ref());
    let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    match o.format {
        Format::Mp4 => push(&[
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-crf",
            "18",
            "-preset",
            "medium",
            "-movflags",
            "+faststart",
        ]),
        Format::Webm => push(&[
            "-c:v",
            "libvpx-vp9",
            "-pix_fmt",
            "yuva420p",
            "-b:v",
            "0",
            "-crf",
            "30",
            "-row-mt",
            "1",
        ]),
        Format::ProRes => push(&[
            "-c:v",
            "prores_ks",
            "-profile:v",
            "4444",
            "-pix_fmt",
            "yuva444p10le",
            "-vendor",
            "apl0",
        ]),
        Format::Png => {}
    }
    audio_output(&mut a, o.audio.is_some(), o.format, o.duration);
    a.push(o.out.to_string_lossy().into_owned());
    a
}

/// The soundtrack as input 1, mapped next to the video of input 0.
fn audio_input(a: &mut Vec<String>, audio: Option<&Audio>) {
    let Some(audio) = audio else { return };
    let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    let file = audio.file.to_string_lossy().into_owned();
    if audio.offset >= 0.0 {
        push(&["-itsoffset", &format!("{:.6}", audio.offset), "-i", &file]);
    } else {
        push(&["-ss", &format!("{:.6}", -audio.offset), "-i", &file]);
    }
    push(&["-map", "0:v:0", "-map", "1:a:0", "-shortest"]);
}

/// The audio codec for `format`, padded to the video's length.
fn audio_output(a: &mut Vec<String>, audio: bool, format: Format, duration: Option<f64>) {
    if !audio {
        return;
    }
    let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    match format {
        Format::Mp4 => push(&["-c:a", "aac", "-b:a", "192k"]),
        Format::Webm => push(&["-c:a", "libopus", "-b:a", "160k"]),
        Format::ProRes => push(&["-c:a", "pcm_s16le"]),
        Format::Png => {}
    }
    // Pad audio with silence so `-shortest` never cuts the video.
    // `whole_dur` stops the pad at the last frame; an open-ended pad
    // ignores `-shortest` and the file runs on.
    if let Some(seconds) = duration {
        let filter = format!("apad=whole_dur={seconds:.6}");
        push(&["-af", &filter, "-t", &format!("{seconds:.6}")]);
    } else {
        push(&["-af", "apad"]);
    }
}

/// Command line for [`concat()`] (without the program name): the segments
/// listed in `list` joined without re-encoding, plus the soundtrack.
pub fn concat_args(list: &Path, o: &ExportOptions) -> Vec<String> {
    let mut a: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push(list.to_string_lossy().into_owned());
    if o.audio.is_some() {
        audio_input(&mut a, o.audio.as_ref());
    } else {
        a.extend(["-map", "0:v:0"].map(String::from));
    }
    a.extend(["-c:v", "copy"].map(String::from));
    if o.format == Format::Mp4 {
        a.extend(["-movflags", "+faststart"].map(String::from));
    }
    audio_output(&mut a, o.audio.is_some(), o.format, o.duration);
    a.push(o.out.to_string_lossy().into_owned());
    a
}

/// Joins video files encoded with the same settings (`segments`, in
/// order) into `opts.out` without re-encoding them, and adds
/// `opts.audio`. Used by parallel rendering: each segment is encoded on
/// its own, and the result does not depend on how many ran at once.
///
/// # Errors
///
/// If ffmpeg is missing or fails (its log is included).
pub fn concat(segments: &[PathBuf], opts: &ExportOptions) -> Result<(), ExportError> {
    let ffmpeg = find_ffmpeg(opts.ffmpeg.as_deref())?;
    let list = opts.out.with_extension("segments.txt");
    let mut text = String::new();
    for seg in segments {
        let path = std::fs::canonicalize(seg)?;
        // The concat list quotes paths with single quotes.
        let quoted = path.to_string_lossy().replace('\'', "'\\''");
        text.push_str(&format!("file '{quoted}'\n"));
    }
    std::fs::write(&list, text)?;
    let output = Command::new(ffmpeg).args(concat_args(&list, opts)).output();
    let _ = std::fs::remove_file(&list);
    let output = output?;
    if !output.status.success() {
        let log = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = log.lines().rev().take(15).collect();
        return Err(ExportError::Ffmpeg {
            status: output.status.to_string(),
            log: tail.into_iter().rev().collect::<Vec<_>>().join("\n"),
        });
    }
    Ok(())
}

/// Peak envelope of an audio file for the editor's waveform: `buckets`
/// values in `0..=1`, decoded with ffmpeg at 8 kHz mono.
///
/// # Errors
///
/// If ffmpeg is missing or cannot decode the file.
pub fn waveform(
    file: &Path,
    buckets: usize,
    ffmpeg: Option<&Path>,
) -> Result<Vec<f32>, ExportError> {
    let ffmpeg = find_ffmpeg(ffmpeg)?;
    let out = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(file)
        .args(["-ac", "1", "-ar", "8000", "-f", "s16le", "-"])
        .stderr(Stdio::piped())
        .output()?;
    if !out.status.success() {
        return Err(ExportError::Ffmpeg {
            status: out.status.to_string(),
            log: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    let samples: Vec<i16> = out
        .stdout
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    Ok(peaks(&samples, buckets))
}

fn peaks(samples: &[i16], buckets: usize) -> Vec<f32> {
    if samples.is_empty() || buckets == 0 {
        return vec![0.0; buckets];
    }
    (0..buckets)
        .map(|i| {
            let a = i * samples.len() / buckets;
            let b = ((i + 1) * samples.len() / buckets)
                .max(a + 1)
                .min(samples.len());
            let peak = samples[a..b]
                .iter()
                .map(|s| s.unsigned_abs())
                .max()
                .unwrap_or(0);
            f32::from(peak) / 32768.0
        })
        .collect()
}

/// Duration of a media file in seconds (via ffmpeg's decoder).
///
/// # Errors
///
/// If ffmpeg is missing or cannot decode the file.
pub fn media_duration(file: &Path, ffmpeg: Option<&Path>) -> Result<f64, ExportError> {
    let ffmpeg = find_ffmpeg(ffmpeg)?;
    let out = Command::new(ffmpeg)
        .args(["-hide_banner", "-i"])
        .arg(file)
        .args(["-f", "null", "-"])
        .stderr(Stdio::piped())
        .output()?;
    let log = String::from_utf8_lossy(&out.stderr);
    // "time=00:00:03.21" of the last progress line.
    let t = log
        .rsplit("time=")
        .next()
        .and_then(|s| s.split_whitespace().next())
        .and_then(|t| {
            let mut parts = t.split(':').map(|p| p.parse::<f64>().ok());
            Some(parts.next()?? * 3600.0 + parts.next()?? * 60.0 + parts.next()??)
        });
    t.ok_or(ExportError::Ffmpeg {
        status: "no duration".into(),
        log: log.into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(format: Format, out: &str) -> ExportOptions {
        ExportOptions {
            out: out.into(),
            format,
            width: 64,
            height: 36,
            fps: 30,
            audio: None,
            ffmpeg: None,
            duration: None,
        }
    }

    #[test]
    fn args_for_formats() {
        let a = ffmpeg_args(&opts(Format::Mp4, "o.mp4")).join(" ");
        assert!(a.contains("-f rawvideo -pix_fmt rgba -s 64x36 -r 30 -i -"));
        assert!(a.contains("libx264") && a.contains("yuv420p") && a.ends_with("o.mp4"));
        assert!(
            ffmpeg_args(&opts(Format::Webm, "o.webm"))
                .join(" ")
                .contains("yuva420p")
        );
        assert!(
            ffmpeg_args(&opts(Format::ProRes, "o.mov"))
                .join(" ")
                .contains("prores_ks")
        );
        let mut o = opts(Format::Mp4, "o.mp4");
        o.audio = Some(Audio {
            file: "a.wav".into(),
            offset: -1.5,
        });
        let a = ffmpeg_args(&o).join(" ");
        assert!(a.contains("-ss 1.500000 -i a.wav") && a.contains("-c:a aac"));
    }

    #[test]
    fn format_from_path() {
        assert_eq!(Format::from_path(Path::new("x.MP4")), Format::Mp4);
        assert_eq!(Format::from_path(Path::new("x.mov")), Format::ProRes);
        assert_eq!(Format::from_path(Path::new("frames")), Format::Png);
    }

    #[test]
    fn peaks_downsample() {
        let s = [0i16, 100, -32768, 5, 7, 8];
        assert_eq!(peaks(&s, 3), vec![100.0 / 32768.0, 1.0, 8.0 / 32768.0]);
    }

    #[test]
    fn encodes_mp4_when_ffmpeg_is_available() {
        if find_ffmpeg(None).is_err() {
            eprintln!("SKIPPED: no ffmpeg");
            return;
        }
        let out =
            std::env::temp_dir().join(format!("oxframe-export-test-{}.mp4", std::process::id()));
        let mut o = opts(Format::Mp4, out.to_str().unwrap());
        o.width = 64;
        let mut ex = Exporter::start(&o).unwrap();
        for i in 0..30u8 {
            ex.push(Image {
                width: 64,
                height: 36,
                data: vec![i * 8; 64 * 36 * 4],
            })
            .unwrap();
        }
        ex.finish().unwrap();
        let d = media_duration(&out, None).unwrap();
        assert!((d - 1.0).abs() < 0.1, "duration {d}");
        let _ = std::fs::remove_file(out);
    }
}
