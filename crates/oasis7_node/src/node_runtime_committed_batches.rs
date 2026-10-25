use crate::NodeCommittedActionBatchesHandle;
use std::time::Duration;

impl NodeCommittedActionBatchesHandle {
    pub fn wait_for_batches(&self, timeout: Duration) -> bool {
        let (batches_lock, signal) = &*self.state;
        let batches = batches_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !batches.is_empty() {
            return true;
        }
        let (batches, _) = signal
            .wait_timeout_while(batches, timeout, |pending| pending.is_empty())
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        !batches.is_empty()
    }
}
