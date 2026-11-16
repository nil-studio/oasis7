//! Borrow the one real Apply owner through every original event publication.
//! After publication it is still moved, with all source fees, into the instance
//! tail. No cloned permission, detached receipt or free-form Body is accepted.
use super::*;
use crate::world::incremental_body_event::AgentEventOriginalV1;

pub(in crate::world) struct PreparedAppliedModuleEventV1<'a> {
    source: &'a CommittedAppliedModuleStateV1,
    event: ReservedValue<crate::WorldEvent>,
    index: usize,
    guard: PreparedGuard,
    core: WorkIdentityV1,
}
impl PreparedAppliedModuleEventV1<'_> {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.source.validate(tx, memory, core, now, live)?;
        tx.validate_transaction_identity(&self.guard)?;
        if self.core != core
            || !self.event.uses_budget(memory)
            || self.event.time != now
            || self.event.caused_by.is_some()
            || self.index >= self.source.bodies.len()
        {
            return Err(invalid());
        }
        // Body is immutable and constructed only below from this exact source slot.
        if std::mem::discriminant(&self.event.body)
            != std::mem::discriminant(&self.source.bodies[self.index])
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_action_module_lifecycle) fn publish_applied_module_events_v1<
    M,
    P,
    V,
    W,
    D,
    E,
    C,
    ME,
    F,
>(
    source: &CommittedAppliedModuleStateV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    common: &mut FormalBoundedContextV1<'_>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        source.validate(
            step.transaction,
            common.memory(),
            common.work().identity(),
            step.time(),
            live,
        )?;
        for (index, body) in source.bodies.iter().enumerate() {
            // The source reservation covers the entire derived graph. Reserve another
            // whole-graph bound conservatively before cloning this one event's body.
            let heap = source
                ._fee
                .bytes()
                .checked_add(std::mem::size_of::<PreparedAppliedModuleEventV1>())
                .ok_or_else(invalid)?;
            pay(common, heap)?;
            let fee = common
                .memory()
                .reserve(heap, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let id = {
                let mut ctx = WorldFieldOverlayContextV1 {
                    transaction: &mut *step.transaction,
                    values: ValueReadContext {
                        source: read.values.source,
                        memory: read.values.memory,
                        class: read.values.class,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: &mut read.take_state_error,
                    cancelled: &mut read.cancelled,
                };
                crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(&mut ctx,common)?
            };
            let event = PreparedAppliedModuleEventV1 {
                source,
                event: fee.retain(crate::WorldEvent {
                    id,
                    time: step.time(),
                    caused_by: None,
                    body: body.clone(),
                }),
                index,
                guard: step.transaction.guard(),
                core: common.work().identity(),
            };
            let cap=crate::world::incremental_normal_system_event::PreparedNormalSystemEventV1::from_module_apply_event(&event,step.transaction,step.time(),common,live)?;
            step.append_prepared_normal_system_event_v1(
                &cap,
                materials,
                common,
                live,
                max_event_bytes,
                &mut *persist,
            )?;
        }
        Ok(())
    }));
    let result = match attempt {
        Ok(result) => result,
        Err(payload) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                step.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            std::panic::resume_unwind(payload)
        }
    };
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            step.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
