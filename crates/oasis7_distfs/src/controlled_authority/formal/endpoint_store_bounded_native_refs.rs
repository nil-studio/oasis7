//! Complete original metadata-reference checks, never publication authority.
use super::super::FormalArchiveReadWorkV1;
use super::bounded_native_draft::PaidNativeCheckpointDraftV1;
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::mem::size_of;
pub(in crate::controlled_authority) struct PaidNativeCheckpointRefsV1<'a> {
    draft: PaidNativeCheckpointDraftV1<'a>,
    outer: ReservedValue<()>,
}
impl std::fmt::Debug for PaidNativeCheckpointRefsV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidNativeCheckpointRefsV1")
            .finish_non_exhaustive()
    }
}
impl PaidNativeCheckpointRefsV1<'_> {
    pub(in crate::controlled_authority) fn draft(&self) -> &PaidNativeCheckpointDraftV1<'_> {
        &self.draft
    }
    pub(in crate::controlled_authority) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.draft.uses_budget(memory) && self.outer.uses_budget(memory)
    }
    pub(in crate::controlled_authority) fn reserved_bytes(&self) -> usize {
        self.draft.reserved_bytes() + self.outer.reserved_bytes()
    }
}
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("bounded native references layout"))
}

struct BoundedRefs<'a, 's> {
    store: &'a Store,
    memory: &'a MemoryBudget,
    work: &'a FormalArchiveReadWorkV1,
    stop: &'s mut dyn FnMut() -> bool,
}
impl BoundedRefs<'_, '_> {
    fn charge(&mut self, n: usize) -> Result<(), FormalError> {
        self.work.charge(
            1,
            u64::try_from(n).map_err(|_| invalid("bounded native references layout"))?,
            self.stop,
        )
    }
    fn raw(&mut self, id: &str) -> Result<super::super::FormalArchiveReadOwnerV1, FormalError> {
        self.store
            .raw_object_bounded(id, LIMIT, self.memory, self.work, self.stop)?
            .ok_or_else(|| invalid("referenced formal archive object missing"))
    }
}

impl super::native_refs::NativeReferenceValidationOps for BoundedRefs<'_, '_> {
    type Error = FormalError;
    type History = super::bounded_history_read::PaidHistoryNodeV1;
    type Closure = super::bounded_local_closure_read::PaidLocalClosureRecordV1;
    type Request = super::bounded_request::PaidRequestStateV1;
    type RecordId = super::bounded_record_id::PaidCanonicalRecordIdV1;
    fn history(&mut self, id: &str) -> Result<Self::History, FormalError> {
        super::bounded_history_read::read_history_object_bounded_v1(
            self.store,
            id,
            self.memory,
            self.work,
            self.stop,
        )
    }
    fn history_node(value: &Self::History) -> &HistoryNode {
        value.node()
    }
    fn closure(&mut self, id: &str) -> Result<Self::Closure, FormalError> {
        super::bounded_local_closure_read::read_local_closure_object_bounded_v1(
            self.store,
            id,
            self.memory,
            self.work,
            self.stop,
        )
    }
    fn closure_record(
        value: &Self::Closure,
    ) -> &super::super::incremental::local_closure::LocalDataClosureRecordV1 {
        value.record()
    }
    fn request(&mut self, id: &str) -> Result<Self::Request, FormalError> {
        let raw = self.raw(id)?;
        super::bounded_request::decode_request_bounded_v1(&raw, self.memory, self.work, self.stop)
    }
    fn request_state(value: &Self::Request) -> &FormalRequestStateV1 {
        value.state()
    }
    fn index(&mut self, id: &str) -> Result<(), FormalError> {
        let raw = self.raw(id)?;
        let _index =
            super::bounded_index::decode_index_bounded_v1(&raw, self.memory, self.work, self.stop)?;
        Ok(())
    }
    fn record_id(&mut self, record: &CanonicalRecordV1) -> Result<Self::RecordId, FormalError> {
        super::bounded_record_id::hash_canonical_record_bounded_v1(
            record,
            self.memory,
            self.work,
            self.stop,
        )
    }
    fn id(value: &Self::RecordId) -> &str {
        value.id()
    }
    fn before_hash_reference(&mut self, id: &str) -> Result<(), FormalError> {
        self.charge(id.len())
    }
    fn before_scope(
        &mut self,
        _record: &super::super::incremental::local_closure::LocalDataClosureRecordV1,
        body: &HotBody,
    ) -> Result<(), FormalError> {
        // Original scope is fixed stack32 decoding and fixed-field equality.
        // Charge all possible original scans before entering the original
        // predicate; no cold continuity or lease issuer is substituted here.
        let bytes = add(
            add(body.head.record_id.len(), body.instance_digest.len())?,
            add(body.authority_id.len(), body.activation_ref.len())?,
        )?;
        self.charge(add(bytes, 4 * 32 + size_of::<u64>() + size_of::<u32>())?)?;
        Ok(())
    }
    fn compare(&mut self, bytes: usize) -> Result<(), FormalError> {
        self.charge(bytes)
    }
}
pub(super) fn validate_native_checkpoint_refs_bounded_v1<'a>(
    draft: PaidNativeCheckpointDraftV1<'a>,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidNativeCheckpointRefsV1<'a>, FormalError> {
    {
        let mut ctx = work.bounded_context(memory, stop);
        draft.validate_context(&mut ctx)?;
    }
    let extra = size_of::<PaidNativeCheckpointRefsV1<'a>>()
        .checked_sub(size_of::<PaidNativeCheckpointDraftV1<'a>>())
        .ok_or_else(|| invalid("bounded native references layout"))?;
    let outer = memory
        .reserve(extra, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded native references owner admission"))?
        .retain(());
    // No formatted input diagnostics: these are all original fixed decision
    // strings plus fixed stack digest buffers and actual Error/String headers.
    let messages = [
        "local data closure requires local Qualified head",
        "formal nonzero head requires permanent request/history roots",
        "formal imported archive missing verified cut",
        "formal imported archive canonical cut",
        "formal head/node",
        "local data closure checkpoint/Qualified mismatch",
        "formal qualified head entry",
        "formal digest/key requires canonical lowercase hex32",
        "local data closure record/head",
        "local data closure protected domain",
        "local data closure identifier",
    ];
    let mut phase = add(4 * 32, add(size_of::<FormalError>(), size_of::<String>())?)?;
    for message in messages {
        phase = add(phase, message.len())?;
    }
    let _phase = memory
        .reserve(phase, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded native references phase admission"))?;
    super::native_refs::validate_refs_with(
        draft.body(),
        &mut BoundedRefs {
            store: draft.source_store(),
            memory,
            work,
            stop,
        },
    )?;
    Ok(PaidNativeCheckpointRefsV1 { draft, outer })
}
#[cfg(test)]
#[path = "endpoint_store_bounded_native_refs_tests.rs"]
mod tests;
