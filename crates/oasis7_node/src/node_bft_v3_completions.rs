//! Delivery after successful startup, retaining completion eligibility on failure.
use super::error;
use crate::{NodeCommittedActionBatch, NodeConsensusAction, NodeError};
use std::sync::{
    Condvar, Mutex,
    atomic::{AtomicUsize, Ordering},
};
pub(super) fn publish(
    cache: &Mutex<Vec<NodeCommittedActionBatch>>,
    pending: &Mutex<Vec<NodeConsensusAction>>,
    pending_bytes: &AtomicUsize,
    batches: &(Mutex<Vec<NodeCommittedActionBatch>>, Condvar),
    max_batches: usize,
    max_bytes: usize,
) -> Result<(), NodeError> {
    let mut cached = cache
        .lock()
        .map_err(|_| error("wire-3 recovery completion cache poisoned"))?;
    while let Some(batch) = cached.first().cloned() {
        let mut pending = pending
            .lock()
            .map_err(|_| error("wire-3 pending queue poisoned"))?;
        let mut published = batches
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::node_runtime_batch_retention::retain_committed_action_batch(
            &mut published,
            batch.clone(),
            max_batches,
            max_bytes,
        );
        if !published.contains(&batch) {
            return Err(error(
                "wire-3 recovered completion exceeds notification retention capacity",
            ));
        }
        pending.retain(|action| !batch.actions.contains(action));
        pending_bytes.store(
            crate::action_payload_bytes(pending.iter()),
            Ordering::Release,
        );
        cached.remove(0);
        drop(published);
        batches.1.notify_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn batch() -> NodeCommittedActionBatch {
        NodeCommittedActionBatch {
            height: 3,
            slot: 0,
            epoch: 1,
            block_hash: "a".repeat(64),
            action_root: "b".repeat(64),
            committed_at_unix_ms: 10,
            actions: vec![NodeConsensusAction::from_payload(1, "reader", vec![1, 2, 3]).unwrap()],
        }
    }
    #[test]
    fn failed_delivery_retains_exact_queue_and_completion_until_actual_enqueue() {
        let original = batch();
        let cache = Mutex::new(vec![original.clone()]);
        let pending = Mutex::new(original.actions.clone());
        let bytes = AtomicUsize::new(3);
        let output = (Mutex::new(Vec::new()), Condvar::new());
        assert!(publish(&cache, &pending, &bytes, &output, 4, 2).is_err());
        assert_eq!(*cache.lock().unwrap(), vec![original.clone()]);
        assert_eq!(*pending.lock().unwrap(), original.actions);
        assert_eq!(bytes.load(Ordering::Acquire), 3);
        assert!(output.0.lock().unwrap().is_empty());
        publish(&cache, &pending, &bytes, &output, 4, 100).unwrap();
        assert!(cache.lock().unwrap().is_empty());
        assert!(pending.lock().unwrap().is_empty());
        assert_eq!(bytes.load(Ordering::Acquire), 0);
        assert_eq!(*output.0.lock().unwrap(), vec![original]);
        output.0.lock().unwrap().clear();
        publish(&cache, &pending, &bytes, &output, 4, 100).unwrap();
        assert!(output.0.lock().unwrap().is_empty());
    }
    #[test]
    fn failed_pending_lock_does_not_forget_already_confirmed_completion() {
        let original = batch();
        let cache = Mutex::new(vec![original.clone()]);
        let pending = std::sync::Arc::new(Mutex::new(original.actions.clone()));
        let poison = pending.clone();
        let _ = std::thread::spawn(move || {
            let _held = poison.lock().unwrap();
            panic!("injected pending lock failure");
        })
        .join();
        let output = (Mutex::new(Vec::new()), Condvar::new());
        assert!(publish(&cache, &pending, &AtomicUsize::new(3), &output, 4, 100).is_err());
        assert_eq!(*cache.lock().unwrap(), vec![original]);
        assert!(output.0.lock().unwrap().is_empty());
    }
}
