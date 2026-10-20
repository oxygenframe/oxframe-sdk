use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use crate::ease::presets;
use crate::runtime;
use crate::{Ease, Lerp, Signal, TIME_EPSILON};

/// A length of time. `f64` means seconds; other types (a theme pace) convert.
pub trait Seconds {
    /// Duration in seconds.
    fn seconds(self) -> f64;
}

impl Seconds for f64 {
    fn seconds(self) -> f64 {
        self
    }
}

/// Animates a [`Signal`] towards a target. Created by [`Signal::to`].
///
/// The first poll captures the start value and the task time; every later
/// poll is one frame. On completion the task time is advanced to exactly
/// `start + duration`, not to the frame time, so chained tweens do not
/// accumulate rounding drift.
#[must_use = "a tween does nothing unless awaited"]
pub struct Tween<T> {
    signal: Signal<T>,
    from: Option<T>,
    target: T,
    duration: f64,
    ease: Rc<dyn Ease>,
    start: Option<f64>,
}

impl<T: Lerp + Clone + 'static> Tween<T> {
    pub(crate) fn new(signal: Signal<T>, target: T, duration: f64) -> Self {
        Self {
            signal,
            from: None,
            target,
            duration: duration.max(0.0),
            ease: Rc::new(presets::DEFAULT),
            start: None,
        }
    }

    /// Easing curve of this tween; default [`presets::DEFAULT`](crate::presets::DEFAULT).
    pub fn ease(mut self, ease: impl Ease + 'static) -> Self {
        self.ease = Rc::new(ease);
        self
    }

    /// Changes the length, e.g. of a helper like `fade_in()` that defaults to
    /// the theme's pace: `title.fade_in().duration(0.3)`.
    pub fn duration(mut self, duration: impl Seconds) -> Self {
        self.duration = duration.seconds().max(0.0);
        self
    }

    /// Starts from `value` instead of the signal's value at the first poll.
    pub fn from(mut self, value: T) -> Self {
        self.from = Some(value);
        self
    }
}

/// Progress of a timed operation that started at `start`; `None` once done
/// (and then the task time is set to the exact end).
pub(crate) fn progress(start: f64, duration: f64) -> Option<f64> {
    runtime::with(|rt| {
        let end = start + duration;
        let now = rt.now();
        if now + TIME_EPSILON >= end {
            rt.task_time.set(end);
            None
        } else {
            Some(((now - start) / duration).clamp(0.0, 1.0))
        }
    })
}

impl<T: Lerp + Clone + Unpin + 'static> Future for Tween<T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        let start = match this.start {
            Some(s) => s,
            None => {
                this.from.get_or_insert_with(|| this.signal.peek());
                *this.start.insert(runtime::time())
            }
        };
        match progress(start, this.duration) {
            None => this.signal.set(this.target.clone()),
            Some(t) => {
                let from = this.from.as_ref().expect("captured on first poll");
                this.signal.set(from.lerp(&this.target, this.ease.ease(t)));
                return Poll::Pending;
            }
        }
        Poll::Ready(())
    }
}
