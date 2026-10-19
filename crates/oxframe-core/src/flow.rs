use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use crate::runtime;
use crate::tween::progress;

/// A boxed scene future, e.g. for [`sequence`].
pub type BoxFuture<T = ()> = Pin<Box<dyn Future<Output = T>>>;

/// Polls `fut` with its own virtual time and writes the time back.
fn poll_scoped<F: Future + ?Sized>(
    fut: Pin<&mut F>,
    time: &mut f64,
    cx: &mut Context<'_>,
) -> Poll<F::Output> {
    runtime::with(|rt| {
        let outer = rt.task_time.replace(*time);
        // `with` holds only a shared borrow of the thread-local, so nested
        // runtime calls from inside the poll are fine.
        let r = fut.poll(cx);
        *time = rt.task_time.replace(outer);
        r
    })
}

// ---------------------------------------------------------------- timers

/// Waits `seconds` of scene time.
pub fn wait_for(seconds: f64) -> WaitFor {
    WaitFor {
        duration: seconds.max(0.0),
        start: None,
    }
}

#[must_use = "futures do nothing unless awaited"]
pub struct WaitFor {
    duration: f64,
    start: Option<f64>,
}

impl Future for WaitFor {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        let start = *self.start.get_or_insert_with(runtime::time);
        match progress(start, self.duration) {
            None => Poll::Ready(()),
            Some(_) => Poll::Pending,
        }
    }
}

/// Waits until marker `name` from `meta.json`. On first encounter the
/// current time becomes the marker's default, so it does not wait at all.
pub fn wait_until(name: &str) -> WaitUntil {
    WaitUntil {
        name: name.to_owned(),
        inner: None,
    }
}

#[must_use = "futures do nothing unless awaited"]
pub struct WaitUntil {
    name: String,
    inner: Option<WaitFor>,
}

impl Future for WaitUntil {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = &mut *self;
        let inner = this.inner.get_or_insert_with(|| {
            let target = runtime::marker(&this.name);
            wait_for(target - runtime::time())
        });
        Pin::new(inner).poll(cx)
    }
}

/// Yields until the next frame.
pub fn next_frame() -> NextFrame {
    NextFrame { frame: None }
}

#[must_use = "futures do nothing unless awaited"]
pub struct NextFrame {
    frame: Option<u32>,
}

impl Future for NextFrame {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        let current = runtime::frame();
        let first = *self.frame.get_or_insert(current);
        if current > first {
            runtime::with(|rt| rt.task_time.set(rt.task_time.get().max(rt.now())));
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

// ---------------------------------------------------------------- all

/// One branch of [`all`]: a future with its own virtual time.
pub struct Branch<F: Future> {
    fut: Option<Pin<Box<F>>>,
    out: Option<F::Output>,
    time: f64,
}

impl<F: Future> Branch<F> {
    fn new(fut: F) -> Self {
        Self {
            fut: Some(Box::pin(fut)),
            out: None,
            time: f64::NAN,
        }
    }

    /// Returns true when finished.
    fn poll(&mut self, start: f64, cx: &mut Context<'_>) -> bool {
        if self.time.is_nan() {
            self.time = start;
        }
        if let Some(fut) = self.fut.as_mut()
            && let Poll::Ready(v) = poll_scoped(fut.as_mut(), &mut self.time, cx)
        {
            self.out = Some(v);
            self.fut = None;
        }
        self.fut.is_none()
    }

    fn take(&mut self) -> F::Output {
        self.out.take().expect("branch output taken twice")
    }
}

/// Runs futures concurrently and completes when all of them have. Takes a
/// tuple of up to 12 futures and returns a tuple of their outputs.
///
/// Every branch starts at the same time; afterwards the task continues from
/// the latest end time among them.
pub fn all<T: IntoAll>(futures: T) -> All<T::Branches> {
    All {
        branches: futures.into_branches(),
        start: None,
    }
}

#[must_use = "futures do nothing unless awaited"]
/// Future returned by [`all`].
pub struct All<B> {
    branches: B,
    start: Option<f64>,
}

pub trait IntoAll {
    type Branches: Branches;
    fn into_branches(self) -> Self::Branches;
}

pub trait Branches {
    type Output;
    /// Polls unfinished branches; returns the max end time once all are done.
    fn poll_all(&mut self, start: f64, cx: &mut Context<'_>) -> Option<f64>;
    fn take(&mut self) -> Self::Output;
}

impl<B: Branches + Unpin> Future for All<B> {
    type Output = B::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<B::Output> {
        let this = &mut *self;
        let start = *this.start.get_or_insert_with(runtime::time);
        match this.branches.poll_all(start, cx) {
            Some(end) => {
                runtime::with(|rt| rt.task_time.set(end));
                Poll::Ready(this.branches.take())
            }
            None => Poll::Pending,
        }
    }
}

macro_rules! impl_all {
    ($($F:ident $i:tt),+) => {
        impl<$($F: Future),+> IntoAll for ($($F,)+) {
            type Branches = ($(Branch<$F>,)+);
            fn into_branches(self) -> Self::Branches {
                ($(Branch::new(self.$i),)+)
            }
        }

        impl<$($F: Future),+> Branches for ($(Branch<$F>,)+) {
            type Output = ($($F::Output,)+);

            fn poll_all(&mut self, start: f64, cx: &mut Context<'_>) -> Option<f64> {
                let mut done = true;
                $( done &= self.$i.poll(start, cx); )+
                done.then(|| {
                    let mut end = start;
                    $( end = end.max(self.$i.time); )+
                    end
                })
            }

            fn take(&mut self) -> Self::Output {
                ($(self.$i.take(),)+)
            }
        }
    };
}

impl_all!(A 0);
impl_all!(A 0, B 1);
impl_all!(A 0, B 1, C 2);
impl_all!(A 0, B 1, C 2, D 3);
impl_all!(A 0, B 1, C 2, D 3, E 4);
impl_all!(A 0, B 1, C 2, D 3, E 4, F 5);
impl_all!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
impl_all!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
impl_all!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8);
impl_all!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9);
impl_all!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10);
impl_all!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11);

/// [`all`] for a dynamic number of futures.
pub fn all_vec<T: 'static>(futures: Vec<BoxFuture<T>>) -> All<VecBranches<T>> {
    All {
        branches: VecBranches(futures.into_iter().map(Branch::new).collect()),
        start: None,
    }
}

pub struct VecBranches<T>(Vec<Branch<BoxFuture<T>>>);

impl<T> Branches for VecBranches<T> {
    type Output = Vec<T>;

    fn poll_all(&mut self, start: f64, cx: &mut Context<'_>) -> Option<f64> {
        let mut done = true;
        for b in &mut self.0 {
            done &= b.poll(start, cx);
        }
        done.then(|| self.0.iter().fold(start, |end, b| end.max(b.time)))
    }

    fn take(&mut self) -> Vec<T> {
        self.0.iter_mut().map(Branch::take).collect()
    }
}

/// Starts the futures one after another with `delay` seconds between their
/// starts; they then run concurrently. Completes when all have.
pub fn sequence(delay: f64, futures: Vec<BoxFuture>) -> All<VecBranches<()>> {
    all_vec(
        futures
            .into_iter()
            .enumerate()
            .map(|(i, f)| -> BoxFuture {
                let offset = delay * i as f64;
                Box::pin(async move {
                    wait_for(offset).await;
                    f.await;
                })
            })
            .collect(),
    )
}

// ---------------------------------------------------------------- spawn

pub(crate) struct TaskState {
    pub finished: Cell<bool>,
    pub cancelled: Cell<bool>,
}

pub(crate) struct Task {
    pub fut: BoxFuture,
    pub time: f64,
    pub state: Rc<TaskState>,
}

impl Task {
    /// Returns true when the task is done (finished or cancelled).
    pub fn poll(&mut self, cx: &mut Context<'_>) -> bool {
        if self.state.cancelled.get() {
            return true;
        }
        if poll_scoped(self.fut.as_mut(), &mut self.time, cx).is_ready() {
            self.state.finished.set(true);
        }
        self.state.finished.get()
    }
}

/// Runs `fut` in the background, starting this frame. Background tasks stop
/// when the scene ends.
pub fn spawn(fut: impl Future<Output = ()> + 'static) -> JoinHandle {
    let state = Rc::new(TaskState {
        finished: Cell::new(false),
        cancelled: Cell::new(false),
    });
    runtime::with(|rt| {
        rt.spawned.borrow_mut().push(Task {
            fut: Box::pin(fut),
            time: rt.task_time.get(),
            state: state.clone(),
        });
    });
    JoinHandle { state }
}

/// Awaiting it waits for the spawned task to finish.
pub struct JoinHandle {
    state: Rc<TaskState>,
}

impl JoinHandle {
    /// Whether the task has completed or was cancelled.
    pub fn is_finished(&self) -> bool {
        self.state.finished.get()
    }

    /// Stops the task at its next await point.
    pub fn cancel(&self) {
        self.state.cancelled.set(true);
    }
}

impl Future for JoinHandle {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        if self.state.finished.get() || self.state.cancelled.get() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
