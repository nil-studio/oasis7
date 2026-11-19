//! Cold conjunction checks fixed v3 attestations and actual cache frontiers.
use super::*;
use crate::{
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_cognition_lifecycle_qualification_v2::readmit_attested_cognition_lifecycle_ordinary_v3,
};
pub struct RecoveredCognitionHistoryComponentsV3 {
    pub selected: ReservedValue<AcceptedCognitionHistoryViewV1>,
    pub current: ReservedValue<CognitionHistoryCurrentBindingV1>,
    pub indexes: [Option<ReservedValue<AttestedCognitionHistoryIndexV3>>; 4],
    pub heads: [Option<ReservedValue<AuthenticatedCognitionHeadSourceV1>>; 3],
    pub lifecycle: Option<ReservedValue<AttestedCognitionLifecycleV3>>,
    pub disabled: Option<ReservedValue<AcceptedDisabledCognitionHistorySourceV3>>,
}
pub fn recover_complete_cognition_history_source_v3(
    parts: RecoveredCognitionHistoryComponentsV3,
    memory: &MemoryBudget,
    work: &mut CognitionHeadHashWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CompleteCognitionHistorySourceV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    validate_accepted_v3_current(&parts.selected, &parts.current, memory)?;
    let selected = &parts.selected;
    for (slot, field) in CognitionHistoryFieldV1::ALL.into_iter().enumerate() {
        let declared = selected.manifest().index_ref(field);
        match (&parts.indexes[slot], declared.present) {
            (Some(index), true) => {
                if !index.uses_budget(memory)
                    || index.reserved_bytes() < 16 << 10
                    || index.allocation_class() != selected.class()
                    || index.field() != field
                    || index.root() != &declared.root
                    || index.record_count() != declared.records
                    || index.head_record_id() != selected.occurrence().head_record_id()
                    || index.roots_id() != selected.manifest().roots
                {
                    return Err(invalid());
                }
            }
            (None, false) => {}
            _ => return Err(invalid()),
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
    }
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
            (Some(head), true) => AuthenticatedCognitionHeadSourceV1::validate_accepted_source_v3(
                head,
                selected,
                field,
                memory,
                work,
                &mut cancelled,
            )?,
            (None, false) => {}
            _ => return Err(invalid()),
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
    }
    match (
        selected.manifest().enabled,
        &parts.lifecycle,
        &parts.disabled,
    ) {
        (true, Some(lifecycle), None) => {
            if !lifecycle.uses_budget(memory) || lifecycle.reserved_bytes() < 4096 {
                return Err(invalid());
            }
            lifecycle.validate_accepted(selected)?;
        }
        (false, None, Some(disabled)) => {
            if !disabled.uses_budget(memory)
                || disabled.reserved_bytes() < 4096
                || disabled.allocation_class() != selected.class()
                || disabled.head_record_id() != selected.occurrence().head_record_id()
                || disabled.roots_id() != selected.manifest().roots
            {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    }
    let charge = memory.reserve(4096, selected.class())?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(CompleteCognitionHistorySourceV3 {
        selected: parts.selected,
        current: parts.current,
        indexes: parts.indexes,
        heads: parts.heads,
        lifecycle: parts.lifecycle.map(CompleteLifecycleV3::Attested),
        disabled: parts.disabled,
    }))
}
/// Every retained Recovery carrier is copied with fresh Ordinary credit. The
/// caller supplies actual, independently admitted Ordinary selected/current
/// bindings for this same head; this function never retains a Recovery Arc.
pub fn readmit_complete_cognition_history_source_ordinary_v3(
    source: &ReservedValue<CompleteCognitionHistorySourceV3>,
    selected: ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: ReservedValue<CognitionHistoryCurrentBindingV1>,
    memory: &MemoryBudget,
    work: &mut CognitionHeadHashWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CompleteCognitionHistorySourceV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 4096
        || source.selected.class() != MemoryClass::Recovery
        || selected.class() != MemoryClass::Ordinary
        || source.selected.occurrence().head_record_id() != selected.occurrence().head_record_id()
        || source.selected.manifest() != selected.manifest()
        || source.selected.derived() != selected.derived()
    {
        return Err(invalid());
    }
    validate_accepted_v3_current(&selected, &current, memory)?;
    let mut indexes = std::array::from_fn(|_| None);
    for (slot, index) in source.indexes.iter().enumerate() {
        if let Some(index) = index {
            indexes[slot] = Some(readmit_attested_cognition_history_index_ordinary_v3(
                index,
                &selected,
                &current,
                memory,
                &mut cancelled,
            )?);
        }
    }
    let mut heads = std::array::from_fn(|_| None);
    for (slot, head) in source.heads.iter().enumerate() {
        if let Some(head) = head {
            heads[slot] = Some(readmit_authenticated_cognition_head_source_ordinary_v1(
                head,
                memory,
                &mut cancelled,
            )?);
        }
    }
    let lifecycle = match &source.lifecycle {
        Some(CompleteLifecycleV3::Attested(value)) => {
            Some(readmit_attested_cognition_lifecycle_ordinary_v3(
                value,
                &selected,
                &current,
                memory,
                &mut cancelled,
            )?)
        }
        None => None,
        _ => return Err(invalid()),
    };
    let disabled = source
        .disabled
        .as_ref()
        .map(|value| {
            readmit_disabled_cognition_history_source_ordinary_v3(value, memory, &mut cancelled)
        })
        .transpose()?;
    recover_complete_cognition_history_source_v3(
        RecoveredCognitionHistoryComponentsV3 {
            selected,
            current,
            indexes,
            heads,
            lifecycle,
            disabled,
        },
        memory,
        work,
        cancelled,
    )
}
