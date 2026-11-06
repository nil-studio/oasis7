//! Recover logical source attestation from an exact accepted v3 Result.
//! Physical availability and reader leases remain independent obligations.
use super::*;
use crate::incremental_cognition_history::attested_index_v3::validate_accepted_v3_current;

#[allow(
    clippy::too_many_arguments,
    reason = "Recovery verifies the selected field/view/current binding/cache/last-point against the caller's pool, finite hash work, and cancellation signal."
)]
pub fn recover_attested_cognition_head_source_v3(
    field: CognitionHistoryFieldV1,
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    caches: &ReservedValue<BoundPersistedCognitionHeadCachesV2>,
    last_point: Option<&ReservedValue<CognitionHistoryIndexPointV1>>,
    memory: &MemoryBudget,
    work: &mut CognitionHeadHashWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AuthenticatedCognitionHeadSourceV1>, ValueTreeError> {
    let invalid = || ValueTreeError::Invalid("accepted v3 head source");
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    validate_accepted_v3_current(view, current, memory)?;
    let slot = match field {
        CognitionHistoryFieldV1::JournalEvents => 0,
        CognitionHistoryFieldV1::EconomyJournal => 1,
        CognitionHistoryFieldV1::EconomyProvisionJournal => 2,
        CognitionHistoryFieldV1::Responses => return Err(invalid()),
    };
    let class = view.class();
    if !caches.uses_budget(memory)
        || caches.reserved_bytes() < 2048
        || caches.class() != class
        || caches.head_record_id() != view.occurrence().head_record_id()
        || caches.roots_id() != view.manifest().roots
    {
        return Err(invalid());
    }
    let declared = view.manifest().index_ref(field);
    if !declared.present {
        return Err(invalid());
    }
    validate_head_source_sequence(
        field,
        declared.records,
        view.manifest().heads[slot].sequence,
    )?;
    let summary = view.source_v3().ok_or_else(invalid)?;
    match (&summary.last[slot], last_point) {
        (Some(last), Some(point))
            if point.uses_budget(memory)
                && point.reserved_bytes() >= 16 << 10
                && point.class == class
                && point.field == field
                && point.root == declared.root
                && point.ordinal().checked_add(1) == Some(declared.records)
                && point.row().location == last.location
                && point.row().encoded_bytes == last.original_bytes => {}
        (None, None) if declared.records == 0 => {}
        _ => return Err(invalid()),
    }
    let cache = caches.cache(slot).ok_or_else(invalid)?;
    if !cache.uses_budget(memory)
        || cache.reserved_bytes() < 1024
        || !cache.state().uses_budget(memory)
        || cache.state().allocation_class() != class
    {
        return Err(invalid());
    }
    let mut digest = CacheDigest(blake3::Hasher::new());
    cache
        .state()
        .write_canonical_cache_v1(&mut digest, work, &mut cancelled)?;
    if Some(*digest.0.finalize().as_bytes()) != summary.cache_state_digests[slot] {
        return Err(invalid());
    }
    let charge = memory.reserve(4096, class)?;
    let state =
        CognitionHeadHashBuilderV1::append_to(cache.state(), memory, class, &mut cancelled)?
            .finish(&mut cancelled)?;
    let result = AuthenticatedCognitionHeadSourceV1 {
        state,
        field,
        head: view.occurrence().head_record_id(),
        roots: view.manifest().roots,
        records: declared.records,
        last: summary.last[slot].as_ref().map(|v| v.semantic_digest),
        last_record: summary.last[slot].clone(),
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}
