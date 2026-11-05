//! Event-original/allocator contribution after the concrete module target has
//! applied the event's business owner. This helper is crate-private, grants no
//! semantic permission, and cannot be used as a substitute for owner mutation.
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
    incremental_body_event::{AgentEventOriginalV1, original, read_fixed},
};
use crate::{WorldEvent, WorldEventBody};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1, ExecutionTransaction},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::incremental_event_fields::{
    EVENT_SEQUENCE_BYTES, EVENT_SEQUENCE_KEY_V1, EventSequenceV1,
};

pub struct ModuleOutputEventContextV1<'a, V, W, C, F> {
    pub values: &'a V,
    pub world_events: &'a super::incremental_world_event_counter::WorldEventCounterV1,
    pub mutation: BodyMutationContext<'a, W, C>,
    pub persist_original: F,
    pub max_event_bytes: usize,
    pub original_work: CanonicalCborWorkV1,
}
/// Called only by the concrete output target, after the corresponding fee,
/// effect/visual/policy owner succeeds in the SAME transaction. Any later
/// persistence/encoding failure poisons that transaction through contribute.
pub(super) fn contribute_module_output_event_original_v1<M, V, W, C, F>(
    now: u64,
    event: &WorldEvent,
    txn: &mut ExecutionTransaction<'_, M>,
    ctx: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    txn.contribute(|txn| {
        ctx.world_events.validate(txn)?;
        let before = txn.ordered_fact_count();
        if !matches!(
            &event.body,
            WorldEventBody::PolicyDecisionRecorded(_)
                | WorldEventBody::EffectQueued(_)
                | WorldEventBody::ModuleEmitted(_)
                | WorldEventBody::ModuleRuntimeCharged(_)
                | WorldEventBody::ModuleCallFailed(_)
        ) || event.time != now
            || !txn.uses_budget(ctx.mutation.memory)
        {
            return Err(BodyMutationError::Invalid(
                "module output event original scope",
            ));
        }
        if (ctx.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        super::incremental_clock::check_clock(now, txn, &mut ctx.mutation)?;
        let seq =
            EventSequenceV1::from_canonical_bytes(&read_fixed::<EVENT_SEQUENCE_BYTES, _, _, _, _>(
                txn,
                StateNamespaceV1::Allocator,
                EVENT_SEQUENCE_KEY_V1,
                ctx.values,
                &mut ctx.mutation,
            )?)
            .map_err(BodyMutationError::Invalid)?;
        let (id, next, era) = World::preview_next_event_id(seq.next, seq.era);
        if id != event.id {
            return Err(BodyMutationError::Invalid("module output event allocator"));
        }
        let measured = original::measure(
            event,
            ctx.mutation.memory,
            &mut ctx.original_work,
            &mut ctx.mutation.cancelled,
            ctx.max_event_bytes,
        )?;
        let original_id = measured.id();
        let next_ref = write_field_bytes(
            &EventSequenceV1 { next, era }.canonical_bytes(),
            &mut ctx.mutation,
        )?;
        txn.write(
            StateNamespaceV1::Allocator,
            EVENT_SEQUENCE_KEY_V1,
            Some(next_ref),
        )?;
        let mut producer = original::Producer::new(
            measured,
            ctx.mutation.memory,
            &mut ctx.original_work,
            &mut ctx.mutation.cancelled,
        );
        (ctx.persist_original)(&mut producer)?;
        producer.check_complete()?;
        drop(producer);
        if (ctx.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        txn.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id: original_id,
        })?;
        if (ctx.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        ctx.world_events.note_success(txn, before)
    })
}

pub(super) fn append_module_output_event_v1<M, V, W, C, F>(
    now: u64,
    body: WorldEventBody,
    tx: &mut ExecutionTransaction<'_, M>,
    events: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(
        &mut dyn super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
{
    let seq = World::read_iws_allocator(
        super::incremental_allocators::RuntimeAllocatorV1::Event,
        tx,
        events.values,
        &mut events.mutation,
    )?;
    let (id, _, _) = World::preview_next_event_id(seq.next, seq.era);
    contribute_module_output_event_original_v1(
        now,
        &WorldEvent {
            id,
            time: now,
            caused_by: None,
            body,
        },
        tx,
        events,
    )
}
