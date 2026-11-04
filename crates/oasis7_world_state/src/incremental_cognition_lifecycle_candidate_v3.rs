//! Candidate lifecycle replay is separate from accepted lifecycle authority.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_material::{
        CandidateFactRequestV1, CheckedCandidateFactOccurrenceV1, check_candidate_fact_v1,
        promote_candidate_fact_v1,
    },
};

pub enum CognitionLifecycleParentSourceV3<'a> {
    Replayed(&'a ReservedValue<CognitionLifecycleQualificationV2>),
    Attested(&'a ReservedValue<AttestedCognitionLifecycleV3>),
}
pub struct CheckedCognitionLifecycleV3 {
    occurrence: ReservedValue<CheckedCandidateFactOccurrenceV1>,
    root: StateRootV1,
    journal: StateRootV1,
    keys: u64,
    records: u64,
}
impl CheckedCognitionLifecycleV3 {
    pub(crate) fn validate_selected(
        &self,
        selected: &CheckedCognitionHistoryViewV1,
    ) -> Result<(), ValueTreeError> {
        let summary = selected.source_v3().ok_or_else(invalid)?;
        let journal = selected
            .manifest()
            .index_ref(CognitionHistoryFieldV1::JournalEvents);
        if self.occurrence.candidate_record_id() != selected.occurrence().candidate_record_id()
            || self.occurrence.body_id() != selected.occurrence().body_id()
            || self.occurrence.roots_id() != selected.manifest().roots
            || self.occurrence.location() != selected.occurrence().location()
            || self.root != *candidate_declared(selected)?
            || self.journal != journal.root
            || self.records != journal.records
            || self.records != summary.lifecycle_journal_records
            || self.keys != summary.lifecycle_keys
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub struct CandidateCognitionLifecycleBuilderV3<'a> {
    selected: &'a ReservedValue<CheckedCognitionHistoryViewV1>,
    occurrence: ReservedValue<CheckedCandidateFactOccurrenceV1>,
    root: StateRootV1,
    keys: u64,
    next: u64,
    failed: bool,
    charge: oasis7_proto::memory_budget::MemoryReservation,
}
fn candidate_error(error: CoreError<std::convert::Infallible>) -> ValueTreeError {
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
fn candidate_declared(
    view: &CheckedCognitionHistoryViewV1,
) -> Result<&StateRootV1, ValueTreeError> {
    view.derived()
        .and_then(|refs| refs.lifecycle.as_ref())
        .ok_or_else(invalid)?
        .validate_binding(
            &lifecycle_schema(),
            &lifecycle_owner(),
            StateNamespaceV1::Artifacts,
        )
        .map_err(|_| invalid())
}
impl<'a> CandidateCognitionLifecycleBuilderV3<'a> {
    pub fn activate(
        request: &CognitionHistorySelectionRequestV1<'_>,
        selected: &'a ReservedValue<CheckedCognitionHistoryViewV1>,
        parent: &ReservedValue<CompleteCognitionHistorySourceV3>,
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        validate_new_cognition_field_source_parent_v3(
            parent,
            request,
            selected,
            CognitionHistoryFieldV1::JournalEvents,
            memory,
        )?;
        candidate_declared(selected)?;
        let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
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
        .map_err(candidate_error)?;
        if occurrence.candidate_record_id() != selected.occurrence().candidate_record_id()
            || occurrence.body_id() != selected.occurrence().body_id()
            || occurrence.roots_id() != selected.manifest().roots
            || occurrence.location() != selected.occurrence().location()
        {
            return Err(invalid());
        }
        let root = StateRootV1::empty(StateNamespaceV1::Artifacts).map_err(|_| invalid())?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(Self {
            selected,
            occurrence,
            root,
            keys: 0,
            next: 0,
            failed: false,
            charge,
        })
    }
    /// A nonempty GEN is checked by replaying every original, within the
    /// existing finite Formal fact limit. Ordinary replay starts at its exact
    /// authenticated parent and does not revisit that prefix.
    pub fn new(
        request: &CognitionHistorySelectionRequestV1<'_>,
        selected: &'a ReservedValue<CheckedCognitionHistoryViewV1>,
        parent: Option<(
            CognitionLifecycleParentSourceV3<'_>,
            &ReservedValue<AcceptedCognitionHistoryViewV1>,
        )>,
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let summary = selected.source_v3().ok_or_else(invalid)?;
        if !selected.uses_budget(memory)
            || selected.reserved_bytes() < 16 << 10
            || !selected.manifest().enabled
            || !summary.uses_budget(memory)
            || summary.reserved_bytes() < COGNITION_HISTORY_SOURCE_SUMMARY_ADMISSION_V3_BYTES
        {
            return Err(invalid());
        }
        summary.validate_shape(selected.manifest(), selected.derived().ok_or_else(invalid)?)?;
        candidate_declared(selected)?;
        let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
        let (root, keys, next) = match (summary.mode, parent) {
            (CognitionHistorySourceModeV3::Bootstrap, None)
                if request.core.body().height == 0
                    && request.core.parent_record_id().is_none()
                    && selected.manifest().revision == 0
                    && selected.manifest().previous_view.is_none() =>
            {
                (
                    StateRootV1::empty(StateNamespaceV1::Artifacts).map_err(|_| invalid())?,
                    0,
                    0,
                )
            }
            (CognitionHistorySourceModeV3::Delta, Some((parent, before))) => {
                let (root, keys, journal, records, head, roots, class, admitted) = match parent {
                    CognitionLifecycleParentSourceV3::Replayed(parent) => (
                        &parent.root,
                        parent.keys,
                        &parent.journal,
                        parent.records,
                        parent.head,
                        parent.roots,
                        parent.class,
                        parent.uses_budget(memory) && parent.reserved_bytes() >= 4096,
                    ),
                    CognitionLifecycleParentSourceV3::Attested(parent) => (
                        &parent.root,
                        parent.keys,
                        &parent.journal,
                        parent.records,
                        parent.head,
                        parent.roots,
                        parent.class,
                        parent.uses_budget(memory) && parent.reserved_bytes() >= 4096,
                    ),
                };
                if !admitted
                    || class != MemoryClass::Ordinary
                    || !before.uses_budget(memory)
                    || before.class() != MemoryClass::Ordinary
                    || head != before.occurrence().head_record_id()
                    || roots != before.manifest().roots
                    || root != declared(before)?
                    || *journal
                        != before
                            .manifest()
                            .index_ref(CognitionHistoryFieldV1::JournalEvents)
                            .root
                    || records
                        != before
                            .manifest()
                            .index_ref(CognitionHistoryFieldV1::JournalEvents)
                            .records
                    || request.core.parent_record_id() != Some(head)
                    || request.core.body().parent_commit != Some(before.occurrence().head_body_id())
                    || request
                        .core
                        .parent_body()
                        .is_none_or(|body| body.roots != roots)
                    || request
                        .core
                        .parent_roots()
                        .is_none_or(|parent_roots| parent_roots.digest().ok() != Some(roots))
                    || selected.manifest().previous_view != Some(before.occurrence().location())
                    || before.manifest().revision.checked_add(1)
                        != Some(selected.manifest().revision)
                {
                    return Err(invalid());
                }
                (root.clone(), keys, records)
            }
            _ => return Err(invalid()),
        };
        if next
            > selected
                .manifest()
                .index_ref(CognitionHistoryFieldV1::JournalEvents)
                .records
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
            &mut cancelled,
        )
        .map_err(candidate_error)?;
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
        Ok(Self {
            selected,
            occurrence,
            charge,
            root,
            keys,
            next,
            failed: false,
        })
    }
    pub fn push<M, R, E, C, W, D>(
        &mut self,
        original: &ReservedValue<CheckedCognitionIndexedOriginalV3>,
        keys: [&str; 5],
        read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, D>,
        work: &mut CognitionLifecycleWorkV2,
    ) -> Result<(), MemberRecordError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter + ValueNodeReader,
        D: FnMut() -> bool,
    {
        if self.failed {
            return Err(invalid().into());
        }
        self.failed = true;
        read.check()?;
        let expected = self
            .selected
            .manifest()
            .index_ref(CognitionHistoryFieldV1::JournalEvents)
            .records;
        if read.values.class != MemoryClass::Ordinary
            || !self.selected.uses_budget(read.values.memory)
            || !original.uses_budget(read.values.memory)
            || original.reserved_bytes() < 2048
            || self.next >= expected
            || original.ordinal() != self.next
            || original.schema().semantic().class() != MemoryClass::Ordinary
            || original.schema().semantic().field() != CognitionHistoryFieldV1::JournalEvents
            || original.candidate_record_id() != self.occurrence.candidate_record_id()
            || original.body_id() != self.occurrence.body_id()
            || original.roots_id() != self.occurrence.roots_id()
            || original.row().location.height != self.occurrence.location().height
        {
            return Err(invalid().into());
        }
        work.cells_remaining =
            work.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "candidate lifecycle records",
                ))?;
        let prepared = prepare_cognition_lifecycle_append_v2(
            &self.root,
            self.keys,
            CognitionLifecycleAppendV2 {
                schema: original.schema_carrier(),
                keys,
                ordinal: original.ordinal(),
                location: original.row().location,
            },
            read,
            write,
            work,
        )?;
        self.root = prepared.root;
        self.keys = prepared.keys;
        self.next = self.next.checked_add(1).ok_or_else(invalid)?;
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
    ) -> Result<ReservedValue<CheckedCognitionLifecycleV3>, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let index = self
            .selected
            .manifest()
            .index_ref(CognitionHistoryFieldV1::JournalEvents);
        let summary = self.selected.source_v3().ok_or_else(invalid)?;
        if self.failed
            || self.next != index.records
            || self.root != *candidate_declared(self.selected)?
            || self.keys != summary.lifecycle_keys
            || self.next != summary.lifecycle_journal_records
        {
            return Err(invalid());
        }
        let value = CheckedCognitionLifecycleV3 {
            occurrence: self.occurrence,
            root: self.root,
            journal: index.root.clone(),
            keys: self.keys,
            records: self.next,
        };
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(self.charge.retain(value))
    }
}
pub fn promote_checked_cognition_lifecycle_v3(
    checked: &ReservedValue<CheckedCognitionLifecycleV3>,
    accepted: &VerifiedFormalCommitV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CognitionLifecycleQualificationV2>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !checked.uses_budget(memory) || checked.reserved_bytes() < 16 << 10 {
        return Err(invalid());
    }
    let occurrence =
        promote_candidate_fact_v1(&checked.occurrence, accepted, memory, &mut cancelled)
            .map_err(candidate_error)?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.fact_body_id() != occurrence.head_body_id()
    {
        return Err(invalid());
    }
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let value = CognitionLifecycleQualificationV2 {
        root: checked.root.clone(),
        keys: checked.keys,
        journal: checked.journal.clone(),
        records: checked.records,
        head: occurrence.head_record_id(),
        roots: occurrence.roots_id(),
        class: MemoryClass::Ordinary,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(value))
}
