use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::{Rc, Weak};

use crate::{Lerp, Seconds, Tween};

/// A reactive value.
///
/// A signal either holds a plain value ([`set`](Signal::set)) or is derived
/// from other signals ([`bind`](Signal::bind)). Derived signals are lazy:
/// writes only mark dependents dirty, and the closure re-runs on the next
/// [`get`](Signal::get). Cloning a `Signal` clones the handle, not the value.
pub struct Signal<T>(Rc<SignalCell<T>>);

struct SignalCell<T> {
    name: RefCell<Option<Rc<str>>>,
    value: RefCell<T>,
    compute: RefCell<Option<Rc<dyn Fn() -> T>>>,
    dirty: Cell<bool>,
    computing: Cell<bool>,
    subscribers: RefCell<Vec<Weak<dyn Observer>>>,
}

trait Observer {
    fn invalidate(&self);
}

thread_local! {
    /// Derived signals currently being computed, innermost last.
    static TRACKING: RefCell<Vec<Rc<dyn Observer>>> = const { RefCell::new(Vec::new()) };
}

impl<T: 'static> Observer for SignalCell<T> {
    fn invalidate(&self) {
        if self.compute.borrow().is_some() && !self.dirty.replace(true) {
            self.notify();
        }
    }
}

impl<T: 'static> SignalCell<T> {
    fn notify(&self) {
        // Hot path: most signals (tweened properties) have no dependents.
        if self.subscribers.borrow().is_empty() {
            return;
        }
        // Collect first: an observer may subscribe while being invalidated.
        let subs: Vec<_> = {
            let mut subs = self.subscribers.borrow_mut();
            subs.retain(|w| w.strong_count() > 0);
            subs.iter().filter_map(Weak::upgrade).collect()
        };
        for s in subs {
            s.invalidate();
        }
    }

    fn display_name(&self) -> String {
        self.name.borrow().as_deref().map_or_else(
            || format!("<unnamed {:p}>", self as *const _),
            str::to_owned,
        )
    }
}

impl<T: Clone + 'static> Signal<T> {
    /// A signal holding `value`.
    pub fn new(value: T) -> Self {
        Self(Rc::new(SignalCell {
            name: RefCell::new(None),
            value: RefCell::new(value),
            compute: RefCell::new(None),
            dirty: Cell::new(false),
            computing: Cell::new(false),
            subscribers: RefCell::new(Vec::new()),
        }))
    }

    /// A derived signal. The closure runs lazily on the first `get`.
    pub fn derived(f: impl Fn() -> T + 'static) -> Self
    where
        T: Default,
    {
        let s = Self::new(T::default());
        s.bind(f);
        s
    }

    /// Name used in diagnostics (cycle panics, inspector).
    pub fn with_name(self, name: impl Into<Rc<str>>) -> Self {
        *self.0.name.borrow_mut() = Some(name.into());
        self
    }

    /// Name shown in the editor's inspector, if any.
    pub fn name(&self) -> Option<Rc<str>> {
        self.0.name.borrow().clone()
    }

    /// Current value; a bound signal recomputes only if its inputs changed.
    ///
    /// # Panics
    ///
    /// In debug builds, if bound signals depend on each other in a cycle
    /// (release builds return the last value instead).
    pub fn get(&self) -> T {
        self.track();
        let cell = &self.0;
        if cell.dirty.get() {
            let compute = cell.compute.borrow().clone();
            if let Some(f) = compute {
                if cell.computing.get() {
                    if cfg!(debug_assertions) {
                        panic!("signal dependency cycle through `{}`", cell.display_name());
                    }
                    return cell.value.borrow().clone();
                }
                cell.computing.set(true);
                let observer: Rc<dyn Observer> = self.0.clone();
                TRACKING.with(|t| t.borrow_mut().push(observer));
                let value = f();
                TRACKING.with(|t| t.borrow_mut().pop());
                cell.computing.set(false);
                *cell.value.borrow_mut() = value;
            }
            cell.dirty.set(false);
        }
        cell.value.borrow().clone()
    }

    /// Reads without registering a dependency.
    pub fn peek(&self) -> T {
        let saved = TRACKING.with(|t| std::mem::take(&mut *t.borrow_mut()));
        let v = self.get();
        TRACKING.with(|t| *t.borrow_mut() = saved);
        v
    }

    /// Sets a plain value; removes any binding. Takes anything that converts,
    /// like [`Signal::to`]: `text.set("Done")` on a `Signal<String>`.
    pub fn set(&self, value: impl Into<T>) {
        *self.0.compute.borrow_mut() = None;
        self.0.dirty.set(false);
        *self.0.value.borrow_mut() = value.into();
        self.0.notify();
    }

    /// Makes this signal derived from `f`.
    pub fn bind(&self, f: impl Fn() -> T + 'static) {
        *self.0.compute.borrow_mut() = Some(Rc::new(f));
        self.0.dirty.set(true);
        self.0.notify();
    }

    /// Whether the value comes from [`bind`](Self::bind).
    pub fn is_bound(&self) -> bool {
        self.0.compute.borrow().is_some()
    }

    /// Tweens from the current value to `target` over `duration` seconds.
    ///
    /// `f64` is seconds. A theme pace (`Normal`) is a duration too.
    pub fn to(&self, target: impl Into<T>, duration: impl Seconds) -> Tween<T>
    where
        T: Lerp,
    {
        Tween::new(self.clone(), target.into(), duration.seconds())
    }

    fn track(&self) {
        TRACKING.with(|t| {
            let t = t.borrow();
            let Some(observer) = t.last() else { return };
            let mut subs = self.0.subscribers.borrow_mut();
            let already = subs
                .iter()
                .any(|w| std::ptr::addr_eq(w.as_ptr(), Rc::as_ptr(observer)));
            if !already {
                subs.push(Rc::downgrade(observer));
            }
        });
    }
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: fmt::Debug> fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signal")
            .field("name", &self.0.name.borrow())
            .field("value", &self.0.value.borrow())
            .field("bound", &self.0.compute.borrow().is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get() {
        let s = Signal::new(1.0);
        s.set(2.0);
        assert_eq!(s.get(), 2.0);
    }

    #[test]
    fn derived_is_lazy_and_updates() {
        let runs = Rc::new(Cell::new(0));
        let a = Signal::new(2.0);
        let b = {
            let (a, runs) = (a.clone(), runs.clone());
            Signal::derived(move || {
                runs.set(runs.get() + 1);
                a.get() * 10.0
            })
        };
        assert_eq!(runs.get(), 0, "not computed before first get");
        assert_eq!(b.get(), 20.0);
        assert_eq!(b.get(), 20.0);
        assert_eq!(runs.get(), 1, "cached while clean");
        a.set(3.0);
        a.set(4.0);
        assert_eq!(runs.get(), 1, "writes only mark dirty");
        assert_eq!(b.get(), 40.0);
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn chains_propagate() {
        let a = Signal::new(1.0);
        let b = {
            let a = a.clone();
            Signal::derived(move || a.get() + 1.0)
        };
        let c = {
            let b = b.clone();
            Signal::derived(move || b.get() * 2.0)
        };
        assert_eq!(c.get(), 4.0);
        a.set(5.0);
        assert_eq!(c.get(), 12.0);
    }

    #[test]
    fn set_unbinds() {
        let a = Signal::new(1.0);
        let b = Signal::new(0.0);
        let a2 = a.clone();
        b.bind(move || a2.get());
        assert_eq!(b.get(), 1.0);
        b.set(7.0);
        a.set(3.0);
        assert_eq!(b.get(), 7.0);
        assert!(!b.is_bound());
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "signal dependency cycle through `loop-a`")]
    fn cycle_panics_with_name() {
        let a = Signal::new(0.0).with_name("loop-a");
        let b = Signal::new(0.0).with_name("loop-b");
        let (a2, b2) = (a.clone(), b.clone());
        a.bind(move || b2.get() + 1.0);
        b.bind(move || a2.get() + 1.0);
        a.get();
    }
}
