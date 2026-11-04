//! Authenticate actual record bytes while feeding the original head cache.
//! A failed read poisons the private accumulator; no historical DTO is retained.
use crate::incremental_cognition_history::*;
use crate::incremental_cognition_history_cbor::{
    CognitionHistoryCborWorkV1, CognitionOriginalCborRequestV1,
    verify_cognition_history_original_record_schema_v1,
};
use crate::incremental_cognition_history_hash_outboard::*;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    cell::RefCell,
    io::{self, Read},
};

pub struct AuthenticatedHeadStreamV1 {
    builder: CognitionHeadHashBuilderV1,
    failed: bool,
    next: u64,
    field: CognitionHistoryFieldV1,
    head: [u8; 32],
    roots: [u8; 32],
    expected: u64,
    last: Option<[u8; 32]>,
    last_record: Option<CognitionHeadSourceLastV3>,
}
impl AuthenticatedHeadStreamV1 {
    pub fn new(
        selected: &ReservedValue<AcceptedCognitionHistoryViewV1>,
        index: &ReservedValue<CognitionHistoryIndexQualificationV1>,
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let field = index.field();
        if field == CognitionHistoryFieldV1::Responses
            || !selected.uses_budget(memory)
            || selected.class() != MemoryClass::Ordinary
            || selected.reserved_bytes() < 16 << 10
            || index.reserved_bytes() < std::mem::size_of::<CognitionHistoryIndexQualificationV1>()
            || !index.uses_budget(memory)
            || index.class() != MemoryClass::Ordinary
        {
            return Err(ValueTreeError::Invalid("authenticated head stream source"));
        }
        index.validate_binding(field, selected.manifest().index_ref(field))?;
        Ok(Self {
            builder: CognitionHeadHashBuilderV1::new(memory, MemoryClass::Ordinary, cancelled)?,
            failed: false,
            next: 0,
            field,
            head: selected.occurrence().head_record_id(),
            roots: selected.manifest().roots,
            expected: index.record_count(),
            last: None,
            last_record: None,
        })
    }
    pub fn push<R: Read, C: FnMut() -> bool>(
        &mut self,
        original: &oasis7_proto::memory_budget::ReservedValue<IndexedCognitionOriginalV1>,
        selected: &oasis7_proto::memory_budget::ReservedValue<AcceptedCognitionHistoryViewV1>,
        input: &mut R,
        memory: &MemoryBudget,
        cbor: &mut CognitionHistoryCborWorkV1,
        hashes: &mut CognitionHeadHashWorkV1,
        cancelled: C,
    ) -> Result<(), ValueTreeError> {
        let invalid = || ValueTreeError::Invalid("authenticated head stream");
        if self.failed {
            return Err(invalid());
        }
        self.failed = true;
        if self.next >= self.expected
            || original.schema().semantic().field() != self.field
            || selected.occurrence().head_record_id() != self.head
            || selected.manifest().roots != self.roots
            || !original.uses_budget(memory)
            || original.reserved_bytes() < 2048
            || original.schema().semantic().class() != MemoryClass::Ordinary
            || !selected.uses_budget(memory)
            || selected.class() != MemoryClass::Ordinary
            || original.head_record_id() != &selected.occurrence().head_record_id()
            || original.roots_id() != &selected.manifest().roots
            || original.head_body_id() != &selected.occurrence().head_body_id()
        {
            return Err(invalid());
        }
        let cancelled = RefCell::new(cancelled);
        let mut tee = HashRead {
            input,
            builder: &mut self.builder,
            hashes,
            cancelled: &cancelled,
            failure: None,
        };
        if original.ordinal() != self.next {
            return Err(invalid());
        }
        let row = original.row();
        let parsed = verify_cognition_history_original_record_schema_v1(
            CognitionOriginalCborRequestV1 {
                field: original.schema().semantic().field(),
                id: row.location.original,
                bytes: row.encoded_bytes,
            },
            &mut tee,
            memory,
            MemoryClass::Ordinary,
            cbor,
            || (cancelled.borrow_mut())(),
        );
        if let Some(error) = tee.failure.take() {
            return Err(error);
        }
        let parsed = parsed?;
        let semantic = parsed.semantic();
        if semantic.sequence() != self.next.checked_add(1) || semantic.parent() != Some(self.last) {
            return Err(invalid());
        }
        let CognitionOriginalIdentityStatusV1::DigestMatched(digest) = semantic.identity_status()
        else {
            return Err(invalid());
        };
        self.last = Some(digest);
        self.last_record = Some(CognitionHeadSourceLastV3 {
            sequence: self.next.checked_add(1).ok_or_else(invalid)?,
            semantic_digest: digest,
            location: row.location,
            original_bytes: row.encoded_bytes,
        });
        if (cancelled.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        self.next = self.next.checked_add(1).ok_or_else(invalid)?;
        self.failed = false;
        Ok(())
    }
}
pub struct AuthenticatedCognitionHeadSourceV1 {
    state: ReservedValue<CognitionHeadHashStateV1>,
    field: CognitionHistoryFieldV1,
    head: [u8; 32],
    roots: [u8; 32],
    records: u64,
    last: Option<[u8; 32]>,
    last_record: Option<CognitionHeadSourceLastV3>,
}
impl AuthenticatedHeadStreamV1 {
    pub fn finish(
        self,
        selected: &ReservedValue<AcceptedCognitionHistoryViewV1>,
        memory: &MemoryBudget,
        work: &mut CognitionHeadHashWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<AuthenticatedCognitionHeadSourceV1>, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if self.failed
            || self.next != self.expected
            || !selected.uses_budget(memory)
            || selected.class() != MemoryClass::Ordinary
            || selected.occurrence().head_record_id() != self.head
            || selected.manifest().roots != self.roots
        {
            return Err(ValueTreeError::Invalid(
                "authenticated head stream incomplete",
            ));
        }
        let (slot, kind) = match self.field {
            CognitionHistoryFieldV1::JournalEvents => (0, CognitionHeadHashV1::Journal),
            CognitionHistoryFieldV1::EconomyJournal => (1, CognitionHeadHashV1::Economy),
            CognitionHistoryFieldV1::EconomyProvisionJournal => (2, CognitionHeadHashV1::Provision),
            CognitionHistoryFieldV1::Responses => {
                return Err(ValueTreeError::Invalid("response head stream"));
            }
        };
        let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
        let state = self.builder.finish(&mut cancelled)?;
        let head = &selected.manifest().heads[slot];
        validate_head_source_sequence(self.field, self.expected, head.sequence)?;
        match head.digest {
            Some(expected)
                if state.digest(kind, self.expected, head.sequence, work, &mut cancelled)?
                    == expected => {}
            None if self.expected == 0 && head.sequence == 0 && state.body_bytes() == 0 => {}
            _ => return Err(ValueTreeError::Invalid("authenticated head stream digest")),
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(charge.retain(AuthenticatedCognitionHeadSourceV1 {
            state,
            field: self.field,
            head: self.head,
            roots: self.roots,
            records: self.expected,
            last: self.last,
            last_record: self.last_record,
        }))
    }
}
struct HashRead<'a, R, C> {
    input: &'a mut R,
    builder: &'a mut CognitionHeadHashBuilderV1,
    hashes: &'a mut CognitionHeadHashWorkV1,
    cancelled: &'a RefCell<C>,
    failure: Option<ValueTreeError>,
}
impl<R: Read, C: FnMut() -> bool> Read for HashRead<'_, R, C> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.failure.is_some() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let n = self.input.read(output)?;
        if let Err(error) = self.builder.append(&output[..n], self.hashes, || {
            (self.cancelled.borrow_mut())()
        }) {
            self.failure = Some(error);
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(n)
    }
}

impl AuthenticatedCognitionHeadSourceV1 {
    pub(crate) fn validate_accepted_source_v3(
        source: &ReservedValue<Self>,
        selected: &AcceptedCognitionHistoryViewV1,
        field: CognitionHistoryFieldV1,
        memory: &MemoryBudget,
        work: &mut CognitionHeadHashWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let slot = match field {
            CognitionHistoryFieldV1::JournalEvents => 0,
            CognitionHistoryFieldV1::EconomyJournal => 1,
            CognitionHistoryFieldV1::EconomyProvisionJournal => 2,
            CognitionHistoryFieldV1::Responses => {
                return Err(ValueTreeError::Invalid("response is not a head source"));
            }
        };
        let summary = selected
            .source_v3()
            .ok_or(ValueTreeError::Invalid("missing source profile"))?;
        if !source.uses_budget(memory)
            || source.reserved_bytes() < 4096
            || !source.state.uses_budget(memory)
            || source.state.reserved_bytes() < CognitionHeadHashBuilderV1::admission_bytes()
            || source.state.allocation_class() != selected.class()
            || source.field != field
            || source.head != selected.occurrence().head_record_id()
            || source.roots != selected.manifest().roots
            || source.records != selected.manifest().index_ref(field).records
            || source.last_record != summary.last[slot]
        {
            return Err(ValueTreeError::Invalid("accepted source component"));
        }
        let mut digest = CacheDigest(blake3::Hasher::new());
        source
            .state
            .write_canonical_cache_v1(&mut digest, work, &mut cancelled)?;
        if summary.cache_state_digests[slot] != Some(*digest.0.finalize().as_bytes()) {
            return Err(ValueTreeError::Invalid("accepted source cache capsule"));
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(())
    }
    pub fn validate_persisted_cache(
        source: &ReservedValue<Self>,
        cache: &ReservedValue<PersistedCognitionHeadCacheV1>,
        selected: &ReservedValue<AcceptedCognitionHistoryViewV1>,
        memory: &MemoryBudget,
        work: &mut CognitionHeadHashWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if !source.uses_budget(memory)
            || source.reserved_bytes() < 4096
            || !selected.uses_budget(memory)
            || selected.class() != MemoryClass::Ordinary
            || selected.occurrence().head_record_id() != source.head
            || selected.manifest().roots != source.roots
            || selected.manifest().index_ref(source.field).records != source.records
            || !source.state.uses_budget(memory)
            || source.state.allocation_class() != MemoryClass::Ordinary
            || !cache.uses_budget(memory)
            || cache.reserved_bytes() < 1024
            || !cache.state().uses_budget(memory)
            || cache.state().allocation_class() != MemoryClass::Ordinary
        {
            return Err(ValueTreeError::Invalid(
                "authenticated persisted cache pool/class",
            ));
        }
        let mut actual = CacheDigest(blake3::Hasher::new());
        source
            .state
            .write_canonical_cache_v1(&mut actual, work, &mut cancelled)?;
        let mut persisted = CacheDigest(blake3::Hasher::new());
        cache
            .state()
            .write_canonical_cache_v1(&mut persisted, work, &mut cancelled)?;
        if actual.0.finalize() != persisted.0.finalize() {
            return Err(ValueTreeError::Invalid(
                "authenticated persisted cache source",
            ));
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(())
    }
}
struct CacheDigest(blake3::Hasher);
impl io::Write for CacheDigest {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[path = "incremental_cognition_history_source_successor.rs"]
mod successor;
pub use successor::*;

#[path = "incremental_cognition_history_source_candidate.rs"]
mod candidate;
pub use candidate::*;
#[path = "incremental_cognition_history_source_cold_v3.rs"]
mod cold_v3;
pub use cold_v3::*;
#[path = "incremental_cognition_history_source_readmission.rs"]
mod readmission;
pub use readmission::*;

fn validate_head_source_sequence(
    field: CognitionHistoryFieldV1,
    records: u64,
    sequence: u64,
) -> Result<(), ValueTreeError> {
    let valid = sequence == records
        || (field == CognitionHistoryFieldV1::JournalEvents
            && records.checked_add(1) == Some(sequence));
    if valid {
        Ok(())
    } else {
        Err(ValueTreeError::Invalid("Cognition head source sequence"))
    }
}
