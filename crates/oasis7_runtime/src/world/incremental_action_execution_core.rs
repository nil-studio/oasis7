//! Current-backed Register/Move business reducer and authenticated callers.
//! The bootstrap caller additionally requires empty module/delegation policy.
use super::{
    incremental_action_execution::take_pending_action, incremental_body::BodyMutationError,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionFence,
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1,
        AgentSubtreeStructureQualificationV1, AgentSubtreeValueV1,
        read_agent_subtree_scalar_overlay_v1,
    },
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::JsonReadWorkV1,
    incremental_pending_action_family::PendingActionFamilyWorkV1,
    incremental_pending_action_overlay::PendingActionCurrentV1,
    incremental_runtime_artifact_config::ArtifactConfigFamilyQualificationV1,
    incremental_runtime_module_authorization::ModuleAuthorizationFamilyQualificationV1,
    incremental_runtime_revocation::RevocationReadWorkV1 as TypedRecordReadWorkV1,
    incremental_runtime_revocation::RuntimeRevocationQualificationV1,
};

/// Consume one genuine queued action. This cannot produce an activation permit.
#[allow(clippy::too_many_arguments)]
pub(super) fn consume_bootstrap_action<M, P, V, W, E, C, D, O, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut super::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    current: &mut ReservedValue<PendingActionCurrentV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    pending_work: &mut PendingActionFamilyWorkV1,
    originals: &mut O,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    authorization: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    registry: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    revocation: &ReservedValue<RuntimeRevocationQualificationV1>,
    delegation_work: &mut JsonReadWorkV1,
    policy_scan: &mut StateScanBudget,
    policy_work: &mut TypedRecordReadWorkV1,
    remaining_registry_entries: &mut usize,
    max_event_bytes: usize,
    live: &ExecutionFence,
    mut persist: F,
) -> Result<bool, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
    O: OriginalObjectStreamReader,
    O::Error: std::error::Error + Send + Sync + 'static,
    F: FnMut(
        &mut dyn super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
{
    let selected = take_pending_action(
        current,
        step.transaction,
        read,
        pending_work,
        originals,
        live,
    )?;
    let Some(selected) = selected else {
        return Ok(false);
    };
    let envelope = selected.envelope();
    // An unsupported proof is a host failure, never downgraded into admission.
    step.transaction.contribute(|_| {
        if envelope.governance_authorization.is_some() {
            return Err(BodyMutationError::Invalid(
                "bootstrap governance proof unsupported",
            ));
        }
        if !matches!(
            envelope.action,
            crate::Action::RegisterAgent { .. } | crate::Action::MoveAgent { .. }
        ) {
            return Err(BodyMutationError::Invalid(
                "bootstrap action kind unsupported",
            ));
        }
        Ok(())
    })?;
    // Account temporary agent-id clones, the event body and bounded rejection
    // formatting until the original serializer has consumed them. The decoded
    // selected action and recovered policy retain their own separate charges.
    let _phase = step.transaction.contribute(|_| {
        let bytes = match &envelope.action {
            crate::Action::RegisterAgent { agent_id, .. }
            | crate::Action::MoveAgent { agent_id, .. } => agent_id.len(),
            _ => unreachable!(),
        }
        .checked_mul(8)
        .and_then(|n| n.checked_add(16 << 10))
        .ok_or(BodyMutationError::Invalid("bootstrap action scratch bound"))?;
        step.mutation
            .memory
            .reserve(bytes, oasis7_proto::memory_budget::MemoryClass::Ordinary)
            .map_err(|error| BodyMutationError::Value(ValueTreeError::Memory(error)))
    })?;
    {
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: step.transaction,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
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
        let applicability = (|| {
            super::incremental_action_execution_applicability::require_empty_current_module_policy(
                authorization,
                &mut overlay,
            )?;
            oasis7_world_state::incremental_runtime_artifact_config::require_empty_artifact_registry_overlay_v1(registry,&mut overlay,remaining_registry_entries)?;
            let guard = overlay.transaction.guard();
            oasis7_world_state::incremental_runtime_revocation::require_current_safe_alternatives_empty_v1(revocation,&mut overlay,&guard,live)?;
            let guard = overlay.transaction.guard();
            oasis7_world_state::incremental_cognition_json_overlay::require_current_delegation_grants_empty_v1(&mut overlay,delegation_work,&guard,live)?;
            Ok(())
        })();
        applicability.map_err(|error| {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                overlay.transaction.contribute(|_| Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted));
            match error {
                oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(error) => BodyMutationError::Execution(error),
                oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(error) => BodyMutationError::Value(error),
                oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(error) => BodyMutationError::Original(Box::new(error)),
            }
        })?;
    }
    execute_core_action_inner_v1(
        step,
        materials,
        &selected,
        &envelope.action,
        agents,
        read,
        policy_scan,
        policy_work,
        max_event_bytes,
        live,
        &mut persist,
    )?;
    Ok(true)
}

/// Shared concrete Register/Move reducer. Both callers use Current overlay reads;
/// only the bootstrap caller requires empty module policy.
#[allow(clippy::too_many_arguments)]
fn execute_core_action_inner_v1<M, P, V, W, E, C, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut super::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    selected: &super::incremental_action_execution::SelectedPendingActionV1,
    action: &crate::Action,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    policy_scan: &mut StateScanBudget,
    policy_work: &mut TypedRecordReadWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    mut persist: F,
) -> Result<crate::WorldEvent, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
    F: FnMut(
        &mut dyn super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
{
    let body = prepare_core_body_v1(
        step,
        selected,
        action,
        agents,
        read,
        policy_scan,
        policy_work,
        live,
    )?;
    publish_core_body_v1(
        step,
        materials,
        selected,
        body,
        agents,
        read,
        max_event_bytes,
        live,
        &mut persist,
    )
}

/// Read the actual Core outcome without publishing an event or applying it.
#[allow(clippy::too_many_arguments)]
fn prepare_core_body_v1<M, P, V, W, E, C, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    selected: &super::incremental_action_execution::SelectedPendingActionV1,
    action: &crate::Action,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    policy_scan: &mut StateScanBudget,
    policy_work: &mut TypedRecordReadWorkV1,
    live: &ExecutionFence,
) -> Result<crate::DomainEvent, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
{
    let position = {
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: step.transaction,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
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
        let applicability = (|| {
            let agent = match action {
                crate::Action::RegisterAgent { agent_id, .. }
                | crate::Action::MoveAgent { agent_id, .. } => agent_id,
                _ => unreachable!(),
            };
            let mut coordinates = [None; 3];
            for (index, field) in [
                AgentFieldV1::PositionX,
                AgentFieldV1::PositionY,
                AgentFieldV1::PositionZ,
            ]
            .into_iter()
            .enumerate()
            {
                let guard = overlay.transaction.guard();
                coordinates[index]=match read_agent_subtree_scalar_overlay_v1(
                    AgentSubtreeQualifiedRequestV1{cell:AgentSubtreeScalarRequestV1{agent,field,ordinal:0},qualification:agents},
                    &mut overlay,read.record_bytes,&mut std::io::sink(),&guard,live)? {
                    Some(AgentSubtreeValueV1::I64(v))=>Some(v),None=>None,
                    _=>return Err(oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid("bootstrap agent position type"))),
                };
            }
            if coordinates.iter().any(Option::is_some) && coordinates.iter().any(Option::is_none) {
                return Err(
                    oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(
                        ValueTreeError::Invalid("bootstrap partial agent position"),
                    ),
                );
            }
            Ok(coordinates)
        })();
        match applicability {
            Ok(v) => v,
            Err(error) => {
                // An inner reader may already have poisoned the transaction.
                // Preserve its concrete cause while also closing local errors.
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    overlay.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                return Err(match error {
                    oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(error) => BodyMutationError::Execution(error),
                    oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(error) => BodyMutationError::Value(error),
                    oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(error) => BodyMutationError::Original(Box::new(error)),
                });
            }
        }
    };
    let body = match action {
        crate::Action::RegisterAgent { agent_id, pos } => {
            if agent_id.starts_with("industrial:") {
                return Ok(crate::DomainEvent::ActionRejected {
                    action_id: selected.envelope().id,
                    reason: crate::RejectReason::RuleDenied {
                        notes: vec!["industrial_identity_requires_delivery".into()],
                    },
                });
            }
            if position[0].is_some() {
                return Ok(crate::DomainEvent::ActionRejected {
                    action_id: selected.envelope().id,
                    reason: crate::RejectReason::AgentAlreadyExists {
                        agent_id: agent_id.clone(),
                    },
                });
            }
            crate::DomainEvent::AgentRegistered {
                registration_contract_version: 1,
                agent_id: agent_id.clone(),
                pos: *pos,
            }
        }
        crate::Action::MoveAgent { agent_id, to } => {
            let [Some(x), Some(y), Some(z)] = position else {
                return Ok(crate::DomainEvent::ActionRejected {
                    action_id: selected.envelope().id,
                    reason: crate::RejectReason::AgentNotFound {
                        agent_id: agent_id.clone(),
                    },
                });
            };
            let policy =
                oasis7_world_state::incremental_world_native::recover_gameplay_policy_overlay_v1(
                    step.transaction,
                    live,
                    read,
                    policy_scan,
                    policy_work,
                )
                .map_err(|error| match error {
                    oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(error) => BodyMutationError::Execution(error),
                    oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(error) => BodyMutationError::Value(error),
                    oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(error) => BodyMutationError::Original(Box::new(error)),
                })?;
            let target = format!("{}:{}:{}", to.x_cm, to.y_cm, to.z_cm);
            if policy
                .value()
                .value()
                .forbidden_location_ids
                .iter()
                .any(|v| v == &target)
            {
                return Ok(crate::DomainEvent::ActionRejected {
                    action_id: selected.envelope().id,
                    reason: crate::RejectReason::RuleDenied {
                        notes: vec![format!(
                            "move denied by gameplay forbidden_location_ids: {target}"
                        )],
                    },
                });
            }
            let mut from = *to;
            from.x_cm = x;
            from.y_cm = y;
            from.z_cm = z;
            crate::DomainEvent::AgentMoved {
                agent_id: agent_id.clone(),
                from,
                to: *to,
            }
        }
        _ => unreachable!(),
    };
    Ok(body)
}

/// Apply and persist the previously chosen Core body in the same transaction.
#[allow(clippy::too_many_arguments)]
fn publish_core_body_v1<M, P, V, W, E, C, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut super::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    selected: &super::incremental_action_execution::SelectedPendingActionV1,
    body: crate::DomainEvent,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    max_event_bytes: usize,
    live: &ExecutionFence,
    mut persist: F,
) -> Result<crate::WorldEvent, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
    F: FnMut(
        &mut dyn super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
{
    if let crate::DomainEvent::ActionRejected { action_id, reason } = body {
        if action_id != selected.envelope().id {
            return Err(BodyMutationError::Invalid(
                "Core plan rejection action binding",
            ));
        }
        return step.append_selected_rejection_event(
            materials,
            live,
            selected,
            reason,
            max_event_bytes,
            persist,
        );
    }
    let sequence = step.read_allocator(super::incremental_allocators::RuntimeAllocatorV1::Event)?;
    // The shared event reducer owns allocation and its exact before-state.
    let (id, _, _) = super::World::preview_next_event_id(sequence.next, sequence.era);
    let event = crate::WorldEvent {
        id,
        time: step.time(),
        caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
        body: crate::WorldEventBody::Domain(body),
    };
    if matches!(
        &event.body,
        crate::WorldEventBody::Domain(crate::DomainEvent::AgentRegistered { .. })
    ) {
        step.dispatch_agent_registration(
            agents,
            &event,
            materials,
            read.key_work,
            read.values.io,
            read.semantic_bytes,
            read.record_bytes,
            &mut read.take_state_error,
            live,
            max_event_bytes,
            &mut persist,
        )?;
    } else {
        step.dispatch_agent_event(&event, materials, live, max_event_bytes, &mut persist)?;
    }
    Ok(event)
}

/// A Core result may enter PostAction only after the reducer, original stream,
/// event allocator and counted fact all completed in this transaction.
pub(super) struct CompletedCoreActionV1 {
    event_before_post_action: bool,
    event: ReservedValue<crate::WorldEvent>,
    identity: oasis7_formal_protocol::incremental_execution::PreparedGuard,
}
impl CompletedCoreActionV1 {
    pub(super) fn event_before_post_action(&self) -> bool {
        self.event_before_post_action
    }
    pub(super) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    ) -> Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> {
        tx.validate_transaction_identity(&self.identity)
    }
}

#[path = "incremental_action_execution_core_plan.rs"]
mod plan;
pub(super) use plan::{
    CoreSettlementErrorV1, apply_settled_core_action_v1, prepare_scoped_selected_core_action_v1,
    prepare_selected_core_action_v1, settle_prepared_core_action_v1,
};
