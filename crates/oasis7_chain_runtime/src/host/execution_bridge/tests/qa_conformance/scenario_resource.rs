//! Isolate independent real-node scenarios from host CPU/CAS/fsync competition.
//! Transport workers, viewers and pressure inside a scenario remain concurrent.
use std::sync::{Arc, Condvar, Mutex};

static AVAILABLE: Mutex<bool> = Mutex::new(true);
static RELEASED: Condvar = Condvar::new();

pub(super) struct ScenarioResourcePermit;

impl ScenarioResourcePermit {
    pub(super) fn acquire() -> Arc<Self> {
        let mut available = AVAILABLE.lock().expect("scenario resource state poisoned");
        while !*available {
            available = RELEASED
                .wait(available)
                .expect("scenario resource state poisoned");
        }
        *available = false;
        drop(available);
        Arc::new(Self)
    }
}

impl Drop for ScenarioResourcePermit {
    fn drop(&mut self) {
        // Release even during scenario unwinding. Poison is not cleared: future
        // acquisition still fails closed, while cleanup never strands a waiter.
        let mut available = AVAILABLE.lock().unwrap_or_else(|error| error.into_inner());
        *available = true;
        RELEASED.notify_all();
    }
}
