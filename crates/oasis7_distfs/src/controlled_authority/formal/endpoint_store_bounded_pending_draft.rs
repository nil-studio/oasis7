//! Real retained RequestState/Index update into the sole native draft producer.
use super::super::FormalArchiveReadWorkV1;
use super::bounded_native_draft::PaidNativeCheckpointDraftV1;
use super::bounded_request_write::PreparedPendingRequestUpdate;
use super::*;
use oasis7_proto::memory_budget::MemoryBudget;
pub(super) fn prepare_pending_native_checkpoint_draft_bounded_v1<'a>(
    store: &'a Store,
    update: PreparedPendingRequestUpdate,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidNativeCheckpointDraftV1<'a>, FormalError> {
    update.validate_source(store, memory, work, stop)?;
    let raw = store
        .raw_object_bounded(update.entry_id(), LIMIT, memory, work, stop)?
        .ok_or_else(|| invalid("referenced formal archive object missing"))?;
    let entry = super::bounded_request::decode_request_bounded_v1(&raw, memory, work, stop)?;
    let bytes = entry
        .state()
        .request_digest()
        .len()
        .checked_add(update.request_digest().len())
        .ok_or_else(|| invalid("bounded pending draft request layout"))?;
    work.charge(
        1,
        u64::try_from(bytes).map_err(|_| invalid("bounded pending draft request layout"))?,
        stop,
    )?;
    if entry.state().request_digest() != update.request_digest() {
        return Err(invalid("bounded pending draft request binding"));
    }
    let decision = match entry.state() {
        FormalRequestStateV1::Qualified { .. } => {
            return Err(invalid(
                "bounded pending draft Qualified history producer required",
            ));
        }
        FormalRequestStateV1::Decided { candidate, .. }
        | FormalRequestStateV1::Ready { candidate, .. } => {
            Some(super::bounded_record_id::hash_canonical_record_bounded_v1(
                &candidate.candidate.record,
                memory,
                work,
                stop,
            )?)
        }
        FormalRequestStateV1::Frozen { .. } | FormalRequestStateV1::Prepared { .. } => None,
    };
    drop(raw);
    super::bounded_native_draft::prepare_pending_draft_from_sources_v1(
        store, update, entry, decision, memory, work, stop,
    )
}
#[cfg(test)]
#[path = "endpoint_store_bounded_pending_draft_tests.rs"]
mod tests;
