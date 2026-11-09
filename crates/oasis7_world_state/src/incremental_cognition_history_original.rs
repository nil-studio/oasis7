//! Continuous borrowed canonical encoding. Completion proves consumption and
//! the selected sink's report, not arbitrary external IO durability. Formal
//! original readback and accepted kind-bound occurrence remain mandatory.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionFactRefV1, ExecutionFence, ExecutionTransaction, PreparedGuard,
    },
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{
    MemoryBudget, MemoryClass, MemoryLimitExceeded, MemoryReservation, ReservedValue,
};
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamContextV1, CanonicalCborStreamError,
    CanonicalCborWorkV1, stream_canonical_cbor_v1,
};
use std::{
    cell::RefCell,
    io::{self, Write},
};
pub struct CognitionHistoryWriteContextV1<'a, C: FnMut() -> bool> {
    pub memory: &'a MemoryBudget,
    pub work: CanonicalCborWorkV1,
    pub records_remaining: u64,
    pub cancelled: C,
    poisoned: bool,
}
impl<'a, C: FnMut() -> bool> CognitionHistoryWriteContextV1<'a, C> {
    pub fn new(
        memory: &'a MemoryBudget,
        work: CanonicalCborWorkV1,
        records_remaining: u64,
        cancelled: C,
    ) -> Self {
        Self {
            memory,
            work,
            records_remaining,
            cancelled,
            poisoned: false,
        }
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    fn encode<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
        output: &mut dyn Write,
    ) -> Result<u64, ValueTreeError> {
        let allocator = Allocator {
            memory: self.memory,
            error: RefCell::new(None),
        };
        let work = std::mem::replace(
            &mut self.work,
            CanonicalCborWorkV1 {
                visits_remaining: 0,
                bytes_remaining: 0,
                max_depth: 0,
                max_map_keys: 0,
                max_key_bytes: 0,
            },
        );
        let mut context = CanonicalCborStreamContextV1::new(&allocator, work, &mut self.cancelled);
        let result = stream_canonical_cbor_v1(value, &mut context, output);
        self.work = context.work;
        if let Some(error) = allocator.error.into_inner() {
            return Err(ValueTreeError::Memory(error));
        }
        result
            .map(|summary| summary.encoded_bytes)
            .map_err(|error| match error {
                CanonicalCborStreamError::ResourceLimited(label) => {
                    ValueTreeError::ResourceLimited(label)
                }
                CanonicalCborStreamError::Invalid(label) => ValueTreeError::Invalid(label),
                CanonicalCborStreamError::Cancelled => ValueTreeError::Cancelled,
                CanonicalCborStreamError::Poisoned => {
                    ValueTreeError::Invalid("Cognition canonical context poisoned")
                }
                CanonicalCborStreamError::Io(error) => ValueTreeError::Io(error),
            })
    }
}
struct Allocator<'a> {
    memory: &'a MemoryBudget,
    error: RefCell<Option<MemoryLimitExceeded>>,
}
impl CanonicalCborAllocator for Allocator<'_> {
    type Charge = MemoryReservation;
    fn reserve(&self, bytes: usize) -> Result<Self::Charge, CanonicalCborStreamError> {
        self.memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|error| {
                *self.error.borrow_mut() = Some(error);
                CanonicalCborStreamError::ResourceLimited("Cognition canonical memory")
            })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionOriginalIdentityStatusV1 {
    DigestMatched([u8; 32]),
    MissingIdentity,
    ArtifactUnavailable,
}
#[derive(Debug)]
pub struct CognitionHistoryStoredOriginalV1 {
    field: CognitionHistoryFieldV1,
    ordinal: u64,
    id: [u8; 32],
    bytes: u64,
    semantic: CognitionOriginalIdentityStatusV1,
}
impl CognitionHistoryStoredOriginalV1 {
    pub fn field(&self) -> CognitionHistoryFieldV1 {
        self.field
    }
    pub fn ordinal(&self) -> u64 {
        self.ordinal
    }
    pub fn id(&self) -> &[u8; 32] {
        &self.id
    }
    pub fn encoded_bytes(&self) -> u64 {
        self.bytes
    }
    pub fn identity_status(&self) -> CognitionOriginalIdentityStatusV1 {
        self.semantic
    }
    pub fn fact(&self) -> ExecutionFactRefV1 {
        ExecutionFactRefV1 {
            kind: self.field.fact_kind(),
            id: self.id,
        }
    }
}
struct HashOutput<'a> {
    output: Option<&'a mut dyn Write>,
    hash: blake3::Hasher,
    bytes: u64,
}
impl Write for HashOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = match &mut self.output {
            Some(output) => output.write(bytes)?,
            None => bytes.len(),
        };
        if n > bytes.len() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.bytes = self
            .bytes
            .checked_add(n as u64)
            .ok_or(io::ErrorKind::InvalidData)?;
        self.hash.update(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn identity(value: &str) -> Result<[u8; 32], ValueTreeError> {
    let hex = value
        .strip_prefix("blake3:")
        .ok_or(ValueTreeError::Invalid(
            "Cognition original identity prefix",
        ))?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ValueTreeError::Invalid("Cognition original identity bytes"));
    }
    Ok(*blake3::Hash::from_hex(hex)
        .map_err(|_| ValueTreeError::Invalid("Cognition original identity hex"))?
        .as_bytes())
}
pub struct CognitionHistoryOriginalProducerV1<'s, 'c, 'm, C: FnMut() -> bool> {
    record: CognitionHistoryRecordRefV1<'s>,
    context: &'c mut CognitionHistoryWriteContextV1<'m, C>,
    metadata: CognitionHistoryStoredOriginalV1,
    charge: Option<MemoryReservation>,
    emitted: bool,
    failed: bool,
    complete: bool,
}
impl<C: FnMut() -> bool> Drop for CognitionHistoryOriginalProducerV1<'_, '_, '_, C> {
    fn drop(&mut self) {
        if !self.complete {
            self.context.poisoned = true;
        }
    }
}
impl<C: FnMut() -> bool> CognitionHistoryOriginalProducerV1<'_, '_, '_, C> {
    pub fn id(&self) -> &[u8; 32] {
        self.metadata.id()
    }
    pub fn encoded_bytes(&self) -> u64 {
        self.metadata.bytes
    }
    pub fn memory(&self) -> &MemoryBudget {
        self.context.memory
    }
    pub fn write_to(&mut self, output: &mut dyn Write) -> Result<(), ValueTreeError> {
        if self.failed || self.emitted || self.complete {
            self.failed = true;
            return Err(ValueTreeError::Invalid(
                "Cognition original producer reused",
            ));
        }
        self.failed = true;
        let mut writer = HashOutput {
            output: Some(output),
            hash: blake3::Hasher::new(),
            bytes: 0,
        };
        let bytes = self.context.encode(self.record.raw(), &mut writer)?;
        if bytes != self.metadata.bytes
            || writer.bytes != bytes
            || writer.hash.finalize().as_bytes() != &self.metadata.id
        {
            return Err(ValueTreeError::Invalid(
                "Cognition original changed during stream",
            ));
        }
        if (self.context.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        self.emitted = true;
        self.failed = false;
        Ok(())
    }
}
/// A real bridge must finish its original object before reporting success.
/// An arbitrary implementation's external durability is checked by formal IO.
pub trait CognitionHistoryOriginalSinkV1 {
    fn store<C: FnMut() -> bool>(
        &mut self,
        original: &mut CognitionHistoryOriginalProducerV1<'_, '_, '_, C>,
    ) -> Result<(), ValueTreeError>;
}
fn prepare<'s, 'c, 'm, C: FnMut() -> bool>(
    record: CognitionHistoryRecordRefV1<'s>,
    context: &'c mut CognitionHistoryWriteContextV1<'m, C>,
) -> Result<CognitionHistoryOriginalProducerV1<'s, 'c, 'm, C>, ValueTreeError> {
    if context.poisoned {
        return Err(ValueTreeError::Invalid(
            "Cognition History context poisoned",
        ));
    }
    context.poisoned = true;
    context.records_remaining = context
        .records_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("Cognition History records"))?;
    let charge = context.memory.reserve(8 << 10, MemoryClass::Ordinary)?;
    let declared = record
        .declared_identity()
        .map_err(ValueTreeError::Invalid)?
        .filter(|value| !value.is_empty());
    let semantic = match (declared, record.preimage()) {
        (Some(declared), Some(preimage)) => {
            let expected = identity(declared)?;
            let mut hash = HashOutput {
                output: None,
                hash: blake3::Hasher::new(),
                bytes: 0,
            };
            context.encode(&preimage, &mut hash)?;
            if hash.hash.finalize().as_bytes() != &expected {
                return Err(ValueTreeError::Invalid(
                    "Cognition original semantic digest",
                ));
            }
            CognitionOriginalIdentityStatusV1::DigestMatched(expected)
        }
        (Some(declared), None) => {
            identity(declared)?;
            CognitionOriginalIdentityStatusV1::ArtifactUnavailable
        }
        (None, _) => CognitionOriginalIdentityStatusV1::MissingIdentity,
    };
    let mut hash = HashOutput {
        output: None,
        hash: blake3::Hasher::new(),
        bytes: 0,
    };
    let bytes = context.encode(record.raw(), &mut hash)?;
    if bytes != hash.bytes {
        return Err(ValueTreeError::Invalid(
            "Cognition original measured length",
        ));
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let metadata = CognitionHistoryStoredOriginalV1 {
        field: record.field,
        ordinal: record.ordinal,
        id: *hash.hash.finalize().as_bytes(),
        bytes,
        semantic,
    };
    Ok(CognitionHistoryOriginalProducerV1 {
        record,
        context,
        metadata,
        charge: Some(charge),
        emitted: false,
        failed: false,
        complete: false,
    })
}
pub fn store_cognition_history_original_v1<
    C: FnMut() -> bool,
    S: CognitionHistoryOriginalSinkV1,
>(
    record: CognitionHistoryRecordRefV1<'_>,
    context: &mut CognitionHistoryWriteContextV1<'_, C>,
    sink: &mut S,
) -> Result<ReservedValue<CognitionHistoryStoredOriginalV1>, ValueTreeError> {
    let mut producer = prepare(record, context)?;
    sink.store(&mut producer)?;
    if producer.failed {
        return Err(ValueTreeError::Invalid(
            "Cognition original producer failed",
        ));
    }
    if !producer.emitted {
        return Err(ValueTreeError::Invalid(
            "Cognition History sink skipped producer",
        ));
    }
    if (producer.context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let metadata = CognitionHistoryStoredOriginalV1 {
        field: producer.metadata.field,
        ordinal: producer.metadata.ordinal,
        id: producer.metadata.id,
        bytes: producer.metadata.bytes,
        semantic: producer.metadata.semantic,
    };
    let charge = producer.charge.take().expect("producer retained charge");
    producer.context.poisoned = false;
    producer.complete = true;
    Ok(charge.retain(metadata))
}
pub fn contribute_cognition_history_original_v1<
    M: MerkleNodeReader,
    C: FnMut() -> bool,
    S: CognitionHistoryOriginalSinkV1,
>(
    record: CognitionHistoryRecordRefV1<'_>,
    transaction: &mut ExecutionTransaction<'_, M>,
    context: &mut CognitionHistoryWriteContextV1<'_, C>,
    sink: &mut S,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CognitionHistoryStoredOriginalV1>, WorldFieldOverlayErrorV1> {
    transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !tx.uses_budget(context.memory) {
            return Err(ValueTreeError::Invalid("Cognition History transaction pool").into());
        }
        let original = store_cognition_history_original_v1(record, context, sink)?;
        tx.append_fact(original.fact())?;
        if (context.cancelled)() {
            context.poisoned = true;
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(original)
    })
}
