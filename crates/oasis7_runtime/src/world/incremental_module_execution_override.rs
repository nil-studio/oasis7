//! Genuine selected Rule Modify -> admitted override original -> counted fact.
use super::*;

struct OverrideMetadataV1 {
    guard: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    action_id: u64,
    event_id: u64,
    time: u64,
    class: MemoryClass,
}
/// Private-field candidate, minted only from the genuine selected Rule result
/// and the narrowly admitted borrowed projection in publish_action_override_v1.
pub(in crate::world) struct PreparedRuleOverrideEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    metadata: ReservedValue<OverrideMetadataV1>,
}
impl PreparedRuleOverrideEventV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.metadata.guard, live)?;
        if !tx.uses_budget(memory)
            || !self.event.uses_budget(memory)
            || !self.metadata.uses_budget(memory)
            || self.metadata.class != MemoryClass::Ordinary
            || self.metadata.reserved_bytes() < std::mem::size_of::<OverrideMetadataV1>()
            || self.event.reserved_bytes() < std::mem::size_of::<crate::WorldEvent>()
            || self.metadata.time != now
            || self.event.time != now
            || self.event.id != self.metadata.event_id
            || self.event.caused_by != Some(crate::CausedBy::Action(self.metadata.action_id))
        {
            return Err(BodyMutationError::Invalid(
                "prepared rule override owner binding",
            ));
        }
        let crate::WorldEventBody::ActionOverridden(record) = &self.event.body else {
            return Err(BodyMutationError::Invalid(
                "prepared rule override event kind",
            ));
        };
        if record.action_id != self.metadata.action_id {
            return Err(BodyMutationError::Invalid(
                "prepared rule override action binding",
            ));
        }
        Ok(())
    }
}
/// Constructed only after the real sink and counted ordered fact both succeed.
pub struct RootRuleOverrideAuditV1 {
    event: ReservedValue<crate::WorldEvent>,
    metadata: ReservedValue<OverrideMetadataV1>,
}
impl RootRuleOverrideAuditV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    ) -> Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> {
        tx.validate_transaction_identity(&self.metadata.guard)
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn publish_action_override_v1(
        &mut self,
        selected: &crate::world::incremental_action_execution::SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<RootRuleOverrideAuditV1>, RootModuleExecutionErrorV1<O::Error>> {
        let result =
            (|| {
                if (self.ctx.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                let live = live_owner();
                self.ctx
                    .transaction
                    .validate_guard(&self.ctx.transaction.guard(), &live)?;
                if self.ctx.values.class != MemoryClass::Ordinary
                    || !outcome.uses_budget(self.ctx.values.memory)
                    || !self.ctx.transaction.uses_budget(self.ctx.values.memory)
                    || !self
                        .events
                        .mutation
                        .memory
                        .same_pool(self.ctx.values.memory)
                {
                    return Err(ValueTreeError::Invalid("rule override outcome pool/class").into());
                }
                let replacement = outcome
                    .override_action_for_selected_v1(selected, self.ctx.transaction)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                let Some(replacement) = replacement else {
                    return Ok(None);
                };
                self.events
                    .world_events
                    .validate(self.ctx.transaction)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                crate::world::incremental_clock::check_clock(
                    self.now,
                    self.ctx.transaction,
                    &mut self.events.mutation,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
                let sequence = crate::world::World::read_iws_allocator(
                    crate::world::incremental_allocators::RuntimeAllocatorV1::Event,
                    self.ctx.transaction,
                    self.events.values,
                    &mut self.events.mutation,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
                let (id, _, _) =
                    crate::world::World::preview_next_event_id(sequence.next, sequence.era);
                let event =
                oasis7_world_state::incremental_world_event_owned::retain_action_override_event_v1(
                    id,
                    self.now,
                    selected.envelope().id,
                    &selected.envelope().action,
                    replacement,
                    self.ctx.values.memory,
                    MemoryClass::Ordinary,
                    audit_work,
                    &mut self.ctx.cancelled,
                )?;
                let live = live_owner();
                self.ctx
                    .transaction
                    .validate_guard(&self.ctx.transaction.guard(), &live)?;
                let metadata_fee = self
                    .ctx
                    .values
                    .memory
                    .reserve(
                        std::mem::size_of::<OverrideMetadataV1>()
                            + std::mem::size_of::<PreparedRuleOverrideEventV1>()
                            + std::mem::size_of::<RootRuleOverrideAuditV1>(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                let prepared = PreparedRuleOverrideEventV1 {
                    event,
                    metadata: metadata_fee.retain(OverrideMetadataV1 {
                        guard: self.ctx.transaction.guard(),
                        action_id: selected.envelope().id,
                        event_id: id,
                        time: self.now,
                        class: MemoryClass::Ordinary,
                    }),
                };
                crate::world::incremental_module_execution_event::contribute_rule_override_event_v1(
                self.now, &prepared, &live, self.ctx.transaction, self.events,
            ).map_err(RootModuleExecutionErrorV1::Body)?;
                if (self.ctx.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                let live = live_owner();
                self.ctx
                    .transaction
                    .validate_guard(&self.ctx.transaction.guard(), &live)?;
                let PreparedRuleOverrideEventV1 { event, metadata } = prepared;
                Ok(Some(RootRuleOverrideAuditV1 { event, metadata }))
            })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}
