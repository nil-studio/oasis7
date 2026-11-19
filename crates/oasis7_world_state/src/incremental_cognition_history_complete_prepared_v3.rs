//! Child of complete_v3. A linear pending value owns every final output charge
//! and copied payload before acceptance, but contains no accepted child token.
use super::*;
use crate::incremental_cognition_lifecycle_qualification_v2::{
    PreparedCognitionLifecyclePromotionV3, finish_prepared_cognition_lifecycle_promotion_v3,
    prepare_cognition_lifecycle_promotion_v3,
};
use oasis7_proto::memory_budget::MemoryReservation;
pub struct PreparedCompleteCognitionHistoryPromotionV3 {
    selected: PreparedCognitionHistoryViewPromotionV3,
    current: PreparedCognitionHistoryCurrentPromotionV3,
    indexes: [Option<PreparedCognitionHistoryIndexPromotionV3>; 4],
    heads: [Option<PreparedCognitionHeadSourcePromotionV3>; 3],
    lifecycle: Option<PreparedCognitionLifecyclePromotionV3>,
    disabled: Option<PreparedDisabledCognitionSourcePromotionV3>,
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    memory: MemoryBudget,
    charge: MemoryReservation,
    _metadata: MemoryReservation,
}
impl PreparedCompleteCognitionHistoryPromotionV3 {
    pub fn candidate_record_id(&self) -> [u8; 32] {
        self.record
    }
    pub fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.memory.same_pool(memory)
    }
}
pub fn prepare_complete_cognition_history_promotion_v3(
    checked: &ReservedValue<CheckedCognitionHistorySourceV3>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PreparedCompleteCognitionHistoryPromotionV3, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !checked.uses_budget(memory) || checked.reserved_bytes() < 4096 {
        return Err(invalid());
    }
    let metadata = memory.reserve(
        std::mem::size_of::<PreparedCompleteCognitionHistoryPromotionV3>(),
        MemoryClass::Ordinary,
    )?;
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let parts = &checked.parts;
    let selected = &parts.selected;
    let summary = selected.source_v3().ok_or_else(invalid)?;
    if !summary.uses_budget(memory)
        || summary.reserved_bytes() < COGNITION_HISTORY_SOURCE_SUMMARY_ADMISSION_V3_BYTES
    {
        return Err(invalid());
    }
    summary.validate_shape(selected.manifest(), selected.derived().ok_or_else(invalid)?)?;
    parts.current.validate_selected(selected)?;
    let prepared_selected =
        prepare_cognition_history_view_promotion_v3(selected, memory, &mut cancelled)?;
    let current =
        prepare_cognition_history_current_promotion_v3(&parts.current, memory, &mut cancelled)?;
    let mut indexes = std::array::from_fn(|_| None);
    for (slot, field) in CognitionHistoryFieldV1::ALL.into_iter().enumerate() {
        match (
            &parts.indexes[slot],
            selected.manifest().index_ref(field).present,
        ) {
            (Some(index), true) if index.field() == field => {
                indexes[slot] = Some(prepare_cognition_history_index_promotion_v3(
                    index,
                    selected,
                    memory,
                    &mut cancelled,
                )?);
            }
            (None, false) => {}
            _ => return Err(invalid()),
        }
    }
    let mut heads = std::array::from_fn(|_| None);
    let refs = selected.derived().ok_or_else(invalid)?;
    for (slot, field) in [
        CognitionHistoryFieldV1::JournalEvents,
        CognitionHistoryFieldV1::EconomyJournal,
        CognitionHistoryFieldV1::EconomyProvisionJournal,
    ]
    .into_iter()
    .enumerate()
    {
        match (&parts.heads[slot], refs.caches[slot].is_some()) {
            (Some(head), true) => {
                head.validate_selected(selected, field)?;
                heads[slot] = Some(prepare_cognition_head_source_promotion_v3(
                    head,
                    memory,
                    &mut cancelled,
                )?);
            }
            (None, false) => {}
            _ => return Err(invalid()),
        }
    }
    let (lifecycle, disabled) = match (
        selected.manifest().enabled,
        &parts.lifecycle,
        &parts.disabled,
    ) {
        (true, Some(lifecycle), None) => {
            lifecycle.validate_selected(selected)?;
            (
                Some(prepare_cognition_lifecycle_promotion_v3(
                    lifecycle,
                    memory,
                    &mut cancelled,
                )?),
                None,
            )
        }
        (false, None, Some(disabled)) => {
            disabled.validate_selected(selected)?;
            (
                None,
                Some(prepare_disabled_cognition_source_promotion_v3(
                    disabled,
                    memory,
                    &mut cancelled,
                )?),
            )
        }
        _ => return Err(invalid()),
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(PreparedCompleteCognitionHistoryPromotionV3 {
        selected: prepared_selected,
        current,
        indexes,
        heads,
        lifecycle,
        disabled,
        record: checked.candidate_record_id(),
        body: checked.body_id(),
        roots: checked.roots_id(),
        memory: memory.clone(),
        charge,
        _metadata: metadata,
    })
}
/// Pure fixed refinement. This deliberately takes no memory, reader, work or
/// cancellation handle. Legacy allocating promoters are never called here.
pub fn finish_prepared_complete_cognition_history_promotion_v3(
    prepared: PreparedCompleteCognitionHistoryPromotionV3,
    accepted: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<CompleteCognitionHistorySourceV3>, ValueTreeError> {
    if accepted.verified_record_id() != prepared.record {
        return Err(invalid());
    }
    let selected =
        finish_prepared_cognition_history_view_promotion_v3(prepared.selected, accepted)?;
    let current =
        finish_prepared_cognition_history_current_promotion_v3(prepared.current, accepted)?;
    let mut indexes = std::array::from_fn(|_| None);
    for (slot, index) in prepared.indexes.into_iter().enumerate() {
        if let Some(index) = index {
            indexes[slot] = Some(finish_prepared_cognition_history_index_promotion_v3(
                index, accepted,
            )?);
        }
    }
    let mut heads = std::array::from_fn(|_| None);
    for (slot, head) in prepared.heads.into_iter().enumerate() {
        if let Some(head) = head {
            heads[slot] = Some(finish_prepared_cognition_head_source_promotion_v3(
                head, accepted,
            )?);
        }
    }
    let lifecycle = prepared
        .lifecycle
        .map(|value| finish_prepared_cognition_lifecycle_promotion_v3(value, accepted))
        .transpose()?
        .map(CompleteLifecycleV3::Replayed);
    let disabled = prepared
        .disabled
        .map(|value| finish_prepared_disabled_cognition_source_promotion_v3(value, accepted))
        .transpose()?;
    Ok(prepared.charge.retain(CompleteCognitionHistorySourceV3 {
        selected,
        current,
        indexes,
        heads,
        lifecycle,
        disabled,
    }))
}
