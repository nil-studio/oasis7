//! Logical candidate conjunction. The endpoint must additionally verify
//! execution legality and physical durability before issuing receipts.
use crate::{
    incremental_cognition_history::CheckedCognitionHistorySourceV3,
    incremental_runtime_history_source::CheckedRuntimeHistorySourceV1,
    incremental_world_current_complete::WorldCurrentStateQualificationV1,
    incremental_world_format::{CompleteWorldFormatScopeV1, select_complete_world_format_v1},
    incremental_world_result_router::CheckedWorldResultRoutingV1,
};
use oasis7_formal_protocol::{
    incremental_material::CheckedCommitCore, incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

/// Required owners are retained, rather than replaced with reported counts.
/// This is candidate evidence only; it cannot select or publish an active head.
pub struct CheckedWorldStorageSourceV1 {
    format: CompleteWorldFormatScopeV1,
    current: ReservedValue<WorldCurrentStateQualificationV1>,
    cognition: ReservedValue<CheckedCognitionHistorySourceV3>,
    runtime: ReservedValue<CheckedRuntimeHistorySourceV1>,
    results: ReservedValue<CheckedWorldResultRoutingV1>,
}
impl CheckedWorldStorageSourceV1 {
    pub fn format(&self) -> CompleteWorldFormatScopeV1 {
        self.format
    }
    pub fn current(&self) -> &ReservedValue<WorldCurrentStateQualificationV1> {
        &self.current
    }
    pub fn cognition(&self) -> &ReservedValue<CheckedCognitionHistorySourceV3> {
        &self.cognition
    }
    pub fn runtime(&self) -> &ReservedValue<CheckedRuntimeHistorySourceV1> {
        &self.runtime
    }
    pub fn results(&self) -> &ReservedValue<CheckedWorldResultRoutingV1> {
        &self.results
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("complete world storage candidate source")
}

/// Join independently checked owners for the exact original Core. A matching
/// state root alone cannot substitute evidence from a different candidate body.
pub fn qualify_checked_world_storage_source_v1(
    core: &ReservedValue<CheckedCommitCore>,
    current: ReservedValue<WorldCurrentStateQualificationV1>,
    cognition: ReservedValue<CheckedCognitionHistorySourceV3>,
    runtime: ReservedValue<CheckedRuntimeHistorySourceV1>,
    results: ReservedValue<CheckedWorldResultRoutingV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedWorldStorageSourceV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    macro_rules! carrier {
        ($value:ident) => {
            if !$value.uses_budget(memory)
                || $value.reserved_bytes() < std::mem::size_of_val(&*$value)
            {
                return Err(invalid());
            }
        };
    }
    carrier!(core);
    carrier!(current);
    carrier!(cognition);
    carrier!(runtime);
    carrier!(results);
    let format =
        select_complete_world_format_v1(core.config(), core.roots()).map_err(|_| invalid())?;
    current
        .validate_binding(core.roots())
        .map_err(|_| invalid())?;
    current.check_pool(memory)?;
    current.raw().validate_candidate_core(core)?;
    if current.allocation_class() != MemoryClass::Ordinary
        || cognition.allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid());
    }
    let roots = core.roots().digest().map_err(|_| invalid())?;
    let body = core.body().digest().map_err(|_| invalid())?;
    let record = core.candidate_record_id();
    let occurrence = runtime.occurrence();
    let selected_cognition = cognition.selected().occurrence();
    for routed in results.rows() {
        let actual = routed.occurrence();
        if !actual.uses_budget(memory)
            || actual.candidate_record_id() != record
            || actual.body_id() != body
            || actual.roots_id() != roots
        {
            return Err(invalid());
        }
    }
    if cognition.candidate_record_id() != record
        || cognition.body_id() != body
        || cognition.roots_id() != roots
        || runtime.manifest().roots != roots
        || occurrence.candidate_record_id() != record
        || occurrence.body_id() != body
        || occurrence.roots_id() != roots
        || !occurrence.uses_budget(memory)
        || results.cognition().occurrence().location() != selected_cognition.location()
        || results.runtime_history().occurrence().location() != occurrence.location()
    {
        return Err(invalid());
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let charge = memory
        .reserve(
            std::mem::size_of::<CheckedWorldStorageSourceV1>() + 512,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(charge.retain(CheckedWorldStorageSourceV1 {
        format,
        current,
        cognition,
        runtime,
        results,
    }))
}
