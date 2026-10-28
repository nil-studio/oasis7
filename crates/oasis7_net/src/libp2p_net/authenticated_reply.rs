//! Reply restrictions only narrow an existing authenticated request's release.
use super::*;
use oasis7_proto::distributed_net::NetworkRequestContext;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
#[derive(Clone)]
pub(super) struct ReplyGuard {
    owner: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
}
struct State {
    original: Option<NetworkRequestContext>,
    deadline: Option<Instant>,
    guards: Vec<Guard>,
    invalid: bool,
}
enum Guard {
    Generation(Arc<AtomicU64>, u64),
    While(Arc<AtomicBool>),
}
impl ReplyGuard {
    pub fn new(owner: Arc<AtomicBool>) -> Self {
        Self {
            owner,
            state: Arc::new(Mutex::new(State {
                original: None,
                deadline: None,
                guards: Vec::new(),
                invalid: false,
            })),
        }
    }
    pub fn initialize(&self, base: &NetworkRequestContext) -> Result<(), WorldError> {
        let mut s = self.state.lock().map_err(|_| fail())?;
        s.deadline = Some(
            s.deadline
                .map_or(base.deadline(), |d| d.min(base.deadline())),
        );
        if s.original.is_none() {
            s.original = Some(base.clone());
        }
        drop(s);
        self.check()
    }
    pub fn until(&self, deadline: Instant) -> Result<(), WorldError> {
        let mut s = self.state.lock().map_err(|_| fail())?;
        s.deadline = Some(s.deadline.map_or(deadline, |d| d.min(deadline)));
        drop(s);
        self.check()
    }
    pub fn generation(&self, value: Arc<AtomicU64>, expected: u64) -> Result<(), WorldError> {
        self.add(Guard::Generation(value, expected))
    }
    pub fn while_true(&self, value: Arc<AtomicBool>) -> Result<(), WorldError> {
        self.add(Guard::While(value))
    }
    fn add(&self, guard: Guard) -> Result<(), WorldError> {
        let mut s = self.state.lock().map_err(|_| fail())?;
        if s.guards.len() >= 4 {
            s.invalid = true;
            return Err(fail());
        }
        s.guards.push(guard);
        drop(s);
        self.check()
    }
    pub fn check(&self) -> Result<(), WorldError> {
        if !self.owner.load(Ordering::Acquire) {
            return Err(fail());
        }
        let s = self.state.lock().map_err(|_| fail())?;
        if s.invalid
            || s.original
                .as_ref()
                .is_none_or(NetworkRequestContext::is_cancelled)
            || s.deadline.is_none_or(|d| Instant::now() >= d)
            || s.guards.iter().any(|g| match g {
                Guard::Generation(v, e) => v.load(Ordering::Acquire) != *e,
                Guard::While(v) => !v.load(Ordering::Acquire),
            })
        {
            return Err(fail());
        }
        Ok(())
    }
}
fn fail() -> WorldError {
    super::authenticated_requests::rejected("authenticated reply restriction rejected")
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn guard() -> ReplyGuard {
        let g = ReplyGuard::new(Arc::new(AtomicBool::new(true)));
        g.initialize(&NetworkRequestContext::new(
            Instant::now() + Duration::from_secs(5),
            Arc::new(AtomicBool::new(false)),
        ))
        .unwrap();
        g
    }
    #[test]
    fn reply_generation_serving_and_deadline_survive_callback() {
        let g = guard();
        let generation = Arc::new(AtomicU64::new(4));
        g.generation(generation.clone(), 4).unwrap();
        assert!(g.check().is_ok());
        generation.store(5, Ordering::Release);
        assert!(g.check().is_err());
        let g = guard();
        let serving = Arc::new(AtomicBool::new(true));
        g.while_true(serving.clone()).unwrap();
        serving.store(false, Ordering::Release);
        assert!(g.check().is_err());
        let g = guard();
        g.until(Instant::now() + Duration::from_millis(10)).unwrap();
        g.until(Instant::now() + Duration::from_secs(30)).unwrap();
        std::thread::sleep(Duration::from_millis(15));
        assert!(g.check().is_err());
    }
    #[test]
    fn reply_guard_overflow_and_poison_fail_closed() {
        let g = guard();
        for _ in 0..4 {
            g.while_true(Arc::new(AtomicBool::new(true))).unwrap();
        }
        assert!(g.while_true(Arc::new(AtomicBool::new(true))).is_err());
        assert!(g.check().is_err());
        let g = guard();
        let state = g.state.clone();
        let _ = std::thread::spawn(move || {
            let _lock = state.lock().unwrap();
            panic!("poison reply restriction");
        })
        .join();
        assert!(g.check().is_err());
    }
}
