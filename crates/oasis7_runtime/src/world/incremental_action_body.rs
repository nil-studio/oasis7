//! Selected queued body actions, using the same Current transaction and reducer.
//! Admission, Rule/Accepted/PostAction sequencing belongs to the shared driver.
use super::{
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_allocators::RuntimeAllocatorV1,
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, read_fixed},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};

pub(super) struct BodyActionWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub pending_entries_remaining: usize,
}
impl BodyActionWorkV1 {
    fn spend(&mut self, bytes: usize) -> Result<(), BodyMutationError> {
        self.steps_remaining =
            self.steps_remaining
                .checked_sub(1)
                .ok_or(BodyMutationError::Value(ValueTreeError::ResourceLimited(
                    "body action steps",
                )))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(u64::try_from(bytes).map_err(|_| {
                BodyMutationError::Value(ValueTreeError::ResourceLimited("body action byte bound"))
            })?)
            .ok_or(BodyMutationError::Value(ValueTreeError::ResourceLimited(
                "body action bytes",
            )))?;
        Ok(())
    }
}
/// Constructed only after a real result reducer/original/counted fact succeeds.
/// Plain owned result data plus a producer receipt, never permission to execute.
pub(super) struct CompletedBodyActionV1 {
    owner: CompletedBodyActionOwnerV1,
}
enum CompletedBodyActionOwnerV1 {
    Rejection(SelectedActionAuditV1),
    Domain {
        event: ReservedValue<crate::WorldEvent>,
        identity: PreparedGuard,
    },
}
impl CompletedBodyActionV1 {
    /// Preserve the genuine selected-result receipt for common driver rejection.
    pub(super) fn from_rejection(owner: SelectedActionAuditV1) -> Self {
        Self {
            owner: CompletedBodyActionOwnerV1::Rejection(owner),
        }
    }
    pub(super) fn event(&self) -> &crate::WorldEvent {
        match &self.owner {
            CompletedBodyActionOwnerV1::Rejection(owner) => owner.event(),
            CompletedBodyActionOwnerV1::Domain { event, .. } => event,
        }
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match &self.owner {
            CompletedBodyActionOwnerV1::Rejection(owner) => owner.validate(tx),
            CompletedBodyActionOwnerV1::Domain { identity, .. } => {
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
/// Execute only EmitBodyAttributes, QueryObservation and BodyAction. The source
/// must be the authentic selected FIFO original. The caller retains its owner
/// and must run the common admission/Rule/Accepted protocol in its original order.
/// The effective action is borrowed only from the same-action genuine Rule outcome;
/// Rule Modify never replaces the selected original identity used for causes.
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_selected_body_action_v1<M, P, V, W, E, C, PE, PC, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<super::incremental_module_invocation::RootRulePhaseOutcomeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut BodyActionWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    mut persist: F,
) -> Result<CompletedBodyActionV1, BodyMutationError>
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
    let result =
        (|| {
            step.transaction.validate_guard(guard, live)?;
            selected.validate(step.transaction)?;
            if !outcome.uses_budget(step.mutation.memory) {
                return Err(BodyMutationError::Invalid("body action rule outcome pool"));
            }
            let replacement =
                outcome.override_action_for_selected_v1(selected, step.transaction)?;
            if outcome.decision.verdict == crate::RuleVerdict::Deny {
                return Err(BodyMutationError::Invalid(
                    "body action denied by genuine rule outcome",
                ));
            }
            let effective_action = replacement.unwrap_or(&selected.envelope().action);

            if !step.transaction.uses_budget(step.mutation.memory)
                || read.values.class != MemoryClass::Ordinary
                || parent_read.values.class != MemoryClass::Ordinary
                || !read.values.memory.same_pool(step.mutation.memory)
                || !parent_read.values.memory.same_pool(step.mutation.memory)
            {
                return Err(BodyMutationError::Invalid(
                    "selected body action pool/class",
                ));
            }
            let agent = match effective_action {
                crate::Action::EmitBodyAttributes { agent_id, .. }
                | crate::Action::QueryObservation { agent_id }
                | crate::Action::BodyAction { agent_id, .. } => agent_id,
                _ => {
                    return Err(BodyMutationError::Invalid(
                        "selected body action kind unsupported",
                    ));
                }
            };
            let reason_bytes = match effective_action {
                crate::Action::EmitBodyAttributes { reason, .. } => reason.len(),
                _ => 0,
            };
            work.spend(agent.len().checked_add(reason_bytes).ok_or(
                BodyMutationError::Invalid("selected body action text bound"),
            )?)?;
            // The old pure guard emits only a fixed template, one of four closed
            // field labels and at most three u64 decimal values (20 bytes each).
            let guard_reason_bytes = "cross_section_cm2".len()
                + 3 * 20
                + "body guard  out of range:  not in ..="
                    .len()
                    .max("body guard  rate violation: prev= next= factor=".len());
            let phase_bytes = agent
                .len()
                .checked_mul(3)
                .and_then(|n| n.checked_add(reason_bytes))
                .and_then(|n| n.checked_add(guard_reason_bytes * 2))
                .and_then(|n| {
                    n.checked_add(
                        std::mem::size_of::<crate::WorldEvent>()
                            + 2 * std::mem::size_of::<AgentBodyValueV1>(),
                    )
                })
                .ok_or(BodyMutationError::Invalid(
                    "selected body action allocation bound",
                ))?;
            let _phase = step
                .mutation
                .memory
                .reserve(phase_bytes, MemoryClass::Ordinary)
                .map_err(ExecutionError::Memory)?;
            if (step.mutation.cancelled)() || (read.cancelled)() || (parent_read.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
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
                    cancelled: &mut read.cancelled,
                };
                read_current_agent_presence_v1(
                    agent,
                    agents,
                    roots,
                    &mut overlay,
                    parent_read,
                    &mut work.pending_entries_remaining,
                    guard,
                    live,
                )
                .map_err(overlay_error)?
            };
            let existing = presence == CurrentAgentPresenceV1::Existing;
            let rejection = match effective_action {
                crate::Action::QueryObservation { .. } => Some(if existing {
                    crate::RejectReason::RuleDenied {
                        notes: vec!["observation requires rule module".to_owned()],
                    }
                } else {
                    crate::RejectReason::AgentNotFound {
                        agent_id: agent.clone(),
                    }
                }),
                crate::Action::BodyAction { .. } => Some(if existing {
                    crate::RejectReason::RuleDenied {
                        notes: vec!["body action requires body module".to_owned()],
                    }
                } else {
                    crate::RejectReason::AgentNotFound {
                        agent_id: agent.clone(),
                    }
                }),
                crate::Action::EmitBodyAttributes { .. } if !existing => {
                    Some(crate::RejectReason::AgentNotFound {
                        agent_id: agent.clone(),
                    })
                }
                _ => None,
            };
            if let Some(reason) = rejection {
                let audit = step.append_selected_rejection_owned(
                    materials,
                    live,
                    selected,
                    reason,
                    max_event_bytes,
                    audit_work,
                    &mut persist,
                )?;
                return Ok(CompletedBodyActionV1 {
                    owner: CompletedBodyActionOwnerV1::Rejection(audit),
                });
            }
            let crate::Action::EmitBodyAttributes { view, reason, .. } = effective_action else {
                return Err(BodyMutationError::Invalid("selected body action branch"));
            };
            work.spend(AGENT_BODY_VALUE_BYTES)?;
            let body_key = AgentBodyKeyV1::new(agent).map_err(BodyMutationError::Invalid)?;
            let current = AgentBodyValueV1::from_canonical_bytes(&read_fixed::<
                AGENT_BODY_VALUE_BYTES,
                _,
                _,
                _,
                _,
            >(
                step.transaction,
                StateNamespaceV1::Business,
                body_key.as_bytes(),
                step.values,
                step.mutation,
            )?)
            .map_err(BodyMutationError::Invalid)?;
            work.spend(guard_reason_bytes)?;
            let domain = match super::body::validate_body_kernel_view(&current.body_view, view) {
                Ok(()) => crate::DomainEvent::BodyAttributesUpdated {
                    agent_id: agent.clone(),
                    view: view.clone(),
                    reason: reason.clone(),
                },
                Err(reason) => crate::DomainEvent::BodyAttributesRejected {
                    agent_id: agent.clone(),
                    reason,
                },
            };
            let sequence = step.read_allocator(RuntimeAllocatorV1::Event)?;
            let (id, _, _) = super::World::preview_next_event_id(sequence.next, sequence.era);
            let event = crate::WorldEvent {
                id,
                time: step.time(),
                caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
                body: crate::WorldEventBody::Domain(domain),
            };
            step.dispatch_agent_event(&event, materials, live, max_event_bytes, &mut persist)?;
            let event = step.transaction.contribute(|_| {
                oasis7_world_state::incremental_world_event_owned::retain_world_event_owned_v1(
                    event,
                    step.mutation.memory,
                    MemoryClass::Ordinary,
                    audit_work,
                    &mut step.mutation.cancelled,
                )
                .map_err(BodyMutationError::Value)
            })?;
            Ok(CompletedBodyActionV1 {
                owner: CompletedBodyActionOwnerV1::Domain {
                    event,
                    identity: step.transaction.guard(),
                },
            })
        })();
    let result = result.and_then(|output| {
        if (step.mutation.cancelled)() || (read.cancelled)() || (parent_read.cancelled)() {
            Err(ValueTreeError::Cancelled.into())
        } else {
            Ok(output)
        }
    });
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
