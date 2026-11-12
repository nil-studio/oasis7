//! Pure verification for the fixed executor gateway's inner world response.
//! The caller remains responsible for current authentication and complete fence
//! validation. This module performs no network, clock or storage operations.
use super::{SignedServiceResponse, private_goal, request_digest, verify_service_response};
use serde::{
    Serialize,
    de::{DeserializeOwned, IntoDeserializer},
};

/// Verify the original typed world domain, never the transport gateway domain.
/// Unknown fields fail closed; the response is bounded before deserialization.
pub fn verify_world_reply<Q: Serialize, R: Serialize + DeserializeOwned>(
    path: &str,
    request: &Q,
    bytes: &[u8],
    bound: usize,
    trusted_key: &str,
) -> Result<SignedServiceResponse<R>, String> {
    if bound == 0 || bound > 2 * 1024 * 1024 {
        return Err("invalid executor response bound".into());
    }
    let value: serde_json::Value = oasis7_agent_api::game::parse_bounded_json(bytes, bound)
        .map_err(|_| "executor gateway signed response malformed")?;
    let mut ignored = false;
    let signed: SignedServiceResponse<R> =
        serde_ignored::deserialize(value.into_deserializer(), |_| ignored = true)
            .map_err(|_| "executor gateway signed response malformed")?;
    if ignored {
        return Err("unsupported executor gateway response field".into());
    }
    verify_service_response(path, &request_digest(path, request)?, &signed, trusted_key)?;
    private_goal::verify_owner_signature_strict(
        path,
        &(&signed.request_digest, &signed.payload),
        trusted_key,
        &signed.signature_hex,
    )?;
    Ok(signed)
}

use super::task_context::*;
use super::{
    LookupIntentRequest, SignedReadRequest, SubmitIntentRequest, WorldIdentity, sign_read_request,
};
use crate::world_service_wire::{AuthenticatedLookup, WorldServicePayloadV1};
use oasis7_agent_api::game::{GameTaskFenceV1, TaskContextAuthorityModeV1};

fn signer(fence: &GameTaskFenceV1, public: &str) -> Result<(), String> {
    fence.validate()?;
    if fence.task_status != oasis7_agent_api::game::GameTaskStatusV1::Active {
        return Err("executor task is not active".into());
    }
    let binding = fence
        .executor_binding
        .as_ref()
        .ok_or("executor binding missing")?;
    if binding.executor_public_key != public || public == fence.owner_public_key {
        return Err("executor signer differs from fence".into());
    }
    Ok(())
}
/// Caller supplies already persisted nonce; this function neither generates nor sends one.
pub fn prepare_request(
    fence: GameTaskFenceV1,
    operation: String,
    nonce: String,
    authority_mode: Option<TaskContextAuthorityModeV1>,
) -> TaskContextPrepareV1 {
    TaskContextPrepareV1 {
        schema_revision: TaskContextAuthorityModeV1::schema_revision(authority_mode),
        authority_mode,
        fence,
        client_operation_id: operation,
        client_nonce: nonce,
    }
}
pub fn prefix_request(
    prepared: &PreparedTaskContextV1,
) -> Result<crate::world_service_wire::TaskSchedulerIntentV1, String> {
    public_prefix(prepared)
}
pub fn signed_prepare(
    fence: GameTaskFenceV1,
    operation: String,
    nonce: String,
    authority_mode: Option<TaskContextAuthorityModeV1>,
    key: &str,
) -> Result<SignedReadRequest<TaskContextPrepareV1>, String> {
    let proof = sign_read_request(
        TASK_CONTEXT_PREPARE_PATH,
        prepare_request(fence, operation, nonce, authority_mode),
        key,
    )?;
    signer(&proof.request.fence, &proof.subject_public_key)?;
    verify_prepare(&proof)?;
    Ok(proof)
}
fn original(
    world: WorldIdentity,
    payload: WorldServicePayloadV1,
) -> Result<SubmitIntentRequest<WorldServicePayloadV1>, String> {
    let correlation = crate::world_service_correlation::derive_correlation(world, &payload)?;
    Ok(SubmitIntentRequest {
        contract_version: 1,
        correlation,
        deadline_unix_ms: None,
        signed_payload: payload,
    })
}
pub fn prefix(
    prepared: &PreparedTaskContextV1,
    key: &str,
) -> Result<SubmitIntentRequest<WorldServicePayloadV1>, String> {
    let signed = sign_read_request("scheduler", public_prefix(prepared)?, key)?;
    signer(&prepared.identity.fence, &signed.subject_public_key)?;
    original(
        WorldIdentity {
            world_id: prepared.identity.fence.world_id.clone(),
            genesis_digest: prepared.identity.fence.genesis_digest.clone(),
        },
        WorldServicePayloadV1::Scheduler(signed),
    )
}
pub fn context_get_request(
    prefix: &SubmitIntentRequest<WorldServicePayloadV1>,
    prepare_operation_id: &str,
) -> Result<TaskContextGetV1, String> {
    lookup(prefix)?;
    let WorldServicePayloadV1::Scheduler(signed) = &prefix.signed_payload else {
        return Err("original is not context prefix".into());
    };
    let crate::world_service_wire::SchedulerOperationV1::TaskProviderPrefix { context } =
        &signed.request.operation
    else {
        return Err("original is not context prefix".into());
    };
    Ok(TaskContextGetV1 {
        schema_revision: 1,
        context: context.clone(),
        prepare_operation_id: prepare_operation_id.to_owned(),
    })
}
pub fn context_get(
    prefix: &SubmitIntentRequest<WorldServicePayloadV1>,
    prepare_operation_id: &str,
    key: &str,
) -> Result<SignedReadRequest<TaskContextGetV1>, String> {
    let request = context_get_request(prefix, prepare_operation_id)?;
    let proof = sign_read_request(TASK_CONTEXT_READ_PATH, request, key)?;
    signer(&proof.request.context.fence, &proof.subject_public_key)?;
    verify_get(&proof)?;
    Ok(proof)
}
pub fn move_intent(
    captured: &CapturedTaskContextV1,
    target: [i64; 3],
    key: &str,
) -> Result<SubmitIntentRequest<WorldServicePayloadV1>, String> {
    let action = oasis7_agent_api::world_action::Action::MoveAgent {
        agent_id: captured.prepared.identity.fence.agent_id.clone(),
        to: oasis7_agent_api::world_action::GeoPos {
            x_cm: target[0],
            y_cm: target[1],
            z_cm: target[2],
        },
    };
    let payload = super::task_context::cognition_intent(captured, action)?;
    let signed = sign_read_request("cognition", payload, key)?;
    signer(
        &captured.prepared.identity.fence,
        &signed.subject_public_key,
    )?;
    original(
        WorldIdentity {
            world_id: captured.prepared.identity.fence.world_id.clone(),
            genesis_digest: captured.prepared.identity.fence.genesis_digest.clone(),
        },
        WorldServicePayloadV1::Cognition(signed),
    )
}
pub fn lookup(
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<AuthenticatedLookup, String> {
    if !matches!(
        &original.signed_payload,
        WorldServicePayloadV1::Cognition(_) | WorldServicePayloadV1::Scheduler(_)
    ) {
        return Err("executor lookup requires task cognition or scheduler original".into());
    }
    original.validate().map_err(|e| e.to_string())?;
    let expected = crate::world_service_correlation::derive_correlation(
        original.correlation.key.world.clone(),
        &original.signed_payload,
    )?;
    if expected != original.correlation {
        return Err("original correlation mismatch".into());
    }

    Ok(AuthenticatedLookup {
        request: LookupIntentRequest {
            contract_version: 1,
            key: original.correlation.key.clone(),
        },
        original: original.signed_payload.clone(),
    })
}

/// Check the action stored under an original Move before recovering it.
/// A valid signature alone cannot establish equality with the requested body.
pub fn validate_cached_move(
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    target: [i64; 3],
) -> Result<(), String> {
    lookup(original)?;
    let WorldServicePayloadV1::Cognition(signed) = &original.signed_payload else {
        return Err("original is not cognition".into());
    };
    let expected = oasis7_agent_api::world_action::Action::MoveAgent {
        agent_id: context.fence.agent_id.clone(),
        to: oasis7_agent_api::world_action::GeoPos {
            x_cm: target[0],
            y_cm: target[1],
            z_cm: target[2],
        },
    };
    let identity = &signed.request.request;
    if identity.agent_id != context.fence.agent_id
        || identity.agent_session_id != context.agent_session_id
        || identity.agent_turn_id != context.agent_turn_id
        || identity.decision_request_id != context.decision_request_id
        || identity.retry_seq != context.retry_seq
        || identity.transport_attempt != context.transport_attempt
        || identity.request_digest != context.request_commitment
        || identity.context_digest != context.context_commitment
    {
        return Err("cached Move differs from original context identity".into());
    }
    if signed.request.action != expected
        || signed.request.task_fence.as_ref() != Some(&context.fence)
    {
        return Err("cached Move differs from original request body".into());
    }
    Ok(())
}

fn original_fence(payload: &WorldServicePayloadV1) -> Result<&GameTaskFenceV1, String> {
    use crate::world_service_wire::SchedulerOperationV1;
    match payload {
        WorldServicePayloadV1::Cognition(p) if p.request.causal_proposal.is_none() => {
            let f = p.request.task_fence.as_ref().ok_or("task fence missing")?;
            signer(f, &p.subject_public_key)?;
            if f.executor_binding
                .as_ref()
                .is_none_or(|b| b.delegation_generation.0 != p.request.delegation_generation)
            {
                return Err("executor original delegation differs".into());
            }
            Ok(f)
        }
        WorldServicePayloadV1::Scheduler(p) => {
            match &p.request.operation {
                SchedulerOperationV1::TaskProviderPrefix { context }
                | SchedulerOperationV1::TaskProviderFailure { context, .. } => {
                    signer(&context.fence, &p.subject_public_key)?;
                    if context.fence.executor_binding.as_ref().is_none_or(|b| {
                        b.delegation_generation.0 != p.request.delegation_generation
                    }) {
                        return Err("scheduler original delegation differs".into());
                    }
                    Ok(&context.fence)
                }
                _ => Err("executor requires task scheduler original".into()),
            }
        }
        _ => Err("executor original domain denied".into()),
    }
}
/// Assemble a fresh access envelope for a fixed typed executor operation.
/// Native login/CSRF and current authority remain the authenticated transport's responsibility.
pub fn gateway(
    fence: GameTaskFenceV1,
    operation: super::entry_executor::EntryExecutorOperationV1,
    nonce: String,
    key: &str,
) -> Result<super::entry_executor::EntryExecutorControlRequestV1, String> {
    use super::entry_executor::*;
    let expected_world = WorldIdentity {
        world_id: fence.world_id.clone(),
        genesis_digest: fence.genesis_digest.clone(),
    };
    let operation_fence = match &operation {
        EntryExecutorOperationV1::PrepareContext(p) => {
            verify_prepare(p)?;
            signer(&p.request.fence, &p.subject_public_key)?;
            Some(&p.request.fence)
        }
        EntryExecutorOperationV1::ReadContext(p) => {
            verify_get(p)?;
            signer(&p.request.context.fence, &p.subject_public_key)?;
            Some(&p.request.context.fence)
        }
        EntryExecutorOperationV1::GoalSnapshot(p) => {
            verify_snapshot_request(p)?;
            signer(&p.request.fence, &p.subject_public_key)?;
            Some(&p.request.fence)
        }
        EntryExecutorOperationV1::RecipeQuote(p) => {
            super::task_recipe_quote::verify_recipe_quote_request(p)?;
            signer(
                &p.request.context_proof.request.context.fence,
                &p.subject_public_key,
            )?;
            Some(&p.request.context_proof.request.context.fence)
        }
        EntryExecutorOperationV1::FactoryQuote(p) => {
            super::task_factory_quote::verify_factory_quote_request(p)?;
            signer(
                &p.request.context_proof.request.context.fence,
                &p.subject_public_key,
            )?;
            Some(&p.request.context_proof.request.context.fence)
        }
        EntryExecutorOperationV1::Submit(p) => {
            lookup(p)?;
            Some(original_fence(&p.signed_payload)?)
        }
        EntryExecutorOperationV1::Lookup(p) => {
            p.request.validate().map_err(|e| e.to_string())?;
            let expected = crate::world_service_correlation::derive_correlation(
                expected_world.clone(),
                &p.original,
            )?;
            if p.request.key != expected.key {
                return Err("lookup original key differs".into());
            }
            let old = original_fence(&p.original)?;
            if old.world_id != fence.world_id
                || old.genesis_digest != fence.genesis_digest
                || old.agent_id != fence.agent_id
                || old.task_id != fence.task_id
                || old.owner_generation != fence.owner_generation
                || old.owner_binding != fence.owner_binding
                || old.owner_public_key != fence.owner_public_key
            {
                return Err("historical original belongs to another resource".into());
            }
            None
        }
        EntryExecutorOperationV1::ReadView(p) => {
            p.request.validate().map_err(|e| e.to_string())?;
            if p.request.world != expected_world
                || p.request.scope_id != format!("agent:{}", fence.agent_id)
                || p.request.observation_nonce.is_none()
                || p.request.formal_anchor.is_some()
            {
                return Err("executor read scope differs".into());
            }
            signer(&fence, &p.subject_public_key)?;
            private_goal::verify_owner_signature_strict(
                super::VIEW_JSON_PATH,
                &p.request,
                &p.subject_public_key,
                &p.signature_hex,
            )?;
            None
        }
        EntryExecutorOperationV1::Describe(p) => {
            if p.request.expected_world != expected_world {
                return Err("executor describe world differs".into());
            }
            signer(&fence, &p.subject_public_key)?;
            private_goal::verify_owner_signature_strict(
                super::DESCRIBE_PATH,
                &p.request,
                &p.subject_public_key,
                &p.signature_hex,
            )?;
            None
        }
        _ => return Err("unsupported executor operation".into()),
    };
    if operation_fence.is_some_and(|f| f != &fence) {
        return Err("executor operation fence differs".into());
    }
    let read = super::ReadWorldViewRequest {
        contract_version: 1,
        observation_nonce: Some(nonce),
        formal_anchor: None,
        world: expected_world,
        scope_id: format!("agent:{}", fence.agent_id),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    read.validate().map_err(|e| e.to_string())?;
    let read = sign_read_request(super::VIEW_JSON_PATH, read, key)?;
    signer(&fence, &read.subject_public_key)?;
    Ok(EntryExecutorControlRequestV1 {
        access: EntryExecutorAccessV1 { read, fence },
        operation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fence() -> GameTaskFenceV1 {
        serde_json::from_value(serde_json::json!({"world_id":"w","genesis_digest":"g","branch_id":"b","reorg_epoch":"0","agent_id":"a","owner_binding":"o","owner_generation":"1","owner_public_key":"ab".repeat(32),"task_id":"t","task_revision":"2","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"cd".repeat(32),"prepare_operation_id":"prepare","publish_operation_id":"publish","activation_committed_height":"2"},"executor_epoch":"1","executor_binding":{"executor_id":"e","executor_public_key":sign_read_request("key",(),&"17".repeat(32)).unwrap().subject_public_key,"delegation_generation":"1","executor_epoch":"1"}})).unwrap()
    }
    #[test]
    fn typed_prepare_and_access_use_the_bound_executor_and_preserve_nonce() {
        let key = "17".repeat(32);
        let f = fence();
        let proof =
            signed_prepare(f.clone(), "operation".into(), "aa".repeat(32), None, &key).unwrap();
        assert_eq!(proof.request.client_nonce, "aa".repeat(32));
        assert!(
            signed_prepare(
                f.clone(),
                "operation".into(),
                "aa".repeat(32),
                None,
                &"18".repeat(32)
            )
            .is_err()
        );
        let operation =
            super::super::entry_executor::EntryExecutorOperationV1::PrepareContext(proof.clone());
        let envelope = gateway(f.clone(), operation.clone(), "bb".repeat(32), &key).unwrap();
        assert_eq!(envelope.access.fence, f);
        assert_eq!(
            envelope.access.read.request.observation_nonce,
            Some("bb".repeat(32))
        );
        assert!(
            gateway(
                f.clone(),
                operation.clone(),
                "bb".repeat(32),
                &"18".repeat(32)
            )
            .is_err()
        );
        let mut changed = f;
        changed.task_revision.0 += 1;
        assert!(gateway(changed, operation, "bb".repeat(32), &key).is_err());
    }
    #[test]
    fn signed_reply_is_bound_to_original_domain_request_key_and_payload() {
        #[derive(Serialize, serde::Deserialize, Debug)]
        struct Payload {
            value: u64,
        }
        let seed = "11".repeat(32);
        let key = ed25519_dalek::SigningKey::from_bytes(&[0x11; 32]);
        let public = hex::encode(key.verifying_key().to_bytes());
        let domain = "/v1/world/view-json";
        let request = serde_json::json!({"nonce":"original"});
        let signed = super::super::sign_service_response(
            domain,
            request_digest(domain, &request).unwrap(),
            Payload { value: 7 },
            &seed,
        )
        .unwrap();
        let bytes = serde_json::to_vec(&signed).unwrap();
        assert_eq!(
            verify_world_reply::<_, Payload>(domain, &request, &bytes, 4096, &public)
                .unwrap()
                .payload
                .value,
            7
        );
        assert!(
            verify_world_reply::<_, Payload>(
                "/entry-executor/control",
                &request,
                &bytes,
                4096,
                &public
            )
            .is_err()
        );
        assert!(
            verify_world_reply::<_, Payload>(
                domain,
                &serde_json::json!({"nonce":"other"}),
                &bytes,
                4096,
                &public
            )
            .is_err()
        );
        let mut changed = serde_json::to_value(&signed).unwrap();
        changed["payload"]["value"] = 8.into();
        assert!(
            verify_world_reply::<_, Payload>(
                domain,
                &request,
                &serde_json::to_vec(&changed).unwrap(),
                4096,
                &public
            )
            .is_err()
        );
        changed = serde_json::to_value(&signed).unwrap();
        changed["payload"]["extra"] = true.into();
        assert!(
            verify_world_reply::<_, Payload>(
                domain,
                &request,
                &serde_json::to_vec(&changed).unwrap(),
                4096,
                &public
            )
            .is_err()
        );
    }
    #[test]
    fn malformed_oversized_and_unknown_fields_fail_closed() {
        for bytes in [
            b"not json".as_slice(),
            br#"{"request_digest":"x","payload":{},"signature_hex":"00","unexpected":true}"#
                .as_slice(),
        ] {
            assert!(
                verify_world_reply::<_, serde_json::Value>(
                    "/v1/world/view-json",
                    &(),
                    bytes,
                    1024,
                    "invalid"
                )
                .is_err()
            );
        }
        assert!(
            verify_world_reply::<_, serde_json::Value>(
                "/v1/world/view-json",
                &(),
                b"{}",
                1,
                "invalid"
            )
            .is_err()
        );
        assert!(
            verify_world_reply::<_, serde_json::Value>(
                "/v1/world/view-json",
                &(),
                b"{}",
                0,
                "invalid"
            )
            .is_err()
        );
    }
}
