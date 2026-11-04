//! Candidate current/head binding is not an accepted CurrentBinding.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::ExecutionError,
    incremental_material::{
        CandidateFactRequestV1, CheckedCandidateFactOccurrenceV1, CoreError,
        check_candidate_fact_v1, promote_candidate_fact_v1,
    },
};
use oasis7_proto::memory_budget::MemoryBudget;
pub struct CheckedCognitionHistoryCurrentBindingV3 {
    occurrence: ReservedValue<CheckedCandidateFactOccurrenceV1>,
    enabled: bool,
}
impl CheckedCognitionHistoryCurrentBindingV3 {
    pub fn roots_id(&self) -> [u8; 32] {
        self.occurrence.roots_id()
    }
    pub fn candidate_record_id(&self) -> [u8; 32] {
        self.occurrence.candidate_record_id()
    }
    pub fn body_id(&self) -> [u8; 32] {
        self.occurrence.body_id()
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub(crate) fn validate_selected(
        &self,
        selected: &CheckedCognitionHistoryViewV1,
    ) -> Result<(), ValueTreeError> {
        if self.enabled != selected.manifest().enabled
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
pub fn bind_candidate_cognition_history_current_v3<M, R, E, C>(
    request: &CognitionHistorySelectionRequestV1<'_>,
    selected: &ReservedValue<CheckedCognitionHistoryViewV1>,
    roots: &RootSetV1,
    current: CognitionHistoryCurrentSourceV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<ReservedValue<CheckedCognitionHistoryCurrentBindingV3>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.check()?;
    let memory = context.values.memory;
    let charge = memory.reserve(8192, MemoryClass::Ordinary)?;
    let digest = roots.digest().map_err(|_| invalid())?;
    if context.values.class != MemoryClass::Ordinary
        || !selected.uses_budget(memory)
        || selected.reserved_bytes() < 16 << 10
        || selected.manifest().roots != digest
        || context.root != roots.state_root(StateNamespaceV1::Allocator)
    {
        return Err(invalid());
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
        &mut context.cancelled,
    )
    .map_err(source_error)?;
    if occurrence.roots_id() != digest
        || occurrence.candidate_record_id() != selected.occurrence().candidate_record_id()
        || occurrence.body_id() != selected.occurrence().body_id()
        || occurrence.location() != selected.occurrence().location()
    {
        return Err(invalid());
    }
    verify_history_current_fields_v3(selected.manifest(), &digest, current, context)?;
    context.check()?;
    Ok(charge.retain(CheckedCognitionHistoryCurrentBindingV3 {
        occurrence,
        enabled: selected.manifest().enabled,
    }))
}
pub fn promote_cognition_history_current_v3(
    checked: &ReservedValue<CheckedCognitionHistoryCurrentBindingV3>,
    accepted: &VerifiedFormalCommitV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CognitionHistoryCurrentBindingV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !checked.uses_budget(memory) || checked.reserved_bytes() < 8192 {
        return Err(invalid());
    }
    let occurrence =
        promote_candidate_fact_v1(&checked.occurrence, accepted, memory, &mut cancelled)
            .map_err(source_error)?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.head_body_id() != occurrence.fact_body_id()
    {
        return Err(invalid());
    }
    let charge = memory.reserve(8192, MemoryClass::Ordinary)?;
    let result = CognitionHistoryCurrentBindingV1 {
        roots: occurrence.roots_id(),
        head: occurrence.head_record_id(),
        enabled: checked.enabled,
        class: MemoryClass::Ordinary,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("candidate current History binding")
}
fn source_error(error: CoreError<std::convert::Infallible>) -> ValueTreeError {
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

#[path = "incremental_cognition_history_current_prepared_v3.rs"]
mod prepared_v3;
pub use prepared_v3::*;
