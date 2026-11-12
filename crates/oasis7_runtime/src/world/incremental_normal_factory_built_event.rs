//! Counted publication accepts only the completed FactoryBuilt Current owner.
//! Included privately by the event sink once the fixed tail is integrated.
use super::*;
use crate::world::incremental_normal_tail::factory_built::PreparedNormalFactoryBuiltV1;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;

/// The reducer has already staged every business mutation. This seam only
/// records the original, ordered Event fact and shared event counter; it does
/// not repeat CorePolicy or run Action PostAction for a system event.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn contribute_normal_factory_built_event_v1<'event, M, V, W, C, F>(
    now: u64,
    prepared: &'event PreparedNormalFactoryBuiltV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    transaction: &mut ExecutionTransaction<'_, M>,
    events: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
) -> Result<PersistedWorldEventV1<'event>, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    transaction.contribute(|transaction| {
        common
            .consume_steps(1)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        if !common.memory().same_pool(events.mutation.memory) {
            return Err(BodyMutationError::Invalid("FactoryBuilt publication pool"));
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
                WorldEventBody::Domain(crate::DomainEvent::FactoryBuilt { .. })
            )
        {
            return Err(BodyMutationError::Invalid(
                "FactoryBuilt system event source",
            ));
        }
        let persisted = persist_counted_event_original_v1(now, event, transaction, events)?;
        // Cancellation after original publication still aborts the enclosing
        // transaction; no routed/completed receipt escapes an unknown result.
        common
            .consume_steps(1)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        transaction.validate_guard(&transaction.guard(), live)?;
        Ok(persisted)
    })
}
