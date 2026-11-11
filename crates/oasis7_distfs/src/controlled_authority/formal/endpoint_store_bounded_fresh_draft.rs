//! Verified disk snapshot admission for the real native Draft producers.
//! This is a point-in-time check, not an exclusive publication/CAS permit.
use super::super::FormalArchiveReadWorkV1;
use super::bounded_native_draft::PaidNativeCheckpointDraftV1;
use super::*;
use oasis7_proto::memory_budget::MemoryBudget;

fn disk_source(
    store: &Store,
    public_key: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidNativeHotV1, FormalError> {
    let source = store.read_hot_bounded(public_key, memory, work, stop)?;
    let mut hasher = blake3::Hasher::new();
    for chunk in source.bytes().chunks(8192) {
        work.charge(1, chunk.len() as u64, stop)?;
        hasher.update(chunk);
    }
    // Same binding definition as Store::binding_with, compared without
    // allocating a second binding/String. Both selected heads remain borrowed.
    let commitment = hasher.finalize().to_hex();
    let body = source.body();
    let head = body.decision_head.as_ref().unwrap_or(&body.head);
    let bytes = commitment
        .len()
        .checked_add(store.checkpoint.commitment.len())
        .and_then(|n| n.checked_add(head.record_id.len()))
        .and_then(|n| n.checked_add(store.checkpoint.head.decision_hash.len()))
        .ok_or_else(|| invalid("bounded disk Draft comparison layout"))?;
    work.charge(1, bytes as u64, stop)?;
    if body.revision != store.checkpoint.revision
        || commitment.as_str() != store.checkpoint.commitment
        || head.position != store.checkpoint.head.position
        || head.record_id != store.checkpoint.head.decision_hash
        || body.decision_head.is_none() != store.checkpoint.head.qualified
    {
        return Err(FormalError::Conflict(
            "bounded Draft disk checkpoint changed".into(),
        ));
    }
    Ok(source)
}

impl Store {
    pub(super) fn prepare_native_checkpoint_draft_from_disk_bounded<'a>(
        &'a self,
        public_key: &str,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidNativeCheckpointDraftV1<'a>, FormalError> {
        let source = disk_source(self, public_key, memory, work, stop)?;
        let mut ctx = work.bounded_context(memory, stop);
        let draft = self.prepare_native_checkpoint_draft_bounded(&mut ctx)?;
        Ok(draft.retain_disk_source(source))
    }

    pub(super) fn prepare_pending_native_checkpoint_draft_from_disk_bounded<'a>(
        &'a self,
        update: PreparedPendingRequestUpdate,
        public_key: &str,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidNativeCheckpointDraftV1<'a>, FormalError> {
        // Reject foreign ownership before disk IO; the producer also rechecks
        // its original source before consuming the actual retained objects.
        update.validate_source(self, memory, work, stop)?;
        let source = disk_source(self, public_key, memory, work, stop)?;
        let draft =
            self.prepare_pending_native_checkpoint_draft_bounded(update, memory, work, stop)?;
        Ok(draft.retain_disk_source(source))
    }
}

#[cfg(test)]
#[path = "endpoint_store_bounded_fresh_draft_tests.rs"]
mod tests;
