//! Exact private lifecycle receipt to original-object and Event-fact bridge.
use super::*;

pub(in crate::world) fn contribute_factory_lifecycle_event_v1<M, V, W, C, F>(
    now: u64,
    prepared: &crate::world::incremental_action_factory_lifecycle::PreparedFactoryLifecycleEventV1,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    txn: &mut ExecutionTransaction<'_, M>,
    values: &V,
    mutation: &mut BodyMutationContext<'_, W, C>,
    original_work: &mut CanonicalCborWorkV1,
    max_event_bytes: usize,
    persist_original: &mut F,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    txn.contribute(|txn| {
        let guard = txn.guard();
        txn.validate_guard(&guard, live)?;
        prepared.validate(txn, mutation.memory, now)?;
        persist_event_original_and_fact_v1(
            now,
            prepared.event(),
            txn,
            values,
            mutation,
            original_work,
            max_event_bytes,
            persist_original,
        )?;
        Ok(())
    })
}
