//! Checked GEN or incremental ordinary source proof. An actual signature is required for promotion.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::{ExecutionError, ExecutionFactKindV1},
    incremental_material::{
        CandidateFactRequestV1, CheckedCandidateFactOccurrenceV1, CoreError,
        check_candidate_fact_v1, promote_candidate_fact_v1,
    },
};
pub struct CandidateCognitionHeadSourceWorkV3<'a> {
    pub cbor: &'a mut CognitionHistoryCborWorkV1,
    pub hashes: &'a mut CognitionHeadHashWorkV1,
}
pub struct CandidateCognitionHeadSourceBuilderV3<'a> {
    request: &'a CognitionHistorySelectionRequestV1<'a>,
    selected: &'a ReservedValue<CheckedCognitionHistoryViewV1>,
    memory: &'a MemoryBudget,
    occurrence: ReservedValue<CheckedCandidateFactOccurrenceV1>,
    builder: CognitionHeadHashBuilderV1,
    field: CognitionHistoryFieldV1,
    expected: u64,
    next: u64,
    last: Option<[u8; 32]>,
    last_record: Option<CognitionHeadSourceLastV3>,
    failed: bool,
    parent_index: Option<oasis7_formal_protocol::incremental_state::StateRootV1>,
    replay_index: Option<oasis7_formal_protocol::incremental_state::StateRootV1>,
    _metadata: oasis7_proto::memory_budget::MemoryReservation,
}
pub struct CheckedCandidateHeadSourceV3 {
    occurrence: ReservedValue<CheckedCandidateFactOccurrenceV1>,
    state: ReservedValue<CognitionHeadHashStateV1>,
    field: CognitionHistoryFieldV1,
    records: u64,
    last: Option<[u8; 32]>,
    last_record: Option<CognitionHeadSourceLastV3>,
}
impl CheckedCandidateHeadSourceV3 {
    pub(crate) fn validate_selected(
        &self,
        selected: &CheckedCognitionHistoryViewV1,
        field: CognitionHistoryFieldV1,
    ) -> Result<(), ValueTreeError> {
        if self.field != field
            || self.records != selected.manifest().index_ref(field).records
            || self.occurrence.candidate_record_id() != selected.occurrence().candidate_record_id()
            || self.occurrence.body_id() != selected.occurrence().body_id()
            || self.occurrence.roots_id() != selected.manifest().roots
            || self.occurrence.location() != selected.occurrence().location()
        {
            return Err(candidate_invalid());
        }
        Ok(())
    }
}
fn candidate_invalid() -> ValueTreeError {
    ValueTreeError::Invalid("checked candidate head source")
}
impl<'a> CandidateCognitionHeadSourceBuilderV3<'a> {
    pub fn genesis(
        request: &'a CognitionHistorySelectionRequestV1<'a>,
        selected: &'a ReservedValue<CheckedCognitionHistoryViewV1>,
        index: &ReservedValue<CognitionHistoryIndexQualificationV1>,
        memory: &'a MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if request.core.body().height != 0
            || request.core.parent_record_id().is_some()
            || selected.manifest().revision != 0
            || selected.manifest().previous_view.is_some()
            || selected
                .source_v3()
                .is_none_or(|summary| summary.mode != CognitionHistorySourceModeV3::Bootstrap)
        {
            return Err(candidate_invalid());
        }
        Self::full_scope(request, selected, index, memory, cancelled)
    }
    /// A newly present active field starts from its actual complete new index,
    /// never from a fabricated enabled parent or from a scan of archived H.
    pub fn activate_field(
        request: &'a CognitionHistorySelectionRequestV1<'a>,
        selected: &'a ReservedValue<CheckedCognitionHistoryViewV1>,
        parent: &ReservedValue<CompleteCognitionHistorySourceV3>,
        index: &ReservedValue<CognitionHistoryIndexQualificationV1>,
        memory: &'a MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        validate_new_cognition_field_source_parent_v3(
            parent,
            request,
            selected,
            index.field(),
            memory,
        )?;
        Self::full_scope(request, selected, index, memory, cancelled)
    }
    fn full_scope(
        request: &'a CognitionHistorySelectionRequestV1<'a>,
        selected: &'a ReservedValue<CheckedCognitionHistoryViewV1>,
        index: &ReservedValue<CognitionHistoryIndexQualificationV1>,
        memory: &'a MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let summary = selected.source_v3().ok_or_else(candidate_invalid)?;
        let field = index.field();
        if !selected.manifest().enabled
            || field == CognitionHistoryFieldV1::Responses
            || !selected.uses_budget(memory)
            || selected.reserved_bytes() < 16 << 10
            || !summary.uses_budget(memory)
            || summary.reserved_bytes() < COGNITION_HISTORY_SOURCE_SUMMARY_ADMISSION_V3_BYTES
            || index.reserved_bytes() < 16 << 10
            || !index.uses_budget(memory)
            || index.class() != MemoryClass::Ordinary
        {
            return Err(candidate_invalid());
        }
        index.validate_binding(field, selected.manifest().index_ref(field))?;
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
        .map_err(candidate_core_error)?;
        if occurrence.candidate_record_id() != selected.occurrence().candidate_record_id()
            || occurrence.body_id() != selected.occurrence().body_id()
            || occurrence.roots_id() != selected.manifest().roots
            || occurrence.location() != selected.occurrence().location()
        {
            return Err(candidate_invalid());
        }
        let builder =
            CognitionHeadHashBuilderV1::new(memory, MemoryClass::Ordinary, &mut cancelled)?;
        Ok(Self {
            request,
            selected,
            memory,
            occurrence,
            builder,
            field,
            expected: index.record_count(),
            next: 0,
            last: None,
            last_record: None,
            failed: false,
            parent_index: None,
            replay_index: None,
            _metadata: memory.reserve(16 << 10, MemoryClass::Ordinary)?,
        })
    }
    /// Inherit only a private actual-source frontier from the exact authenticated
    /// parent of this checked candidate. Only the new originals are consumed.
    pub fn ordinary(
        request: &'a CognitionHistorySelectionRequestV1<'a>,
        selected: &'a ReservedValue<CheckedCognitionHistoryViewV1>,
        before: &ReservedValue<AcceptedCognitionHistoryViewV1>,
        parent: &ReservedValue<AuthenticatedCognitionHeadSourceV1>,
        index: &ReservedValue<AttestedCognitionHistoryIndexV3>,
        memory: &'a MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let summary = selected.source_v3().ok_or_else(candidate_invalid)?;
        let field = index.field();
        let old = before.manifest().index_ref(field);
        let after = selected.manifest().index_ref(field);
        if !before.uses_budget(memory)
            || before.reserved_bytes() < 16 << 10
            || before.class() != MemoryClass::Ordinary
            || !parent.uses_budget(memory)
            || parent.reserved_bytes() < 4096
            || !parent.state.uses_budget(memory)
            || parent.state.allocation_class() != MemoryClass::Ordinary
            || parent.field != field
            || parent.head != before.occurrence().head_record_id()
            || parent.roots != before.manifest().roots
            || parent.records != old.records
            || !index.uses_budget(memory)
            || index.reserved_bytes() < 16 << 10
            || index.allocation_class() != MemoryClass::Ordinary
            || index.head_record_id() != parent.head
            || index.roots_id() != parent.roots
            || index.root() != &old.root
            || index.record_count() != parent.records
            || !old.present
            || !after.present
            || after.records < parent.records
            || field == CognitionHistoryFieldV1::Responses
            || !selected.uses_budget(memory)
            || selected.reserved_bytes() < 16 << 10
            || !summary.uses_budget(memory)
            || summary.reserved_bytes() < COGNITION_HISTORY_SOURCE_SUMMARY_ADMISSION_V3_BYTES
            || summary.mode != CognitionHistorySourceModeV3::Delta
            || request.core.parent_record_id() != Some(parent.head)
            || request.core.body().parent_commit != Some(before.occurrence().head_body_id())
            || request
                .core
                .parent_body()
                .is_none_or(|body| body.roots != parent.roots)
            || request
                .core
                .parent_roots()
                .is_none_or(|roots| roots.digest().ok() != Some(parent.roots))
            || selected.manifest().previous_view != Some(before.occurrence().location())
            || before.manifest().revision.checked_add(1) != Some(selected.manifest().revision)
        {
            return Err(candidate_invalid());
        }
        summary.validate_shape(
            selected.manifest(),
            selected.derived().ok_or_else(candidate_invalid)?,
        )?;
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
        .map_err(candidate_core_error)?;
        if occurrence.candidate_record_id() != selected.occurrence().candidate_record_id()
            || occurrence.body_id() != selected.occurrence().body_id()
            || occurrence.roots_id() != selected.manifest().roots
            || occurrence.location() != selected.occurrence().location()
        {
            return Err(candidate_invalid());
        }
        let metadata = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
        let builder = CognitionHeadHashBuilderV1::append_to(
            &parent.state,
            memory,
            MemoryClass::Ordinary,
            &mut cancelled,
        )?;
        let root = index.root().clone();
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(Self {
            request,
            selected,
            memory,
            occurrence,
            builder,
            field,
            expected: after.records,
            next: parent.records,
            last: parent.last,
            last_record: parent.last_record.clone(),
            failed: false,
            parent_index: Some(root),
            replay_index: None,
            _metadata: metadata,
        })
    }
    /// Replay the exact newly checked locator into the inherited typed index.
    /// Failure poisons the builder; neither the locator nor this provisional
    /// replay is an accepted index qualification.
    pub fn push_delta<M, R, E, C, W, CW, I: Read>(
        &mut self,
        point: ReservedValue<CognitionHistoryIndexPointV1>,
        input: &mut I,
        read: &mut crate::incremental_field_read::WorldFieldReadContextV1<'_, M, R, E, C>,
        write: &mut crate::incremental_field_member::MemberRecordWriteContext<'_, W, CW>,
        indexes: &mut CognitionHistoryIndexWorkV1,
        work: &mut CandidateCognitionHeadSourceWorkV3<'_>,
    ) -> Result<(), crate::incremental_field_member::MemberRecordError>
    where
        M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
        R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: oasis7_formal_protocol::incremental_state::StatePageWriter,
        CW: FnMut() -> bool,
    {
        if self.parent_index.is_none() {
            return Err(candidate_invalid().into());
        }
        let row = point.row();
        if row.location.height != self.request.core.body().height {
            self.failed = true;
            return Err(candidate_invalid().into());
        }
        let ordinal = self.next;
        self.push_core(point, input, work, || {
            (read.cancelled)() || (write.cancelled)()
        })?;
        self.failed = true;
        let root = self
            .replay_index
            .as_ref()
            .or(self.parent_index.as_ref())
            .ok_or_else(candidate_invalid)?;
        let (next, count) = super::super::index::append_cognition_history_index_parts_v3(
            root, self.field, ordinal, row, read, write, indexes,
        )?;
        if count != self.next {
            return Err(candidate_invalid().into());
        }
        self.replay_index = Some(next);
        read.check()?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        self.failed = false;
        Ok(())
    }
    pub fn push<R: Read, C: FnMut() -> bool>(
        &mut self,
        point: ReservedValue<CognitionHistoryIndexPointV1>,
        input: &mut R,
        work: &mut CandidateCognitionHeadSourceWorkV3<'_>,
        cancelled: C,
    ) -> Result<(), ValueTreeError> {
        if self.parent_index.is_some() {
            self.failed = true;
            return Err(candidate_invalid());
        }
        self.push_core(point, input, work, cancelled)
    }
    fn push_core<R: Read, C: FnMut() -> bool>(
        &mut self,
        point: ReservedValue<CognitionHistoryIndexPointV1>,
        input: &mut R,
        work: &mut CandidateCognitionHeadSourceWorkV3<'_>,
        mut cancelled: C,
    ) -> Result<(), ValueTreeError> {
        if self.failed {
            return Err(candidate_invalid());
        }
        self.failed = true;
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if self.next >= self.expected || point.ordinal() != self.next || point.field != self.field {
            return Err(candidate_invalid());
        }
        let row = point.row();
        let occurrence = check_candidate_fact_v1(
            &CandidateFactRequestV1 {
                kind: self.field.fact_kind(),
                candidate: self.request.candidate,
                core: self.request.core,
                originals: self.request.originals,
                ordinal: row.location.ordinal,
            },
            self.memory,
            &mut cancelled,
        )
        .map_err(candidate_core_error)?;
        let cancelled = RefCell::new(cancelled);
        let mut tee = HashRead {
            input,
            builder: &mut self.builder,
            hashes: work.hashes,
            cancelled: &cancelled,
            failure: None,
        };
        let schema = verify_cognition_history_original_record_schema_v1(
            CognitionOriginalCborRequestV1 {
                field: self.field,
                id: row.location.original,
                bytes: row.encoded_bytes,
            },
            &mut tee,
            self.memory,
            MemoryClass::Ordinary,
            work.cbor,
            || (cancelled.borrow_mut())(),
        );
        if let Some(error) = tee.failure.take() {
            return Err(error);
        }
        let joined = bind_checked_cognition_indexed_original_v3(
            point,
            schema?,
            &occurrence,
            self.selected,
            self.memory,
            || (cancelled.borrow_mut())(),
        )?;
        let semantic = joined.schema().semantic();
        if semantic.sequence() != self.next.checked_add(1) || semantic.parent() != Some(self.last) {
            return Err(candidate_invalid());
        }
        let CognitionOriginalIdentityStatusV1::DigestMatched(digest) = semantic.identity_status()
        else {
            return Err(candidate_invalid());
        };
        self.last = Some(digest);
        self.last_record = Some(CognitionHeadSourceLastV3 {
            sequence: self.next.checked_add(1).ok_or_else(candidate_invalid)?,
            semantic_digest: digest,
            location: row.location,
            original_bytes: row.encoded_bytes,
        });
        self.next = self.next.checked_add(1).ok_or_else(candidate_invalid)?;
        if (cancelled.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        self.failed = false;
        Ok(())
    }
    pub fn finish(
        self,
        cache: &ReservedValue<PersistedCognitionHeadCacheV1>,
        work: &mut CognitionHeadHashWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<CheckedCandidateHeadSourceV3>, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if self.failed || self.next != self.expected {
            return Err(candidate_invalid());
        }
        if let Some(parent) = &self.parent_index {
            let replay = self.replay_index.as_ref().unwrap_or(parent);
            if replay != &self.selected.manifest().index_ref(self.field).root {
                return Err(candidate_invalid());
            }
        }
        let slot = match self.field {
            CognitionHistoryFieldV1::JournalEvents => 0,
            CognitionHistoryFieldV1::EconomyJournal => 1,
            CognitionHistoryFieldV1::EconomyProvisionJournal => 2,
            CognitionHistoryFieldV1::Responses => return Err(candidate_invalid()),
        };
        let kind = CognitionHeadHashV1::ALL[slot];
        let refs = self.selected.derived().ok_or_else(candidate_invalid)?;
        let summary = self.selected.source_v3().ok_or_else(candidate_invalid)?;
        if !cache.uses_budget(self.memory)
            || cache.reserved_bytes() < 1024
            || cache.state().reserved_bytes() < CognitionHeadHashBuilderV1::admission_bytes()
            || !cache.state().uses_budget(self.memory)
            || cache.state().allocation_class() != MemoryClass::Ordinary
            || refs.caches[slot] != Some(cache.id())
            || summary.last[slot] != self.last_record
        {
            return Err(candidate_invalid());
        }
        let charge = self.memory.reserve(4096, MemoryClass::Ordinary)?;
        let state = self.builder.finish(&mut cancelled)?;
        let head = &self.selected.manifest().heads[slot];
        validate_head_source_sequence(self.field, self.expected, head.sequence)?;
        match head.digest {
            Some(digest)
                if state.digest(kind, self.expected, head.sequence, work, &mut cancelled)?
                    == digest => {}
            None if self.expected == 0 && head.sequence == 0 && state.body_bytes() == 0 => {}
            _ => return Err(candidate_invalid()),
        }
        let mut actual = CacheDigest(blake3::Hasher::new());
        state.write_canonical_cache_v1(&mut actual, work, &mut cancelled)?;
        let digest = *actual.0.finalize().as_bytes();
        let mut stored = CacheDigest(blake3::Hasher::new());
        cache
            .state()
            .write_canonical_cache_v1(&mut stored, work, &mut cancelled)?;
        if digest != *stored.0.finalize().as_bytes()
            || summary.cache_state_digests[slot] != Some(digest)
        {
            return Err(candidate_invalid());
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(charge.retain(CheckedCandidateHeadSourceV3 {
            occurrence: self.occurrence,
            state,
            field: self.field,
            records: self.expected,
            last: self.last,
            last_record: self.last_record,
        }))
    }
}
pub fn promote_checked_cognition_head_source_v3(
    checked: &ReservedValue<CheckedCandidateHeadSourceV3>,
    accepted: &VerifiedFormalCommitV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AuthenticatedCognitionHeadSourceV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !checked.uses_budget(memory)
        || checked.reserved_bytes() < 4096
        || !checked.state.uses_budget(memory)
        || checked.state.allocation_class() != MemoryClass::Ordinary
    {
        return Err(candidate_invalid());
    }
    let occurrence =
        promote_candidate_fact_v1(&checked.occurrence, accepted, memory, &mut cancelled)
            .map_err(candidate_core_error)?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.head_body_id() != occurrence.fact_body_id()
    {
        return Err(candidate_invalid());
    }
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let state = CognitionHeadHashBuilderV1::append_to(
        &checked.state,
        memory,
        MemoryClass::Ordinary,
        &mut cancelled,
    )?
    .finish(&mut cancelled)?;
    let result = AuthenticatedCognitionHeadSourceV1 {
        state,
        field: checked.field,
        head: occurrence.head_record_id(),
        roots: occurrence.roots_id(),
        records: checked.records,
        last: checked.last,
        last_record: checked.last_record.clone(),
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}

fn candidate_core_error(error: CoreError<std::convert::Infallible>) -> ValueTreeError {
    match error {
        CoreError::Cancelled => ValueTreeError::Cancelled,
        CoreError::ResourceLimited(reason)
        | CoreError::Execution(ExecutionError::ResourceLimited(reason)) => {
            ValueTreeError::ResourceLimited(reason)
        }
        CoreError::Execution(ExecutionError::Memory(error)) => ValueTreeError::Memory(error),
        _ => candidate_invalid(),
    }
}
