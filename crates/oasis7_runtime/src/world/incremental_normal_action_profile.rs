//! Four profile-governance actions over actual Current owners.
//! Common admission/Accepted/Rule fees/PostAction remain with the sole driver.
use crate::world::{
    World,
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
    incremental_world_native::WorldNativeFieldsQualificationV1,
};
use std::cell::RefCell;
#[path = "incremental_normal_action_profile_commit.rs"]
mod commit;
#[path = "incremental_normal_action_profile_current.rs"]
mod current;
#[path = "incremental_normal_action_profile_event.rs"]
mod event;
#[path = "incremental_normal_action_profile_rules.rs"]
mod rules;
pub(in crate::world) struct ProfileGovernanceSourcesV1<'a> {
    pub agents: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub native: &'a ReservedValue<WorldNativeFieldsQualificationV1>,
    pub proposals: &'a ReservedValue<
        oasis7_world_state::incremental_runtime_proposal::RuntimeProposalsQualificationV1,
    >,
    pub materials: &'a ReservedValue<
        oasis7_world_state::incremental_material_records::MaterialFamilyQualificationV1,
    >,
}
pub(in crate::world) struct ProfileGovernanceWorkV1 {
    pub shared: crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1,
    pub proposal:
        oasis7_world_state::incremental_runtime_proposal::RuntimeProposalReadWorkV1,
    pub material: oasis7_world_state::incremental_material_records::MaterialWorkBudgetV1,
    pub material_scan: StateScanBudget,
}
impl ProfileGovernanceWorkV1 {
    fn spend(&mut self, n: usize) -> Result<(), BodyMutationError> {
        self.shared.steps_remaining = self
            .shared
            .steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("profile governance steps"))?;
        self.shared.bytes_remaining = self
            .shared
            .bytes_remaining
            .checked_sub(n as u64)
            .ok_or(ValueTreeError::ResourceLimited("profile governance bytes"))?;
        Ok(())
    }
}
fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}

/// Constructed only after this module's actual Current publication succeeds.
pub(in crate::world) struct PreparedProfileGovernanceEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    action_id: u64,
    time: u64,
    touch_operator: bool,
    identity: PreparedGuard,
}
impl PreparedProfileGovernanceEventV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if !self.event.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.time != now
            || self.event.time != now
            || self.event.caused_by != Some(crate::CausedBy::Action(self.action_id))
        {
            return Err(BodyMutationError::Invalid(
                "profile governance prepared identity",
            ));
        }
        if !rules::event_matches(self.event()) {
            return Err(BodyMutationError::Invalid("profile governance event kind"));
        }
        Ok(())
    }
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
}
pub(in crate::world) struct ProfileGovernanceRejectionReservationV1 {
    memory: MemoryBudget,
    fee: MemoryReservation,
}
pub(in crate::world) struct CompletedProfileGovernanceV1 {
    owner: CompletedOwner,
    _fee: MemoryReservation,
}
enum CompletedOwner {
    Published {
        event: ReservedValue<crate::WorldEvent>,
        identity: PreparedGuard,
    },
    Rejected(SelectedActionAuditV1),
}
impl CompletedProfileGovernanceV1 {
    pub(in crate::world) fn reserve_rejection(
        memory: &MemoryBudget,
    ) -> Result<ProfileGovernanceRejectionReservationV1, BodyMutationError> {
        Ok(ProfileGovernanceRejectionReservationV1 {
            memory: memory.clone(),
            fee: memory
                .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?,
        })
    }
    pub(in crate::world) fn from_rejection<M: MerkleNodeReader>(
        owner: SelectedActionAuditV1,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        prepaid: ProfileGovernanceRejectionReservationV1,
    ) -> Result<Self, BodyMutationError> {
        selected.validate(tx)?;
        owner.validate(tx)?;
        if !tx.uses_budget(memory)
            || !prepaid.memory.same_pool(memory)
            || owner.event().caused_by != Some(crate::CausedBy::Action(selected.envelope().id))
            || !matches!(&owner.event().body,crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected{action_id,..})if *action_id==selected.envelope().id)
        {
            return Err(BodyMutationError::Invalid(
                "factory rejection receipt binding",
            ));
        }
        Ok(Self {
            owner: CompletedOwner::Rejected(owner),
            _fee: prepaid.fee,
        })
    }

    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match &self.owner {
            CompletedOwner::Published { event, .. } => event,
            CompletedOwner::Rejected(owner) => owner.event(),
        }
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match &self.owner {
            CompletedOwner::Published { identity, .. } => {
                tx.validate_transaction_identity(identity)
            }
            CompletedOwner::Rejected(owner) => owner.validate(tx),
        }
    }
}

/// Execute only an actual FIFO selection and the same transaction's genuine
/// Rule outcome. No Accepted event is introduced for either lifecycle action.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_selected_profile_governance_v1<M, P, V, W, E, C, PE, PC, ME, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<crate::world::incremental_module_invocation::RootRulePhaseOutcomeV1>,
    sources: &ProfileGovernanceSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut ProfileGovernanceWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    mut persist: F,
) -> Result<CompletedProfileGovernanceV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        step.transaction.validate_guard(guard, live)?;
        selected.validate(step.transaction)?;
        let memory = step.mutation.memory;
        if !step.transaction.uses_budget(memory)
            || !outcome.uses_budget(memory)
            || read.values.class != MemoryClass::Ordinary
            || parent_read.values.class != MemoryClass::Ordinary
            || !read.values.memory.same_pool(memory)
            || !parent_read.values.memory.same_pool(memory)
            || !sources.agents.uses_budget(memory)
            || sources.agents.allocation_class() != MemoryClass::Ordinary
            || !sources.native.uses_budget(memory)
            || !sources.proposals.uses_budget(memory)
            || !sources.materials.uses_budget(memory)
        {
            return Err(BodyMutationError::Invalid(
                "profile governance source pool/class",
            ));
        }
        read.check()?;
        parent_read.check()?;
        let _binding_fee = memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let digest = materials.roots.digest()?;
        if digest != step.transaction.parent_fence().parent_roots {
            return Err(BodyMutationError::Invalid("profile governance roots"));
        }
        sources.native.validate_binding(materials.roots)?;
        sources.proposals.validate_binding(materials.roots)?;
        sources.materials.validate_binding(&digest)?;
        let replacement = outcome.override_action_for_selected_v1(selected, step.transaction)?;
        if outcome.decision.verdict == crate::RuleVerdict::Deny {
            return Err(BodyMutationError::Invalid(
                "profile governance denied Rule outcome",
            ));
        }
        let action = replacement.unwrap_or(&selected.envelope().action);
        let _phase = rules::reserve_phase(action, work, memory, &mut step.mutation.cancelled)?;
        let body = current::evaluate(
            action,
            selected.envelope().id,
            step,
            sources,
            materials.roots,
            read,
            parent_read,
            work,
            live,
        )?;
        if let crate::DomainEvent::ActionRejected { reason, .. } = body {
            let prepaid = CompletedProfileGovernanceV1::reserve_rejection(memory)?;
            let audit = step.append_selected_rejection_owned(
                materials,
                live,
                selected,
                reason,
                max_event_bytes,
                audit_work,
                &mut persist,
            )?;
            return CompletedProfileGovernanceV1::from_rejection(
                audit,
                selected,
                step.transaction,
                memory,
                prepaid,
            );
        }
        // The completion carrier is paid before state/original publication.
        let completed_fee = memory
            .reserve(
                std::mem::size_of::<CompletedProfileGovernanceV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        commit::apply(&body, step, materials, sources, read, work, live)?;
        let sequence =
            step.read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let (id, _, _) = World::preview_next_event_id(sequence.next, sequence.era);
        let event = crate::WorldEvent {
            id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(body),
        };
        let event = oasis7_world_state::incremental_world_event_owned::retain_world_event_owned_v1(
            event,
            memory,
            MemoryClass::Ordinary,
            audit_work,
            &mut step.mutation.cancelled,
        )?;
        let prepared = PreparedProfileGovernanceEventV1 {
            event,
            action_id: selected.envelope().id,
            time: step.time(),
            touch_operator: true,
            identity: step.transaction.guard(),
        };
        prepared.validate(step.transaction, memory, step.time())?;
        let before = step.begin_world_event()?;
        event::sink(step, &prepared, work, max_event_bytes, live, &mut persist)?;
        step.finish_world_event(before)?;
        if (step.mutation.cancelled)() || (read.cancelled)() || (parent_read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(CompletedProfileGovernanceV1 {
            owner: CompletedOwner::Published {
                event: prepared.event,
                identity: step.transaction.guard(),
            },
            _fee: completed_fee,
        })
    }));
    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            let _: Result<(), ExecutionError> = step
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
            Err(error)
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> = step
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}
