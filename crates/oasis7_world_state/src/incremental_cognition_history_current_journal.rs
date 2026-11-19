//! Provisional Journal frontier inherited from the actual accepted source.
//! It cannot become an authenticated head: candidate facts, the selected view
//! and ordinary CandidateCognitionHeadSourceBuilderV3 remain mandatory later.
use super::*;
use crate::{
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionFactKindV1, ExecutionFence, ExecutionTransaction, PreparedGuard,
    },
    incremental_material::FactLocationV1,
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryReservation;
pub type CognitionJournalHashRowsV1 = Vec<(u64, [u8; 32])>;
pub struct PendingCurrentCognitionJournalV1<'a> {
    field: CognitionHistoryFieldV1,
    height: u64,
    parent: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
    builder: Option<CognitionHeadHashBuilderV1>,
    root: StateRootV1,
    count: u64,
    base_count: u64,
    head_present: bool,
    sequence: u64,
    last: Option<[u8; 32]>,
    guard: PreparedGuard,
    appended: Vec<(u64, [u8; 32])>,
    _appended_fee: MemoryReservation,
    _fee: ReservedValue<()>,
}
pub struct WrittenCurrentCognitionJournalV1<'a> {
    field: CognitionHistoryFieldV1,
    parent: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
    state: ReservedValue<CognitionHeadHashStateV1>,
    root: StateRootV1,
    count: u64,
    sequence: u64,
    head_present: bool,
    last: Option<[u8; 32]>,
    digest: [u8; 32],
    guard: PreparedGuard,
    _fee: ReservedValue<()>,
}
fn field_profile(
    field: CognitionHistoryFieldV1,
) -> Result<(usize, usize, CognitionHeadHashV1), ValueTreeError> {
    match field {
        CognitionHistoryFieldV1::JournalEvents => Ok((0, 0, CognitionHeadHashV1::Journal)),
        CognitionHistoryFieldV1::EconomyProvisionJournal => {
            Ok((2, 3, CognitionHeadHashV1::Provision))
        }
        CognitionHistoryFieldV1::EconomyJournal => Ok((1, 2, CognitionHeadHashV1::Economy)),
        CognitionHistoryFieldV1::Responses => Err(bad()),
    }
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current Cognition Journal source")
}
fn charge_frontier_copy(hashes: &mut CognitionHeadHashWorkV1) -> Result<(), ValueTreeError> {
    // append_to admits allocation; the resident cache copy also consumes the
    // same continuing hash lane's byte and fixed-size chunk work allowance.
    let bytes = u64::try_from(CognitionHeadHashBuilderV1::admission_bytes()).map_err(|_| bad())?;
    let chunks = bytes.checked_add(1023).ok_or_else(bad)? / 1024;
    hashes.input_bytes_remaining =
        hashes
            .input_bytes_remaining
            .checked_sub(bytes)
            .ok_or(ValueTreeError::ResourceLimited(
                "Current Journal frontier copy bytes",
            ))?;
    hashes.hash_nodes_remaining =
        hashes
            .hash_nodes_remaining
            .checked_sub(chunks)
            .ok_or(ValueTreeError::ResourceLimited(
                "Current Journal frontier copy work",
            ))?;
    Ok(())
}
fn charge_digest_notes(
    work: &mut CognitionHeadHashWorkV1,
    count: usize,
) -> Result<(), ValueTreeError> {
    let bytes = count
        .checked_mul(std::mem::size_of::<(u64, [u8; 32])>())
        .ok_or_else(bad)?;
    work.input_bytes_remaining = work.input_bytes_remaining.checked_sub(bytes as u64).ok_or(
        ValueTreeError::ResourceLimited("Current Journal digest-note bytes"),
    )?;
    work.hash_nodes_remaining = work.hash_nodes_remaining.checked_sub(count as u64).ok_or(
        ValueTreeError::ResourceLimited("Current Journal digest-note visits"),
    )?;
    Ok(())
}
impl<'a> PendingCurrentCognitionJournalV1<'a> {
    pub fn field(&self) -> CognitionHistoryFieldV1 {
        self.field
    }
    pub fn from_accepted<M: MerkleNodeReader>(
        parent: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        hashes: &mut CognitionHeadHashWorkV1,
        live: &ExecutionFence,
        stop: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        // Legacy direct constructor is valid only when the selected fact is
        // from this very accepted head. Inherited views need the real head API.
        if parent.selected().occurrence().fact_body_id()
            != parent.selected().occurrence().head_body_id()
        {
            return Err(bad());
        }
        Self::from_accepted_field(
            parent,
            tx,
            memory,
            hashes,
            live,
            stop,
            CognitionHistoryFieldV1::JournalEvents,
        )
    }
    pub fn from_accepted_at_head_v1<M: MerkleNodeReader>(
        parent: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
        head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        hashes: &mut CognitionHeadHashWorkV1,
        live: &ExecutionFence,
        stop: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        let mut value = Self::from_accepted_field(
            parent,
            tx,
            memory,
            hashes,
            live,
            stop,
            CognitionHistoryFieldV1::JournalEvents,
        )?;
        value.bind_actual_parent_head(head, hashes)?;
        Ok(value)
    }
    fn bind_actual_parent_head(
        &mut self,
        head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
        hashes: &mut CognitionHeadHashWorkV1,
    ) -> Result<(), ValueTreeError> {
        hashes.hash_nodes_remaining =
            hashes
                .hash_nodes_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Current Journal actual head work",
                ))?;
        hashes.input_bytes_remaining =
            hashes
                .input_bytes_remaining
                .checked_sub(32)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Current Journal actual head bytes",
                ))?;
        if head.verified_record_id() != self.parent.selected().occurrence().head_record_id() {
            return Err(bad());
        }
        self.height = head.record().position.checked_add(1).ok_or_else(bad)?;
        Ok(())
    }
    /// Existing genuine Provision prefix. Activation of an absent economy is
    /// separate and requires actual Current absence evidence and a real index.
    pub fn from_accepted_provision_v1<M: MerkleNodeReader>(
        parent: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        hashes: &mut CognitionHeadHashWorkV1,
        live: &ExecutionFence,
        stop: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if parent.selected().occurrence().fact_body_id()
            != parent.selected().occurrence().head_body_id()
        {
            return Err(bad());
        }
        Self::from_accepted_field(
            parent,
            tx,
            memory,
            hashes,
            live,
            stop,
            CognitionHistoryFieldV1::EconomyProvisionJournal,
        )
    }
    fn from_accepted_field<M: MerkleNodeReader>(
        parent: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        hashes: &mut CognitionHeadHashWorkV1,
        live: &ExecutionFence,
        mut stop: impl FnMut() -> bool,
        field: CognitionHistoryFieldV1,
    ) -> Result<Self, ValueTreeError> {
        let (head_index, index_index, _) = field_profile(field)?;
        let view = parent.selected();
        let source = parent.heads()[head_index].as_ref().ok_or_else(bad)?;
        let index = parent.indexes()[index_index].as_ref().ok_or_else(bad)?;
        tx.validate_guard(&tx.guard(), live).map_err(|_| bad())?;
        if !parent.uses_budget(memory)
            || !view.uses_budget(memory)
            || view.class() != MemoryClass::Ordinary
            || view.manifest().roots != tx.parent_fence().parent_roots
            || view.occurrence().head_body_id() != tx.parent_fence().parent_commit
            || !index.uses_budget(memory)
            || index.field() != field
            || index.allocation_class() != MemoryClass::Ordinary
            || index.head_record_id() != view.occurrence().head_record_id()
            || index.roots_id() != view.manifest().roots
            || index.root() != &view.manifest().index_ref(field).root
            || index.record_count() != view.manifest().index_ref(field).records
        {
            return Err(bad());
        }
        AuthenticatedCognitionHeadSourceV1::validate_accepted_source_v3(
            source, view, field, memory, hashes, &mut stop,
        )?;
        // A one-ahead head is the legacy committed terminal-marker recovery
        // case, not permission to skip a missing event in an ordinary append.
        // The original validator rejects every gap in the durable prefix.
        if view.manifest().heads[head_index].sequence != index.record_count() {
            return Err(ValueTreeError::Invalid(
                "Current Cognition Journal requires terminal-marker recovery",
            ));
        }
        charge_frontier_copy(hashes)?;
        let builder = CognitionHeadHashBuilderV1::append_to(
            &source.state,
            memory,
            MemoryClass::Ordinary,
            &mut stop,
        )?;
        let fee = memory
            .reserve(std::mem::size_of::<Self>() + 4096, MemoryClass::Ordinary)?
            .retain(());
        Ok(Self {
            field,
            height: view
                .occurrence()
                .location()
                .height
                .checked_add(1)
                .ok_or_else(bad)?,
            parent,
            builder: Some(builder),
            root: index.root().clone(),
            count: index.record_count(),
            base_count: index.record_count(),
            head_present: view.manifest().heads[head_index].digest.is_some(),
            sequence: view.manifest().heads[head_index].sequence,
            last: source.last,
            guard: tx.guard(),
            appended: Vec::new(),
            _appended_fee: memory.reserve(0, MemoryClass::Ordinary)?,
            _fee: fee,
        })
    }
    /// Hash the actual provisional frontier, then reopen that same frontier
    /// for the next service append. This is DATA, never an authenticated head.
    /// A failed digest/copy leaves the owner unfinishable, just like append.
    pub fn current_digest(
        &mut self,
        memory: &MemoryBudget,
        hashes: &mut CognitionHeadHashWorkV1,
        stop: impl FnMut() -> bool,
    ) -> Result<Option<[u8; 32]>, ValueTreeError> {
        self.current_digest_inner(true, memory, hashes, stop)
    }
    fn current_digest_inner(
        &mut self,
        respect_presence: bool,
        memory: &MemoryBudget,
        hashes: &mut CognitionHeadHashWorkV1,
        mut stop: impl FnMut() -> bool,
    ) -> Result<Option<[u8; 32]>, ValueTreeError> {
        let builder = self.builder.take().ok_or_else(bad)?;
        if !self._fee.uses_budget(memory) || !self.parent.uses_budget(memory) {
            return Err(bad());
        }
        if stop() {
            return Err(ValueTreeError::Cancelled);
        }
        if respect_presence && self.count == 0 && !self.head_present {
            self.builder = Some(builder);
            return Ok(None);
        }
        let state = builder.finish(&mut stop)?;
        let digest = state.digest(
            field_profile(self.field)?.2,
            self.count,
            self.sequence,
            hashes,
            &mut stop,
        )?;
        charge_frontier_copy(hashes)?;
        let builder = CognitionHeadHashBuilderV1::append_to(
            &state,
            memory,
            MemoryClass::Ordinary,
            &mut stop,
        )?;
        self.builder = Some(builder);
        Ok(Some(digest))
    }
    /// Historical append provenance, not a refreshed Current read permission.
    /// The pending frontier remains poisoned after any partial append failure.
    pub fn validate_evidence<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        tx.validate_transaction_identity(&self.guard)
            .map_err(|_| bad())?;
        tx.validate_guard(&tx.guard(), live).map_err(|_| bad())?;
        if self.builder.is_none()
            || !self._fee.uses_budget(memory)
            || !self.parent.uses_budget(memory)
            || self.parent.selected().manifest().roots != tx.parent_fence().parent_roots
            || self.parent.selected().occurrence().head_body_id() != tx.parent_fence().parent_commit
        {
            return Err(bad());
        }
        Ok(())
    }
    /// Point-read the actual retained provisional index, never a caller root.
    /// The caller must additionally bind a newly appended row to its actual
    /// transaction Event fact before using its Original as business evidence.
    pub fn read_index_point<M, P, R, E, C>(
        &self,
        ordinal: u64,
        tx: &ExecutionTransaction<'_, M>,
        read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
        work: &mut CognitionHistoryIndexWorkV1,
        live: &ExecutionFence,
    ) -> Result<ReservedValue<CognitionHistoryIndexPointV1>, ValueTreeError>
    where
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        self.validate_evidence(tx, read.values.memory, live)?;
        if self.field != CognitionHistoryFieldV1::JournalEvents {
            return Err(bad());
        }
        read.check()?;
        if ordinal >= self.count || read.values.class != MemoryClass::Ordinary {
            return Err(bad());
        }
        super::super::index::read_cognition_history_index_point_parts_v1(
            &self.root,
            CognitionHistoryFieldV1::JournalEvents,
            ordinal,
            read,
            work,
        )
    }
    fn record_appended_digest(
        &mut self,
        ordinal: u64,
        digest: [u8; 32],
        memory: &MemoryBudget,
        work: &mut CognitionHeadHashWorkV1,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if stop() {
            return Err(ValueTreeError::Cancelled);
        }
        let first = self.base_count;
        if first.checked_add(self.appended.len() as u64) != Some(ordinal) {
            return Err(bad());
        }
        if self.appended.len() == self.appended.capacity() {
            let capacity = self
                .appended
                .capacity()
                .checked_mul(2)
                .ok_or_else(bad)?
                .max(4);
            let bytes = capacity
                .checked_mul(std::mem::size_of::<(u64, [u8; 32])>())
                .ok_or_else(bad)?;
            charge_digest_notes(work, self.appended.len())?;
            let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
            let mut next = Vec::new();
            next.try_reserve_exact(capacity).map_err(|_| bad())?;
            if next.capacity() != capacity {
                return Err(bad());
            }
            for chunk in self.appended.chunks(128) {
                if stop() {
                    return Err(ValueTreeError::Cancelled);
                }
                next.extend_from_slice(chunk);
            }
            let old = std::mem::replace(&mut self.appended, next);
            let old_fee = std::mem::replace(&mut self._appended_fee, fee);
            drop(old);
            drop(old_fee);
        }
        charge_digest_notes(work, 1)?;
        self.appended.push((ordinal, digest));
        Ok(())
    }
    /// A paid copy of only the actual newly appended suffix. This DATA is
    /// consumed together with this genuine Pending owner by the evidence
    /// directory; a caller-created Vec cannot replace that source obligation.
    pub fn appended_semantic_digests_v2<M: MerkleNodeReader>(
        &self,
        from: u64,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        work: &mut CognitionHeadHashWorkV1,
        live: &ExecutionFence,
        mut stop: impl FnMut() -> bool,
    ) -> Result<ReservedValue<CognitionJournalHashRowsV1>, ValueTreeError> {
        self.validate_evidence(tx, memory, live)?;
        if self.field != CognitionHistoryFieldV1::JournalEvents {
            return Err(bad());
        }
        let first = self.parent.indexes()[0]
            .as_ref()
            .ok_or_else(bad)?
            .record_count();
        if from < first
            || from > self.count
            || first.checked_add(self.appended.len() as u64) != Some(self.count)
        {
            return Err(bad());
        }
        let start = usize::try_from(from - first).map_err(|_| bad())?;
        let notes = &self.appended[start..];
        charge_digest_notes(work, notes.len())?;
        let bytes = notes
            .len()
            .checked_mul(std::mem::size_of::<(u64, [u8; 32])>())
            .and_then(|n| n.checked_add(std::mem::size_of::<Vec<(u64, [u8; 32])>>()))
            .ok_or_else(bad)?;
        let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
        let mut copied = Vec::new();
        copied.try_reserve_exact(notes.len()).map_err(|_| bad())?;
        if copied.capacity() != notes.len() {
            return Err(bad());
        }
        for chunk in notes.chunks(128) {
            if stop() {
                return Err(ValueTreeError::Cancelled);
            }
            copied.extend_from_slice(chunk);
        }
        if stop() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(fee.retain(copied))
    }
    /// Borrow actual retained DATA only. Callers still validate this owner and
    /// compare against the genuine checked/sealed source before any promotion.
    pub fn index_root_data_v2(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn count(&self) -> u64 {
        self.count
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn last_digest(&self) -> Option<[u8; 32]> {
        self.last
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Bounded inputs are independently validated"
    )]
    pub fn append<
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter,
        CW: FnMut() -> bool,
    >(
        &mut self,
        raw: &[u8],
        id: [u8; 32],
        location: FactLocationV1,
        tx: &ExecutionTransaction<'_, M>,
        read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        indexes: &mut CognitionHistoryIndexWorkV1,
        cbor: &mut CognitionHistoryCborWorkV1,
        hashes: &mut CognitionHeadHashWorkV1,
        live: &ExecutionFence,
    ) -> Result<(), MemberRecordError> {
        // A failed append may have advanced hash or index construction. Never
        // expose that partial frontier to retry or finish through the same owner.
        let mut builder = self.builder.take().ok_or_else(bad)?;
        tx.validate_transaction_identity(&self.guard)
            .map_err(|e| MemberRecordError::Io(std::io::Error::other(e)))?;
        tx.validate_guard(&tx.guard(), live)
            .map_err(|e| MemberRecordError::Io(std::io::Error::other(e)))?;
        if location.original != id
            || !self._fee.uses_budget(read.values.memory)
            || !write.memory.same_pool(read.values.memory)
            || !tx
                .ordered_fact(usize::try_from(location.ordinal).map_err(|_| bad())?)
                .is_some_and(|f| f.kind == ExecutionFactKindV1::Event && f.id == id)
            || location.height != self.height
        {
            return Err(bad().into());
        }
        let schema = verify_cognition_history_original_record_schema_v1(
            CognitionOriginalCborRequestV1 {
                field: self.field,
                id,
                bytes: raw.len() as u64,
            },
            &mut &raw[..],
            read.values.memory,
            MemoryClass::Ordinary,
            cbor,
            || (read.cancelled)() || (write.cancelled)(),
        )?;
        let semantic = schema.semantic();
        let next = self.count.checked_add(1).ok_or_else(bad)?;
        if semantic.sequence() != Some(next) || semantic.parent() != Some(self.last) {
            return Err(bad().into());
        }
        let CognitionOriginalIdentityStatusV1::DigestMatched(digest) = semantic.identity_status()
        else {
            return Err(bad().into());
        };
        builder.append(raw, hashes, || (read.cancelled)() || (write.cancelled)())?;
        let (root, count) = super::super::index::append_cognition_history_index_parts_v3(
            &self.root,
            self.field,
            self.count,
            CognitionHistoryIndexRowV1 {
                location,
                encoded_bytes: raw.len() as u64,
            },
            read,
            write,
            indexes,
        )?;
        if count != next {
            return Err(bad().into());
        }
        self.record_appended_digest(self.count, digest, read.values.memory, hashes, &mut || {
            (read.cancelled)() || (write.cancelled)()
        })?;
        self.root = root;
        self.count = count;
        self.sequence = next;
        self.last = Some(digest);
        self.builder = Some(builder);
        Ok(())
    }
    pub fn finish<M: MerkleNodeReader>(
        mut self,
        tx: &ExecutionTransaction<'_, M>,
        hashes: &mut CognitionHeadHashWorkV1,
        live: &ExecutionFence,
        mut stop: impl FnMut() -> bool,
    ) -> Result<WrittenCurrentCognitionJournalV1<'a>, ValueTreeError> {
        tx.validate_transaction_identity(&self.guard)
            .map_err(|_| bad())?;
        tx.validate_guard(&tx.guard(), live).map_err(|_| bad())?;
        let state = self.builder.take().ok_or_else(bad)?.finish(&mut stop)?;
        let digest = state.digest(
            field_profile(self.field)?.2,
            self.count,
            self.sequence,
            hashes,
            &mut stop,
        )?;
        Ok(WrittenCurrentCognitionJournalV1 {
            field: self.field,
            parent: self.parent,
            state,
            root: self.root,
            count: self.count,
            sequence: self.sequence,
            head_present: self.head_present,
            last: self.last,
            digest,
            guard: tx.guard(),
            _fee: self._fee,
        })
    }
}
impl WrittenCurrentCognitionJournalV1<'_> {
    pub fn field(&self) -> CognitionHistoryFieldV1 {
        self.field
    }
    pub fn index_root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn records(&self) -> u64 {
        self.count
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    pub fn last_digest(&self) -> Option<[u8; 32]> {
        self.last
    }
    pub fn cache(&self) -> &ReservedValue<CognitionHeadHashStateV1> {
        &self.state
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        tx.validate_transaction_identity(&self.guard)
            .map_err(|_| bad())?;
        tx.validate_guard(&tx.guard(), live).map_err(|_| bad())?;
        if !self._fee.uses_budget(memory)
            || !self.state.uses_budget(memory)
            || !self.parent.uses_budget(memory)
            || self.parent.selected().manifest().roots != tx.parent_fence().parent_roots
        {
            return Err(bad());
        }
        Ok(())
    }
}

/// Owned frontier retained across the real Host's borrowed parent IO scope.
/// This is provisional DATA, never an accepted history or head qualification.
pub struct RetainedWrittenCurrentCognitionJournalV1 {
    field: CognitionHistoryFieldV1,
    state: ReservedValue<CognitionHeadHashStateV1>,
    root: StateRootV1,
    count: u64,
    sequence: u64,
    last: Option<[u8; 32]>,
    digest: [u8; 32],
    head_digest: Option<[u8; 32]>,
    parent_roots: [u8; 32],
    guard: PreparedGuard,
    _fee: ReservedValue<()>,
    _retained_fee: MemoryReservation,
}
impl WrittenCurrentCognitionJournalV1<'_> {
    pub fn retain_owned_v1<M: MerkleNodeReader>(
        self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<RetainedWrittenCurrentCognitionJournalV1, ValueTreeError> {
        self.validate(tx, memory, live)?;
        let retained_fee = memory.reserve(
            std::mem::size_of::<RetainedWrittenCurrentCognitionJournalV1>(),
            MemoryClass::Ordinary,
        )?;
        let head_digest = if self.count == 0 && !self.head_present {
            None
        } else {
            Some(self.digest)
        };
        Ok(RetainedWrittenCurrentCognitionJournalV1 {
            head_digest,
            field: self.field,
            parent_roots: self.parent.selected().manifest().roots,
            state: self.state,
            root: self.root,
            count: self.count,
            sequence: self.sequence,
            last: self.last,
            digest: self.digest,
            guard: self.guard,
            _fee: self._fee,
            _retained_fee: retained_fee,
        })
    }
}
impl RetainedWrittenCurrentCognitionJournalV1 {
    pub fn field(&self) -> CognitionHistoryFieldV1 {
        self.field
    }
    pub fn index_root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn records(&self) -> u64 {
        self.count
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    pub fn head_digest_v1(&self) -> Option<[u8; 32]> {
        self.head_digest
    }
    pub fn last_digest(&self) -> Option<[u8; 32]> {
        self.last
    }
    pub fn cache(&self) -> &ReservedValue<CognitionHeadHashStateV1> {
        &self.state
    }
    pub fn validate_evidence_v1<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        tx.validate_transaction_identity(&self.guard)
            .map_err(|_| bad())?;
        tx.validate_guard(&tx.guard(), live).map_err(|_| bad())?;
        if self.parent_roots != tx.parent_fence().parent_roots
            || !self.state.uses_budget(memory)
            || !self._fee.uses_budget(memory)
        {
            return Err(bad());
        }
        Ok(())
    }
}

/// Both mandatory Economy history frontiers. Opening is fixed to the genuine
/// paired accepted prefix or a real absent Current Economy cohort. These are
/// provisional DATA; complete candidate/source closure remains mandatory.
pub struct PendingCurrentCognitionEconomyV1<'a> {
    journal: PendingCurrentCognitionJournalV1<'a>,
    provision: PendingCurrentCognitionJournalV1<'a>,
    absence: Option<ReservedValue<crate::incremental_cognition_groups::CognitionGroupExpectedV1>>,
    _fee: ReservedValue<()>,
}
impl<'a> PendingCurrentCognitionEconomyV1<'a> {
    pub fn journal(&self) -> &PendingCurrentCognitionJournalV1<'a> {
        &self.journal
    }
    pub fn provision(&self) -> &PendingCurrentCognitionJournalV1<'a> {
        &self.provision
    }
    pub fn provision_mut(&mut self) -> &mut PendingCurrentCognitionJournalV1<'a> {
        &mut self.provision
    }
    pub fn activation_expected(
        &self,
    ) -> Option<&ReservedValue<crate::incremental_cognition_groups::CognitionGroupExpectedV1>> {
        self.absence.as_ref()
    }
    /// No naked roots, flag, or empty index can select the absent branch.
    #[allow(clippy::too_many_arguments)]
    pub fn open_v1<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter + ValueNodeReader,
        CW: FnMut() -> bool,
    >(
        parent: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
        actual_head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
        current:&ReservedValue<crate::incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3>,
        roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
        ctx: &mut crate::incremental_field_overlay::WorldFieldOverlayContextV1<
            '_,
            '_,
            '_,
            M,
            R,
            E,
            C,
        >,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        indexes: &mut CognitionHistoryIndexWorkV1,
        hashes: &mut CognitionHeadHashWorkV1,
        live: &ExecutionFence,
    ) -> Result<Self, crate::incremental_field_overlay::WorldFieldOverlayErrorV1> {
        use crate::incremental_cognition_groups::{
            CognitionOptionalGroupV1, read_cognition_group_overlay_v1,
        };
        use crate::incremental_cognition_history::CognitionHistoryCurrentSourceV1;
        let result =
            (|| {
                ctx.transaction
                    .validate_guard(&ctx.transaction.guard(), live)?;
                current.validate_binding(roots)?;
                if !current.uses_budget(ctx.values.memory)
                    || current.allocation_class() != MemoryClass::Ordinary
                    || !write.memory.same_pool(ctx.values.memory)
                {
                    return Err(bad().into());
                }
                let CognitionHistoryCurrentSourceV1::Enabled(base) = current.history_source()
                else {
                    return Err(bad().into());
                };
                indexes.cells_remaining = indexes.cells_remaining.checked_sub(1).ok_or(
                    ValueTreeError::ResourceLimited("Current Economy frontier header work"),
                )?;
                let fee = ctx
                    .values
                    .memory
                    .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?
                    .retain(());
                let group =
                    read_cognition_group_overlay_v1(CognitionOptionalGroupV1::Economy, base, ctx)?;
                let view = parent.selected();
                if !parent.uses_budget(ctx.values.memory)
                    || view.class() != MemoryClass::Ordinary
                    || view.manifest().roots != ctx.transaction.parent_fence().parent_roots
                    || view.occurrence().head_body_id()
                        != ctx.transaction.parent_fence().parent_commit
                {
                    return Err(bad().into());
                }
                let mut stop = || (ctx.cancelled)() || (write.cancelled)();
                if group.present() {
                    let mut journal = PendingCurrentCognitionJournalV1::from_accepted_field(
                        parent,
                        ctx.transaction,
                        ctx.values.memory,
                        hashes,
                        live,
                        &mut stop,
                        CognitionHistoryFieldV1::EconomyJournal,
                    )?;
                    let mut provision = PendingCurrentCognitionJournalV1::from_accepted_field(
                        parent,
                        ctx.transaction,
                        ctx.values.memory,
                        hashes,
                        live,
                        &mut stop,
                        CognitionHistoryFieldV1::EconomyProvisionJournal,
                    )?;
                    journal.bind_actual_parent_head(actual_head, hashes)?;
                    provision.bind_actual_parent_head(actual_head, hashes)?;
                    return Ok(Self {
                        journal,
                        provision,
                        absence: None,
                        _fee: fee,
                    });
                }
                if view.manifest().indexes[2].present
                    || view.manifest().indexes[3].present
                    || parent.indexes()[2].is_some()
                    || parent.indexes()[3].is_some()
                    || parent.heads()[1].is_some()
                    || parent.heads()[2].is_some()
                    || view.manifest().heads[1..]
                        .iter()
                        .any(|h| h.sequence != 0 || h.digest.is_some())
                {
                    return Err(bad().into());
                }
                let mut empty = |field| -> Result<
                    PendingCurrentCognitionJournalV1<'a>,
                    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
                > {
                    let root = super::super::index::prepare_cognition_history_index_v1(
                        field,
                        0,
                        std::iter::empty::<Result<CognitionHistoryIndexRowV1, ValueTreeError>>(),
                        write,
                        indexes,
                    )?;
                    charge_frontier_copy(hashes)?;
                    let builder = CognitionHeadHashBuilderV1::new(
                        ctx.values.memory,
                        MemoryClass::Ordinary,
                        || (ctx.cancelled)() || (write.cancelled)(),
                    )?;
                    let fee = ctx
                        .values
                        .memory
                        .reserve(
                            std::mem::size_of::<PendingCurrentCognitionJournalV1<'a>>() + 4096,
                            MemoryClass::Ordinary,
                        )
                        .map_err(ValueTreeError::Memory)?
                        .retain(());
                    if actual_head.verified_record_id() != view.occurrence().head_record_id() {
                        return Err(bad().into());
                    }
                    Ok(PendingCurrentCognitionJournalV1 {
                        field,
                        height: actual_head
                            .record()
                            .position
                            .checked_add(1)
                            .ok_or_else(bad)?,
                        parent,
                        builder: Some(builder),
                        root,
                        count: 0,
                        base_count: 0,
                        head_present: false,
                        sequence: 0,
                        last: None,
                        guard: ctx.transaction.guard(),
                        appended: Vec::new(),
                        _appended_fee: ctx
                            .values
                            .memory
                            .reserve(0, MemoryClass::Ordinary)
                            .map_err(ValueTreeError::Memory)?,
                        _fee: fee,
                    })
                };
                let journal = empty(CognitionHistoryFieldV1::EconomyJournal)?;
                let provision = empty(CognitionHistoryFieldV1::EconomyProvisionJournal)?;
                Ok(Self {
                    journal,
                    provision,
                    absence: Some(group),
                    _fee: fee,
                })
            })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
    /// Bind the two provisional frontiers to the actual written Economy
    /// control rows. This also closes first activation of empty histories;
    /// parent absence alone can never turn into a present empty head.
    #[allow(clippy::too_many_arguments)]
    pub fn confirm_actual_heads_v1<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        &mut self,
        current:&ReservedValue<crate::incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3>,
        roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
        ctx: &mut crate::incremental_field_overlay::WorldFieldOverlayContextV1<
            '_,
            '_,
            '_,
            M,
            R,
            E,
            C,
        >,
        hashes: &mut CognitionHeadHashWorkV1,
        live: &ExecutionFence,
    ) -> Result<(), crate::incremental_field_overlay::WorldFieldOverlayErrorV1> {
        use crate::incremental_cognition_fields::CognitionControlFieldV1 as F;
        use crate::incremental_cognition_read::CognitionControlReadV1 as V;
        for (pending, seq, digest) in [
            (&mut self.journal, F::EconomyHeadSeq, F::EconomyHeadDigest),
            (
                &mut self.provision,
                F::EconomyProvisionHeadSeq,
                F::EconomyProvisionHeadDigest,
            ),
        ] {
            pending.validate_evidence(ctx.transaction, ctx.values.memory, live)?;
            let guard = ctx.transaction.guard();
            let actual=crate::incremental_cognition_control_overlay::read_current_cognition_economy_control_v1(seq,current,roots,ctx,&mut std::io::sink(),0,&guard,live)?;
            if actual != V::Unsigned(pending.sequence) {
                return Err(bad().into());
            }
            let hash = pending
                .current_digest_inner(false, ctx.values.memory, hashes, &mut ctx.cancelled)?
                .ok_or_else(bad)?;
            let mut text = [0u8; 71];
            text[..7].copy_from_slice(b"blake3:");
            const HEX: &[u8; 16] = b"0123456789abcdef";
            for (n, b) in hash.iter().enumerate() {
                text[7 + n * 2] = HEX[(b >> 4) as usize];
                text[8 + n * 2] = HEX[(b & 15) as usize];
            }
            struct Compare<'a> {
                expected: &'a [u8],
                offset: usize,
                equal: bool,
            }
            impl std::io::Write for Compare<'_> {
                fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                    let end = self
                        .offset
                        .checked_add(b.len())
                        .ok_or(std::io::ErrorKind::InvalidData)?;
                    if self.expected.get(self.offset..end) != Some(b) {
                        self.equal = false
                    }
                    self.offset = end;
                    Ok(b.len())
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            let mut compare = Compare {
                expected: &text,
                offset: 0,
                equal: true,
            };
            let guard = ctx.transaction.guard();
            let actual=crate::incremental_cognition_control_overlay::read_current_cognition_economy_control_v1(digest,current,roots,ctx,&mut compare,71,&guard,live)?;
            if actual != (V::Text { bytes: 71 }) || !compare.equal || compare.offset != 71 {
                return Err(bad().into());
            }
            pending.head_present = true;
        }
        Ok(())
    }
    pub fn finish_v1<M: MerkleNodeReader>(
        self,
        tx: &ExecutionTransaction<'_, M>,
        hashes: &mut CognitionHeadHashWorkV1,
        live: &ExecutionFence,
        mut stop: impl FnMut() -> bool,
    ) -> Result<
        (
            WrittenCurrentCognitionJournalV1<'a>,
            WrittenCurrentCognitionJournalV1<'a>,
        ),
        ValueTreeError,
    > {
        if self.absence.is_some() && (!self.journal.head_present || !self.provision.head_present) {
            return Err(bad());
        }
        let journal = self.journal.finish(tx, hashes, live, &mut stop)?;
        let provision = self.provision.finish(tx, hashes, live, &mut stop)?;
        Ok((journal, provision))
    }
}
