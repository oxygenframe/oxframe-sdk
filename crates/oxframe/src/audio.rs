//! Sound effects and the project soundtrack.
//!
//! `sound()` records a clip at the current frame. Mixing happens when the
//! video is rendered, so the same scene produces the same samples.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use oxframe_core::{SoundClip, WaitFor, now, record_sound, wait_for};

/// A soundtrack for [`Project::audio`](crate::Project::audio).
///
/// `Project::audio("music.mp3")` is the same as `Audio::track("music.mp3")`.
#[derive(Debug, Clone)]
pub struct Audio {
    pub(crate) file: String,
    pub(crate) volume: f32,
    pub(crate) fade_in: f64,
    pub(crate) fade_out: f64,
    pub(crate) duck: bool,
    pub(crate) master_in: f64,
    pub(crate) master_out: f64,
}

impl Audio {
    /// Music or voice that plays for the whole project.
    pub fn track(path: impl Into<String>) -> Self {
        Self {
            file: path.into(),
            volume: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            duck: false,
            master_in: 0.0,
            master_out: 0.0,
        }
    }

    /// Linear gain. `1.0` leaves the file as it is.
    pub fn volume(mut self, gain: f64) -> Self {
        self.volume = finite(gain).max(0.0) as f32;
        self
    }

    /// Fade the track in from silence.
    pub fn fade_in(mut self, seconds: f64) -> Self {
        self.fade_in = finite(seconds).max(0.0);
        self
    }

    /// Fade the track out to silence.
    pub fn fade_out(mut self, seconds: f64) -> Self {
        self.fade_out = finite(seconds).max(0.0);
        self
    }

    /// Lower the music while a [`sound`] marked `.priority()` is playing.
    pub fn duck_under_voice(mut self) -> Self {
        self.duck = true;
        self
    }

    /// Fade the whole mix in and out, including sound effects.
    pub fn master_fade(mut self, fade_in: f64, fade_out: f64) -> Self {
        self.master_in = finite(fade_in).max(0.0);
        self.master_out = finite(fade_out).max(0.0);
        self
    }
}

impl From<&str> for Audio {
    fn from(path: &str) -> Self {
        Self::track(path)
    }
}

impl From<String> for Audio {
    fn from(path: String) -> Self {
        Self::track(path)
    }
}

/// A sound effect. [`play`](Sound::play) starts it and moves on;
/// `.await` waits until it ends.
///
/// ```no_run
/// # async fn scene() {
/// use oxframe::prelude::*;
/// sound("assets/whoosh.wav").volume(0.6).play();
/// sound("assets/click.wav").trim(0.0, 0.2).await;
/// # }
/// ```
#[must_use = "a sound does nothing until you call play() or await it"]
pub struct Sound {
    file: String,
    volume: f32,
    fade_in: f64,
    fade_out: f64,
    trim_start: f64,
    trim_end: Option<f64>,
    priority: bool,
    wait: Option<WaitFor>,
}

/// A sound effect from a file, starting at the current frame.
pub fn sound(path: impl Into<String>) -> Sound {
    Sound {
        file: path.into(),
        volume: 1.0,
        fade_in: 0.0,
        fade_out: 0.0,
        trim_start: 0.0,
        trim_end: None,
        priority: false,
        wait: None,
    }
}

impl Sound {
    /// Linear gain. `1.0` is the file's own level.
    pub fn volume(mut self, gain: f64) -> Self {
        self.volume = finite(gain).max(0.0) as f32;
        self
    }

    /// Fade in from silence, in seconds.
    pub fn fade_in(mut self, seconds: f64) -> Self {
        self.fade_in = finite(seconds).max(0.0);
        self
    }

    /// Fade out to silence, in seconds.
    pub fn fade_out(mut self, seconds: f64) -> Self {
        self.fade_out = finite(seconds).max(0.0);
        self
    }

    /// Play from `start` to `end` seconds inside the file.
    pub fn trim(mut self, start: f64, end: f64) -> Self {
        let start = finite(start).max(0.0);
        let end = finite(end).max(start);
        self.trim_start = start;
        self.trim_end = Some(end);
        self
    }

    /// Duck the music (see [`Audio::duck_under_voice`]) while this clip plays.
    pub fn priority(mut self) -> Self {
        self.priority = true;
        self
    }

    /// Start the clip at this frame and keep going. The scene does not wait.
    pub fn play(self) {
        self.schedule();
    }

    fn schedule(&self) -> f64 {
        let seconds = self.length();
        record_sound(SoundClip {
            file: self.file.clone(),
            at: now(),
            volume: self.volume,
            fade_in: self.fade_in,
            fade_out: self.fade_out,
            trim_start: self.trim_start,
            trim_end: self.trim_end,
            priority: self.priority,
            speech: false,
        });
        seconds
    }

    fn length(&self) -> f64 {
        if let Some(end) = self.trim_end {
            return (end - self.trim_start).max(0.0);
        }
        file_length(&self.file) - self.trim_start
    }
}

impl Future for Sound {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.wait.is_none() {
            let seconds = this.schedule();
            this.wait = Some(wait_for(seconds));
        }
        Pin::new(this.wait.as_mut().expect("wait was just set")).poll(cx)
    }
}

fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

fn file_length(path: &str) -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        match oxframe_export::probe_duration(std::path::Path::new(path)) {
            Ok(seconds) => seconds.max(0.0),
            Err(e) => panic!(
                "oxframe: sound(\"{path}\") — {e}. \
                 .await needs the file to know when the clip ends; use .play() or .trim(start, end)."
            ),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        0.0
    }
}

/// One sound effect on the project timeline.
#[derive(Debug, Clone)]
pub(crate) struct Scheduled {
    pub file: String,
    pub start: f64,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub volume: f32,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fade_in: f64,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fade_out: f64,
    pub trim_start: f64,
    pub trim_end: Option<f64>,
    pub priority: bool,
    /// A spoken line. The timeline draws it from the line, not from this clip.
    pub speech: bool,
}

impl Scheduled {
    /// Length when the scene named it with `.trim`. Otherwise the file decides.
    pub fn known_seconds(&self) -> Option<f64> {
        self.trim_end.map(|end| (end - self.trim_start).max(0.0))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn mix_range(
    engine: &crate::Engine,
    origin: f64,
    seconds: f64,
) -> Result<Option<oxframe_export::Mixed>, oxframe_export::MixError> {
    use oxframe_export::{MixClip, MixRequest};
    let mut clips = Vec::new();
    if let Some(track) = engine.track() {
        let start = track.offset - origin;
        clips.push(MixClip {
            file: track.file.into(),
            start,
            volume: track.volume,
            fade_in: track.fade_in,
            fade_out: track.fade_out,
            trim_start: 0.0,
            trim_end: None,
            priority: false,
            music: true,
        });
    }
    for sound in engine.scheduled() {
        clips.push(MixClip {
            file: sound.file.into(),
            start: sound.start - origin,
            volume: sound.volume,
            fade_in: sound.fade_in,
            fade_out: sound.fade_out,
            trim_start: sound.trim_start,
            trim_end: sound.trim_end,
            priority: sound.priority,
            music: false,
        });
    }
    if clips.is_empty() || seconds <= 0.0 {
        return Ok(None);
    }
    let duck = engine.track().is_some_and(|t| t.duck);
    let (master_in, master_out) = engine
        .track()
        .map(|t| (t.master_in, t.master_out))
        .unwrap_or((0.0, 0.0));
    Ok(Some(oxframe_export::mix(&MixRequest {
        seconds,
        duck,
        master_fade_in: master_in,
        master_fade_out: master_out,
        clips,
    })?))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use oxframe_2d::View;
    use oxframe_core::{BoxFuture, wait_for};
    use oxframe_protocol::Meta;

    use crate::engine::Engine;
    use crate::project::Project;
    use crate::scene::{SceneDef, SourceLocation};

    use super::*;

    fn scene(name: &'static str, factory: fn(View) -> BoxFuture) -> SceneDef {
        SceneDef::new(
            name,
            SourceLocation {
                file: "src/audio.rs",
                manifest_dir: env!("CARGO_MANIFEST_DIR"),
            },
            factory,
        )
    }

    fn project(factory: fn(View) -> BoxFuture) -> Project {
        let mut p = Project::new("sounds")
            .size(32, 18)
            .fps(60)
            .write_meta(false)
            .inline_meta(BTreeMap::<String, Meta>::new());
        p = p.scene(scene("main", factory));
        p
    }

    #[test]
    fn play_lands_on_the_frame_and_does_not_hold_the_scene() {
        fn body(_view: View) -> BoxFuture {
            Box::pin(async move {
                wait_for(0.5).await;
                sound("click.wav").trim(0.0, 0.2).volume(0.6).play();
            })
        }
        let engine = Engine::new(project(body)).unwrap();
        let sounds = engine.scheduled();
        assert_eq!(sounds.len(), 1);
        assert!((sounds[0].start - 0.5).abs() < 1e-6, "{}", sounds[0].start);
        assert_eq!(sounds[0].known_seconds(), Some(0.2));
        assert!(engine.total_frames() as f64 / 60.0 < 0.7);
    }

    #[test]
    fn await_holds_until_the_trim_ends() {
        fn body(_view: View) -> BoxFuture {
            Box::pin(async move {
                sound("click.wav").trim(0.1, 0.35).await;
            })
        }
        let engine = Engine::new(project(body)).unwrap();
        let seconds = engine.total_frames() as f64 / 60.0;
        assert!(seconds + 1e-6 >= 0.25, "{seconds}");
        assert!(!engine.scheduled()[0].priority);
    }
}
