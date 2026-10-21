//! A video clip as a node.
//!
//! The node shows the frame of its clip at `time` (seconds into the clip),
//! a signal the scene advances while the clip plays: `play` starts a task
//! that adds `playback_rate × dt` every frame. So the frame is a function
//! of the scene's own steps, and a seek or a render shows the same frame
//! every time.
//!
//! Decoding is not done here: this crate also builds for the browser. The
//! native engine installs an opener backed by ffmpeg ([`set_video_opener`]);
//! without one, the node draws a placeholder box.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::image::RasterImage;

/// What a clip is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoInfo {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Frames per second of the clip.
    pub fps: f64,
    /// Length in seconds.
    pub duration: f64,
    /// The file has an audio stream.
    pub has_audio: bool,
}

/// Frames of one clip, by index. Implemented by the native ffmpeg decoder.
pub trait VideoSource {
    /// Size, frame rate and length.
    fn info(&self) -> VideoInfo;
    /// Frame `index` (0 is the first), straight-alpha RGBA. `None` past the
    /// end or when decoding failed.
    fn frame(&self, index: u64) -> Option<Rc<RasterImage>>;
}

/// Opens a clip by path.
pub type VideoOpener = Rc<dyn Fn(&Path) -> Result<Rc<dyn VideoSource>, String>>;

thread_local! {
    static OPENER: RefCell<Option<VideoOpener>> = const { RefCell::new(None) };
}

/// Installs the function that opens clips for [`Video`](crate::Video)
/// nodes. The engine does this when it is built with the `video` feature.
pub fn set_video_opener(opener: VideoOpener) {
    OPENER.with(|o| *o.borrow_mut() = Some(opener));
}

/// The clip at `path`, or a placeholder and the reason it is one.
pub(crate) fn open(path: &Path) -> (Rc<dyn VideoSource>, Option<String>) {
    let opened = OPENER.with(|o| o.borrow().clone()).map(|open| open(path));
    match opened {
        Some(Ok(source)) => (source, None),
        Some(Err(e)) => (Rc::new(Missing), Some(e)),
        None => (
            Rc::new(Missing),
            Some("video decoding needs the native engine with the `video` feature".into()),
        ),
    }
}

/// A clip that could not be opened: a 16:9 box with no frames.
struct Missing;

impl VideoSource for Missing {
    fn info(&self) -> VideoInfo {
        VideoInfo {
            width: 1280,
            height: 720,
            fps: 30.0,
            duration: 0.0,
            has_audio: false,
        }
    }

    fn frame(&self, _: u64) -> Option<Rc<RasterImage>> {
        None
    }
}

/// Playback of one node, shared by its handle and the task that plays it.
#[doc(hidden)]
#[derive(Default)]
pub struct Playback {
    pub(crate) playing: Cell<bool>,
    /// Bumped by every `play`, so an older task stops.
    pub(crate) generation: Cell<u32>,
    /// `seek` moved the time: the sound starts again from there.
    pub(crate) jumped: Cell<bool>,
    /// The sound being recorded: (clip index, scene time, start in file).
    pub(crate) segment: Cell<Option<(usize, f64, f64)>>,
}

/// Stored on a video node.
#[doc(hidden)]
pub struct VideoData {
    pub(crate) source: Rc<dyn VideoSource>,
    pub(crate) path: PathBuf,
    /// Why there are no frames, for `inspect` and `check`.
    pub(crate) problem: Option<String>,
    pub(crate) muted: bool,
    pub(crate) playback: Rc<Playback>,
}

impl VideoData {
    /// Index of the frame shown at clip time `t`.
    pub(crate) fn index_at(&self, t: f64) -> u64 {
        let info = self.source.info();
        let last = (info.duration * info.fps).ceil().max(1.0) as u64 - 1;
        ((t.max(0.0) * info.fps + 1e-6).floor() as u64).min(last)
    }
}
