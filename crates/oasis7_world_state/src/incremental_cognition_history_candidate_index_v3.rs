//! All four candidate indexes require actual kind-bound original schemas.
use super::*;
use crate::incremental_field_member::{MemberRecordError, MemberRecordWriteContext};
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::{ExecutionError, ExecutionFactKindV1},
    incremental_material::{
        CandidateFactRequestV1, CheckedCandidateFactOccurrenceV1, CoreError,
        check_candidate_fact_v1, promote_candidate_fact_v1,
    },
    incremental_state::StatePageWriter,
};
pub struct CheckedCognitionHistoryIndexSourceV3 {
    occurrence: ReservedValue<CheckedCandidateFactOccurrenceV1>,
    root: StateRootV1,
    field: CognitionHistoryFieldV1,
    records: u64,
}
impl CheckedCognitionHistoryIndexSourceV3 {
    pub fn field(&self) -> CognitionHistoryFieldV1 {
        self.field
    }
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn record_count(&self) -> u64 {
        self.records
    }
    pub fn candidate_record_id(&self) -> [u8; 32] {
        self.occurrence.candidate_record_id()
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.occurrence.roots_id()
    }
    pub fn body_id(&self) -> [u8; 32] {
        self.occurrence.body_id()
    }
}
pub struct CandidateCognitionHistoryIndexBuilderV3<'a> {
    selected: &'a ReservedValue<CheckedCognitionHistoryViewV1>,
    occurrence: ReservedValue<CheckedCandidateFactOccurrenceV1>,
    root: StateRootV1,
    field: CognitionHistoryFieldV1,
    next: u64,
    replay: bool,
    failed: bool,
    charge: oasis7_proto::memory_budget::MemoryReservation,
}
fn invalid_candidate_index() -> ValueTreeError {
    ValueTreeError::Invalid("candidate index source")
}
fn core_error(error: CoreError<std::convert::Infallible>) -> ValueTreeError {
    match error {
        CoreError::Cancelled => ValueTreeError::Cancelled,
        CoreError::ResourceLimited(reason)
        | CoreError::Execution(ExecutionError::ResourceLimited(reason)) => {
            ValueTreeError::ResourceLimited(reason)
        }
        CoreError::Execution(ExecutionError::Memory(error)) => ValueTreeError::Memory(error),
        _ => invalid_candidate_index(),
    }
}
impl<'a> CandidateCognitionHistoryIndexBuilderV3<'a> {
    pub fn new(
        request: &CognitionHistorySelectionRequestV1<'_>,
        selected: &'a ReservedValue<CheckedCognitionHistoryViewV1>,
        field: CognitionHistoryFieldV1,
        genesis: Option<&ReservedValue<CognitionHistoryIndexQualificationV1>>,
        parent: Option<(
            &ReservedValue<AttestedCognitionHistoryIndexV3>,
            &ReservedValue<AcceptedCognitionHistoryViewV1>,
        )>,
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let bad = invalid_candidate_index;
        let summary = selected.source_v3().ok_or_else(bad)?;
        if !selected.uses_budget(memory)
            || selected.reserved_bytes() < 16 << 10
            || !summary.uses_budget(memory)
            || summary.reserved_bytes() < COGNITION_HISTORY_SOURCE_SUMMARY_ADMISSION_V3_BYTES
        {
            return Err(bad());
        }
        summary.validate_shape(selected.manifest(), selected.derived().ok_or_else(bad)?)?;
        let declared = selected.manifest().index_ref(field);
        if !declared.present {
            return Err(bad());
        }
        let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
        let (root, next, replay) = match (summary.mode, genesis, parent) {
            (CognitionHistorySourceModeV3::Bootstrap, Some(index), None)
                if request.core.body().height == 0
                    && request.core.parent_record_id().is_none()
                    && selected.manifest().revision == 0
                    && selected.manifest().previous_view.is_none() =>
            {
                if !index.uses_budget(memory)
                    || index.reserved_bytes() < 16 << 10
                    || index.class() != MemoryClass::Ordinary
                {
                    return Err(bad());
                }
                index.validate_binding(field, declared)?;
                (index.root().clone(), 0, false)
            }
            (CognitionHistorySourceModeV3::Delta, None, Some((index, before))) => {
                let old = before.manifest().index_ref(field);
                if !index.uses_budget(memory)
                    || index.reserved_bytes() < 16 << 10
                    || index.class != MemoryClass::Ordinary
                    || index.field != field
                    || !before.uses_budget(memory)
                    || before.class() != MemoryClass::Ordinary
                    || index.head != before.occurrence().head_record_id()
                    || index.roots != before.manifest().roots
                    || !old.present
                    || index.root != old.root
                    || index.records != old.records
                    || declared.records < index.records
                    || request.core.parent_record_id() != Some(index.head)
                    || request.core.body().parent_commit != Some(before.occurrence().head_body_id())
                    || request
                        .core
                        .parent_body()
                        .is_none_or(|body| body.roots != index.roots)
                    || request
                        .core
                        .parent_roots()
                        .is_none_or(|roots| roots.digest().ok() != Some(index.roots))
                    || selected.manifest().previous_view != Some(before.occurrence().location())
                    || before.manifest().revision.checked_add(1)
                        != Some(selected.manifest().revision)
                {
                    return Err(bad());
                }
                (index.root.clone(), index.records, true)
            }
            _ => return Err(bad()),
        };
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
        .map_err(core_error)?;
        if occurrence.candidate_record_id() != selected.occurrence().candidate_record_id()
            || occurrence.body_id() != selected.occurrence().body_id()
            || occurrence.roots_id() != selected.manifest().roots
            || occurrence.location() != selected.occurrence().location()
        {
            return Err(bad());
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(Self {
            selected,
            occurrence,
            root,
            field,
            next,
            replay,
            failed: false,
            charge,
        })
    }
    pub fn push<M, R, E, C, W, D>(
        &mut self,
        original: &ReservedValue<CheckedCognitionIndexedOriginalV3>,
        read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, D>,
        work: &mut CognitionHistoryIndexWorkV1,
    ) -> Result<(), MemberRecordError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter,
        D: FnMut() -> bool,
    {
        if self.failed {
            return Err(invalid_candidate_index().into());
        }
        self.failed = true;
        read.check()?;
        if read.values.class != MemoryClass::Ordinary
            || !self.selected.uses_budget(read.values.memory)
            || !original.uses_budget(read.values.memory)
            || original.reserved_bytes() < 2048
            || self.next >= self.selected.manifest().index_ref(self.field).records
            || original.ordinal() != self.next
            || original.schema().semantic().field() != self.field
            || original.schema().semantic().class() != MemoryClass::Ordinary
            || original.candidate_record_id() != self.occurrence.candidate_record_id()
            || original.body_id() != self.occurrence.body_id()
            || original.roots_id() != self.occurrence.roots_id()
            || original.row().location.height != self.occurrence.location().height
        {
            return Err(invalid_candidate_index().into());
        }
        if self.replay {
            let (root, count) = super::super::index::append_cognition_history_index_parts_v3(
                &self.root,
                self.field,
                self.next,
                original.row(),
                read,
                write,
                work,
            )?;
            self.root = root;
            self.next = count;
        } else {
            work.cells_remaining =
                work.cells_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "candidate index source rows",
                    ))?;
            self.next = self
                .next
                .checked_add(1)
                .ok_or_else(invalid_candidate_index)?;
        }
        read.check()?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        self.failed = false;
        Ok(())
    }
    pub fn finish(
        self,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<CheckedCognitionHistoryIndexSourceV3>, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let declared = self.selected.manifest().index_ref(self.field);
        if self.failed || self.next != declared.records || self.root != declared.root {
            return Err(invalid_candidate_index());
        }
        let value = CheckedCognitionHistoryIndexSourceV3 {
            occurrence: self.occurrence,
            root: self.root,
            field: self.field,
            records: self.next,
        };
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(self.charge.retain(value))
    }
}
pub fn promote_checked_cognition_history_index_v3(
    source: &ReservedValue<CheckedCognitionHistoryIndexSourceV3>,
    accepted: &VerifiedFormalCommitV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AttestedCognitionHistoryIndexV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory) || source.reserved_bytes() < 16 << 10 {
        return Err(invalid_candidate_index());
    }
    let occurrence =
        promote_candidate_fact_v1(&source.occurrence, accepted, memory, &mut cancelled)
            .map_err(core_error)?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.fact_body_id() != occurrence.head_body_id()
    {
        return Err(invalid_candidate_index());
    }
    let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
    let value = AttestedCognitionHistoryIndexV3 {
        root: source.root.clone(),
        field: source.field,
        records: source.records,
        head: occurrence.head_record_id(),
        roots: occurrence.roots_id(),
        class: MemoryClass::Ordinary,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(value))
}
