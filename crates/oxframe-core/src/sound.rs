//! A sound effect scheduled while a scene runs.
//!
//! The scene records *when* the clip starts. Decoding and mixing happen
//! later, so this module stays free of audio codecs and works in the browser.

use crate::runtime;

/// One sound effect placed on the scene timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundClip {
    /// Audio file, relative to the project root.
    pub file: String,
    /// Start, in seconds from the beginning of the scene.
    pub at: f64,
    /// Linear gain. `1.0` is unity.
    pub volume: f32,
    /// Fade in at the start of the clip, in seconds.
    pub fade_in: f64,
    /// Fade out at the end of the clip, in seconds.
    pub fade_out: f64,
    /// Seconds skipped from the start of the file.
    pub trim_start: f64,
    /// End point in the file, in seconds. `None` plays through to the end.
    pub trim_end: Option<f64>,
    /// Ducks the music while this clip is audible.
    pub priority: bool,
}

/// Records `clip` on the running scene. Called by `sound()`.
pub fn record_sound(clip: SoundClip) {
    runtime::with(|rt| rt.sounds.borrow_mut().push(clip));
}
