//! Per-thread context of the scene being executed.
//!
//! The [`Executor`](crate::Executor) installs its runtime for the duration
//! of each step, so free functions like [`now`] and [`wait_until`]
//! (crate::wait_until) work inside scene code without passing handles.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use oxframe_protocol::Meta;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::CubicBezier;
use crate::ease::presets;
use crate::flow::Task;
use crate::transition::Recorded;

pub(crate) struct Runtime {
    pub fps: f64,
    pub frame: Cell<u32>,
    /// Virtual time of the task being polled. It can lag behind the frame
    /// time by a fraction of a frame: when a tween ends between frames, the
    /// next tween starts at the exact end time, so timings do not drift.
    pub task_time: Cell<f64>,
    pub spawned: RefCell<Vec<Task>>,
    pub meta: RefCell<Meta>,
    pub meta_changed: Cell<bool>,
    pub rng: RefCell<ChaCha8Rng>,
    /// Set by the engine: this scene has one before it, so a transition blends.
    pub has_previous: Cell<bool>,
    /// The transition declared by the scene, if it starts one.
    pub transition: RefCell<Option<Recorded>>,
    /// Sound effects scheduled by `sound()`, in scene time.
    pub sounds: RefCell<Vec<crate::SoundClip>>,
}

impl Runtime {
    pub fn new(fps: f64, meta: Meta, seed: u64) -> Rc<Self> {
        assert!(fps > 0.0 && fps.is_finite(), "fps must be positive");
        Rc::new(Self {
            fps,
            frame: Cell::new(0),
            task_time: Cell::new(0.0),
            spawned: RefCell::new(Vec::new()),
            meta: RefCell::new(meta),
            meta_changed: Cell::new(false),
            rng: RefCell::new(ChaCha8Rng::seed_from_u64(seed)),
            has_previous: Cell::new(false),
            transition: RefCell::new(None),
            sounds: RefCell::new(Vec::new()),
        })
    }

    pub fn now(&self) -> f64 {
        f64::from(self.frame.get()) / self.fps
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Runtime>>> = const { RefCell::new(None) };
}

/// Restores the previously installed runtime on drop.
pub(crate) struct Installed(Option<Rc<Runtime>>);

pub(crate) fn install(rt: &Rc<Runtime>) -> Installed {
    Installed(CURRENT.with(|c| c.borrow_mut().replace(rt.clone())))
}

impl Drop for Installed {
    fn drop(&mut self) {
        let prev = self.0.take();
        CURRENT.with(|c| *c.borrow_mut() = prev);
    }
}

pub(crate) fn with<R>(f: impl FnOnce(&Runtime) -> R) -> R {
    CURRENT.with(|c| {
        let c = c.borrow();
        let rt = c
            .as_ref()
            .expect("oxframe: this function can only be called while a scene is running");
        f(rt)
    })
}

/// Index of the frame being computed.
pub fn frame() -> u32 {
    with(|rt| rt.frame.get())
}

/// Frames per second of the running project.
pub fn fps() -> f64 {
    with(|rt| rt.fps)
}

/// Duration of one frame in seconds, `1 / fps`.
pub fn dt() -> f64 {
    with(|rt| 1.0 / rt.fps)
}

/// Time of the current frame, `frame / fps` seconds.
pub fn now() -> f64 {
    with(|rt| rt.now())
}

/// Whether the running scene has a scene before it.
pub fn has_previous() -> bool {
    with(|rt| rt.has_previous.get())
}

/// Records `spec`, letting `meta.json` override its duration the way markers
/// do. Returns the duration to wait. Panics if the scene already started one.
pub fn begin_transition(mut spec: Recorded) -> f64 {
    with(|rt| {
        if rt.transition.borrow().is_some() {
            panic!("oxframe: a scene can declare only one transition; call it once, at the start");
        }
        let stored = rt.meta.borrow().transition.as_ref().map(|t| t.seconds);
        if let Some(seconds) = stored {
            spec.seconds = if seconds.is_finite() {
                seconds.max(0.0)
            } else {
                0.0
            };
        } else if spec.seconds.is_finite() && spec.seconds > 0.0 {
            let seconds = spec.seconds;
            rt.meta.borrow_mut().transition = Some(oxframe_protocol::TransitionMeta { seconds });
            rt.meta_changed.set(true);
        } else {
            spec.seconds = 0.0;
        }
        let seconds = spec.seconds;
        if seconds > 0.0 {
            *rt.transition.borrow_mut() = Some(spec);
        }
        seconds
    })
}

/// Virtual time of the current task — where the next animation starts.
pub fn time() -> f64 {
    with(|rt| rt.task_time.get())
}

/// Value of tunable `name` from `meta.json`; `default` is recorded there on
/// first use. Use through the `tune!` macro.
pub fn tunable(name: &str, default: f64) -> f64 {
    with(|rt| {
        let mut meta = rt.meta.borrow_mut();
        if let Some(v) = meta.tunables.get(name) {
            return *v;
        }
        meta.tunables.insert(name.to_owned(), default);
        rt.meta_changed.set(true);
        default
    })
}

/// Named easing from `meta.json`; `default` is recorded there on first use.
/// Use through the `ease!` macro.
pub fn ease_named(name: &str, default: Option<CubicBezier>) -> CubicBezier {
    with(|rt| {
        let mut meta = rt.meta.borrow_mut();
        if let Some(v) = meta.eases.get(name) {
            return CubicBezier::from_array(*v);
        }
        let default = default.unwrap_or(presets::DEFAULT);
        meta.eases.insert(name.to_owned(), default.to_array());
        rt.meta_changed.set(true);
        default
    })
}

/// Marker time from `meta.json`; the current task time is recorded on first
/// encounter.
pub(crate) fn marker(name: &str) -> f64 {
    with(|rt| {
        let mut meta = rt.meta.borrow_mut();
        if let Some(v) = meta.markers.get(name) {
            return *v;
        }
        let t = rt.task_time.get();
        meta.markers.insert(name.to_owned(), t);
        rt.meta_changed.set(true);
        t
    })
}

/// Copy of the running scene's markers, eases and tunables.
pub fn meta_snapshot() -> Meta {
    with(|rt| rt.meta.borrow().clone())
}

/// Runs `f` with the scene's random generator. It is seeded from the
/// project seed and the scene name, so results are reproducible; never use
/// `rand::rng()` or other entropy sources in scenes.
pub fn with_rng<R>(f: impl FnOnce(&mut ChaCha8Rng) -> R) -> R {
    with(|rt| f(&mut rt.rng.borrow_mut()))
}

/// Uniform random number in `[0, 1)` from the scene's seeded generator.
pub fn random() -> f64 {
    use rand::RngExt;
    with_rng(|r| r.random::<f64>())
}

/// Uniform random number in `[min, max)` from the scene's seeded generator.
pub fn random_range(min: f64, max: f64) -> f64 {
    min + (max - min) * random()
}

/// A generator seeded by the caller. Drawing with this does not advance the
/// scene's [`random`] generator, and the same seed always yields the same
/// sequence, including after a seek.
#[derive(Clone)]
pub struct SeededRng(ChaCha8Rng);

impl SeededRng {
    /// Generator for `seed`. The same seed is the same sequence.
    pub fn new(seed: u64) -> Self {
        Self(ChaCha8Rng::seed_from_u64(seed))
    }

    /// Uniform number in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        use rand::RngExt;
        self.0.random::<f64>()
    }

    /// Uniform number in `span` (`end` is excluded).
    pub fn range(&mut self, span: std::ops::Range<f64>) -> f64 {
        let w = span.end - span.start;
        span.start + w * self.next_f64()
    }
}
