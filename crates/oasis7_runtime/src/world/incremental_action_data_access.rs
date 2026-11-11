//! Genuine selected Grant/RevokeDataAccess business owner; no admission authority.
use crate::world::{
    World,
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};

use oasis7_world_state::{
    incremental_authorization_mapsets::{
        AuthorizationMapSetFieldV1, AuthorizationMembershipV1,
        read_authorization_membership_overlay_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_world_collection_controls::{
        WorldCollectionControlsQualificationV1, WorldCollectionControlsWorkV1,
    },
};
#[path = "incremental_action_data_access_event.rs"]
mod event;
pub(in crate::world) struct DataAccessWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub controls: WorldCollectionControlsWorkV1,
}
impl DataAccessWorkV1 {
    fn spend(&mut self, bytes: usize) -> Result<(), BodyMutationError> {
        self.steps_remaining = self
            .steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("data access steps"))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(
                u64::try_from(bytes)
                    .map_err(|_| ValueTreeError::ResourceLimited("data access byte bound"))?,
            )
            .ok_or(ValueTreeError::ResourceLimited("data access byte work"))?;
        Ok(())
    }
}

struct PreparedDataAccessV1 {
    event: ReservedValue<crate::WorldEvent>,
    action_id: u64,
    now: u64,
    expected: AuthorizationMembershipV1,
    identity: PreparedGuard,
}
impl PreparedDataAccessV1 {
    fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if !tx.uses_budget(memory)
            || !self.event.uses_budget(memory)
            || self.now != now
            || self.event.time != now
            || self.event.caused_by != Some(crate::CausedBy::Action(self.action_id))
        {
            return Err(BodyMutationError::Invalid("prepared DataAccess binding"));
        }
        let (from, to, _) = self.route();
        if from == to {
            return Err(BodyMutationError::Invalid(
                "prepared DataAccess distinct agents",
            ));
        }
        Ok(())
    }
    fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    fn route(&self) -> (&str, &str, bool) {
        match &self.event.body {
            crate::WorldEventBody::Domain(crate::DomainEvent::DataAccessGranted {
                owner_agent_id,
                grantee_agent_id,
            }) => (owner_agent_id, grantee_agent_id, true),
            crate::WorldEventBody::Domain(crate::DomainEvent::DataAccessRevoked {
                owner_agent_id,
                grantee_agent_id,
            }) => (owner_agent_id, grantee_agent_id, false),
            _ => unreachable!("private prepared DataAccess event"),
        }
    }
}
pub(in crate::world) struct CompletedDataAccessV1 {
    owner: CompletedDataAccessOwnerV1,
    _fee: MemoryReservation,
}
enum CompletedDataAccessOwnerV1 {
    Rejection(SelectedActionAuditV1),
    Changed {
        event: ReservedValue<crate::WorldEvent>,
        identity: PreparedGuard,
    },
}
impl CompletedDataAccessV1 {
    pub(in crate::world) fn from_rejection<M: MerkleNodeReader>(
        owner: SelectedActionAuditV1,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<Self, BodyMutationError> {
        selected.validate(tx)?;
        owner.validate(tx)?;
        if !tx.uses_budget(memory)
            || owner.event().caused_by != Some(crate::CausedBy::Action(selected.envelope().id))
            || !matches!(&owner.event().body,
                crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { action_id, .. })
                if *action_id == selected.envelope().id)
        {
            return Err(BodyMutationError::Invalid(
                "DataAccess rejection receipt binding",
            ));
        }
        let fee = memory
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        Ok(Self {
            owner: CompletedDataAccessOwnerV1::Rejection(owner),
            _fee: fee,
        })
    }
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match &self.owner {
            CompletedDataAccessOwnerV1::Rejection(owner) => owner.event(),
            CompletedDataAccessOwnerV1::Changed { event, .. } => event,
        }
    }

    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match &self.owner {
            CompletedDataAccessOwnerV1::Rejection(owner) => owner.validate(tx),
            CompletedDataAccessOwnerV1::Changed { identity, .. } => {
                tx.validate_transaction_identity(identity)
            }
        }
    }
}

fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(error) => BodyMutationError::Execution(error),
        WorldFieldOverlayErrorV1::Value(error) => BodyMutationError::Value(error),
        WorldFieldOverlayErrorV1::Member(error) => BodyMutationError::Original(Box::new(error)),
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_selected_data_access_v1<M, P, V, W, E, C, PE, PC, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<crate::world::incremental_module_invocation::RootRulePhaseOutcomeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    roots: &RootSetV1,
    controls: &ReservedValue<WorldCollectionControlsQualificationV1>,
    writer: &mut MemberRecordWriteContext<'_, W, impl FnMut() -> bool>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut DataAccessWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    mut persist_original: F,
) -> Result<CompletedDataAccessV1, BodyMutationError>
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
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        step.transaction.validate_guard(guard, live)?;
        selected.validate(step.transaction)?;
        if !outcome.uses_budget(step.mutation.memory)
            || read.values.class != MemoryClass::Ordinary
            || parent_read.values.class != MemoryClass::Ordinary
            || !read.values.memory.same_pool(step.mutation.memory)
            || !parent_read.values.memory.same_pool(step.mutation.memory)
            || !step.transaction.uses_budget(step.mutation.memory)
            || !agents.uses_budget(step.mutation.memory)
        {
            return Err(BodyMutationError::Invalid(
                "DataAccess selected-action pool/class",
            ));
        }
        read.check()?;
        parent_read.check()?;

        let replacement = outcome.override_action_for_selected_v1(selected, step.transaction)?;
        if outcome.decision.verdict == crate::RuleVerdict::Deny {
            return Err(BodyMutationError::Invalid(
                "data access denied by Rule outcome",
            ));
        }
        let action = replacement.unwrap_or(&selected.envelope().action);
        let (from, to, grant) = match action {
            crate::Action::GrantDataAccess {
                owner_agent_id,
                grantee_agent_id,
            } => (owner_agent_id, grantee_agent_id, true),
            crate::Action::RevokeDataAccess {
                owner_agent_id,
                grantee_agent_id,
            } => (owner_agent_id, grantee_agent_id, false),
            _ => {
                return Err(BodyMutationError::Invalid(
                    "selected DataAccess action kind",
                ));
            }
        };
        work.spend(
            from.len()
                .checked_add(to.len())
                .ok_or(ValueTreeError::ResourceLimited("data access identifiers"))?,
        )?;
        let phase_bytes = from
            .len()
            .checked_add(to.len())
            .and_then(|n| n.checked_mul(8))
            .and_then(|n| n.checked_add(16 << 10))
            .ok_or(ValueTreeError::ResourceLimited("data access phase"))?;
        let _phase_fee = step
            .mutation
            .memory
            .reserve(phase_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        for agent in [from, to] {
            work.spend(agent.len())?;
            let presence = {
                let mut overlay = WorldFieldOverlayContextV1 {
                    transaction: step.transaction,
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
                    cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
                };
                let guard = overlay.transaction.guard();
                read_current_agent_presence_v1(
                    agent,
                    agents,
                    roots,
                    &mut overlay,
                    parent_read,
                    &mut work.pending_entries_remaining,
                    &guard,
                    live,
                )
                .map_err(overlay_error)?
            };
            if presence != CurrentAgentPresenceV1::Existing {
                return reject(
                    step,
                    materials,
                    live,
                    selected,
                    crate::RejectReason::AgentNotFound {
                        agent_id: agent.clone(),
                    },
                    max_event_bytes,
                    audit_work,
                    &mut persist_original,
                );
            }
        }
        if from == to {
            return reject(
                step,
                materials,
                live,
                selected,
                crate::RejectReason::RuleDenied {
                    notes: vec![
                        if grant {
                            "data access grant requires distinct owner and grantee"
                        } else {
                            "data access revoke requires distinct owner and grantee"
                        }
                        .into(),
                    ],
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        let expected = {
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: step.transaction,
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
            read_authorization_membership_overlay_v1(
                AuthorizationMapSetFieldV1::DataAccessPermissions,
                from,
                to,
                &mut overlay,
            )
            .map_err(overlay_error)?
        };
        let owner_fee = reserve_completed_fee(
            step.mutation.memory,
            from.len()
                .checked_add(to.len())
                .ok_or(ValueTreeError::ResourceLimited("data access owner text"))?,
        )?;
        let sequence =
            step.read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let (event_id, _, _) = World::preview_next_event_id(sequence.next, sequence.era);
        let body = if grant {
            crate::DomainEvent::DataAccessGranted {
                owner_agent_id: from.clone(),
                grantee_agent_id: to.clone(),
            }
        } else {
            crate::DomainEvent::DataAccessRevoked {
                owner_agent_id: from.clone(),
                grantee_agent_id: to.clone(),
            }
        };
        let event = crate::WorldEvent {
            id: event_id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(body),
        };
        let event = oasis7_world_state::incremental_world_event_owned::retain_world_event_owned_v1(
            event,
            step.mutation.memory,
            MemoryClass::Ordinary,
            audit_work,
            &mut step.mutation.cancelled,
        )
        .map_err(BodyMutationError::Value)?;
        let prepared = PreparedDataAccessV1 {
            event,
            action_id: selected.envelope().id,
            now: step.time(),
            expected,
            identity: step.transaction.guard(),
        };
        let before = step.begin_world_event()?;
        event::sink_prepared_data_access_v1(
            step,
            materials,
            &prepared,
            agents,
            roots,
            controls,
            writer,
            read,
            work,
            max_event_bytes,
            live,
            &mut persist_original,
        )?;
        step.finish_world_event(before)?;
        Ok(CompletedDataAccessV1 {
            owner: CompletedDataAccessOwnerV1::Changed {
                event: prepared.event,
                identity: step.transaction.guard(),
            },
            _fee: owner_fee,
        })
    })();
    let result = result.and_then(|completed| {
        if (step.mutation.cancelled)() || (read.cancelled)() || (parent_read.cancelled)() {
            Err(ValueTreeError::Cancelled.into())
        } else {
            Ok(completed)
        }
    });
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

fn reserve_completed_fee(
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    text_bytes: usize,
) -> Result<MemoryReservation, BodyMutationError> {
    let bytes = std::mem::size_of::<CompletedDataAccessV1>()
        .checked_add(std::mem::size_of::<PreparedDataAccessV1>())
        .and_then(|bytes| bytes.checked_add(text_bytes))
        .and_then(|bytes| bytes.checked_add(256))
        .ok_or(ValueTreeError::ResourceLimited(
            "DataAccess owner allocation",
        ))?;
    memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(Into::into)
}

#[allow(
    clippy::too_many_arguments,
    reason = "The genuine material owner and live fence remain explicit across the rejection sink."
)]
fn reject<M, V, W, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    live: &ExecutionFence,
    selected: &SelectedPendingActionV1,
    reason: crate::RejectReason,
    max_event_bytes: usize,
    audit_work: &mut WorldEventDecodeWorkV1,
    persist_original: &mut F,
) -> Result<CompletedDataAccessV1, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let owner_fee = reserve_completed_fee(step.mutation.memory, 0)?;
    let audit = step.append_selected_rejection_owned(
        materials,
        live,
        selected,
        reason,
        max_event_bytes,
        audit_work,
        persist_original,
    )?;
    Ok(CompletedDataAccessV1 {
        owner: CompletedDataAccessOwnerV1::Rejection(audit),
        _fee: owner_fee,
    })
}
