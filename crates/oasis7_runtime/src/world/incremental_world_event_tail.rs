//! Recover the actual last typed WorldEvent for the step's routing order.
//! This is source provenance, never business authority or durable acceptance.
use super::{incremental_body::BodyMutationError, incremental_world_event_counter::*};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::incremental_world_event_decode::{
    DecodedWorldEventV1, WorldEventDecodeWorkV1, decode_world_event_original_v1,
};
use std::io::Write;

pub struct CurrentWorldEventTailWorkV1 {
    pub originals_remaining: u64,
    pub original_bytes_remaining: u64,
    pub decode: WorldEventDecodeWorkV1,
}
#[derive(Debug)]
pub enum CurrentWorldEventTailErrorV1<E> {
    Body(BodyMutationError),
    Value(ValueTreeError),
    Source(E),
}
impl<E> From<BodyMutationError> for CurrentWorldEventTailErrorV1<E> {
    fn from(value: BodyMutationError) -> Self {
        Self::Body(value)
    }
}
impl<E> From<ValueTreeError> for CurrentWorldEventTailErrorV1<E> {
    fn from(value: ValueTreeError) -> Self {
        Self::Value(value)
    }
}
impl<E> From<ExecutionError> for CurrentWorldEventTailErrorV1<E> {
    fn from(value: ExecutionError) -> Self {
        Self::Body(value.into())
    }
}
pub(super) struct RecoveredCountedWorldEventV1 {
    source: CountedWorldEventSourceV1,
    event: ReservedValue<DecodedWorldEventV1>,
}
impl RecoveredCountedWorldEventV1 {
    pub(super) fn event(&self) -> &crate::WorldEvent {
        self.event.event()
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        transaction: &ExecutionTransaction<'_, M>,
    ) -> Result<(), BodyMutationError> {
        self.source.validate(transaction)
    }
}

struct OriginalBuffer<'a, C> {
    bytes: Vec<u8>,
    max: usize,
    remaining: &'a mut u64,
    cancelled: &'a mut C,
    failure: Option<ValueTreeError>,
}
impl<C: FnMut() -> bool> Write for OriginalBuffer<'_, C> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let error = if (self.cancelled)() {
            Some(ValueTreeError::Cancelled)
        } else if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.max)
        {
            Some(ValueTreeError::ResourceLimited(
                "WorldEvent tail original bound",
            ))
        } else if let Some(after) = self.remaining.checked_sub(bytes.len() as u64) {
            *self.remaining = after;
            None
        } else {
            Some(ValueTreeError::ResourceLimited(
                "WorldEvent tail original work",
            ))
        };
        if let Some(error) = error {
            if self.failure.is_none() {
                self.failure = Some(error);
            }
            return Err(std::io::Error::other("WorldEvent tail source stopped"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn recover_latest_counted_world_event_v1<M, O, C>(
    counter: &WorldEventCounterV1,
    transaction: &mut ExecutionTransaction<'_, M>,
    originals: &mut O,
    memory: &MemoryBudget,
    now: u64,
    work: &mut CurrentWorldEventTailWorkV1,
    mut cancelled: C,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
) -> Result<
    Option<ReservedValue<RecoveredCountedWorldEventV1>>,
    CurrentWorldEventTailErrorV1<O::Error>,
>
where
    M: MerkleNodeReader,
    O: OriginalObjectStreamReader,
    C: FnMut() -> bool,
{
    let result = (|| {
        transaction.validate_guard(&transaction.guard(), &live_owner())?;
        if !transaction.uses_budget(memory)
            || !originals.allocation_budget().same_pool(memory)
            || originals.allocation_class() != MemoryClass::Ordinary
        {
            return Err(ValueTreeError::Invalid("WorldEvent tail source pool/class").into());
        }
        let Some(source) = counter.latest_source(transaction)? else {
            return Ok(None);
        };
        work.originals_remaining =
            work.originals_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "WorldEvent tail original count",
                ))?;
        let max = work.decode.max_original_bytes;
        let raw_charge = memory
            .reserve(
                max.checked_add(4096)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "WorldEvent tail raw allowance",
                    ))?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let output_charge = memory
            .reserve(
                std::mem::size_of::<RecoveredCountedWorldEventV1>() + 1024,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(max)
            .map_err(|_| ValueTreeError::ResourceLimited("WorldEvent tail buffer allocation"))?;
        if bytes.capacity() > max {
            return Err(ValueTreeError::ResourceLimited("WorldEvent tail buffer capacity").into());
        }
        let mut buffer = OriginalBuffer {
            bytes,
            max,
            remaining: &mut work.original_bytes_remaining,
            cancelled: &mut cancelled,
            failure: None,
        };
        let streamed =
            originals.stream_original(source.original(), max as u64, memory, &mut buffer);
        if let Some(error) = buffer.failure.take() {
            return Err(error.into());
        }
        let streamed = streamed.map_err(CurrentWorldEventTailErrorV1::Source)?;
        if streamed != buffer.bytes.len() as u64
            || blake3::hash(&buffer.bytes).as_bytes() != source.original()
        {
            return Err(ValueTreeError::Invalid("WorldEvent tail original identity").into());
        }
        let OriginalBuffer { bytes, .. } = buffer;
        let event = decode_world_event_original_v1(
            &bytes,
            memory,
            MemoryClass::Ordinary,
            &mut work.decode,
            &mut cancelled,
        )?;
        drop(bytes);
        drop(raw_charge);
        if event.event().time != now {
            return Err(ValueTreeError::Invalid("WorldEvent tail time").into());
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.validate_guard(&transaction.guard(), &live_owner())?;
        source.validate(transaction)?;
        Ok(Some(
            output_charge.retain(RecoveredCountedWorldEventV1 { source, event }),
        ))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
