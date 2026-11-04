//! Explicit disabled source identity, never a substitute for World readiness.
use super::*;
use crate::incremental_cognition_absence::CognitionCurrentAbsenceQualificationV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::ExecutionFactKindV1,
    incremental_material::{
        CandidateFactRequestV1, CheckedCandidateFactOccurrenceV1, FactLocationV1,
        check_candidate_fact_v1, promote_candidate_fact_v1,
    },
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub struct CheckedDisabledCognitionHistorySourceV3 {
    occurrence: ReservedValue<CheckedCandidateFactOccurrenceV1>,
    revision: u64,
}
impl CheckedDisabledCognitionHistorySourceV3 {
    pub(crate) fn validate_selected(
        &self,
        selected: &CheckedCognitionHistoryViewV1,
    ) -> Result<(), ValueTreeError> {
        if selected.manifest().enabled
            || self.revision != selected.manifest().revision
            || self.occurrence.candidate_record_id() != selected.occurrence().candidate_record_id()
            || self.occurrence.body_id() != selected.occurrence().body_id()
            || self.occurrence.roots_id() != selected.manifest().roots
            || self.occurrence.location() != selected.occurrence().location()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub struct AcceptedDisabledCognitionHistorySourceV3 {
    head: [u8; 32],
    roots: [u8; 32],
    body: [u8; 32],
    location: FactLocationV1,
    revision: u64,
    class: MemoryClass,
}
impl AcceptedDisabledCognitionHistorySourceV3 {
    pub fn head_record_id(&self) -> [u8; 32] {
        self.head
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("disabled v3 source binding")
}
pub fn check_candidate_disabled_cognition_history_source_v3(
    request: &CognitionHistorySelectionRequestV1<'_>,
    selected: &ReservedValue<CheckedCognitionHistoryViewV1>,
    absence: &ReservedValue<CognitionCurrentAbsenceQualificationV1>,
    parent: Option<&ReservedValue<AcceptedDisabledCognitionHistorySourceV3>>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedDisabledCognitionHistorySourceV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let summary = selected.source_v3().ok_or_else(invalid)?;
    if !selected.uses_budget(memory)
        || selected.reserved_bytes() < 16 << 10
        || selected.manifest().enabled
        || !summary.uses_budget(memory)
        || summary.reserved_bytes() < COGNITION_HISTORY_SOURCE_SUMMARY_ADMISSION_V3_BYTES
        || !absence.uses_budget(memory)
        || absence.reserved_bytes() < 1024
        || absence.class() != MemoryClass::Ordinary
    {
        return Err(invalid());
    }
    summary.validate_shape(selected.manifest(), selected.derived().ok_or_else(invalid)?)?;
    absence.validate_binding(&selected.manifest().roots)?;
    match (summary.mode, parent) {
        (CognitionHistorySourceModeV3::Bootstrap, None)
            if request.core.body().height == 0
                && request.core.parent_record_id().is_none()
                && selected.manifest().revision == 0
                && selected.manifest().previous_view.is_none() => {}
        (CognitionHistorySourceModeV3::Delta, Some(parent)) => {
            if !parent.uses_budget(memory)
                || parent.reserved_bytes() < 4096
                || parent.class != MemoryClass::Ordinary
                || request.core.parent_record_id() != Some(parent.head)
                || request.core.body().parent_commit != Some(parent.body)
                || request
                    .core
                    .parent_body()
                    .is_none_or(|body| body.roots != parent.roots)
                || selected.manifest().previous_view != Some(parent.location)
                || parent.revision.checked_add(1) != Some(selected.manifest().revision)
            {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    }
    let occurrence = check_candidate_fact_v1(
        &CandidateFactRequestV1 {
            kind: ExecutionFactKindV1::Result,
            candidate: request.candidate,
            core: request.core,
            originals: request.originals,
            ordinal: selected.occurrence().location().ordinal,
        },
        memory,
        &mut cancelled,
    )
    .map_err(source_error)?;
    if occurrence.candidate_record_id() != selected.occurrence().candidate_record_id()
        || occurrence.body_id() != selected.occurrence().body_id()
        || occurrence.roots_id() != selected.manifest().roots
        || occurrence.location() != selected.occurrence().location()
    {
        return Err(invalid());
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(CheckedDisabledCognitionHistorySourceV3 {
        occurrence,
        revision: selected.manifest().revision,
    }))
}
pub fn promote_disabled_cognition_history_source_v3(
    source: &ReservedValue<CheckedDisabledCognitionHistorySourceV3>,
    accepted: &VerifiedFormalCommitV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AcceptedDisabledCognitionHistorySourceV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory) || source.reserved_bytes() < 4096 {
        return Err(invalid());
    }
    let occurrence =
        promote_candidate_fact_v1(&source.occurrence, accepted, memory, &mut cancelled)
            .map_err(source_error)?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.head_body_id() != occurrence.fact_body_id()
    {
        return Err(invalid());
    }
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let result = AcceptedDisabledCognitionHistorySourceV3 {
        head: occurrence.head_record_id(),
        roots: occurrence.roots_id(),
        body: occurrence.head_body_id(),
        location: occurrence.location(),
        revision: source.revision,
        class: MemoryClass::Ordinary,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}
pub fn recover_disabled_cognition_history_source_v3(
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AcceptedDisabledCognitionHistorySourceV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    super::attested_index_v3::validate_accepted_v3_current(view, current, memory)?;
    if current.enabled() {
        return Err(invalid());
    }
    let charge = memory.reserve(4096, view.class())?;
    let result = AcceptedDisabledCognitionHistorySourceV3 {
        head: view.occurrence().head_record_id(),
        roots: view.manifest().roots,
        body: view.occurrence().head_body_id(),
        location: view.occurrence().location(),
        revision: view.manifest().revision,
        class: view.class(),
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}
pub fn readmit_disabled_cognition_history_source_ordinary_v3(
    source: &ReservedValue<AcceptedDisabledCognitionHistorySourceV3>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AcceptedDisabledCognitionHistorySourceV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 4096
        || source.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let result = AcceptedDisabledCognitionHistorySourceV3 {
        head: source.head,
        roots: source.roots,
        body: source.body,
        location: source.location,
        revision: source.revision,
        class: MemoryClass::Ordinary,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}
fn source_error(
    error: oasis7_formal_protocol::incremental_material::CoreError<std::convert::Infallible>,
) -> ValueTreeError {
    use oasis7_formal_protocol::{
        incremental_execution::ExecutionError, incremental_material::CoreError,
    };
    match error {
        CoreError::Cancelled => ValueTreeError::Cancelled,
        CoreError::ResourceLimited(reason)
        | CoreError::Execution(ExecutionError::ResourceLimited(reason)) => {
            ValueTreeError::ResourceLimited(reason)
        }
        CoreError::Execution(ExecutionError::Memory(error)) => ValueTreeError::Memory(error),
        _ => invalid(),
    }
}
