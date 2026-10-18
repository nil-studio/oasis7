use super::*;

impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn validate_provider_cognition_lease_for_request(
        &self,
        world: &RuntimeWorld,
        agent_id: &str,
        request: &crate::simulator::ContinuousAgentRequestContextV1,
        lease: &crate::runtime::CognitionLeaseV1,
        operation: &str,
    ) -> Result<(), String> {
        if self.provider_service_required {
            lineage_generation_recovery::validate_provider_lease_identity(
                agent_id, request, lease,
            )?;
            let id = format!("{}:reserve", request.provider_invocation_key());
            let pending = self
                .provider_scheduler_pending
                .get(&id)
                .ok_or("canonical lease reservation proof missing")?;
            let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
                &pending.payload
            else {
                return Err("canonical lease reservation operation mismatch".into());
            };
            let crate::world_service::wire::SchedulerOperationV1::ReserveLease(reserved) =
                &signed.request.operation
            else {
                return Err("canonical lease reservation operation mismatch".into());
            };
            if reserved.quote != lease.quote || reserved.idempotency_key != lease.idempotency_key {
                return Err("canonical lease reservation identity mismatch".into());
            }
            return Ok(());
        }
        lineage_generation_recovery::validate_provider_lease_binding(
            world, agent_id, request, lease, operation,
        )
    }
}

/// Retain the exact signed intent across ambiguous transport outcomes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::viewer::runtime_live) struct PendingProviderServiceIntent {
    pub(in crate::viewer::runtime_live) correlation:
        oasis7_client_api::world_service::RequestCorrelation,
    pub(in crate::viewer::runtime_live) payload: crate::world_service::wire::WorldServicePayloadV1,
    pub(in crate::viewer::runtime_live) cognition: RuntimeProviderActionContext,
    pub(in crate::viewer::runtime_live) action: SimulatorAction,
    #[serde(default)]
    pub(in crate::viewer::runtime_live) feedback_ack:
        Option<crate::viewer::runtime_live::control_plane::llm_sidecar::service_feedback_ack::PendingFeedbackAck>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::viewer::runtime_live) struct PendingProviderSchedulerIntent {
    #[serde(default)]
    pub(in crate::viewer::runtime_live) resume_context:
        Option<cognition_context::ProviderContextState>,
    #[serde(default)]
    pub(in crate::viewer::runtime_live) resume_current_context:
        Option<crate::simulator::ContinuationCurrentContextV1>,
    pub(in crate::viewer::runtime_live) correlation:
        oasis7_client_api::world_service::RequestCorrelation,
    pub(in crate::viewer::runtime_live) payload: crate::world_service::wire::WorldServicePayloadV1,
}

pub(super) fn decode_provider_lineage_checkpoint(
    bytes: &[u8],
) -> Result<(PersistedProviderLineageV1, bool), String> {
    let mut value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("provider lineage checkpoint decode failed: {error}"))?;
    let schema_version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "provider lineage checkpoint decode failed: missing schema_version".to_string()
        })?;
    let migrated = match u16::try_from(schema_version).unwrap_or(u16::MAX) {
        PROVIDER_LINEAGE_SCHEMA_VERSION => false,
        LEGACY_PROVIDER_LINEAGE_SCHEMA_VERSION => {
            lineage_recovery::migrate_legacy_budget_contracts(&mut value)?;
            value["schema_version"] = json!(PROVIDER_LINEAGE_SCHEMA_VERSION);
            true
        }
        other => {
            return Err(format!(
                "unsupported provider lineage checkpoint schema {other}"
            ));
        }
    };
    let checkpoint: PersistedProviderLineageV1 = serde_json::from_value(value)
        .map_err(|error| format!("provider lineage checkpoint decode failed: {error}"))?;
    for (agent_id, pending) in &checkpoint.provider_service_pending {
        let derived = crate::world_service::derive_correlation(
            pending.correlation.key.world.clone(),
            &pending.payload,
        )?;
        if derived != pending.correlation {
            return Err("pending canonical Agent intent correlation mismatch".into());
        }
        let crate::world_service::wire::WorldServicePayloadV1::Cognition(signed) = &pending.payload
        else {
            return Err("pending canonical Agent intent is not cognition".into());
        };
        if let Some(ack) = &pending.feedback_ack {
            ack.validate_memory(pending, &checkpoint.provider_memory_store)?;
        }
        let request = &pending.cognition.request.request_context;
        let response = &pending.cognition.response;
        let identity = response.response_artifact_identity();
        response
            .validate_response_artifact_identity(&identity)
            .map_err(|error| format!("pending cognition response artifact invalid: {error}"))?;
        if signed.request.response_artifact.artifact_digest != identity.artifact_digest.to_string()
            || signed.request.response_artifact.response_digest
                != response.response_digest.to_string()
            || pending.cognition.memory_write_intents
                != response.base_decision_response.memory_write_intents
        {
            return Err("pending cognition response or memory artifact mismatch".into());
        }
        if agent_id != &request.agent_subject
            || signed.request.request.agent_id != request.agent_subject
            || signed.request.request.request_digest != request.request_digest.to_string()
        {
            return Err("pending canonical Agent intent checkpoint identity mismatch".into());
        }
    }
    for (request_id, pending) in &checkpoint.provider_scheduler_pending {
        let derived = crate::world_service::derive_correlation(
            pending.correlation.key.world.clone(),
            &pending.payload,
        )?;
        let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) = &pending.payload
        else {
            return Err("pending canonical scheduler intent has wrong operation".into());
        };
        if derived != pending.correlation || request_id != &signed.request.request_id {
            return Err("pending canonical scheduler checkpoint identity mismatch".into());
        }
        if let crate::world_service::wire::SchedulerOperationV1::ResumeWake {
            resume,
            current_context,
            ..
        } = &signed.request.operation
        {
            let prepared = pending
                .resume_context
                .as_ref()
                .ok_or("pending canonical ResumeWake complete context missing; fenced")?;
            let current = pending
                .resume_current_context
                .as_ref()
                .ok_or("pending canonical ResumeWake current context missing; fenced")?;
            let request = &prepared.request_context;
            request
                .validate_production_lane()
                .map_err(|error| format!("pending ResumeWake request invalid: {error}"))?;
            if request.agent_subject != signed.request.agent_id
                || request.agent_session_id != resume.agent_session_id
                || request.agent_turn_id != resume.agent_turn_id
                || request.decision_request_id != resume.decision_request_id
                || request.request_digest.to_string() != resume.request_digest
                || async_support::runtime_provider_context_digest(request) != resume.context_digest
                || current.authority.baseline_observation_digest
                    != current_context.baseline_observation_digest
                || current.authority.goal_digest != current_context.goal_digest
                || current.authority.policy_digest != current_context.policy_digest
                || current.authority.precondition_digest != current_context.precondition_digest
            {
                return Err("pending canonical ResumeWake prepared identity mismatch".into());
            }
        }
    }
    Ok((checkpoint, migrated))
}
