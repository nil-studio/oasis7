//! Per-runtime scheduling for real protocol tests. No production build includes
//! these controls. A pause neither edits a World nor supplies durable evidence.
use oasis7_node::NodeExecutionCommitContext;
use std::{
    cell::RefCell,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Point {
    BeforeResume,
    CandidateSaved,
    Published,
}

#[derive(Clone, Debug)]
pub(crate) struct Observation {
    pub(crate) point: Point,
    pub(crate) position: u64,
    pub(crate) record_id: Option<String>,
    pub(crate) node_block_hash: Option<String>,
    pub(crate) action_count: usize,
}

struct Target {
    point: Point,
    position: Option<u64>,
    exact_action: Option<Vec<u8>>,
    deadline: Instant,
}

#[derive(Default)]
struct State {
    target: Option<Target>,
    paused: Option<Observation>,
    cancelled: bool,
}

#[derive(Default)]
pub(crate) struct BoundaryState {
    state: Mutex<State>,
    changed: Condvar,
}

/// The fixture owns the sole cancellation handle. Dropping it releases any
/// blocked execution before the runtime/endpoint owners are dropped.
pub(crate) struct BoundaryControl {
    shared: Arc<BoundaryState>,
}

impl BoundaryControl {
    pub(crate) fn new() -> Self {
        Self {
            shared: Arc::new(BoundaryState::default()),
        }
    }

    pub(crate) fn shared(&self) -> Arc<BoundaryState> {
        Arc::clone(&self.shared)
    }

    /// Also releases the currently paused boundary. Installing the next target
    /// and releasing the old one happen under one lock, without a ticking gap.
    pub(crate) fn arm(
        &self,
        point: Point,
        position: Option<u64>,
        exact_action: Option<Vec<u8>>,
        deadline: Instant,
    ) -> Result<(), String> {
        if Instant::now() >= deadline {
            return Err("formal test boundary deadline expired before arming".into());
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| "formal test boundary lock poisoned")?;
        if state.cancelled || (state.target.is_some() && state.paused.is_none()) {
            return Err("formal test boundary already armed or cancelled".into());
        }
        state.paused = None;
        state.target = Some(Target {
            point,
            position,
            exact_action,
            deadline,
        });
        self.shared.changed.notify_all();
        Ok(())
    }

    pub(crate) fn wait(&self, deadline: Instant) -> Result<Observation, String> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| "formal test boundary lock poisoned")?;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if state.cancelled || remaining.is_zero() {
                return Err("formal test boundary was not observed before deadline".into());
            }
            if let Some(observation) = &state.paused {
                return Ok(observation.clone());
            }
            let (next, _) = self
                .shared
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| "formal test boundary wait poisoned")?;
            state = next;
        }
    }

    pub(crate) fn release(&self) -> Result<(), String> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| "formal test boundary lock poisoned")?;
        state.target = None;
        state.paused = None;
        self.shared.changed.notify_all();
        Ok(())
    }
}

impl Drop for BoundaryControl {
    fn drop(&mut self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.cancelled = true;
        state.target = None;
        state.paused = None;
        self.shared.changed.notify_all();
    }
}

impl BoundaryState {
    fn arrive(
        &self,
        observation: Observation,
        context: Option<&NodeExecutionCommitContext>,
    ) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "formal test boundary lock poisoned")?;
        let Some(target) = &state.target else {
            return Ok(());
        };
        if state.cancelled
            || target.point != observation.point
            || target
                .position
                .is_some_and(|position| position != observation.position)
            || target.exact_action.as_ref().is_some_and(|expected| {
                context.is_none_or(|context| {
                    !context
                        .committed_actions
                        .iter()
                        .any(|action| &action.payload_cbor == expected)
                })
            })
        {
            return Ok(());
        }
        let deadline = target.deadline;
        state.target = None;
        state.paused = Some(observation);
        self.changed.notify_all();
        while state.paused.is_some() && !state.cancelled {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                state.paused = None;
                self.changed.notify_all();
                return Err("formal test boundary release deadline expired".into());
            }
            let (next, _) = self
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| "formal test boundary wait poisoned")?;
            state = next;
        }
        Ok(())
    }

    pub(crate) fn published(&self, position: u64) -> Result<(), String> {
        // Only the currently executing continuation of this same runtime can
        // satisfy an exact-action publication target. Startup has no resume
        // context, and another runtime's thread-local scope cannot stand in.
        let active = ACTIVE_RESUME.with(|current| current.borrow().clone());
        let context = active
            .as_ref()
            .filter(|active| std::ptr::eq(active.control.as_ref(), self))
            .map(|active| &active.context);
        self.arrive(
            Observation {
                point: Point::Published,
                position,
                record_id: None,
                node_block_hash: context.map(|context| context.node_block_hash.clone()),
                action_count: context.map_or(0, |context| context.committed_actions.len()),
            },
            context,
        )
    }

    pub(crate) fn resume_scope(
        self: &Arc<Self>,
        context: &NodeExecutionCommitContext,
    ) -> Result<ResumeScope, String> {
        self.arrive(
            Observation {
                point: Point::BeforeResume,
                position: context.height,
                record_id: None,
                node_block_hash: Some(context.node_block_hash.clone()),
                action_count: context.committed_actions.len(),
            },
            Some(context),
        )?;
        let active = ActiveResume {
            control: Arc::clone(self),
            context: context.clone(),
        };
        let previous = ACTIVE_RESUME.with(|current| current.replace(Some(active)));
        Ok(ResumeScope {
            previous,
            _not_send: PhantomData,
        })
    }
}

#[derive(Clone)]
struct ActiveResume {
    control: Arc<BoundaryState>,
    context: NodeExecutionCommitContext,
}

thread_local! {
    static ACTIVE_RESUME: RefCell<Option<ActiveResume>> = const { RefCell::new(None) };
}

/// Thread-local installation is limited to this exact hook invocation. It does
/// not use process-wide environment selectors or affect another fixture's Node.
pub(crate) struct ResumeScope {
    previous: Option<ActiveResume>,
    _not_send: PhantomData<Rc<()>>,
}
impl Drop for ResumeScope {
    fn drop(&mut self) {
        ACTIVE_RESUME.with(|current| current.replace(self.previous.take()));
    }
}

pub(crate) fn candidate_saved(position: u64, record_id: String) -> Result<(), String> {
    let active = ACTIVE_RESUME.with(|current| current.borrow().clone());
    if let Some(active) = active {
        active.control.arrive(
            Observation {
                point: Point::CandidateSaved,
                position,
                record_id: Some(record_id),
                node_block_hash: Some(active.context.node_block_hash.clone()),
                action_count: active.context.committed_actions.len(),
            },
            Some(&active.context),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread, time::Duration};

    #[test]
    fn dropping_the_fixture_control_releases_an_in_flight_publication() {
        let control = BoundaryControl::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        control
            .arm(Point::Published, Some(2), None, deadline)
            .unwrap();
        let shared = control.shared();
        let (done, result) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || done.send(shared.published(2)).unwrap());
        assert_eq!(control.wait(deadline).unwrap().position, 2);
        assert!(control.wait(Instant::now()).is_err());
        assert!(result.recv_timeout(Duration::from_millis(10)).is_err());
        drop(control);
        result
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn missing_or_unreleased_boundaries_expire_without_resetting_the_deadline() {
        let control = BoundaryControl::new();
        let deadline = Instant::now() + Duration::from_millis(20);
        control
            .arm(Point::Published, Some(2), None, deadline)
            .unwrap();
        assert!(control.wait(deadline).is_err());
        assert!(control.shared.published(2).is_err());
        assert!(
            control
                .arm(Point::Published, Some(3), None, deadline)
                .is_err()
        );
    }
}
