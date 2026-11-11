//! Complete paid RequestState data update, before any native Hot publication.
use super::super::FormalArchiveReadWorkV1;
use super::bounded_index::copy_paid;
use super::bounded_insert_write::RetainedIndexRewriteV1;
use super::bounded_request::PaidRequestStateV1;
use super::*;
use crate::controlled_authority::formal::physical;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub(in crate::controlled_authority::formal::endpoint) struct PreparedPendingRequestUpdate {
    request: ReservedValue<String>,
    entry: ReservedValue<String>,
    index: RetainedIndexRewriteV1,
    before_checkpoint: ReservedValue<fencing::EndpointCheckpointBindingV1>,
    identity: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
}
impl std::fmt::Debug for PreparedPendingRequestUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedPendingRequestUpdate")
            .field("request", &self.request.as_str())
            .field("entry", &self.entry.as_str())
            .field("index", &self.index.id())
            .finish_non_exhaustive()
    }
}
impl PreparedPendingRequestUpdate {
    pub(super) fn validate_source(
        &self,
        store: &Store,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        work.consume_steps(1, stop)?;
        if !self.uses_budget(memory)
            || self.identity != work.bounded_context(memory, stop).work().identity()
        {
            return Err(invalid("bounded RequestState update foreign context"));
        }
        self.index.validate_source_directory(store, work, stop)?;
        let bytes = self
            .before_checkpoint
            .commitment
            .len()
            .checked_add(self.before_checkpoint.head.decision_hash.len())
            .and_then(|n| n.checked_add(store.checkpoint.commitment.len()))
            .and_then(|n| n.checked_add(store.checkpoint.head.decision_hash.len()))
            .and_then(|n| n.checked_add(self.before_index().map_or(0, str::len)))
            .and_then(|n| n.checked_add(store.body.index.as_ref().map_or(0, String::len)))
            .ok_or_else(|| invalid("bounded RequestState source layout"))?;
        work.charge(
            1,
            u64::try_from(bytes).map_err(|_| invalid("bounded RequestState source layout"))?,
            stop,
        )?;
        if *self.before_checkpoint != store.checkpoint
            || self.before_index() != store.body.index.as_deref()
        {
            return Err(FormalError::Conflict(
                "bounded RequestState source changed".into(),
            ));
        }
        Ok(())
    }
    pub(in crate::controlled_authority::formal::endpoint) fn request_digest(&self) -> &str {
        &self.request
    }
    pub(in crate::controlled_authority::formal::endpoint) fn entry_id(&self) -> &str {
        &self.entry
    }
    pub(in crate::controlled_authority::formal::endpoint) fn index_root(&self) -> &str {
        self.index.id()
    }
    pub(in crate::controlled_authority::formal::endpoint) fn before_index(&self) -> Option<&str> {
        self.index.before_index()
    }
    pub(in crate::controlled_authority::formal::endpoint) fn uses_budget(
        &self,
        memory: &MemoryBudget,
    ) -> bool {
        self.request.uses_budget(memory)
            && self.entry.uses_budget(memory)
            && self.index.uses_budget(memory)
            && self.before_checkpoint.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal::endpoint) fn reserved_bytes(&self) -> usize {
        self.request.reserved_bytes()
            + self.entry.reserved_bytes()
            + self.index.reserved_bytes()
            + self.before_checkpoint.reserved_bytes()
    }
}
pub(super) fn prepare_request_update_bounded_v1(
    store: &Store,
    source: &PaidRequestStateV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PreparedPendingRequestUpdate, FormalError> {
    work.consume_steps(1, stop)?;
    if !source.uses_budget(memory) {
        return Err(invalid("bounded RequestState update foreign pool"));
    }
    let identity = work.bounded_context(memory, stop).work().identity();
    let payload = store
        .checkpoint
        .commitment
        .len()
        .checked_add(store.checkpoint.head.decision_hash.len())
        .ok_or_else(|| invalid("bounded RequestState checkpoint layout"))?;
    work.charge(
        1,
        u64::try_from(payload).map_err(|_| invalid("bounded RequestState checkpoint layout"))?,
        stop,
    )?;
    let layout = std::mem::size_of::<ReservedValue<fencing::EndpointCheckpointBindingV1>>()
        .checked_add(std::mem::size_of_val(&identity))
        .and_then(|n| n.checked_add(payload))
        .ok_or_else(|| invalid("bounded RequestState checkpoint layout"))?;
    let before_fee = memory
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded RequestState checkpoint admission"))?;
    let before = store.checkpoint.clone();
    if before.commitment.capacity() > before.commitment.len()
        || before.head.decision_hash.capacity() > before.head.decision_hash.len()
    {
        return Err(invalid("bounded RequestState checkpoint capacity"));
    }
    let before_checkpoint = before_fee.retain(before);
    let digest = source.state().request_digest();
    work.consume_read_bytes(
        u64::try_from(digest.len()).map_err(|_| invalid("bounded RequestState digest layout"))?,
        stop,
    )?;
    hash_hex(digest)?;
    let request = copy_paid(digest, memory, work, stop)?;
    let entry = {
        work.consume_steps(1, stop)?;
        let cap = store
            .path
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .checked_add(1 + "formal-objects-v1".len())
            .ok_or_else(|| invalid("bounded RequestState directory layout"))?;
        let fee = memory
            .reserve(
                cap.checked_add(std::mem::size_of::<PathBuf>())
                    .ok_or_else(|| invalid("bounded RequestState directory layout"))?,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("bounded RequestState directory admission"))?;
        let mut path = PathBuf::with_capacity(cap);
        if path.capacity() > cap {
            return Err(invalid("bounded RequestState directory capacity"));
        }
        path.push(&store.path);
        path.push("formal-objects-v1");
        if path.capacity() > cap {
            return Err(invalid("bounded RequestState directory growth"));
        }
        let directory = fee.retain(path);
        let guard = super::stage_io::guard_initialized(&directory, memory, work, stop)?;
        let node =
            super::stage_io::stage_request_state_bounded_v1(&guard, source, memory, work, stop)?;
        let receipt = physical::roots::append_staged_request_bounded_v1(&node, memory, work, stop)?;
        let id = copy_paid(node.id(), memory, work, stop)?;
        // Retention is complete before the only digest copy outlives the wire.
        // Release this publication lease before the Index writer takes its own.
        drop(receipt);
        drop(node);
        drop(guard);
        drop(directory);
        id
    };
    work.consume_steps(1, stop)?;
    let plan = store.prepare_index_insert_bounded(&request, &entry, memory, work, stop)?;
    let index = store.write_index_insert_plan_bounded(plan, memory, work, stop)?;
    work.consume_steps(1, stop)?;
    Ok(PreparedPendingRequestUpdate {
        before_checkpoint,
        identity,
        request,
        entry,
        index,
    })
}
#[cfg(test)]
#[path = "endpoint_store_bounded_request_write_tests.rs"]
mod tests;

impl PreparedPendingRequestUpdate {
    pub(super) fn publication_guard(&self) -> &ReservedValue<physical::PublicationGuard> {
        self.index.publication_guard()
    }
}
