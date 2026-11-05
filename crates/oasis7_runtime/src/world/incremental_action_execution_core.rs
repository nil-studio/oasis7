//! Authenticated bootstrap action consumer in the sole live transaction.
//! Nonempty rule/delegation/revocation configuration is unsupported here.
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
            super::incremental_action_execution_applicability::require_empty_current_module_policy(
                authorization,
                &mut overlay,
            )?;
            oasis7_world_state::incremental_runtime_artifact_config::require_empty_artifact_registry_overlay_v1(registry,&mut overlay,remaining_registry_entries)?;
            let guard = overlay.transaction.guard();
            oasis7_world_state::incremental_runtime_revocation::require_current_safe_alternatives_empty_v1(revocation,&mut overlay,&guard,live)?;
            let guard = overlay.transaction.guard();
            oasis7_world_state::incremental_cognition_json_overlay::require_current_delegation_grants_empty_v1(&mut overlay,delegation_work,&guard,live)?;
            let agent = match &envelope.action {
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
    let body = match &envelope.action {
        crate::Action::RegisterAgent { agent_id, pos } => {
            if agent_id.starts_with("industrial:") {
                step.append_selected_rejection(
                    &selected,
                    crate::RejectReason::RuleDenied {
                        notes: vec!["industrial_identity_requires_delivery".into()],
                    },
                    max_event_bytes,
                    &mut persist,
                )?;
                return Ok(true);
            }
            if position[0].is_some() {
                step.append_selected_rejection(
                    &selected,
                    crate::RejectReason::AgentAlreadyExists {
                        agent_id: agent_id.clone(),
                    },
                    max_event_bytes,
                    &mut persist,
                )?;
                return Ok(true);
            }
            crate::DomainEvent::AgentRegistered {
                registration_contract_version: 1,
                agent_id: agent_id.clone(),
                pos: *pos,
            }
        }
        crate::Action::MoveAgent { agent_id, to } => {
            let [Some(x), Some(y), Some(z)] = position else {
                step.append_selected_rejection(
                    &selected,
                    crate::RejectReason::AgentNotFound {
                        agent_id: agent_id.clone(),
                    },
                    max_event_bytes,
                    &mut persist,
                )?;
                return Ok(true);
            };
            let policy =
                oasis7_world_state::incremental_world_native::recover_gameplay_policy_overlay_v1(
                    step.transaction,
                    live,
                    read,
                    policy_scan,
                    policy_work,
                )
                .map_err(|_| BodyMutationError::Invalid("bootstrap current gameplay policy"))?;
            let target = format!("{}:{}:{}", to.x_cm, to.y_cm, to.z_cm);
            if policy
                .value()
                .value()
                .forbidden_location_ids
                .iter()
                .any(|v| v == &target)
            {
                step.append_selected_rejection(
                    &selected,
                    crate::RejectReason::RuleDenied {
                        notes: vec![format!(
                            "move denied by gameplay forbidden_location_ids: {target}"
                        )],
                    },
                    max_event_bytes,
                    &mut persist,
                )?;
                return Ok(true);
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
    let sequence = step.read_allocator(super::incremental_allocators::RuntimeAllocatorV1::Event)?;
    // The shared event reducer owns allocation and its exact before-state.
    let (id, _, _) = super::World::preview_next_event_id(sequence.next, sequence.era);
    let event = crate::WorldEvent {
        id,
        time: step.time(),
        caused_by: Some(crate::CausedBy::Action(envelope.id)),
        body: crate::WorldEventBody::Domain(body),
    };
    if matches!(envelope.action, crate::Action::RegisterAgent { .. }) {
        step.dispatch_agent_registration(
            agents,
            &event,
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
        step.dispatch_agent_event(&event, max_event_bytes, &mut persist)?;
    }
    Ok(true)
}
