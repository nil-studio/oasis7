//! Test-only observation of one real CLI invocation, never a bind substitute.
use std::{
    cell::RefCell,
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ValidationRejection {
    QualifiedLedger = 1,
    ExactDurableCut = 2,
}

#[derive(Default)]
struct Observation {
    bind_attempts: AtomicUsize,
    rejection: AtomicU8,
}

thread_local! {
    static ACTIVE: RefCell<Option<Arc<Observation>>> = const { RefCell::new(None) };
}

#[derive(Clone, Default)]
pub(crate) struct HeaderListenerProbe {
    observation: Arc<Observation>,
}

impl HeaderListenerProbe {
    pub(crate) fn scope(&self) -> HeaderListenerProbeScope {
        let previous = ACTIVE.with(|active| active.replace(Some(self.observation.clone())));
        HeaderListenerProbeScope {
            previous,
            _same_thread: PhantomData,
        }
    }

    pub(crate) fn bind_attempts(&self) -> usize {
        self.observation.bind_attempts.load(Ordering::Relaxed)
    }

    pub(crate) fn rejection(&self) -> Option<ValidationRejection> {
        match self.observation.rejection.load(Ordering::Relaxed) {
            0 => None,
            1 => Some(ValidationRejection::QualifiedLedger),
            2 => Some(ValidationRejection::ExactDurableCut),
            _ => panic!("invalid test-only header rejection category"),
        }
    }
}

// Only the probe may be cloned into the real CLI thread. A scope must be
// dropped on the same thread whose previous observation it restores.
pub(crate) struct HeaderListenerProbeScope {
    previous: Option<Arc<Observation>>,
    _same_thread: PhantomData<Rc<()>>,
}

impl Drop for HeaderListenerProbeScope {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            let _ = active.replace(self.previous.take());
        });
    }
}

pub(crate) fn record_bind_attempt() {
    ACTIVE.with(|active| {
        if let Some(observation) = active.borrow().as_ref() {
            observation.bind_attempts.fetch_add(1, Ordering::Relaxed);
        }
    });
}

pub(crate) fn record_validation_rejection(kind: ValidationRejection) {
    ACTIVE.with(|active| {
        if let Some(observation) = active.borrow().as_ref() {
            let _ = observation.rejection.compare_exchange(
                0,
                kind as u8,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
    });
}

#[test]
fn header_listener_probes_restore_parent_scope_and_do_not_cross_threads() {
    let outer = HeaderListenerProbe::default();
    let scope = outer.scope();
    record_bind_attempt();
    let inner = HeaderListenerProbe::default();
    {
        let _scope = inner.scope();
        record_bind_attempt();
        record_validation_rejection(ValidationRejection::ExactDurableCut);
    }
    record_bind_attempt();
    std::thread::spawn(record_bind_attempt).join().unwrap();
    assert_eq!(outer.bind_attempts(), 2);
    assert_eq!(outer.rejection(), None);
    assert_eq!(inner.bind_attempts(), 1);
    assert_eq!(
        inner.rejection(),
        Some(ValidationRejection::ExactDurableCut)
    );
    drop(scope);
    record_bind_attempt();
    assert_eq!(outer.bind_attempts(), 2);
}
