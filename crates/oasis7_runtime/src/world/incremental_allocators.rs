//! Original rolling ID state in the shared IWS allocator overlay.
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
    incremental_body_event::read_fixed,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::incremental_event_fields::{
    EVENT_SEQUENCE_BYTES, EVENT_SEQUENCE_KEY_V1, EventSequenceV1,
};

const KEY: &[u8] = b"oasis7.iws.runtime-sequence-key.v1\0";
const VALUE: &[u8] = b"oasis7.iws.runtime-sequence.v1\0";
const BYTES: usize = VALUE.len() + 17;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAllocatorV1 {
    Event,
    Action,
    Intent,
    Proposal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeSequenceV1 {
    pub next: u64,
    pub era: u64,
}
impl RuntimeAllocatorV1 {
    pub const ALL: [Self; 4] = [Self::Event, Self::Action, Self::Intent, Self::Proposal];
    fn tag(self) -> u8 {
        match self {
            Self::Event => 0,
            Self::Action => 1,
            Self::Intent => 2,
            Self::Proposal => 3,
        }
    }
    fn key(self) -> ([u8; KEY.len() + 1], usize) {
        let mut bytes = [0; KEY.len() + 1];
        let length = if self == Self::Event {
            bytes[..EVENT_SEQUENCE_KEY_V1.len()].copy_from_slice(EVENT_SEQUENCE_KEY_V1);
            EVENT_SEQUENCE_KEY_V1.len()
        } else {
            bytes[..KEY.len()].copy_from_slice(KEY);
            bytes[KEY.len()] = self.tag();
            bytes.len()
        };
        (bytes, length)
    }
    fn encode(self, sequence: RuntimeSequenceV1) -> ([u8; BYTES], usize) {
        let mut bytes = [0; BYTES];
        if self == Self::Event {
            bytes[..EVENT_SEQUENCE_BYTES].copy_from_slice(
                &EventSequenceV1 {
                    next: sequence.next,
                    era: sequence.era,
                }
                .canonical_bytes(),
            );
            (bytes, EVENT_SEQUENCE_BYTES)
        } else {
            bytes[..VALUE.len()].copy_from_slice(VALUE);
            bytes[VALUE.len()] = self.tag();
            bytes[VALUE.len() + 1..VALUE.len() + 9].copy_from_slice(&sequence.next.to_be_bytes());
            bytes[VALUE.len() + 9..].copy_from_slice(&sequence.era.to_be_bytes());
            (bytes, BYTES)
        }
    }
    fn decode(self, bytes: &[u8]) -> Result<RuntimeSequenceV1, BodyMutationError> {
        if self == Self::Event {
            let value =
                EventSequenceV1::from_canonical_bytes(bytes).map_err(BodyMutationError::Invalid)?;
            return Ok(RuntimeSequenceV1 {
                next: value.next,
                era: value.era,
            });
        }
        if bytes.len() != BYTES || !bytes.starts_with(VALUE) || bytes[VALUE.len()] != self.tag() {
            return Err(BodyMutationError::Invalid(
                "runtime allocator domain/tag/length",
            ));
        }
        Ok(RuntimeSequenceV1 {
            next: u64::from_be_bytes(
                bytes[VALUE.len() + 1..VALUE.len() + 9]
                    .try_into()
                    .map_err(|_| BodyMutationError::Invalid("runtime allocator next width"))?,
            ),
            era: u64::from_be_bytes(
                bytes[VALUE.len() + 9..]
                    .try_into()
                    .map_err(|_| BodyMutationError::Invalid("runtime allocator era width"))?,
            ),
        })
    }
}
fn check<W, C: FnMut() -> bool>(
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    if (context.cancelled)() {
        Err(ValueTreeError::Cancelled.into())
    } else {
        Ok(())
    }
}
fn read<R: MerkleNodeReader, V: ValueNodeReader, W, C: FnMut() -> bool>(
    allocator: RuntimeAllocatorV1,
    txn: &mut ExecutionTransaction<'_, R>,
    values: &V,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<RuntimeSequenceV1, BodyMutationError> {
    if !txn.uses_budget(context.memory) {
        return Err(BodyMutationError::Invalid("runtime allocator memory pool"));
    }
    check(context)?;
    let (key, length) = allocator.key();
    let value = if allocator == RuntimeAllocatorV1::Event {
        allocator.decode(&read_fixed::<EVENT_SEQUENCE_BYTES, _, _, _, _>(
            txn,
            Ns::Allocator,
            &key[..length],
            values,
            context,
        )?)?
    } else {
        allocator.decode(&read_fixed::<BYTES, _, _, _, _>(
            txn,
            Ns::Allocator,
            &key[..length],
            values,
            context,
        )?)?
    };
    check(context)?;
    Ok(value)
}
impl World {
    /// Borrow all four original pairs. No journal scan, ID allocation or head publication.
    pub fn write_iws_allocator_genesis<
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&[u8], [u8; 32]) -> Result<(), BodyMutationError>,
    >(
        &self,
        context: &mut BodyMutationContext<'_, W, C>,
        mut emit: F,
    ) -> Result<(), BodyMutationError> {
        let _charge = context
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        for (allocator, next, era) in [
            (
                RuntimeAllocatorV1::Event,
                self.next_event_id,
                self.next_event_id_era,
            ),
            (
                RuntimeAllocatorV1::Action,
                self.next_action_id,
                self.next_action_id_era,
            ),
            (
                RuntimeAllocatorV1::Intent,
                self.next_intent_id,
                self.next_intent_id_era,
            ),
            (
                RuntimeAllocatorV1::Proposal,
                self.next_proposal_id,
                self.next_proposal_id_era,
            ),
        ] {
            check(context)?;
            let (key, length) = allocator.key();
            let (bytes, size) = allocator.encode(RuntimeSequenceV1 { next, era });
            let id = write_field_bytes(&bytes[..size], context)?;
            emit(&key[..length], id)?;
        }
        check(context)
    }
    /// Read exact original control values; a failure poisons the shared transaction.
    pub fn read_iws_allocator<R: MerkleNodeReader, V: ValueNodeReader, W, C: FnMut() -> bool>(
        allocator: RuntimeAllocatorV1,
        txn: &mut ExecutionTransaction<'_, R>,
        values: &V,
        context: &mut BodyMutationContext<'_, W, C>,
    ) -> Result<RuntimeSequenceV1, BodyMutationError> {
        txn.contribute(|txn| read(allocator, txn, values, context))
    }
    /// Allocate using the original rollover rule and exact overlay precondition.
    /// The caller owns all linked business, event, preallocation and acceptance obligations.
    pub fn contribute_iws_allocate<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
    >(
        allocator: RuntimeAllocatorV1,
        expected: RuntimeSequenceV1,
        txn: &mut ExecutionTransaction<'_, R>,
        values: &V,
        context: &mut BodyMutationContext<'_, W, C>,
    ) -> Result<(u64, RuntimeSequenceV1), BodyMutationError> {
        txn.contribute(|txn| {
            let current = read(allocator, txn, values, context)?;
            if current != expected {
                return Err(BodyMutationError::Invalid("runtime allocator precondition"));
            }
            let (allocated, next, era) =
                Self::preview_rolling_sequence_id(current.next, current.era);
            let after = RuntimeSequenceV1 { next, era };
            let (key, length) = allocator.key();
            let (bytes, size) = allocator.encode(after);
            let id = write_field_bytes(&bytes[..size], context)?;
            txn.write(Ns::Allocator, &key[..length], Some(id))?;
            check(context)?;
            Ok((allocated, after))
        })
    }
}

#[cfg(test)]
#[path = "incremental_allocators_tests.rs"]
mod tests;
