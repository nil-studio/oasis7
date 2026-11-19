//! Dedicated opaque module-state storage and step contribution. Module
//! authorization, artifact selection and active-head publication remain with
//! their respective owners.
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
    incremental_body_event::{AgentEventOriginalV1, original},
};
use crate::{WorldEvent, WorldEventBody};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFactKindV1, ExecutionFactRefV1, ExecutionTransaction,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::{
    incremental_event_fields::{EVENT_SEQUENCE_BYTES, EVENT_SEQUENCE_KEY_V1, EventSequenceV1},
    incremental_module_fields::ModuleStateKeyV1,
};
use std::io::Write;

pub use oasis7_world_state::incremental_module_state_recovery::ModuleStateContentBudgetV1;

/// Stream opaque state values into their dedicated key family. The callback
/// owns provisional state-index writes; it does not install a World or head.
impl World {
    pub fn write_iws_module_state_genesis<W, C, F>(
        &self,
        context: &mut BodyMutationContext<'_, W, C>,
        content: &mut ModuleStateContentBudgetV1,
        mut emit: F,
    ) -> Result<u64, BodyMutationError>
    where
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&[u8], [u8; 32]) -> Result<(), BodyMutationError>,
    {
        let _metadata = context
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let mut count = 0u64;
        for (state_key, state) in &self.state.module_states {
            if (context.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let length = u64::try_from(state.len())
                .map_err(|_| BodyMutationError::Invalid("module state length"))?;
            if length > content.max_state_bytes || length > content.logical_bytes_remaining {
                return Err(ValueTreeError::ResourceLimited("module state genesis bytes").into());
            }
            content.states_remaining =
                content
                    .states_remaining
                    .checked_sub(1)
                    .ok_or(ExecutionError::ResourceLimited(
                        "module state genesis count",
                    ))?;
            content.logical_bytes_remaining -= length;
            let key = ModuleStateKeyV1::new(state_key).map_err(BodyMutationError::Invalid)?;
            let value = write_field_bytes(state, context)?;
            emit(key.as_bytes(), value)?;
            count = count.checked_add(1).ok_or(BodyMutationError::Invalid(
                "module state genesis count overflow",
            ))?;
        }
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(count)
    }
}

/// Borrowed persistence context for one ModuleStateUpdated step event.
pub(super) struct ModuleStateEventContext<'a, V, W, C, F> {
    pub qualification: &'a oasis7_proto::memory_budget::ReservedValue<
        super::incremental_module_state_family::ModuleStateFamilyQualificationV1,
    >,
    pub values: &'a V,
    pub mutation: BodyMutationContext<'a, W, C>,
    pub persist_original: F,
    pub max_event_bytes: usize,
    pub original_work: CanonicalCborWorkV1,
}

/// Contribute one ModuleStateUpdated event to the caller's existing step
/// transaction. This deliberately has no agent-mailbox writes.
pub(super) fn contribute_module_state_event<R, V, W, C, F>(
    now: u64,
    event: &WorldEvent,
    txn: &mut ExecutionTransaction<'_, R>,
    context: &mut ModuleStateEventContext<'_, V, W, C, F>,
) -> Result<(), BodyMutationError>
where
    R: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    txn.contribute(|txn| {
        super::incremental_module_state_family::check(
            context.qualification,
            txn,
            context.mutation.memory,
        )?;
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
            return Err(BodyMutationError::Invalid(
                "module state transaction memory pool",
            ));
        }
        if (context.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if *time != now {
            return Err(BodyMutationError::Invalid("module state event clock"));
        }
        super::incremental_clock::check_clock(now, txn, &mut context.mutation)?;

        let key = ModuleStateKeyV1::new(&update.module_id).map_err(BodyMutationError::Invalid)?;
        let sequence =
            EventSequenceV1::from_canonical_bytes(&super::incremental_body_event::read_fixed::<
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
        let (allocated, next, era) = World::preview_next_event_id(sequence.next, sequence.era);
        if *id != allocated {
            return Err(BodyMutationError::Invalid(
                "module state event allocator precondition",
            ));
        }
        // Register the exact pre-state even though this event is an absolute
        // replacement. A later write in the same transaction remains ordered.
        let _before = txn.read(Ns::Business, key.as_bytes())?;

        // Measure before writing either root. The shared producer performs the
        // actual stream, verifies its measured flat ID, and requires sink finish.
        let measured = original::measure(
            event,
            context.mutation.memory,
            &mut context.original_work,
            &mut context.mutation.cancelled,
            context.max_event_bytes,
        )?;
        let original_id = measured.id();

        let state = write_field_bytes(&update.state, &mut context.mutation)?;
        txn.write(Ns::Business, key.as_bytes(), Some(state))?;
        let next_ref = write_field_bytes(
            &EventSequenceV1 { next, era }.canonical_bytes(),
            &mut context.mutation,
        )?;
        txn.write(Ns::Allocator, EVENT_SEQUENCE_KEY_V1, Some(next_ref))?;

        let mut producer = original::Producer::new(
            measured,
            context.mutation.memory,
            &mut context.original_work,
            &mut context.mutation.cancelled,
        );
        (context.persist_original)(&mut producer)?;
        producer.check_complete()?;
        drop(producer);
        if (context.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        txn.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id: original_id,
        })?;
        if (context.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}

/// Point-read one opaque module value without materializing or scanning the
/// module map. Empty bytes are a present value and return `Some(0)`.
pub fn read_iws_module_state<R, V, C>(
    qualification: &oasis7_proto::memory_budget::ReservedValue<
        super::incremental_module_state_family::ModuleStateFamilyQualificationV1,
    >,
    state_key: &str,
    txn: &mut ExecutionTransaction<'_, R>,
    reader: &mut ValueReadContext<'_, V>,
    cancelled: &mut C,
    max_state_bytes: u64,
    mut output: &mut dyn Write,
) -> Result<Option<u64>, BodyMutationError>
where
    R: MerkleNodeReader,
    V: ValueNodeReader,
    C: FnMut() -> bool,
{
    txn.contribute(|txn| {
        super::incremental_module_state_family::check(qualification, txn, reader.memory)?;
        let key = ModuleStateKeyV1::new(state_key).map_err(BodyMutationError::Invalid)?;
        if !txn.uses_budget(reader.memory) || reader.class != MemoryClass::Ordinary {
            return Err(BodyMutationError::Invalid("module state read memory pool"));
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let Some(value) = txn.read(Ns::Business, key.as_bytes())? else {
            if cancelled() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(None);
        };
        let root = reader.load_root(&value)?;
        let length = root.0.logical_len;
        if length > max_state_bytes {
            return Err(ValueTreeError::ResourceLimited("module state read bytes").into());
        }
        reader.read_range(&root, 0, length, &mut output, &mut *cancelled)?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(length))
    })
}
