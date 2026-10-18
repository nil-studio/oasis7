//! Canonical owner/delegate authorization; no provider-local key registry.
use super::{
    AgentSignerDelegationChangeV1, CognitionIntentV1, SignedReadRequest, verify_read_request,
};
use crate::World;

pub fn owner_public_key(world: &World, agent_id: &str) -> Result<String, String> {
    let identity = world
        .capability_revocation_state()
        .agent_identities
        .get(agent_id)
        .ok_or("Agent identity unavailable")?;
    let mut claims =
        world.state().starter_oc_claims.values().filter(|claim| {
            claim.agent_id == agent_id && claim.player_id == identity.owner_binding
        });
    let claim = claims.next().ok_or("canonical owner claim unavailable")?;
    if claims.next().is_some() {
        return Err("ambiguous canonical owner claim".into());
    }
    claim
        .public_key
        .clone()
        .ok_or("canonical owner key unavailable".into())
}

pub fn validate_delegation(
    world: &World,
    signed: &SignedReadRequest<AgentSignerDelegationChangeV1>,
) -> Result<(), String> {
    verify_read_request("delegation", signed)?;
    let change = &signed.request;
    if owner_public_key(world, &change.agent_id)? != signed.subject_public_key {
        return Err("delegation requires canonical owner signature".into());
    }
    let identity = &world.capability_revocation_state().agent_identities[&change.agent_id];
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    if change.owner_binding != identity.owner_binding
        || change.agent_identity_generation != identity.generation
        || change.world_id != binding.world_id
        || change.branch_id != binding.branch_id
        || change.generation == 0
        || change.nonce == 0
        || hex::decode(&change.delegate_public_key)
            .map_err(|e| e.to_string())?
            .len()
            != 32
    {
        return Err("delegation identity, history or generation mismatch".into());
    }
    if let Some(previous) = world
        .capability_revocation_state()
        .agent_signer_delegations
        .get(&change.agent_id)
    {
        if previous == change {
            return Ok(());
        }
        if change.generation <= previous.generation || change.nonce <= previous.nonce {
            return Err("delegation generation or nonce did not advance".into());
        }
    }
    Ok(())
}

pub fn validate_cognition(
    world: &World,
    signed: &SignedReadRequest<CognitionIntentV1>,
) -> Result<(), String> {
    validate_cognition_staged(world, signed, &[])
}
pub fn validate_cognition_staged(
    world: &World,
    signed: &SignedReadRequest<CognitionIntentV1>,
    staged: &[super::CanonicalIntentResultV1],
) -> Result<(), String> {
    verify_read_request("cognition", signed)?;
    let intent = &signed.request;
    if intent.task_fence.is_some() {
        super::private_goal_prepare::verify_owner_signature_strict(
            "cognition",
            intent,
            &signed.subject_public_key,
            &signed.signature_hex,
        )?;
    }
    if matches!(
        intent.action,
        crate::Action::WorldServiceIntent { .. }
    ) || oasis7_agent_api::world_action::main_token_action_auth_required(&intent.action)
    {
        return Err(
            "cognition cannot authorize nested service or main-token controller actions".into(),
        );
    }
    intent.request.validate().map_err(|e| e.to_string())?;
    intent
        .response_artifact
        .validate_for_request(&intent.request)
        .map_err(|e| e.to_string())?;
    if let Some(public) = &intent.causal_proposal {
        // These are signed explanation/evidence references, not Runtime claims
        // of truth. The observation reference is the authenticated request's.
        if public.evidence_refs.len() != 2
            || public.evidence_refs[0] != intent.request.observation_digest
            || !oasis7_agent_api::Digest32(public.evidence_refs[1].clone()).is_canonical_blake3()
            || public.correction_refs.len() > 64
            || public
                .correction_refs
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 4096)
            || public
                .reason
                .as_ref()
                .is_some_and(|value| value.len() > 4096)
            || public
                .dissent
                .as_ref()
                .is_some_and(|value| value.len() > 4096)
            || serde_json::to_vec(public).map_err(|e| e.to_string())?.len() > 65536
        {
            return Err("signed cognition causal proposal invalid".into());
        }
    }
    super::game_task::validate_cognition_fence_staged(
        world,
        &intent.request.agent_id,
        intent.task_fence.as_ref(),
        &signed.subject_public_key,
        intent.delegation_generation,
        staged,
    )?;
    if let Some(fence) = &intent.task_fence {
        super::game_task::validate_task_commit_context(
            world,
            &intent.request,
            fence,
            &signed.subject_public_key,
            staged,
        )?;
    }
    validate_agent_signer(
        world,
        &intent.request.agent_id,
        &signed.subject_public_key,
        intent.delegation_generation,
    )
}

pub fn validate_agent_signer(
    world: &World,
    agent_id: &str,
    public_key: &str,
    generation: u64,
) -> Result<(), String> {
    if public_key == owner_public_key(world, agent_id)? {
        if generation != 0 {
            return Err("owner proof has delegate generation".into());
        }
        return Ok(());
    }
    let delegation = world
        .capability_revocation_state()
        .agent_signer_delegations
        .get(agent_id)
        .ok_or("provider signer has no canonical delegation")?;
    let identity = &world.capability_revocation_state().agent_identities[agent_id];
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    if delegation.revoked
        || delegation.delegate_public_key != public_key
        || delegation.generation != generation
        || delegation.owner_binding != identity.owner_binding
        || delegation.agent_identity_generation != identity.generation
        || delegation.world_id != binding.world_id
        || delegation.branch_id != binding.branch_id
    {
        return Err("provider delegation revoked or fenced".into());
    }
    Ok(())
}

/// Pure admission: verify an Agent proof and match the original canonical
/// outbox identity. Mutation remains exclusively in registered execution.
pub fn validate_feedback_ack(
    world: &World,
    signed: &SignedReadRequest<super::FeedbackAckIntentV1>,
) -> Result<(), String> {
    verify_read_request("feedback_ack", signed)?;
    let ack = &signed.request;
    ack.validate()?;
    validate_agent_signer(
        world,
        &ack.agent_id,
        &signed.subject_public_key,
        ack.delegation_generation,
    )?;
    let record = world
        .runtime_feedback_outbox()
        .map_err(|e| format!("{e:?}"))?
        .into_iter()
        .find(|r| r.feedback_id == ack.feedback_id)
        .ok_or("canonical feedback acknowledgement record missing")?;
    record.validate().map_err(|e| e.to_string())?;
    let feedback: oasis7_agent_api::FeedbackEnvelopeV1 =
        serde_json::from_value(record.payload).map_err(|e| e.to_string())?;
    if feedback.status != "committed" {
        return Err(
            "canonical feedback acknowledgement supports committed service feedback only".into(),
        );
    }
    if record.agent_subject != ack.agent_id
        || record.agent_session_id != ack.agent_session_id
        || record.agent_turn_id != ack.agent_turn_id
        || record.decision_request_id != ack.decision_request_id
        || record.request_digest != ack.request_digest
        || record.feedback_seq != ack.feedback_seq
        || record.envelope_digest != ack.original_envelope_digest
        || feedback.runtime_receipt_id != ack.runtime_receipt_id
    {
        return Err("canonical feedback acknowledgement identity mismatch".into());
    }
    let published_service_feedback = world.capability_revocation_state().world_service_results.values()
        .any(|value| serde_json::from_value::<super::CanonicalIntentResultV1>(value.clone()).ok()
            .is_some_and(|result| result.rejected.is_none()
                && matches!(result.request.signed_payload, super::WorldServicePayloadV1::Cognition(ref signed)
                    if signed.request.request.agent_id == ack.agent_id
                        && signed.request.request.agent_session_id == ack.agent_session_id
                        && signed.request.request.agent_turn_id == ack.agent_turn_id
                        && signed.request.request.decision_request_id == ack.decision_request_id
                        && signed.request.request.request_digest == ack.request_digest)
                && result.receipt.get("lineage").and_then(|v| v.get("feedback_id")).and_then(serde_json::Value::as_str) == Some(ack.feedback_id.as_str())
                && result.receipt.get("lineage").and_then(|v| v.get("receipt_id")).and_then(serde_json::Value::as_str) == ack.runtime_receipt_id.as_deref()));
    let published_service_feedback = published_service_feedback
        || (world.state().retained_history_root.is_some()
            && world
                .read_retained_feedback_publication_exists(
                    &crate::FeedbackPublicationLookupV1::from(ack),
                )
                .map_err(|error| error.to_string())?);
    if !published_service_feedback {
        return Err("canonical feedback acknowledgement service publication missing".into());
    }
    if let Some(receipt_id) = &ack.runtime_receipt_id {
        let lineage = world
            .read_runtime_receipt_lineage(receipt_id)
            .map_err(|e| format!("{e:?}"))?;
        world
            .verify_runtime_receipt_lineage(&lineage)
            .map_err(|e| format!("{e:?}"))?;
        if lineage.agent_id != ack.agent_id
            || lineage.agent_session_id != ack.agent_session_id
            || lineage.agent_turn_id != ack.agent_turn_id
            || lineage.decision_request_id != ack.decision_request_id
            || lineage.request_digest != ack.request_digest
            || lineage.feedback_id != ack.feedback_id
            || feedback
                .candidate_action_id
                .is_none_or(|id| lineage.action_id != format!("action:{id}"))
        {
            return Err("canonical feedback acknowledgement receipt mismatch".into());
        }
    } else if feedback.status == "committed" {
        return Err("committed feedback acknowledgement receipt missing".into());
    }
    Ok(())
}
