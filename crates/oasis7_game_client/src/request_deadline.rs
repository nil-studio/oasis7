//! Scoped deadlines for the synchronous SDK. New worker threads must enter
//! their request scope explicitly; guards cannot move across threads.
use std::{
    cell::Cell,
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};
thread_local! { static CURRENT: Cell<Option<Instant>> = const { Cell::new(None) }; }
pub(crate) fn now() -> Instant {
    #[cfg(test)]
    if let Some(now) = test_clock::current() {
        return now;
    }
    Instant::now()
}
pub(crate) struct Scope {
    previous: Option<Instant>,
    _thread_bound: PhantomData<Rc<()>>,
}
impl Scope {
    pub(crate) fn enter(deadline: Instant) -> Self {
        let previous = CURRENT.with(|current| {
            let previous = current.get();
            current.set(Some(
                previous.map_or(deadline, |parent| parent.min(deadline)),
            ));
            previous
        });
        Self {
            previous,
            _thread_bound: PhantomData,
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|current| current.set(self.previous));
    }
}
pub(crate) fn remaining() -> Result<Option<Duration>, &'static str> {
    CURRENT.with(|current| match current.get() {
        None => Ok(None),
        Some(deadline) => deadline
            .checked_duration_since(now())
            .filter(|value| !value.is_zero())
            .map(Some)
            .ok_or("request_deadline_exceeded"),
    })
}
pub(crate) fn bound(
    request: reqwest::blocking::RequestBuilder,
    maximum: Duration,
) -> Result<reqwest::blocking::RequestBuilder, &'static str> {
    Ok(match remaining()? {
        Some(time) => request.timeout(time.min(maximum)),
        None => request,
    })
}
/// Advance only this test's deadline clock after a real protocol phase arrives.
/// Production always uses Instant; other test threads never inherit this override.
#[cfg(test)]
pub(crate) mod test_clock {
    use super::*;
    use std::{cell::RefCell, sync::Arc, sync::Mutex};

    thread_local! {
        static CLOCK: RefCell<Option<ManualClock>> = const { RefCell::new(None) };
    }

    #[derive(Clone)]
    pub(crate) struct ManualClock(Arc<Mutex<Instant>>);

    impl ManualClock {
        pub(crate) fn new(now: Instant) -> Self {
            Self(Arc::new(Mutex::new(now)))
        }
        pub(crate) fn now(&self) -> Instant {
            *self.0.lock().unwrap()
        }
        pub(crate) fn advance(&self, elapsed: Duration) {
            let mut now = self.0.lock().unwrap();
            *now += elapsed;
        }
        pub(crate) fn enter(&self) -> ClockScope {
            ClockScope {
                previous: CLOCK.with(|clock| clock.replace(Some(self.clone()))),
                _thread_bound: PhantomData,
            }
        }
    }

    pub(crate) struct ClockScope {
        previous: Option<ManualClock>,
        _thread_bound: PhantomData<Rc<()>>,
    }
    impl Drop for ClockScope {
        fn drop(&mut self) {
            CLOCK.with(|clock| clock.replace(self.previous.take()));
        }
    }
    pub(super) fn current() -> Option<Instant> {
        CLOCK.with(|clock| clock.borrow().as_ref().map(ManualClock::now))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_scopes_take_minimum_restore_and_unwind() {
        assert_eq!(remaining().unwrap(), None);
        let parent = Scope::enter(Instant::now() + Duration::from_secs(2));
        {
            let _child = Scope::enter(Instant::now() + Duration::from_secs(20));
            assert!(remaining().unwrap().unwrap() <= Duration::from_secs(2));
        }
        {
            let _child = Scope::enter(Instant::now() - Duration::from_millis(1));
            assert!(remaining().is_err());
        }
        assert!(remaining().unwrap().is_some());
        assert!(
            std::panic::catch_unwind(|| {
                let _child = Scope::enter(Instant::now() - Duration::from_secs(1));
                panic!("synthetic unwind");
            })
            .is_err()
        );
        assert!(remaining().unwrap().is_some());
        drop(parent);
        assert_eq!(remaining().unwrap(), None);
    }
    #[test]
    fn new_thread_requires_explicit_scope() {
        let deadline = Instant::now() - Duration::from_secs(1);
        let _scope = Scope::enter(deadline);
        std::thread::spawn(move || {
            assert_eq!(remaining().unwrap(), None);
            let _worker = Scope::enter(deadline);
            assert!(remaining().is_err());
        })
        .join()
        .unwrap();
        assert!(remaining().is_err());
    }

    #[test]
    fn manual_clock_is_scoped_restored_and_only_shared_explicitly() {
        use test_clock::ManualClock;
        let start = Instant::now();
        let clock = ManualClock::new(start);
        assert!(test_clock::current().is_none());
        let clock_scope = clock.enter();
        let _deadline = Scope::enter(start + Duration::from_millis(500));
        assert_eq!(remaining().unwrap(), Some(Duration::from_millis(500)));
        {
            let child = ManualClock::new(start + Duration::from_secs(1));
            let _child_scope = child.enter();
            assert!(remaining().is_err());
        }
        assert_eq!(now(), start);
        assert!(
            std::panic::catch_unwind(|| {
                let child = ManualClock::new(start + Duration::from_secs(2));
                let _child_scope = child.enter();
                panic!("synthetic clock unwind");
            })
            .is_err()
        );
        assert_eq!(now(), start);
        let controller = clock.clone();
        std::thread::spawn(move || {
            assert!(test_clock::current().is_none());
            assert_eq!(remaining().unwrap(), None);
            controller.advance(Duration::from_millis(800));
            assert!(test_clock::current().is_none());
        })
        .join()
        .unwrap();
        assert_eq!(now(), start + Duration::from_millis(800));
        assert!(remaining().is_err());
        drop(clock_scope);
        assert!(test_clock::current().is_none());
        assert!(now() >= start);
    }
}
