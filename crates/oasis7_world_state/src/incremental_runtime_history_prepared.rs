//! Prepaid Ordinary promotion of an already checked Runtime History source.
//! Included as a private child of incremental_runtime_history_source. This
//! token is not accepted authority; only a genuine matching Formal commit can
//! refine its prepared fact. Cold recovery remains a separate IO/admission path.
use super::*;
use oasis7_formal_protocol::incremental_material::{
    PreparedFactPromotionV1, finish_prepared_fact_promotion_v1, prepare_candidate_fact_promotion_v1,
};
use oasis7_proto::memory_budget::MemoryReservation;

/// Owns all future accepted carriers before Formal acceptance. The existing
/// four Ordinary index outputs never share Recovery owners.
pub struct PreparedRuntimeHistorySourceV1 {
    manifest: RuntimeHistoryManifestV1,
    selected: PreparedFactPromotionV1,
    indexes: [ReservedValue<RuntimeHistoryIndexQualificationV1>; 4],
    output_charge: MemoryReservation,
}

/// The checked source already seals the actual Core parent, optional certified
/// prefix branch, index lineage, typed originals and legal window transition.
/// This step cannot substitute a caller-reported parent or re-run a count-only
/// shortcut: it retains exactly that private source's four actual index views.
/// All validation, encoding/hash work, cloning and admission finish here.
pub fn prepare_runtime_history_source_promotion_v1(
    checked: &ReservedValue<CheckedRuntimeHistorySourceV1>,
    indexes: [ReservedValue<RuntimeHistoryIndexQualificationV1>; 4],
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PreparedRuntimeHistorySourceV1, RuntimeHistorySourceErrorV1> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !checked.uses_budget(memory)
        || checked.reserved_bytes() < CARRIER
        || !checked.selected.uses_budget(memory)
        || checked.selected.reserved_bytes()
            < std::mem::size_of::<CheckedCandidateFactOccurrenceV1>()
        || checked.selected.kind() != ExecutionFactKindV1::Result
        || checked.selected.roots_id() != checked.manifest.roots
        || checked.manifest.previous_view.is_none() != (checked.selected.location().height == 0)
    {
        return Err(invalid().into());
    }
    checked.manifest.validate()?;
    let encoded = checked.manifest.canonical_bytes()?;
    if checked.selected.location().original != *blake3::hash(&encoded).as_bytes()
        || checked.selected.original_bytes() != encoded.len() as u64
    {
        return Err(invalid().into());
    }
    for lane in RuntimeHistoryLaneV1::ALL {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let index = &indexes[lane.index()];
        if !index.uses_budget(memory)
            || index.reserved_bytes()
                < std::mem::size_of::<RuntimeHistoryIndexQualificationV1>() + 512
            || index.allocation_class() != MemoryClass::Ordinary
            || index.lane() != lane
            || index.window() != &checked.manifest.lanes[lane.index()]
        {
            return Err(invalid().into());
        }
        // Lineage is private evidence set by the actual index append builder.
        // Parent identity/transition is sealed by the checked source, including
        // rollback prefix branches. Do not reinterpret it as a fresh full Q.
        if let Some((parent_root, parent_total)) = index.lineage() {
            parent_root.validate().map_err(|_| invalid())?;
            if parent_root.namespace != index.window().root.namespace
                || parent_root.leaf_count != parent_total.checked_add(2).ok_or_else(invalid)?
                || parent_total >= index.window().total
            {
                return Err(invalid().into());
            }
        }
    }
    let tick = &checked.manifest.lanes[RuntimeHistoryLaneV1::Tick.index()];
    if tick.start != checked.manifest.archived_tick_count || tick.end()? != tick.total {
        return Err(invalid().into());
    }
    // Detach the next append baseline from this commit's private delta lineage.
    // Every replacement owns an admitted clone before acceptance; finish must
    // never clone, re-read or reset a proof after the accepted head is published.
    let indexes = [
        prepare_runtime_history_index_promotion_v1(&indexes[0], memory, &mut cancelled)?,
        prepare_runtime_history_index_promotion_v1(&indexes[1], memory, &mut cancelled)?,
        prepare_runtime_history_index_promotion_v1(&indexes[2], memory, &mut cancelled)?,
        prepare_runtime_history_index_promotion_v1(&indexes[3], memory, &mut cancelled)?,
    ];
    // CARRIER covers the accepted struct and the four validated 64-byte root
    // String clones. Existing index carriers retain their own complete charges.
    let output_charge = memory
        .reserve(CARRIER, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let selected = prepare_candidate_fact_promotion_v1(&checked.selected, memory, &mut cancelled)
        .map_err(RuntimeHistorySourceErrorV1::Formal)?;
    let manifest = checked.manifest.clone();
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(PreparedRuntimeHistorySourceV1 {
        manifest,
        selected,
        indexes,
        output_charge,
    })
}

/// Fixed-field refinement only. No memory admission, reads, cancellation,
/// manifest encoding/hash, cloning or Recovery-to-Ordinary conversion occurs.
/// A mismatched genuine accepted record is rejected and consumes the token.
pub fn finish_prepared_runtime_history_source_promotion_v1(
    prepared: PreparedRuntimeHistorySourceV1,
    accepted: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<AcceptedRuntimeHistorySourceV1>, RuntimeHistorySourceErrorV1> {
    let PreparedRuntimeHistorySourceV1 {
        manifest,
        selected,
        indexes,
        output_charge,
    } = prepared;
    let selected = finish_prepared_fact_promotion_v1(selected, accepted)
        .map_err(RuntimeHistorySourceErrorV1::Formal)?;
    Ok(output_charge.retain(AcceptedRuntimeHistorySourceV1 {
        manifest,
        selected,
        indexes,
        class: MemoryClass::Ordinary,
    }))
}
