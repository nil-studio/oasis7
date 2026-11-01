//! Borrowed concrete WorldEvent producer. A callback cannot acknowledge an
//! original without consuming its canonical stream and finishing its sink.
use super::BodyMutationError;
use crate::WorldEvent;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionError, incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{
    MemoryBudget, MemoryClass, MemoryLimitExceeded, MemoryReservation,
};
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamContextV1, CanonicalCborStreamError,
    CanonicalCborWorkV1, stream_canonical_cbor_v1,
};
use std::{
    cell::RefCell,
    io::{self, Write},
};

/// The persistence implementation must consume the source and finish all sink
/// barriers before returning success. A locator/ACK without full stream+finish
/// is not success. The production ObjectStore push API supplies this contract.
pub trait AgentEventOriginalSinkV1 {
    fn finish_original(
        &mut self,
        source: &mut dyn AgentEventOriginalStreamV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), BodyMutationError>;
}
impl<F> AgentEventOriginalSinkV1 for F
where
    F: FnMut(
        &mut dyn AgentEventOriginalStreamV1,
        &mut dyn FnMut() -> bool,
    ) -> Result<(), BodyMutationError>,
{
    fn finish_original(
        &mut self,
        source: &mut dyn AgentEventOriginalStreamV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), BodyMutationError> {
        self(source, cancelled)
    }
}
/// Available only during one persistence attempt; callers cannot construct the
/// concrete stream or its private completion state.
pub trait AgentEventOriginalStreamV1 {
    fn original_id(&self) -> [u8; 32];
    fn original_bytes(&self) -> u64;
    fn write_to(&mut self, output: &mut dyn Write) -> Result<(), BodyMutationError>;
}
/// Callback-facing one-shot producer. `persist_to` checks stream completion
/// after the sink's finish succeeds; returning Ok without calling it is rejected.
pub trait AgentEventOriginalV1 {
    fn original_id(&self) -> [u8; 32];
    fn original_bytes(&self) -> u64;
    fn persist_to(
        &mut self,
        sink: &mut dyn AgentEventOriginalSinkV1,
    ) -> Result<(), BodyMutationError>;
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
                CanonicalCborStreamError::ResourceLimited("agent original memory")
            })
    }
}
fn error(value: CanonicalCborStreamError) -> BodyMutationError {
    match value {
        CanonicalCborStreamError::Cancelled => ValueTreeError::Cancelled.into(),
        CanonicalCborStreamError::ResourceLimited(label) => {
            ExecutionError::ResourceLimited(label).into()
        }
        CanonicalCborStreamError::Io(error) => BodyMutationError::Original(Box::new(error)),
        CanonicalCborStreamError::Invalid(label) => BodyMutationError::Invalid(label),
        CanonicalCborStreamError::Poisoned => {
            BodyMutationError::Invalid("agent original canonical context poisoned")
        }
    }
}
struct HashOutput<'a> {
    output: &'a mut dyn Write,
    hash: blake3::Hasher,
    length: u64,
    max: u64,
    exceeded: bool,
}
impl Write for HashOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if (bytes.len() as u64)
            .checked_add(self.length)
            .is_none_or(|n| n > self.max)
        {
            self.exceeded = true;
            return Err(io::Error::other("agent original byte limit"));
        }
        let n = self.output.write(bytes)?;
        if n > bytes.len() {
            return Err(io::Error::other("agent original overreported write"));
        }
        self.hash.update(&bytes[..n]);
        self.length += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}
fn encode<C: FnMut() -> bool>(
    event: &WorldEvent,
    memory: &MemoryBudget,
    work: &mut CanonicalCborWorkV1,
    cancel: &mut C,
    max: u64,
    output: &mut dyn Write,
) -> Result<([u8; 32], u64), BodyMutationError> {
    if cancel() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let allocator = Allocator {
        memory,
        error: RefCell::new(None),
    };
    let placeholder = CanonicalCborWorkV1 {
        visits_remaining: 0,
        bytes_remaining: 0,
        max_depth: work.max_depth,
        max_map_keys: work.max_map_keys,
        max_key_bytes: work.max_key_bytes,
    };
    let mut encoding = CanonicalCborStreamContextV1::new(
        &allocator,
        std::mem::replace(work, placeholder),
        &mut *cancel,
    );
    let mut sink = HashOutput {
        output,
        hash: blake3::Hasher::new(),
        length: 0,
        max,
        exceeded: false,
    };
    let result = stream_canonical_cbor_v1(event, &mut encoding, &mut sink);
    *work = encoding.work;
    if let Some(memory) = allocator.error.borrow_mut().take() {
        return Err(ExecutionError::Memory(memory).into());
    }
    if sink.exceeded {
        return Err(ExecutionError::ResourceLimited("original event bytes").into());
    }
    let summary = result.map_err(error)?;
    if summary.encoded_bytes != sink.length {
        return Err(BodyMutationError::Invalid("agent original length"));
    }
    if cancel() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((*sink.hash.finalize().as_bytes(), sink.length))
}
struct Stream<'a> {
    event: &'a WorldEvent,
    memory: &'a MemoryBudget,
    work: &'a mut CanonicalCborWorkV1,
    cancel: &'a dyn Fn() -> bool,
    id: [u8; 32],
    length: u64,
    attempted: bool,
    complete: bool,
}
impl AgentEventOriginalStreamV1 for Stream<'_> {
    fn original_id(&self) -> [u8; 32] {
        self.id
    }
    fn original_bytes(&self) -> u64 {
        self.length
    }
    fn write_to(&mut self, output: &mut dyn Write) -> Result<(), BodyMutationError> {
        self.complete = false;
        if self.attempted {
            return Err(BodyMutationError::Invalid("agent original stream reused"));
        }
        self.attempted = true;
        let actual = encode(
            self.event,
            self.memory,
            self.work,
            &mut || (self.cancel)(),
            self.length,
            output,
        )?;
        if actual != (self.id, self.length) {
            return Err(BodyMutationError::Invalid(
                "agent original changed between passes",
            ));
        }
        self.complete = true;
        Ok(())
    }
}
pub(super) struct Measured<'a> {
    event: &'a WorldEvent,
    id: [u8; 32],
    length: u64,
    _scratch: MemoryReservation,
}
impl Measured<'_> {
    pub(super) fn id(&self) -> [u8; 32] {
        self.id
    }
}
pub(super) fn measure<'a, C: FnMut() -> bool>(
    event: &'a WorldEvent,
    memory: &MemoryBudget,
    work: &mut CanonicalCborWorkV1,
    cancel: &mut C,
    max: usize,
) -> Result<Measured<'a>, BodyMutationError> {
    let scratch = memory
        .reserve(8 << 10, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    let (id, length) = encode(event, memory, work, cancel, max as u64, &mut io::sink())?;
    Ok(Measured {
        event,
        id,
        length,
        _scratch: scratch,
    })
}
pub(super) struct Producer<'a, C> {
    measured: Measured<'a>,
    memory: &'a MemoryBudget,
    work: &'a mut CanonicalCborWorkV1,
    cancel: &'a mut C,
    attempted: bool,
    complete: bool,
}
impl<'a, C: FnMut() -> bool> Producer<'a, C> {
    pub(super) fn new(
        measured: Measured<'a>,
        memory: &'a MemoryBudget,
        work: &'a mut CanonicalCborWorkV1,
        cancel: &'a mut C,
    ) -> Self {
        Self {
            measured,
            memory,
            work,
            cancel,
            attempted: false,
            complete: false,
        }
    }
    pub(super) fn check_complete(&mut self) -> Result<(), BodyMutationError> {
        if !self.complete {
            return Err(BodyMutationError::Invalid(
                "agent original persistence incomplete",
            ));
        }
        if (self.cancel)() {
            self.complete = false;
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    }
}
impl<C: FnMut() -> bool> AgentEventOriginalV1 for Producer<'_, C> {
    fn original_id(&self) -> [u8; 32] {
        self.measured.id
    }
    fn original_bytes(&self) -> u64 {
        self.measured.length
    }
    fn persist_to(
        &mut self,
        sink: &mut dyn AgentEventOriginalSinkV1,
    ) -> Result<(), BodyMutationError> {
        self.complete = false;
        if self.attempted {
            return Err(BodyMutationError::Invalid("agent original producer reused"));
        }
        self.attempted = true;
        let complete = {
            // Both the canonical encoder and physical sink poll the same
            // FnMut through short-lived borrows, including nested put/flush.
            let shared = RefCell::new(&mut *self.cancel);
            let cancelled = || (shared.borrow_mut())();
            let mut stream = Stream {
                event: self.measured.event,
                memory: self.memory,
                work: &mut *self.work,
                cancel: &cancelled,
                id: self.measured.id,
                length: self.measured.length,
                attempted: false,
                complete: false,
            };
            sink.finish_original(&mut stream, &mut || cancelled())?;
            if cancelled() {
                return Err(ValueTreeError::Cancelled.into());
            }
            stream.complete
        };
        if !complete {
            return Err(BodyMutationError::Invalid(
                "agent original stream incomplete",
            ));
        }
        self.complete = true;
        Ok(())
    }
}
