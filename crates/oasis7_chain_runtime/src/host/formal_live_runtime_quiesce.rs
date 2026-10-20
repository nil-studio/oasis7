//! Admission-gated wait for an already prepared formal continuation.
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

/// A late gate acquisition is allowed only for the caller's read-only lookup
/// of an exact existing arm. The caller must reject a new arm after deadline.
pub(super) fn acquire_for_idempotency_read<'a>(
    gate: &'a Mutex<()>,
    deadline: Instant,
) -> Result<MutexGuard<'a, ()>, String> {
    loop {
        match gate.try_lock() {
            Ok(admission) => return Ok(admission),
            Err(TryLockError::Poisoned(_)) => {
                return Err("formal Node admission gate poisoned".into());
            }
            Err(TryLockError::WouldBlock) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err("OutcomeUnknown: durable-set preparation deadline expired".into());
                }
                std::thread::sleep(remaining.min(Duration::from_millis(5)));
            }
        }
    }
}

pub(super) fn acquire_after_pending_clear<'a>(
    gate: &'a Mutex<()>,
    mut pending_open: impl FnMut() -> Result<bool, String>,
    deadline: Instant,
) -> Result<MutexGuard<'a, ()>, String> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(
                "OutcomeUnknown: durable-set preparation deadline expired; no new arm was written"
                    .into(),
            );
        }
        let admission = match gate.try_lock() {
            Ok(admission) => admission,
            Err(TryLockError::Poisoned(_)) => {
                return Err("formal Node admission gate poisoned".into());
            }
            Err(TryLockError::WouldBlock) => {
                std::thread::sleep(remaining.min(Duration::from_millis(5)));
                continue;
            }
        };
        if Instant::now() >= deadline {
            drop(admission);
            return Err(
                "OutcomeUnknown: durable-set preparation deadline expired; no new arm was written"
                    .into(),
            );
        }
        if !pending_open()? {
            if Instant::now() >= deadline {
                drop(admission);
                return Err("OutcomeUnknown: durable-set preparation deadline expired; no new arm was written".into());
            }
            return Ok(admission);
        }
        drop(admission);
        std::thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

pub(super) fn arm_before_deadline<T>(
    deadline: Instant,
    arm: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if Instant::now() >= deadline {
        return Err(
            "OutcomeUnknown: durable-set preparation deadline expired; no new arm was started"
                .into(),
        );
    }
    let value = arm()?;
    if Instant::now() >= deadline {
        return Err("OutcomeUnknown: durable-set arm may have persisted; query the original operation before retry".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn pending_clear_reacquires_gate_after_fresh_commit_publication() {
        let gate = Mutex::new(());
        let pending = Arc::new(AtomicBool::new(true));
        let published_position = Arc::new(Mutex::new(40_u64));
        let pending_on_tick = Arc::clone(&pending);
        let published_on_tick = Arc::clone(&published_position);
        let tick = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(25));
            *published_on_tick.lock().unwrap() = 41;
            pending_on_tick.store(false, Ordering::Release);
        });
        let _admission = acquire_after_pending_clear(
            &gate,
            || Ok(pending.load(Ordering::Acquire)),
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(*published_position.lock().unwrap(), 41);
        tick.join().unwrap();
    }

    #[test]
    fn pending_timeout_returns_without_crossing_the_arm_boundary() {
        let gate = Mutex::new(());
        let pending = AtomicBool::new(true);
        let mut arm_written = false;
        let result = acquire_after_pending_clear(
            &gate,
            || Ok(pending.load(Ordering::Acquire)),
            Instant::now() + Duration::from_millis(15),
        );
        if result.is_ok() {
            arm_written = true;
        }
        assert!(result.is_err());
        assert!(!arm_written);
    }

    #[test]
    fn held_gate_past_deadline_with_no_pending_never_admits_new_arm() {
        let gate = Arc::new(Mutex::new(()));
        let held = gate.lock().unwrap();
        let deadline = Instant::now() + Duration::from_millis(25);
        let worker_gate = Arc::clone(&gate);
        let worker = std::thread::spawn(move || {
            let arm_written = AtomicBool::new(false);
            let result = acquire_after_pending_clear(&worker_gate, || Ok(false), deadline);
            if result.is_ok() {
                arm_written.store(true, Ordering::Release);
            }
            (result.is_err(), arm_written.load(Ordering::Acquire))
        });
        let (rejected, arm_written) = worker.join().unwrap();
        drop(held);
        assert!(rejected);
        assert!(!arm_written);
    }

    #[test]
    fn late_persisted_arm_returns_unknown_and_queries_original_value() {
        let persisted = Arc::new(Mutex::new(None::<String>));
        let during_write = Arc::clone(&persisted);
        let result = arm_before_deadline(Instant::now() + Duration::from_millis(15), move || {
            std::thread::sleep(Duration::from_millis(25));
            *during_write.lock().unwrap() = Some("original-signed-arm".into());
            Ok("original-signed-arm".to_owned())
        });
        assert!(
            result
                .as_ref()
                .is_err_and(|error| error.contains("arm may have persisted"))
        );
        assert_eq!(
            persisted.lock().unwrap().as_deref(),
            Some("original-signed-arm")
        );
        assert!(
            arm_before_deadline::<()>(Instant::now(), || {
                panic!("expired deadline must not start another arm")
            })
            .is_err()
        );
    }
}
