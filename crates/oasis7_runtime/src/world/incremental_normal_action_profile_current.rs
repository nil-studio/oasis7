//! Genuine Current agent, manifest-proposal status and immutable factory
//! profile observations, with original gate/rejection order.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<M, P, V, W, E, C, PE, PC, D>(
    action: &crate::Action,
    action_id: u64,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    sources: &ProfileGovernanceSourcesV1<'_>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut ProfileGovernanceWorkV1,
    live: &ExecutionFence,
) -> Result<crate::DomainEvent, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    D: FnMut() -> bool,
{
    let (operator, id, label) =
        rules::action(action).ok_or(BodyMutationError::Invalid("profile action family"))?;
    work.spend(operator.len())?;
    let present = {
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: step.transaction,
            values: ValueReadContext {
                source: read.values.source,
                memory: step.mutation.memory,
                class: MemoryClass::Ordinary,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
        };
        let guard = ctx.transaction.guard();
        read_current_agent_presence_v1(
            operator,
            sources.agents,
            roots,
            &mut ctx,
            parent_read,
            &mut work.shared.pending_entries_remaining,
            &guard,
            live,
        )
        .map_err(overlay_error)?
            == CurrentAgentPresenceV1::Existing
    };
    // The original agent/zero-id short circuits precede the proposal read.
    if !present || id == 0 {
        return Ok(World::evaluate_profile_governance_gate_from_current_v1(
            action_id, operator, id, label, present, None,
        )
        .expect("failed original early gate"));
    }
    let status = {
        let mut ctx = WorldFieldReadContextV1 {
            root: roots.state_root(
                oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
            ),
            state: read.state,
            values: ValueReadContext {
                source: read.values.source,
                memory: step.mutation.memory,
                class: MemoryClass::Ordinary,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
        };
        let guard = step.transaction.guard();
        oasis7_world_state::incremental_runtime_proposal::read_current_runtime_proposal_status_kind_v1(id,sources.proposals,roots,step.transaction,&mut ctx,&mut work.proposal,&guard,live).map_err(overlay_error)?
    };
    if let Some(rejected) = World::evaluate_profile_governance_gate_from_current_v1(
        action_id, operator, id, label, true, status,
    ) {
        return Ok(rejected);
    }
    let event = rules::profile(action_id, action, None)?;
    if let crate::DomainEvent::FactoryProfileGoverned { profile, .. } = &event {
        let mut ctx = WorldFieldReadContextV1 {
            root: roots
                .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business),
            state: read.state,
            values: ValueReadContext {
                source: read.values.source,
                memory: step.mutation.memory,
                class: MemoryClass::Ordinary,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
        };
        let guard = step.transaction.guard();
        let current =
            oasis7_world_state::incremental_world_native::read_current_governed_factory_profile_v1(
                &profile.factory_id,
                sources.native,
                roots,
                step.transaction,
                &mut ctx,
                &mut work.shared.native_scan,
                &mut work.shared.native_read,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
        if let Err(e)=oasis7_world_state::state::module_release_transition::validate_governed_factory_profile_v1(profile,current.value()) {return Ok(crate::DomainEvent::ActionRejected{action_id,reason:crate::RejectReason::RuleDenied{notes:vec![format!("{label} rejected: {e:?}")]}})}
    }
    read.check()?;
    parent_read.check()?;
    if (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(event)
}
