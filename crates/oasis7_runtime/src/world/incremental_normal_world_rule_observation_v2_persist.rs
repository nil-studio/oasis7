//! Raw format-2 JSON persistence. This is deliberately independent of the
//! canonical-CBOR AgentEvent original producer. A sink report is DATA only;
//! acceptance must independently verify the actual original and sealed cut.
use super::*;
use std::cell::RefCell;
#[derive(Debug)]
pub(in crate::world) enum ObservationOriginalPersistenceErrorV2<E> {
    Source(ObservationCurrentSourceErrorV2),
    Sink(E),
}
#[derive(Debug)]
pub struct ObservationOriginalStreamFailedV2;
/// Adapter must use the actual object's byte push/finish/readback protocol.
/// In production this is IncrementalObjectStore::write_object_from_stream,
/// not the CBOR AgentEventOriginalSink. Returning an ID without finish fails
/// the private consumption check; arbitrary sinks still cannot attest custody.
pub trait ObservationJsonOriginalSinkV2 {
    type Error;
    fn finish_json_original(
        &mut self,
        source: &mut dyn ObservationJsonOriginalStreamV2,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<[u8; 32], Self::Error>;
}
pub trait ObservationJsonOriginalStreamV2 {
    fn memory(&self) -> &MemoryBudget;
    fn original_id(&self) -> [u8; 32];
    fn original_bytes(&self) -> u64;
    fn write_to(&mut self, output: &mut dyn Write)
    -> Result<(), ObservationOriginalStreamFailedV2>;
}
/// The source remains owned (and paid) through the later registration step.
/// This receipt proves local stream/finish contract completion, not acceptance,
/// an execution permission, a physical lease or arbitrary sink durability.
pub(in crate::world) struct PersistedObservationCurrentSourceV2 {
    source: ObservationCurrentSourceV2,
    id: [u8; 32],
    locator: ReservedValue<String>,
    _fee: ReservedValue<()>,
}
impl PersistedObservationCurrentSourceV2 {
    pub(in crate::world) fn source(&self) -> &ObservationCurrentSourceV2 {
        &self.source
    }
    pub(in crate::world) fn object_id(&self) -> &[u8; 32] {
        &self.id
    }
    /// Lowercase BLAKE3 object ID, distinct from source.current_commitment SHA256.
    pub(in crate::world) fn source_original(&self) -> &str {
        &self.locator
    }
    pub(in crate::world) fn validate_capture<M: MerkleNodeReader>(
        &self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), ObservationCurrentSourceErrorV2> {
        if !self.locator.uses_budget(common.memory()) || !self._fee.uses_budget(common.memory()) {
            return Err(invalid("Current JSON receipt pool"));
        }
        self.source.validate_capture(parent, tx, common, live)
    }
}
struct State {
    attempted: bool,
    complete: bool,
    source_error: Option<ObservationCurrentSourceErrorV2>,
    output_error: Option<io::Error>,
}
struct JsonStream<'a, 'b, 'c, 'd> {
    bytes: &'a [u8],
    memory: &'a MemoryBudget,
    id: [u8; 32],
    common: &'b RefCell<&'c mut FormalBoundedContextV1<'d>>,
    state: &'b RefCell<State>,
}
impl ObservationJsonOriginalStreamV2 for JsonStream<'_, '_, '_, '_> {
    fn memory(&self) -> &MemoryBudget {
        self.memory
    }
    fn original_id(&self) -> [u8; 32] {
        self.id
    }
    fn original_bytes(&self) -> u64 {
        self.bytes.len() as u64
    }
    fn write_to(&mut self, out: &mut dyn Write) -> Result<(), ObservationOriginalStreamFailedV2> {
        {
            let mut state = self.state.borrow_mut();
            if state.attempted {
                if state.source_error.is_none() {
                    state.source_error = Some(invalid("Current JSON stream reused"));
                }
                state.complete = false;
                return Err(ObservationOriginalStreamFailedV2);
            }
            state.attempted = true;
        }
        let mut hash = blake3::Hasher::new();
        for chunk in self.bytes.chunks(16 * 1024) {
            if let Err(error) = self.common.borrow_mut().charge(1, chunk.len() as u64) {
                self.state.borrow_mut().source_error = Some(error.into());
                return Err(ObservationOriginalStreamFailedV2);
            }
            // Check every partial-write boundary. A hostile writer cannot
            // overreport into slice indexing or complete an incomplete source.
            let mut remaining = chunk;
            while !remaining.is_empty() {
                if let Err(error) = self.common.borrow_mut().consume_steps(1) {
                    self.state.borrow_mut().source_error = Some(error.into());
                    return Err(ObservationOriginalStreamFailedV2);
                }
                match out.write(remaining) {
                    Ok(n) if n > 0 && n <= remaining.len() => remaining = &remaining[n..],
                    Ok(_) => {
                        self.state.borrow_mut().output_error =
                            Some(io::Error::other("Current JSON invalid write count"));
                        return Err(ObservationOriginalStreamFailedV2);
                    }
                    Err(error) => {
                        self.state.borrow_mut().output_error = Some(error);
                        return Err(ObservationOriginalStreamFailedV2);
                    }
                }
            }
            hash.update(chunk);
        }
        if *hash.finalize().as_bytes() != self.id {
            self.state.borrow_mut().source_error = Some(invalid("Current JSON stream digest"));
            return Err(ObservationOriginalStreamFailedV2);
        }
        if let Err(error) = self.common.borrow_mut().consume_steps(0) {
            self.state.borrow_mut().source_error = Some(error.into());
            return Err(ObservationOriginalStreamFailedV2);
        }
        self.state.borrow_mut().complete = true;
        Ok(())
    }
}
impl ObservationCurrentSourceV2 {
    /// One attempt consumes this owner even on failure. The only bytes available
    /// to the sink are this genuine capture; there is no caller-bytes constructor.
    pub(in crate::world) fn persist_json_original_v2<
        M: MerkleNodeReader,
        S: ObservationJsonOriginalSinkV2,
    >(
        self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        tx: &mut ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
        sink: &mut S,
    ) -> Result<PersistedObservationCurrentSourceV2, ObservationOriginalPersistenceErrorV2<S::Error>>
    {
        use ObservationOriginalPersistenceErrorV2::{Sink, Source};
        let result = (|| {
            self.validate_capture(parent, tx, common, live)
                .map_err(Source)?;
            let fee = common
                .memory()
                .reserve(
                    size_of::<PersistedObservationCurrentSourceV2>(),
                    MemoryClass::Ordinary,
                )
                .map_err(|e| Source(e.into()))?
                .retain(());
            let locator_fee = common
                .memory()
                .reserve(size_of::<String>() + 64, MemoryClass::Ordinary)
                .map_err(|e| Source(e.into()))?;
            let mut hash = blake3::Hasher::new();
            for chunk in self.original.chunks(16 * 1024) {
                common
                    .charge(1, chunk.len() as u64)
                    .map_err(|e| Source(e.into()))?;
                hash.update(chunk);
            }
            let id = *hash.finalize().as_bytes();
            let state = RefCell::new(State {
                attempted: false,
                complete: false,
                source_error: None,
                output_error: None,
            });
            let finished = {
                let shared = RefCell::new(&mut *common);
                let mut stream = JsonStream {
                    bytes: &self.original,
                    memory: &self.memory,
                    id,
                    common: &shared,
                    state: &state,
                };
                let mut cancelled = || {
                    if state.borrow().source_error.is_some() {
                        return true;
                    }
                    if let Err(error) = shared.borrow_mut().consume_steps(0) {
                        state.borrow_mut().source_error = Some(error.into());
                        return true;
                    }
                    false
                };
                sink.finish_json_original(&mut stream, &mut cancelled)
            };
            let mut state = state.into_inner();
            // Preserve continuous Common's actual first cancellation/resource
            // error even when the physical adapter reports a generic cancel.
            if let Some(error) = state.source_error.take() {
                return Err(Source(error));
            }
            let actual = finished.map_err(Sink)?;
            if let Some(error) = state.output_error.take() {
                return Err(Source(error.into()));
            }
            if !state.complete || actual != id {
                return Err(Source(invalid("Current JSON incomplete finish/object ID")));
            }
            self.validate_capture(parent, tx, common, live)
                .map_err(Source)?;
            common.charge(1, 64).map_err(|e| Source(e.into()))?;
            let mut locator = String::new();
            locator
                .try_reserve_exact(64)
                .map_err(|_| Source(limited()))?;
            if locator.capacity() > 64 {
                return Err(Source(limited()));
            }
            const HEX: &[u8; 16] = b"0123456789abcdef";
            for byte in id {
                locator.push(HEX[(byte >> 4) as usize] as char);
                locator.push(HEX[(byte & 15) as usize] as char);
            }
            Ok(PersistedObservationCurrentSourceV2 {
                source: self,
                id,
                locator: locator_fee.retain(locator),
                _fee: fee,
            })
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
