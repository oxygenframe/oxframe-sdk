use std::future::Future;
use std::rc::Rc;
use std::task::{Context, Waker};

use oxframe_protocol::Meta;

use crate::flow::{Task, TaskState};
use crate::runtime::{self, Runtime};

/// Deterministic single-threaded executor: one [`step`](Executor::step) is
/// one frame. The root future and every spawned task are polled once per
/// step, in a fixed order (root, then tasks in spawn order). No wakers, no
/// clocks, no threads — the same input always produces the same frames.
pub struct Executor {
    rt: Rc<Runtime>,
    root: Option<Task>,
    tasks: Vec<Task>,
    next_frame: u32,
}

impl Executor {
    /// `seed` feeds the scene's random generator ([`random`](crate::random)).
    pub fn new(fps: f64, meta: Meta, seed: u64, root: impl Future<Output = ()> + 'static) -> Self {
        Self {
            rt: Runtime::new(fps, meta, seed),
            root: Some(Task {
                fut: Box::pin(root),
                time: 0.0,
                state: Rc::new(TaskState {
                    finished: Default::default(),
                    cancelled: Default::default(),
                }),
            }),
            tasks: Vec::new(),
            next_frame: 0,
        }
    }

    /// Runs `f` with this executor's runtime installed, so that `now()`,
    /// `tune!` and friends work outside of [`step`](Executor::step).
    pub fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = runtime::install(&self.rt);
        f()
    }

    /// Computes the next frame. Returns `true` if the scene is finished
    /// (its root future completed during this or an earlier step).
    pub fn step(&mut self) -> bool {
        let Some(root) = self.root.as_mut() else {
            return true;
        };
        let _guard = runtime::install(&self.rt);
        self.rt.frame.set(self.next_frame);
        let mut cx = Context::from_waker(Waker::noop());

        let root_done = root.poll(&mut cx);
        self.tasks.retain_mut(|t| !t.poll(&mut cx));
        // Tasks spawned during this frame start in this frame.
        loop {
            let new: Vec<Task> = std::mem::take(&mut *self.rt.spawned.borrow_mut());
            if new.is_empty() {
                break;
            }
            for mut t in new {
                if !t.poll(&mut cx) {
                    self.tasks.push(t);
                }
            }
        }

        self.next_frame += 1;
        if root_done {
            self.root = None;
            self.tasks.clear();
        }
        root_done
    }

    /// Whether the scene's root task has completed.
    pub fn is_finished(&self) -> bool {
        self.root.is_none()
    }

    /// Number of frames computed so far; the last computed frame is
    /// `frames_done() - 1`.
    pub fn frames_done(&self) -> u32 {
        self.next_frame
    }

    /// Frames per second.
    pub fn fps(&self) -> f64 {
        self.rt.fps
    }

    /// Current markers, eases and tunables, including defaults recorded so far.
    pub fn meta(&self) -> Meta {
        self.rt.meta.borrow().clone()
    }

    /// True if scene code recorded new defaults into the meta since the
    /// last call.
    pub fn take_meta_changed(&self) -> bool {
        self.rt.meta_changed.replace(false)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::{Signal, all, next_frame, presets, sequence, spawn, wait_for, wait_until};

    /// Runs to completion, sampling `f` after each frame.
    fn run<T>(fps: f64, fut: impl Future<Output = ()> + 'static, f: impl Fn() -> T) -> Vec<T> {
        let mut ex = Executor::new(fps, Meta::default(), 0, fut);
        let mut out = Vec::new();
        for _ in 0..10_000 {
            let done = ex.step();
            out.push(f());
            if done {
                return out;
            }
        }
        panic!("scene did not finish");
    }

    #[test]
    fn tween_takes_exact_frames_and_hits_target() {
        let s = Signal::new(0.0f64);
        let s2 = s.clone();
        let values = run(
            60.0,
            async move { s2.to(1.0, 1.0).ease(presets::LINEAR).await },
            || s.get(),
        );
        // Frames 0..=60: t = 0, 1/60, ..., 1.
        assert_eq!(values.len(), 61);
        assert_eq!(values[0], 0.0);
        assert!((values[30] - 0.5).abs() < 1e-12);
        assert_eq!(values[60], 1.0);
    }

    #[test]
    fn chained_waits_do_not_drift() {
        // 0.1 s is not a multiple of 1/24; 30 of them must still end at 3 s.
        let frames = run(
            24.0,
            async {
                for _ in 0..30 {
                    wait_for(0.1).await;
                }
            },
            || (),
        );
        assert_eq!(frames.len(), 3 * 24 + 1);
    }

    #[test]
    fn all_waits_for_longest_and_continues_from_its_end() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let l = log.clone();
        run(
            10.0,
            async move {
                all((wait_for(0.5), wait_for(1.0), async {})).await;
                l.borrow_mut().push((crate::frame(), crate::time()));
            },
            || (),
        );
        assert_eq!(*log.borrow(), vec![(10, 1.0)]);
    }

    #[test]
    fn sequence_staggers_starts() {
        let a = Signal::new(0.0f64);
        let b = Signal::new(0.0f64);
        let (a2, b2) = (a.clone(), b.clone());
        let values = run(
            10.0,
            async move {
                sequence(
                    0.5,
                    vec![
                        Box::pin(a2.to(1.0, 1.0).ease(presets::LINEAR)),
                        Box::pin(b2.to(1.0, 1.0).ease(presets::LINEAR)),
                    ],
                )
                .await;
            },
            || (a.get(), b.get()),
        );
        assert_eq!(values.len(), 16);
        assert!((values[5].0 - 0.5).abs() < 1e-12);
        assert_eq!(values[5].1, 0.0);
        assert!((values[10].1 - 0.5).abs() < 1e-12);
    }

    #[test]
    fn spawn_runs_in_background_and_join_waits() {
        let s = Signal::new(0.0f64);
        let s2 = s.clone();
        let values = run(
            10.0,
            async move {
                let h = spawn(async move { s2.to(1.0, 0.5).ease(presets::LINEAR).await });
                next_frame().await;
                h.await;
            },
            || s.get(),
        );
        assert!((values[2] - 0.4).abs() < 1e-12);
        assert_eq!(*values.last().unwrap(), 1.0);
    }

    #[test]
    fn wait_until_records_default_and_reads_meta() {
        let mut ex = Executor::new(10.0, Meta::default(), 0, async {
            wait_for(0.3).await;
            wait_until("beat").await;
        });
        while !ex.step() {}
        assert!((ex.meta().markers["beat"] - 0.3).abs() < 1e-12);
        assert!(ex.take_meta_changed());

        let mut meta = Meta::default();
        meta.markers.insert("beat".into(), 1.0);
        let mut ex = Executor::new(10.0, meta, 0, async {
            wait_for(0.3).await;
            wait_until("beat").await;
        });
        while !ex.step() {}
        assert_eq!(ex.frames_done(), 11);
        assert!(!ex.take_meta_changed());
    }

    #[test]
    fn random_is_seeded() {
        let draw = |seed| {
            let out = Rc::new(RefCell::new(Vec::new()));
            let o = out.clone();
            let mut ex = Executor::new(10.0, Meta::default(), seed, async move {
                for _ in 0..4 {
                    o.borrow_mut().push(crate::random());
                }
            });
            ex.step();
            out.take()
        };
        assert_eq!(draw(1), draw(1));
        assert_ne!(draw(1), draw(2));
        assert!(draw(3).iter().all(|v| (0.0..1.0).contains(v)));
    }
}
