//! Scoped deadlines for the synchronous SDK. New worker threads must enter
//! their request scope explicitly; guards cannot move across threads.
use std::{
    cell::Cell,
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};
thread_local! { static CURRENT: Cell<Option<Instant>> = const { Cell::new(None) }; }
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
            .checked_duration_since(Instant::now())
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
}
