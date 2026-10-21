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

/// A wait returned by [`wait_for`].
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

/// Runs animations at the same time and completes when all of them have.
///
/// Takes a tuple of up to 12 futures of any types (`all((a, b, c))`), or any
/// number of futures of one type: a `Vec`, an array or an iterator, e.g.
/// `all(cards.iter().map(|c| c.fade_in()))`. Returns their outputs (a tuple
/// or a `Vec`).
///
/// Every branch starts at the same time; afterwards the task continues from
/// the latest end time among them.
pub fn all<T: IntoAll>(futures: T) -> All<T::Branches> {
    All {
        branches: futures.into_branches(),
        start: None,
    }
}

/// Runs animations at the same time and completes as soon as the first one
/// does; the others stop where they are. Returns the index of the one that
/// finished. Takes the same arguments as [`all`].
pub fn any<T: IntoAll>(futures: T) -> Any<T::Branches> {
    Any {
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

#[must_use = "futures do nothing unless awaited"]
/// Future returned by [`any`].
pub struct Any<B> {
    branches: B,
    start: Option<f64>,
}

/// What [`all`] and [`any`] accept: a tuple of futures, or a `Vec`, array or
/// iterator of futures of one type.
#[diagnostic::on_unimplemented(
    message = "`all`/`any` cannot run `{Self}` concurrently",
    label = "expected a tuple, Vec, array or iterator of animations",
    note = "write `all((a, b))` with double parentheses for a few animations, or \
            `all(items.iter().map(|x| x.fade_in()))` for many of the same kind",
    note = "did you forget to call the animation, e.g. `x.fade_in()` instead of `x.fade_in`?"
)]
pub trait IntoAll {
    /// The branches being polled.
    type Branches: Branches;
    /// Wraps every future in a branch.
    fn into_branches(self) -> Self::Branches;
}

/// Branches run by [`all`] and [`any`]. Implemented by the crate only.
pub trait Branches: sealed::Sealed {
    /// Outputs of all branches.
    type Output;
    /// Polls unfinished branches; returns the max end time once all are done.
    fn poll_all(&mut self, start: f64, cx: &mut Context<'_>) -> Option<f64>;
    /// Polls all branches; returns `(index, end time)` of the first finished.
    fn poll_any(&mut self, start: f64, cx: &mut Context<'_>) -> Option<(usize, f64)>;
    /// Outputs, once all are done.
    fn take(&mut self) -> Self::Output;
}

mod sealed {
    pub trait Sealed {}
    impl<F: std::future::Future> Sealed for super::VecBranches<F> {}
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

impl<B: Branches + Unpin> Future for Any<B> {
    type Output = usize;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<usize> {
        let this = &mut *self;
        let start = *this.start.get_or_insert_with(runtime::time);
        match this.branches.poll_any(start, cx) {
            Some((index, end)) => {
                runtime::with(|rt| rt.task_time.set(end));
                Poll::Ready(index)
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

        impl<$($F: Future),+> sealed::Sealed for ($(Branch<$F>,)+) {}

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

            fn poll_any(&mut self, start: f64, cx: &mut Context<'_>) -> Option<(usize, f64)> {
                let mut first: Option<(usize, f64)> = None;
                $(
                    if self.$i.poll(start, cx) && first.is_none() {
                        first = Some(($i, self.$i.time));
                    }
                )+
                first
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

impl<F: Future> IntoAll for Vec<F> {
    type Branches = VecBranches<F>;
    fn into_branches(self) -> VecBranches<F> {
        VecBranches(self.into_iter().map(Branch::new).collect())
    }
}

impl<F: Future, const N: usize> IntoAll for [F; N] {
    type Branches = VecBranches<F>;
    fn into_branches(self) -> VecBranches<F> {
        VecBranches(self.into_iter().map(Branch::new).collect())
    }
}

/// `items.iter().map(|x| animation)`: the common way to animate many nodes.
impl<I: Iterator, G: FnMut(I::Item) -> F, F: Future> IntoAll for std::iter::Map<I, G> {
    type Branches = VecBranches<F>;
    fn into_branches(self) -> VecBranches<F> {
        VecBranches(self.map(Branch::new).collect())
    }
}

/// Branches of [`all`] or [`any`] over many futures of one type.
pub struct VecBranches<F: Future>(Vec<Branch<F>>);

impl<F: Future> Branches for VecBranches<F> {
    type Output = Vec<F::Output>;

    fn poll_all(&mut self, start: f64, cx: &mut Context<'_>) -> Option<f64> {
        let mut done = true;
        for b in &mut self.0 {
            done &= b.poll(start, cx);
        }
        done.then(|| self.0.iter().fold(start, |end, b| end.max(b.time)))
    }

    fn poll_any(&mut self, start: f64, cx: &mut Context<'_>) -> Option<(usize, f64)> {
        if self.0.is_empty() {
            return Some((0, start));
        }
        let mut first = None;
        for (i, b) in self.0.iter_mut().enumerate() {
            if b.poll(start, cx) && first.is_none() {
                first = Some((i, b.time));
            }
        }
        first
    }

    fn take(&mut self) -> Vec<F::Output> {
        self.0.iter_mut().map(Branch::take).collect()
    }
}

/// Anything that can be awaited in a scene: a tween, `wait_for`, `all(..)`,
/// an `async` block… Every future is one; the trait exists for the error
/// message when a closure returns something else.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an animation",
    label = "expected an animation such as `x.fade_in()` or `x.x().to(100.0, 1.0)`",
    note = "a closure body ending in `;` returns `()`: drop the `;`, or write \
            `async move {{ a.await; b.await; }}` for several steps"
)]
pub trait Animation: Future {}

impl<F: Future> Animation for F {}

/// [`all`] for boxed futures.
#[deprecated(since = "0.3.0", note = "`all` takes a Vec now: `all(futures)`")]
pub fn all_vec<T: 'static>(futures: Vec<BoxFuture<T>>) -> All<VecBranches<BoxFuture<T>>> {
    all(futures)
}

/// Starts animations one after another, `delay` seconds apart; they then run
/// concurrently. Completes when all have. Takes a `Vec`, an array or an
/// iterator of animations of one type. For "the same animation on many
/// nodes", [`stagger`] reads better.
pub fn sequence<I>(delay: f64, futures: I) -> All<VecBranches<Delayed<I::Item>>>
where
    I: IntoIterator,
    I::Item: Animation,
{
    all(futures
        .into_iter()
        .enumerate()
        .map(|(i, fut)| Delayed {
            wait: wait_for(delay * i as f64),
            waited: false,
            fut: Box::pin(fut),
        })
        .collect::<Vec<_>>())
}

/// Runs `animate` on every item, starting each `delay` seconds after the
/// previous one; completes when all have finished:
///
/// ```ignore
/// stagger(0.15, &cards, |c| c.fade_in()).await;
/// ```
pub fn stagger<I, F>(
    delay: f64,
    items: I,
    animate: impl FnMut(I::Item) -> F,
) -> All<VecBranches<Delayed<F>>>
where
    I: IntoIterator,
    F: Animation,
{
    sequence(delay, items.into_iter().map(animate))
}

/// A future that starts after a wait; see [`sequence`].
#[must_use = "futures do nothing unless awaited"]
pub struct Delayed<F: Future> {
    wait: WaitFor,
    waited: bool,
    fut: Pin<Box<F>>,
}

impl<F: Future> Future for Delayed<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = &mut *self;
        if !this.waited {
            if Pin::new(&mut this.wait).poll(cx).is_pending() {
                return Poll::Pending;
            }
            this.waited = true;
        }
        this.fut.as_mut().poll(cx)
    }
}

/// Runs the animation `make()` returns `times` times in a row.
pub async fn repeat<F: Animation>(times: usize, mut make: impl FnMut() -> F) {
    for _ in 0..times {
        make().await;
    }
}

/// Runs the animation `make()` returns again and again while `keep_going()`
/// is true (checked before each run). Usually `spawn`ed for a background
/// loop: `spawn(loop_while(|| true, || pulse.scale().to(1.1, 0.5)))`.
pub async fn loop_while<F: Animation>(
    mut keep_going: impl FnMut() -> bool,
    mut make: impl FnMut() -> F,
) {
    while keep_going() {
        make().await;
    }
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
