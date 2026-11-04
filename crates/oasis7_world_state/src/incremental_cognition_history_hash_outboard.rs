//! Provisional append-only cache for the ORIGINAL three canonical head hashes.
//! This does not prove canonical records, occurrence, accepted head, schema,
//! availability or History qualification. Those consumers must bind the cache.
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};

#[path = "incremental_cognition_history_hash_alignment.rs"]
mod alignment;
#[path = "incremental_cognition_history_hash_forest.rs"]
mod forest;

const CHUNK: usize = 1024; // BLAKE3's fixed chunk size, not the storage page size.
const HEADER_MAX: usize = 128;
const ALIGNMENTS: usize = 29;
// A dyadic cover [1,n) needs at most twice the bit width of n. Here n is
// additionally <= floor(u64::MAX/1024)+1 (<2^55), so 128 is conservative.
const SPANS: usize = 2 * u64::BITS as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionHeadHashV1 {
    Journal,
    Economy,
    Provision,
}
impl CognitionHeadHashV1 {
    pub const ALL: [Self; 3] = [Self::Journal, Self::Economy, Self::Provision];
    pub fn domain(self) -> &'static str {
        match self {
            Self::Journal => "oasis7.cognition.journal-head.v1",
            Self::Economy => "oasis7.cognition.economy.journal-head.v1",
            Self::Provision => "oasis7.cognition.economy.provisioning-journal.v1",
        }
    }
}
#[derive(Debug)]
pub struct CognitionHeadHashWorkV1 {
    pub input_bytes_remaining: u64,
    /// A chunk hash or binary parent merge is one bounded action (<=1024B).
    pub hash_nodes_remaining: u64,
}
impl CognitionHeadHashWorkV1 {
    fn node(&mut self) -> Result<(), ValueTreeError> {
        self.hash_nodes_remaining = self
            .hash_nodes_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("cognition head hash nodes"))?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Span {
    start: u64,
    height: u8,
    cv: [u8; 32],
}
impl Span {
    const EMPTY: Self = Self {
        start: 0,
        height: 0,
        cv: [0; 32],
    };
}
#[derive(Clone, Copy)]
struct Row {
    kind: CognitionHeadHashV1,
    prefix_len: u16,
    complete: u64,
    tail_len: u16,
    tail: [u8; CHUNK],
    used: u8,
    spans: [Span; SPANS],
}
impl Row {
    const EMPTY: Self = Self {
        kind: CognitionHeadHashV1::Journal,
        prefix_len: 0,
        complete: 0,
        tail_len: 0,
        tail: [0; CHUNK],
        used: 0,
        spans: [Span::EMPTY; SPANS],
    };
}
#[derive(Clone, Copy)]
struct Data {
    body_bytes: u64,
    first: [u8; CHUNK],
    rows: [Row; ALIGNMENTS],
}
impl Data {
    const EMPTY: Self = Self {
        body_bytes: 0,
        first: [0; CHUNK],
        rows: [Row::EMPTY; ALIGNMENTS],
    };
}

pub struct CognitionHeadHashStateV1 {
    data: Box<Data>,
    class: MemoryClass,
}
impl CognitionHeadHashStateV1 {
    pub(crate) fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn body_bytes(&self) -> u64 {
        self.data.body_bytes
    }
    pub fn digest<C: FnMut() -> bool>(
        &self,
        kind: CognitionHeadHashV1,
        count: u64,
        seq: u64,
        work: &mut CognitionHeadHashWorkV1,
        mut cancelled: C,
    ) -> Result<[u8; 32], ValueTreeError> {
        check(&mut cancelled)?;
        // Every canonical record occupies >=1 byte. Exact record boundaries
        // are a separate consumer obligation, never inferred by this cache.
        if count > self.data.body_bytes {
            return Err(invalid());
        }
        let header = alignment::header(kind, count, seq)?;
        let row = self
            .data
            .rows
            .iter()
            .find(|row| row.kind == kind && usize::from(row.prefix_len) == header.prefix.len())
            .ok_or_else(invalid)?;
        let result = row.digest(
            &self.data.first,
            self.data.body_bytes,
            &header,
            work,
            &mut cancelled,
        )?;
        check(&mut cancelled)?;
        Ok(result)
    }
}

/// Owns one pre-paid fixed-capacity cache. Any error/panic during append makes
/// it unfinishable, so partial per-alignment progress never becomes a state.
pub struct CognitionHeadHashBuilderV1 {
    data: Box<Data>,
    class: MemoryClass,
    charge: MemoryReservation,
    failed: bool,
}
impl CognitionHeadHashBuilderV1 {
    pub fn new<C: FnMut() -> bool>(
        memory: &MemoryBudget,
        class: MemoryClass,
        mut cancelled: C,
    ) -> Result<Self, ValueTreeError> {
        check(&mut cancelled)?;
        let charge = memory.reserve(Self::admission_bytes(), class)?;
        let mut data = Box::new(Data::EMPTY);
        alignment::initialize(&mut data)?;
        check(&mut cancelled)?;
        Ok(Self {
            data,
            class,
            charge,
            failed: false,
        })
    }
    pub fn append_to<C: FnMut() -> bool>(
        parent: &ReservedValue<CognitionHeadHashStateV1>,
        memory: &MemoryBudget,
        class: MemoryClass,
        mut cancelled: C,
    ) -> Result<Self, ValueTreeError> {
        check(&mut cancelled)?;
        if !parent.uses_budget(memory)
            || parent.class != class
            || parent.reserved_bytes() < Self::admission_bytes()
        {
            return Err(invalid());
        }
        let charge = memory.reserve(Self::admission_bytes(), class)?;
        let data = Box::new(*parent.data);
        check(&mut cancelled)?;
        Ok(Self {
            data,
            class,
            charge,
            failed: false,
        })
    }
    pub const fn admission_bytes() -> usize {
        2 * std::mem::size_of::<Data>() + (32 << 10)
    }
    pub fn append<C: FnMut() -> bool>(
        &mut self,
        bytes: &[u8],
        work: &mut CognitionHeadHashWorkV1,
        mut cancelled: C,
    ) -> Result<(), ValueTreeError> {
        if self.failed {
            return Err(invalid());
        }
        self.failed = true;
        check(&mut cancelled)?;
        work.input_bytes_remaining = work
            .input_bytes_remaining
            .checked_sub(bytes.len() as u64)
            .ok_or(ValueTreeError::ResourceLimited("cognition head hash input"))?;
        let after = self
            .data
            .body_bytes
            .checked_add(bytes.len() as u64)
            .filter(|after| *after <= u64::MAX - (2 * HEADER_MAX) as u64)
            .ok_or_else(invalid)?;
        let first_used = self.data.body_bytes.min(CHUNK as u64) as usize;
        let take = (CHUNK - first_used).min(bytes.len());
        self.data.first[first_used..first_used + take].copy_from_slice(&bytes[..take]);
        for row in &mut self.data.rows {
            row.append(bytes, work, &mut cancelled)?;
        }
        self.data.body_bytes = after;
        check(&mut cancelled)?;
        self.failed = false;
        Ok(())
    }
    pub fn finish<C: FnMut() -> bool>(
        self,
        mut cancelled: C,
    ) -> Result<ReservedValue<CognitionHeadHashStateV1>, ValueTreeError> {
        if self.failed {
            return Err(invalid());
        }
        check(&mut cancelled)?;
        for row in &self.data.rows {
            row.validate(self.data.body_bytes)?;
            check(&mut cancelled)?;
        }
        Ok(self.charge.retain(CognitionHeadHashStateV1 {
            data: self.data,
            class: self.class,
        }))
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("cognition head hash cache")
}
fn check<C: FnMut() -> bool>(cancelled: &mut C) -> Result<(), ValueTreeError> {
    if cancelled() {
        Err(ValueTreeError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "incremental_cognition_history_hash_tests.rs"]
mod tests;

#[path = "incremental_cognition_history_hash_codec.rs"]
mod codec;
pub use codec::{COGNITION_HEAD_CACHE_BYTES_V1, read_cognition_head_cache_v1};

/// Fixed metadata resource conversion only; original-head and accepted-view
/// binding must still be checked by the consumer after this copy.
pub fn readmit_cognition_head_cache_ordinary_v1(
    source: &ReservedValue<CognitionHeadHashStateV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CognitionHeadHashStateV1>, ValueTreeError> {
    check(&mut cancelled)?;
    if !source.uses_budget(memory)
        || source.class != MemoryClass::Recovery
        || source.reserved_bytes() < CognitionHeadHashBuilderV1::admission_bytes()
    {
        return Err(invalid());
    }
    let charge = memory.reserve(
        CognitionHeadHashBuilderV1::admission_bytes(),
        MemoryClass::Ordinary,
    )?;
    let data = Box::new(*source.data);
    check(&mut cancelled)?;
    Ok(charge.retain(CognitionHeadHashStateV1 {
        data,
        class: MemoryClass::Ordinary,
    }))
}
