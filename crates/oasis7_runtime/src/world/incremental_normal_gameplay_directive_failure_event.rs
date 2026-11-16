//! Counted publication of the actual selected emit/serde failure source.
//! Included privately by the event sink once the fixed tail is integrated.
use super::*;
use crate::world::incremental_normal_gameplay_cycles::PreparedGameplayDirectiveFailureV1;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;

/// The fixed decoder has produced the original InvalidOutput diagnostic. This seam only
/// records the original, Event fact, counter, CorePolicy normalization and
/// existing-agent mailbox. System events do not run Action PostAction.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn contribute_normal_gameplay_directive_failure_v1<'event, M, V, W, C, F, ME>(
    now: u64,
    prepared: &'event PreparedGameplayDirectiveFailureV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    transaction: &mut ExecutionTransaction<'_, M>,
    events: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        ME,
    >,
) -> Result<PersistedWorldEventV1<'event>, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    ME: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
{
    transaction.contribute(|transaction| {
        common
            .consume_steps(1)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        if !common.memory().same_pool(events.mutation.memory) {
            return Err(BodyMutationError::Invalid("gameplay directive failure publication pool"));
        }
        prepared.validate(
            transaction,
            events.mutation.memory,
            now,
            common.work().identity(),
            live,
        )?;
        let event = prepared.event();
        if event.caused_by.is_some()
            || !matches!(
                &event.body,
                WorldEventBody::ModuleCallFailed(f) if f.code==oasis7_wasm_abi::ModuleCallErrorCode::InvalidOutput
            )
        {
            return Err(BodyMutationError::Invalid(
                "gameplay directive failure system event source",
            ));
        }
        let cap = crate::world::incremental_normal_system_event::PreparedNormalSystemEventV1::from_gameplay_directive_failure(prepared,transaction,now,common,live)?;
        cap.validate_common(common)?;
        let guard = transaction.guard();
        let mut step = crate::world::incremental_step::IwsStepContext::borrow_counted_at(
            transaction,now,&guard,live,events.values,&mut events.mutation,&mut events.original_work,events.world_events,
        )?;
        step.append_prepared_normal_system_event_v1(&cap,materials,common,live,events.max_event_bytes,&mut events.persist_original)?;
        let persisted = PersistedWorldEventV1 { owner: PersistedEventOwnerV1::Module {event,identity:step.transaction.guard()} };
        // Cancellation after original publication still aborts the enclosing
        // transaction; no routed/completed receipt escapes an unknown result.
        common
            .consume_steps(1)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        transaction.validate_guard(&transaction.guard(), live)?;
        Ok(persisted)
    })
}
