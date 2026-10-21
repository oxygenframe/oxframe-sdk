//! Stops and speaker notes for `oxframe present`.
//!
//! A scene runs the same way in a render and in a presentation. The stops
//! are only recorded: `oxframe present` pauses at each one and waits for
//! the next key press, and `render --slides` takes a frame at each.

use crate::runtime;

/// What a scene recorded for the presentation, in scene time.
#[derive(Debug, Clone, PartialEq)]
pub enum SlideMark {
    /// [`slide_break`]: the presentation pauses here.
    Break {
        /// Seconds from the beginning of the scene.
        at: f64,
    },
    /// [`notes`]: speaker notes for the part of the scene around `at`.
    Notes {
        /// Seconds from the beginning of the scene.
        at: f64,
        /// The notes.
        text: String,
    },
}

/// A stop for `oxframe present`: the presentation pauses here until the
/// next key press. A render plays straight through, so a scene with stops
/// is still a video. Every scene also ends with a stop.
///
/// ```ignore
/// title.fade_in().await;
/// slide_break().await;
/// list.next().await;
/// ```
pub fn slide_break() -> std::future::Ready<()> {
    runtime::with(|rt| {
        let at = rt.task_time.get();
        let mut marks = rt.slides.borrow_mut();
        // Two breaks at the same time are one stop.
        let dup = marks.iter().any(
            |m| matches!(m, SlideMark::Break { at: t } if (t - at).abs() < crate::TIME_EPSILON),
        );
        if !dup {
            marks.push(SlideMark::Break { at });
        }
    });
    std::future::ready(())
}

/// Speaker notes for the slide being shown: the presenter window of
/// `oxframe present` shows them until the next stop. Several calls before
/// one stop are joined.
pub fn notes(text: impl Into<String>) {
    let text = text.into();
    runtime::with(|rt| {
        let at = rt.task_time.get();
        rt.slides.borrow_mut().push(SlideMark::Notes { at, text });
    });
}
