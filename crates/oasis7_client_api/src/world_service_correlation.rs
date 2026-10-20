//! Pure signed intent correlation shared by execution and native clients.
use crate::world_service::private_goal::verify_owner_signature_strict;
use crate::world_service::*;
use crate::world_service_wire::*;
use serde::Serialize;

/// Browser proof authority stays in its adapter; native task clients have no
/// browser payloads. A callback must verify the legacy request before returning
/// its subject, domain, nonce scope and request identity.
pub type LegacyIdentity = (String, &'static str, &'static str, String);

pub fn derive_correlation_with_legacy<
    C: Serialize,
    P: Serialize,
    L: Serialize,
    CP: Serialize,
    ST: Serialize,
    R: Serialize,
    D: Serialize,
    X: Serialize,
>(
    world: WorldIdentity,
    payload: &WorldServicePayloadProfileV1<C, P, L, CP, ST, R, D, X>,
    legacy: impl FnOnce(
        &WorldServicePayloadProfileV1<C, P, L, CP, ST, R, D, X>,
    ) -> Result<LegacyIdentity, String>,
) -> Result<RequestCorrelation, String> {
    if let WorldServicePayloadV1::SafeAlternative(signed) = payload {
        signed.request.validate()?;
        verify_owner_signature_strict(
            signed.request.signing_domain(),
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
        )?;
        if signed.request.world_id != world.world_id
            || signed.request.genesis_digest != world.genesis_digest
        {
            return Err("safe alternative world identity mismatch".into());
        }
        let correlation = RequestCorrelation {
            key: RequestKey {
                world,
                verified_subject: signed.subject_public_key.clone(),
                operation_domain: signed.request.signing_domain().into(),
                nonce_scope: format!("agent:{}", signed.request.agent_id),
                request_id_or_nonce: signed.request.client_operation_id.clone(),
            },
            payload_digest: request_digest("intent", payload)?,
        };
        correlation.validate().map_err(|e| e.to_string())?;
        return Ok(correlation);
    }
    if let WorldServicePayloadV1::GameExecutorControl(signed) = payload {
        signed.request.validate()?;
        if let oasis7_agent_api::game::GameExecutorOperationV1::Bind {
            executor_public_key,
            ..
        } = &signed.request.operation
        {
            strong_key(executor_public_key)?;
        }
        if signed.subject_public_key != signed.request.fence.owner_public_key {
            return Err("executor control requires owner signature".into());
        }
        verify_owner_signature_strict(
            oasis7_agent_api::game::GAME_EXECUTOR_CONTROL_DOMAIN,
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
        )?;
        let f = &signed.request.fence;
        if f.world_id != world.world_id || f.genesis_digest != world.genesis_digest {
            return Err("executor world identity mismatch".into());
        }
        let correlation = RequestCorrelation {
            key: RequestKey {
                world,
                verified_subject: signed.subject_public_key.clone(),
                operation_domain: oasis7_agent_api::game::GAME_EXECUTOR_CONTROL_DOMAIN.into(),
                nonce_scope: format!("task:{}", f.task_id),
                request_id_or_nonce: signed.request.client_operation_id.clone(),
            },
            payload_digest: request_digest("intent", payload)?,
        };
        correlation.validate().map_err(|e| e.to_string())?;
        return Ok(correlation);
    }
    if let WorldServicePayloadV1::GameGoalPublish(signed) = payload {
        signed.request.validate()?;
        if signed.subject_public_key != signed.request.owner_public_key {
            return Err("goal publish signer differs from owner".into());
        }
        verify_owner_signature_strict(
            oasis7_agent_api::game::GAME_GOAL_PUBLISH_DOMAIN,
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
        )?;
        if signed.request.world_id != world.world_id
            || signed.request.genesis_digest != world.genesis_digest
        {
            return Err("goal publish world identity mismatch".into());
        }
        let correlation = RequestCorrelation {
            key: RequestKey {
                world,
                verified_subject: signed.subject_public_key.clone(),
                operation_domain: oasis7_agent_api::game::GAME_GOAL_PUBLISH_DOMAIN.into(),
                nonce_scope: format!("task:{}", signed.request.task_id),
                request_id_or_nonce: signed.request.client_operation_id.clone(),
            },
            payload_digest: request_digest("intent", payload)?,
        };
        correlation.validate().map_err(|e| e.to_string())?;
        return Ok(correlation);
    }
    if let WorldServicePayloadV1::GameTaskCreate(signed) = payload {
        verify_read_request(oasis7_agent_api::game::GAME_TASK_CREATE_DOMAIN, signed)?;
        signed.request.validate()?;
        if signed.request.world_id != world.world_id
            || signed.request.genesis_digest != world.genesis_digest
        {
            return Err("Game task world identity mismatch".into());
        }
        let correlation = RequestCorrelation {
            key: RequestKey {
                world,
                verified_subject: signed.subject_public_key.clone(),
                operation_domain: oasis7_agent_api::game::GAME_TASK_CREATE_DOMAIN.into(),
                nonce_scope: format!("agent:{}", signed.request.agent_id),
                request_id_or_nonce: signed.request.client_operation_id.clone(),
            },
            payload_digest: request_digest("intent", payload)?,
        };
        correlation.validate().map_err(|e| e.to_string())?;
        return Ok(correlation);
    }
    let (subject, domain, scope, id) = match payload {
        WorldServicePayloadV1::SafeAlternative(_)
        | WorldServicePayloadV1::GameTaskCreate(_)
        | WorldServicePayloadV1::GameGoalPublish(_)
        | WorldServicePayloadV1::GameExecutorControl(_) => {
            unreachable!("handled above")
        }
        WorldServicePayloadV1::GameplayJson(_) | WorldServicePayloadV1::AgentChat(_) => {
            legacy(payload)?
        }
        WorldServicePayloadV1::Cognition(signed) => {
            verify_read_request("cognition", signed)?;
            if let Some(fence) = &signed.request.task_fence {
                if signed.request.causal_proposal.is_some() {
                    return Err("task cognition does not publish private reasoning".into());
                }
                fence.validate()?;
                verify_owner_signature_strict(
                    "cognition",
                    &signed.request,
                    &signed.subject_public_key,
                    &signed.signature_hex,
                )?;
            }
            let request = &signed.request.request;
            request.validate().map_err(|e| e.to_string())?;
            signed
                .request
                .response_artifact
                .validate_for_request(request)
                .map_err(|e| e.to_string())?;
            (
                signed.subject_public_key.clone(),
                "cognition",
                "agent_request",
                request.request_digest.clone(),
            )
        }
        WorldServicePayloadV1::FeedbackAck(signed) => {
            verify_read_request("feedback_ack", signed)?;
            signed.request.validate()?;
            (
                signed.subject_public_key.clone(),
                "feedback_ack",
                "agent_feedback_ack",
                signed.request.feedback_id.clone(),
            )
        }
        WorldServicePayloadV1::Delegation(signed) => {
            verify_read_request("delegation", signed)?;
            (
                signed.subject_public_key.clone(),
                "delegation",
                "owner_delegation",
                signed.request.nonce.to_string(),
            )
        }
        WorldServicePayloadV1::Scheduler(signed) => {
            verify_read_request("scheduler", signed)?;
            if let SchedulerOperationV1::TaskProviderPrefix { context }
            | SchedulerOperationV1::TaskProviderFailure { context, .. } =
                &signed.request.operation
            {
                context.validate()?;
                if matches!(
                    &signed.request.operation,
                    SchedulerOperationV1::TaskProviderPrefix { .. }
                ) {
                    context.validate_capture_deadline(
                        signed.request.captured_base_binding.base_tick,
                    )?;
                }

                if let SchedulerOperationV1::TaskProviderFailure { reason, .. } =
                    &signed.request.operation
                    && !matches!(
                        reason.as_str(),
                        "provider_failed"
                            | "transport_exhausted"
                            | "context_expired"
                            | "executor_aborted"
                    )
                {
                    return Err("task failure requires bounded public code".into());
                }
                verify_owner_signature_strict(
                    "scheduler",
                    &signed.request,
                    &signed.subject_public_key,
                    &signed.signature_hex,
                )?;
            }
            if signed.request.request_id.trim().is_empty() {
                return Err("missing scheduler request identity".into());
            }
            (
                signed.subject_public_key.clone(),
                "scheduler",
                "agent_scheduler",
                signed.request.request_id.clone(),
            )
        }
    };
    let correlation = RequestCorrelation {
        key: RequestKey {
            world,
            verified_subject: subject,
            operation_domain: domain.into(),
            nonce_scope: scope.into(),
            request_id_or_nonce: id,
        },
        payload_digest: request_digest("intent", payload)?,
    };
    correlation.validate().map_err(|e| e.to_string())?;
    Ok(correlation)
}

fn strong_key(key: &str) -> Result<(), String> {
    let bytes: [u8; 32] = hex::decode(key)
        .map_err(|_| "invalid executor key")?
        .try_into()
        .map_err(|_| "invalid executor key")?;
    let key =
        ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid executor key")?;
    if key.is_weak() {
        return Err("weak executor key".into());
    }
    Ok(())
}

/// Native task profile: unsupported legacy browser variants are never accepted.
pub fn derive_correlation(
    world: WorldIdentity,
    payload: &WorldServicePayloadV1,
) -> Result<RequestCorrelation, String> {
    derive_correlation_with_legacy(world, payload, |_| {
        Err("unsupported native browser payload".into())
    })
}
