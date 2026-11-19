//! Selective Accepted is an actual audit occurrence, never business permission.
use super::*;
use crate::world::incremental_action_execution::{
    AcceptedActorReadContextV1, SelectedActionAuditV1, SelectedPendingActionV1,
};

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
    /// The family driver owns legacy ordering: business preflight first when
    /// applicable, then this original audit, then the actual result. This method
    /// deliberately does not route Accepted as a separate PostEvent.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn append_selected_accepted_v1<P, ER, CR, PE, PC>(
        &mut self,
        selected: &SelectedPendingActionV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, impl FnMut() -> Option<ValueTreeError>>,
        agents: &ReservedValue<
            oasis7_world_state::incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
        >,
        read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            ER,
            CR,
        >,
        parent_read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            PE,
            PC,
        >,
        pending_entries: &mut usize,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<SelectedActionAuditV1>, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result = (|| {
            if self.ctx.values.class != MemoryClass::Ordinary
                || read.values.class != MemoryClass::Ordinary
                || parent_read.values.class != MemoryClass::Ordinary
                || !self
                    .ctx
                    .values
                    .memory
                    .same_pool(self.events.mutation.memory)
                || !self.ctx.values.memory.same_pool(read.values.memory)
                || !self.ctx.values.memory.same_pool(parent_read.values.memory)
                || !agents.uses_budget(self.ctx.values.memory)
            {
                return Err(RootModuleExecutionErrorV1::Body(
                    BodyMutationError::Invalid("selective Accepted context pool/class"),
                ));
            }
            if (self.ctx.cancelled)() || (self.events.mutation.cancelled)() {
                return Err(RootModuleExecutionErrorV1::Value(ValueTreeError::Cancelled));
            }
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            let mut step = crate::world::incremental_step::IwsStepContext::borrow_counted_at(
                self.ctx.transaction,
                self.now,
                &guard,
                &live,
                self.events.values,
                &mut self.events.mutation,
                &mut self.events.original_work,
                self.events.world_events,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let mut actor_read = AcceptedActorReadContextV1 {
                roots: self.families.roots,
                agents,
                current: read,
                parent: parent_read,
                pending_entries,
                live: &live,
            };
            step.append_selected_accepted(
                materials,
                &live,
                selected,
                &mut actor_read,
                self.events.max_event_bytes,
                audit_work,
                &mut self.events.persist_original,
            )
            .map_err(RootModuleExecutionErrorV1::Body)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn append_selected_accepted_disposition_v1<P, ER, CR, PE, PC>(
        &mut self,
        selected: &SelectedPendingActionV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, impl FnMut() -> Option<ValueTreeError>>,
        agents: &ReservedValue<
            oasis7_world_state::incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
        >,
        read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            ER,
            CR,
        >,
        parent_read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            PE,
            PC,
        >,
        pending_entries: &mut usize,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<SelectedAcceptedDispositionV1, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result = (|| {
            let charge = self
                .ctx
                .values
                .memory
                .reserve(
                    std::mem::size_of::<SelectedAcceptedDispositionV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(|e| RootModuleExecutionErrorV1::Value(ValueTreeError::Memory(e)))?;
            let audit = self.append_selected_accepted_v1(
                selected,
                materials,
                agents,
                read,
                parent_read,
                pending_entries,
                audit_work,
                live_owner,
            )?;
            let result = SelectedAcceptedDispositionV1 {
                identity: self.ctx.transaction.guard(),
                action_id: selected.envelope().id,
                audit,
                charge: charge.retain(()),
            };
            result
                .validate(selected, self.ctx.transaction, self.ctx.values.memory)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            Ok(result)
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

/// Minted only by the genuine selective Accepted append above. None is a
/// verified original-action disposition, never a caller-supplied bypass.
pub(in crate::world) struct SelectedAcceptedDispositionV1 {
    identity: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    action_id: crate::ActionId,
    audit: Option<SelectedActionAuditV1>,
    charge: ReservedValue<()>,
}
impl SelectedAcceptedDispositionV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<(), BodyMutationError> {
        selected.validate(tx)?;
        tx.validate_transaction_identity(&self.identity)?;
        if !tx.uses_budget(memory)
            || !self.charge.uses_budget(memory)
            || self.action_id != selected.envelope().id
            || crate::world::World::should_emit_action_accepted(&selected.envelope().action)
                != self.audit.is_some()
        {
            return Err(BodyMutationError::Invalid(
                "selected Accepted disposition source",
            ));
        }
        if let Some(audit) = &self.audit {
            audit.validate(tx)?;
            if !audit.uses_budget(memory) {
                return Err(BodyMutationError::Invalid(
                    "selected Accepted disposition pool",
                ));
            }
        }
        Ok(())
    }
    pub(in crate::world) fn audit(&self) -> Option<&SelectedActionAuditV1> {
        self.audit.as_ref()
    }
    pub(in crate::world) fn into_audit(self) -> Option<SelectedActionAuditV1> {
        self.audit
    }
    pub(in crate::world) fn uses_budget(
        &self,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> bool {
        self.charge.uses_budget(memory) && self.audit.as_ref().is_none_or(|a| a.uses_budget(memory))
    }
}
