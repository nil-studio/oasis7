//! Complete Cognition source conjunction only. Runtime History, current World
//! families and physical leases are independent required obligations.
use super::*;
use crate::incremental_cognition_lifecycle_qualification_v2::{
    AttestedCognitionLifecycleV3, CheckedCognitionLifecycleV3, CognitionLifecycleParentSourceV3,
    CognitionLifecycleQualificationV2, promote_checked_cognition_lifecycle_v3,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1, incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub struct CheckedCognitionHistoryComponentsV3 {
    pub selected: ReservedValue<CheckedCognitionHistoryViewV1>,
    pub current: ReservedValue<CheckedCognitionHistoryCurrentBindingV3>,
    pub indexes: [Option<ReservedValue<CheckedCognitionHistoryIndexSourceV3>>; 4],
    pub heads: [Option<ReservedValue<CheckedCandidateHeadSourceV3>>; 3],
    pub lifecycle: Option<ReservedValue<CheckedCognitionLifecycleV3>>,
    pub disabled: Option<ReservedValue<CheckedDisabledCognitionHistorySourceV3>>,
}
pub struct CheckedCognitionHistorySourceV3 {
    parts: CheckedCognitionHistoryComponentsV3,
}
impl CheckedCognitionHistorySourceV3 {
    pub fn selected(&self) -> &ReservedValue<CheckedCognitionHistoryViewV1> {
        &self.parts.selected
    }
    pub fn current(&self) -> &ReservedValue<CheckedCognitionHistoryCurrentBindingV3> {
        &self.parts.current
    }
    pub fn candidate_record_id(&self) -> [u8; 32] {
        self.parts.selected.occurrence().candidate_record_id()
    }
    pub fn body_id(&self) -> [u8; 32] {
        self.parts.selected.occurrence().body_id()
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.parts.selected.manifest().roots
    }
    pub fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
}
pub struct CompleteCognitionHistorySourceV3 {
    selected: ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: ReservedValue<CognitionHistoryCurrentBindingV1>,
    indexes: [Option<ReservedValue<AttestedCognitionHistoryIndexV3>>; 4],
    heads: [Option<ReservedValue<AuthenticatedCognitionHeadSourceV1>>; 3],
    lifecycle: Option<CompleteLifecycleV3>,
    disabled: Option<ReservedValue<AcceptedDisabledCognitionHistorySourceV3>>,
}
enum CompleteLifecycleV3 {
    Replayed(ReservedValue<CognitionLifecycleQualificationV2>),
    Attested(ReservedValue<AttestedCognitionLifecycleV3>),
}
#[path = "incremental_cognition_history_complete_cold_v3.rs"]
mod cold_v3;
pub use cold_v3::*;
impl CompleteCognitionHistorySourceV3 {
    pub fn selected(&self) -> &ReservedValue<AcceptedCognitionHistoryViewV1> {
        &self.selected
    }
    pub fn current(&self) -> &ReservedValue<CognitionHistoryCurrentBindingV1> {
        &self.current
    }
    pub fn indexes(&self) -> &[Option<ReservedValue<AttestedCognitionHistoryIndexV3>>; 4] {
        &self.indexes
    }
    pub fn heads(&self) -> &[Option<ReservedValue<AuthenticatedCognitionHeadSourceV1>>; 3] {
        &self.heads
    }
    pub fn lifecycle(&self) -> Option<&ReservedValue<CognitionLifecycleQualificationV2>> {
        match &self.lifecycle {
            Some(CompleteLifecycleV3::Replayed(value)) => Some(value),
            _ => None,
        }
    }
    pub fn attested_lifecycle(&self) -> Option<&ReservedValue<AttestedCognitionLifecycleV3>> {
        match &self.lifecycle {
            Some(CompleteLifecycleV3::Attested(value)) => Some(value),
            _ => None,
        }
    }
    pub fn lifecycle_parent(&self) -> Option<CognitionLifecycleParentSourceV3<'_>> {
        self.lifecycle.as_ref().map(|value| match value {
            CompleteLifecycleV3::Replayed(value) => {
                CognitionLifecycleParentSourceV3::Replayed(value)
            }
            CompleteLifecycleV3::Attested(value) => {
                CognitionLifecycleParentSourceV3::Attested(value)
            }
        })
    }
    pub fn disabled(&self) -> Option<&ReservedValue<AcceptedDisabledCognitionHistorySourceV3>> {
        self.disabled.as_ref()
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("complete Cognition history source")
}
pub fn qualify_checked_cognition_history_source_v3(
    parts: CheckedCognitionHistoryComponentsV3,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedCognitionHistorySourceV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let selected = &parts.selected;
    let summary = selected.source_v3().ok_or_else(invalid)?;
    if !selected.uses_budget(memory)
        || selected.reserved_bytes() < 16 << 10
        || !summary.uses_budget(memory)
        || summary.reserved_bytes() < COGNITION_HISTORY_SOURCE_SUMMARY_ADMISSION_V3_BYTES
        || !parts.current.uses_budget(memory)
        || parts.current.reserved_bytes() < 8192
    {
        return Err(invalid());
    }
    summary.validate_shape(selected.manifest(), selected.derived().ok_or_else(invalid)?)?;
    parts.current.validate_selected(selected)?;
    for (slot, field) in CognitionHistoryFieldV1::ALL.into_iter().enumerate() {
        let declared = selected.manifest().index_ref(field);
        match (&parts.indexes[slot], declared.present) {
            (Some(index), true) => {
                if !index.uses_budget(memory)
                    || index.reserved_bytes() < 16 << 10
                    || index.field() != field
                    || index.root() != &declared.root
                    || index.record_count() != declared.records
                    || index.candidate_record_id() != selected.occurrence().candidate_record_id()
                    || index.body_id() != selected.occurrence().body_id()
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
            (Some(head), true) => {
                if !head.uses_budget(memory) || head.reserved_bytes() < 4096 {
                    return Err(invalid());
                }
                head.validate_selected(selected, field)?;
            }
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
            if !lifecycle.uses_budget(memory) || lifecycle.reserved_bytes() < 16 << 10 {
                return Err(invalid());
            }
            lifecycle.validate_selected(selected)?;
        }
        (false, None, Some(disabled)) => {
            if !disabled.uses_budget(memory) || disabled.reserved_bytes() < 4096 {
                return Err(invalid());
            }
            disabled.validate_selected(selected)?;
        }
        _ => return Err(invalid()),
    }
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(CheckedCognitionHistorySourceV3 { parts }))
}
pub fn promote_complete_cognition_history_source_v3(
    checked: &ReservedValue<CheckedCognitionHistorySourceV3>,
    accepted: &VerifiedFormalCommitV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CompleteCognitionHistorySourceV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !checked.uses_budget(memory) || checked.reserved_bytes() < 4096 {
        return Err(invalid());
    }
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let parts = &checked.parts;
    let selected =
        promote_cognition_history_view_v1(&parts.selected, accepted, memory, &mut cancelled)
            .map_err(selection_error)?;
    let current =
        promote_cognition_history_current_v3(&parts.current, accepted, memory, &mut cancelled)?;
    let mut indexes = std::array::from_fn(|_| None);
    for (slot, index) in parts.indexes.iter().enumerate() {
        if let Some(index) = index {
            indexes[slot] = Some(promote_checked_cognition_history_index_v3(
                index,
                accepted,
                memory,
                &mut cancelled,
            )?);
        }
    }
    let mut heads = std::array::from_fn(|_| None);
    for (slot, head) in parts.heads.iter().enumerate() {
        if let Some(head) = head {
            heads[slot] = Some(promote_checked_cognition_head_source_v3(
                head,
                accepted,
                memory,
                &mut cancelled,
            )?);
        }
    }
    let lifecycle = parts
        .lifecycle
        .as_ref()
        .map(|proof| {
            promote_checked_cognition_lifecycle_v3(proof, accepted, memory, &mut cancelled)
        })
        .transpose()?;
    let disabled = parts
        .disabled
        .as_ref()
        .map(|proof| {
            promote_disabled_cognition_history_source_v3(proof, accepted, memory, &mut cancelled)
        })
        .transpose()?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(CompleteCognitionHistorySourceV3 {
        selected,
        current,
        indexes,
        heads,
        lifecycle: lifecycle.map(CompleteLifecycleV3::Replayed),
        disabled,
    }))
}

fn selection_error(
    error: CognitionHistorySelectionErrorV1<std::convert::Infallible>,
) -> ValueTreeError {
    use oasis7_formal_protocol::{
        incremental_execution::ExecutionError, incremental_material::CoreError,
    };
    match error {
        CognitionHistorySelectionErrorV1::Value(error) => error,
        CognitionHistorySelectionErrorV1::Original(never) => match never {},
        CognitionHistorySelectionErrorV1::Formal(error)
        | CognitionHistorySelectionErrorV1::Provenance(error) => match error {
            CoreError::Cancelled => ValueTreeError::Cancelled,
            CoreError::ResourceLimited(reason)
            | CoreError::Execution(ExecutionError::ResourceLimited(reason)) => {
                ValueTreeError::ResourceLimited(reason)
            }
            CoreError::Execution(ExecutionError::Memory(error)) => ValueTreeError::Memory(error),
            _ => invalid(),
        },
    }
}
