//! Actual EmitObservation business publication. Common Rule/fees/Accepted/Post
//! remain in the fixed sole driver; this owner never selects or pops an input.
use super::{
    incremental_action_execution::SelectedPendingActionV1,
    incremental_allocators::RuntimeAllocatorV1,
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::{RootRulePhaseOutcomeV1, SelectedAcceptedDispositionV1},
    incremental_step::IwsStepContext,
};
use oasis7_agent_api::world_action::{Observation, ObservedAgent};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::mem::size_of;
fn invalid(reason: &'static str) -> BodyMutationError {
    BodyMutationError::Invalid(reason)
}
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("normal observation layout").into()
}
fn charge(ctx: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    ctx.charge(1, u64::try_from(n).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
fn copy_text(text: &str) -> Result<String, BodyMutationError> {
    let mut out = String::with_capacity(text.len());
    if out.capacity() > text.len() {
        return Err(limited());
    }
    out.push_str(text);
    Ok(out)
}
pub(in crate::world) struct PreparedObservationActionV1<'s, 'r> {
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    observation: Observation,
    identity: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    time: u64,
    fee: MemoryReservation,
}
pub(in crate::world) struct CompletedObservationActionV1 {
    accepted: Option<super::incremental_action_execution::SelectedActionAuditV1>,
    event: ReservedValue<crate::WorldEvent>,
    identity: PreparedGuard,
}
impl CompletedObservationActionV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        tx.validate_transaction_identity(&self.identity)?;
        if let Some(accepted) = &self.accepted {
            accepted.validate(tx)?;
        }
        Ok(())
    }
}
pub(in crate::world) fn prepare_observation_action_v1<'s, 'r, M: MerkleNodeReader>(
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    time: u64,
    ctx: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedObservationActionV1<'s, 'r>, BodyMutationError> {
    let result = (|| {
        charge(ctx, 0)?;
        tx.validate_guard(&tx.guard(), live)?;
        selected.validate(tx)?;
        if !tx.uses_budget(ctx.memory())
            || !outcome.uses_budget(ctx.memory())
            || outcome.decision.verdict == crate::RuleVerdict::Deny
        {
            return Err(invalid("normal observation Selected/Rule/pool"));
        }
        let effective = outcome
            .override_action_for_selected_v1(selected, tx)?
            .unwrap_or(&selected.envelope().action);
        let crate::Action::EmitObservation { observation } = effective else {
            return Err(invalid("normal observation effective action"));
        };
        let mut bytes = size_of::<crate::WorldEvent>()
            .checked_add(size_of::<CompletedObservationActionV1>())
            .and_then(|n| n.checked_add(size_of::<PreparedObservationActionV1<'s, 'r>>()))
            .and_then(|n| {
                n.checked_add(
                    size_of::<ObservedAgent>().checked_mul(observation.visible_agents.len())?,
                )
            })
            .and_then(|n| n.checked_add(observation.agent_id.len()))
            .ok_or_else(limited)?;
        charge(ctx, observation.agent_id.len())?;
        for visible in &observation.visible_agents {
            charge(ctx, visible.agent_id.len())?;
            bytes = bytes
                .checked_add(visible.agent_id.len())
                .ok_or_else(limited)?;
        }
        let fee = ctx
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        // Exact-capacity allocations are admitted before any payload copy.
        let mut visible_agents = Vec::with_capacity(observation.visible_agents.len());
        if visible_agents.capacity() > observation.visible_agents.len() {
            return Err(limited());
        }
        for visible in &observation.visible_agents {
            charge(ctx, visible.agent_id.len())?;
            visible_agents.push(ObservedAgent {
                agent_id: copy_text(&visible.agent_id)?,
                pos: visible.pos,
                distance_cm: visible.distance_cm,
            });
        }
        charge(ctx, observation.agent_id.len())?;
        let observation = Observation {
            time: observation.time,
            agent_id: copy_text(&observation.agent_id)?,
            pos: observation.pos,
            visibility_range_cm: observation.visibility_range_cm,
            visible_agents,
        };
        charge(ctx, 0)?;
        Ok(PreparedObservationActionV1 {
            selected,
            outcome,
            observation,
            identity: tx.guard(),
            core: ctx.work().identity(),
            memory: ctx.memory().clone(),
            time,
            fee,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
/// Root inserts the genuine selective disposition between prepare and commit.
/// No state field is replaced by Observation: original CorePolicy normalizes
/// materials and routes the existing agent mailbox, preserving old last_active.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn commit_observation_action_v1<M, V, W, D, ME, F>(
    prepared: PreparedObservationActionV1<'_, '_>,
    accepted: SelectedAcceptedDispositionV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    ctx: &mut FormalBoundedContextV1<'_>,
    max_event_bytes: usize,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<CompletedObservationActionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        charge(ctx, 0)?;
        step.transaction
            .validate_transaction_identity(&prepared.identity)?;
        prepared.selected.validate(step.transaction)?;
        accepted.validate(prepared.selected, step.transaction, step.mutation.memory)?;
        if prepared.core != ctx.work().identity()
            || !prepared.memory.same_pool(step.mutation.memory)
            || !prepared.memory.same_pool(ctx.memory())
            || prepared.time != step.time()
        {
            return Err(invalid("normal observation commit source"));
        }
        prepared
            .outcome
            .override_action_for_selected_v1(prepared.selected, step.transaction)?;
        let sequence = step.read_allocator(RuntimeAllocatorV1::Event)?;
        let (id, _, _) = super::World::preview_next_event_id(sequence.next, sequence.era);
        let event = crate::WorldEvent {
            id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(prepared.selected.envelope().id)),
            body: crate::WorldEventBody::Domain(crate::DomainEvent::Observation {
                observation: prepared.observation,
            }),
        };
        step.dispatch_agent_event(&event, materials, live, max_event_bytes, &mut *persist)?;
        charge(ctx, 0)?;
        Ok(CompletedObservationActionV1 {
            accepted: accepted.into_audit(),
            event: prepared.fee.retain(event),
            identity: step.transaction.guard(),
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
