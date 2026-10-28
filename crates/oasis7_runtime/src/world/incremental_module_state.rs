//! Actual ModuleStateUpdated reducer contribution. Invocation authorization,
//! sandbox qualification, fees and durable publication remain their owners.
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
    incremental_body_event::read_fixed,
    incremental_module_original::ModuleStateOriginal,
};
use crate::{WorldEvent, WorldEventBody};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFactKindV1, ExecutionFactRefV1, ExecutionTransaction,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_world_state::{
    incremental_agent_body::{WORLD_TIME_KEY_V1, world_time_bytes_v1},
    incremental_event_fields::{EVENT_SEQUENCE_BYTES, EVENT_SEQUENCE_KEY_V1, EventSequenceV1},
    incremental_module_fields::ModuleStateKeyV1,
};
use std::io::{self, Read};

pub struct ModuleStateEventContext<'a, V, W, C, F> {
    pub values: &'a V,
    pub mutation: BodyMutationContext<'a, W, C>,
    /// Consume and persist the full original under expected flat ID using the
    /// same process pool. This callback grants no head/acceptance capability.
    pub persist_original: F,
    pub max_event_bytes: u64,
}
impl World {
    /// Preserve the exact state_key (the event's module_id may be an instance
    /// key), opaque state, original trace/cause and the existing event allocator.
    /// The supplied event is borrowed: no full state map, old blob or journal.
    pub fn contribute_iws_module_state_event<R, V, W, C, F>(
        &self,
        event: &WorldEvent,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut ModuleStateEventContext<'_, V, W, C, F>,
    ) -> Result<(), BodyMutationError>
    where
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn Read, u64, [u8; 32]) -> Result<(), BodyMutationError>,
    {
        txn.contribute(|txn| {
            let WorldEvent {
                id,
                time,
                caused_by: _,
                body,
            } = event;
            let WorldEventBody::ModuleStateUpdated(update) = body else {
                return Err(BodyMutationError::Invalid("unsupported module state event"));
            };
            if !txn.uses_budget(context.mutation.memory) {
                return Err(BodyMutationError::Invalid("module transaction pool"));
            }
            if (context.mutation.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if *time != self.state.time {
                return Err(BodyMutationError::Invalid("module event clock cache"));
            }
            let clock = read_fixed::<{ b"oasis7.iws.world-time-value.v1\0".len() + 8 }, _, _, _, _>(
                txn,
                Ns::Allocator,
                WORLD_TIME_KEY_V1,
                context.values,
                &mut context.mutation,
            )?;
            if clock != world_time_bytes_v1(*time) {
                return Err(BodyMutationError::Invalid(
                    "module event clock precondition",
                ));
            }
            let key =
                ModuleStateKeyV1::new(&update.module_id).map_err(BodyMutationError::Invalid)?;
            let sequence = EventSequenceV1::from_canonical_bytes(&read_fixed::<
                EVENT_SEQUENCE_BYTES,
                _,
                _,
                _,
                _,
            >(
                txn,
                Ns::Allocator,
                EVENT_SEQUENCE_KEY_V1,
                context.values,
                &mut context.mutation,
            )?)
            .map_err(BodyMutationError::Invalid)?;
            let (allocated, next, era) = Self::preview_next_event_id(sequence.next, sequence.era);
            if *id != allocated {
                return Err(BodyMutationError::Invalid(
                    "module event allocator precondition",
                ));
            }
            let original =
                ModuleStateOriginal::new(event, context.max_event_bytes, context.mutation.memory)?;
            let original_id = original.digest(&mut context.mutation.cancelled)?;
            let value = write_field_bytes(&update.state, &mut context.mutation)?;
            txn.write(Ns::Business, key.as_bytes(), Some(value))?;
            let next_ref = write_field_bytes(
                &EventSequenceV1 { next, era }.canonical_bytes(),
                &mut context.mutation,
            )?;
            txn.write(Ns::Allocator, EVENT_SEQUENCE_KEY_V1, Some(next_ref))?;
            let mut reader = CheckedReader {
                inner: original.reader(),
                cancelled: &mut context.mutation.cancelled,
                was_cancelled: false,
                read: 0,
            };
            let result = (context.persist_original)(&mut reader, original.length, original_id);
            if reader.was_cancelled {
                return Err(ValueTreeError::Cancelled.into());
            }
            result?;
            if reader.read != original.length {
                return Err(BodyMutationError::Invalid(
                    "module original was not fully consumed",
                ));
            }
            if (context.mutation.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            txn.append_fact(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Event,
                id: original_id,
            })?;
            Ok(())
        })
    }

    /// Concrete genesis lowering of opaque state values. The receiver admits
    /// and canonically orders its output; all emissions remain provisional.
    pub fn write_iws_module_state_genesis<W: ValueNodeWriter, C: FnMut() -> bool>(
        &self,
        context: &mut BodyMutationContext<'_, W, C>,
        remaining_entries: &mut u64,
        mut emit: impl FnMut(&[u8], [u8; 32]) -> Result<(), BodyMutationError>,
    ) -> Result<u64, BodyMutationError> {
        let mut count = 0u64;
        for (state_key, state) in &self.state.module_states {
            if (context.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            *remaining_entries =
                remaining_entries
                    .checked_sub(1)
                    .ok_or(ExecutionError::ResourceLimited(
                        "module state genesis entries",
                    ))?;
            let key = ModuleStateKeyV1::new(state_key).map_err(BodyMutationError::Invalid)?;
            let value = write_field_bytes(state, context)?;
            emit(key.as_bytes(), value)?;
            count = count
                .checked_add(1)
                .ok_or(BodyMutationError::Invalid("module state count overflow"))?;
        }
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(count)
    }
}
struct CheckedReader<R, C> {
    inner: R,
    cancelled: C,
    was_cancelled: bool,
    read: u64,
}
impl<R: Read, C: FnMut() -> bool> Read for CheckedReader<R, C> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if (self.cancelled)() {
            self.was_cancelled = true;
            return Err(io::Error::other("module original cancelled"));
        }
        let n = self.inner.read(output)?;
        self.read = self
            .read
            .checked_add(n as u64)
            .ok_or_else(|| io::Error::other("module original read overflow"))?;
        Ok(n)
    }
}
