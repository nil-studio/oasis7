use super::*;
use crate::runtime::World as RuntimeWorld;

/// Compare the complete provider request identity carried by two sidecar
/// contexts. Map keys are only routing hints: a restored context is safe to
/// reuse only when every Runtime correlation field agrees.
pub(super) fn provider_context_identity_matches(
    left: &cognition_context::ProviderContextState,
    right: &cognition_context::ProviderContextState,
) -> bool {
    left.request_context.agent_subject == right.request_context.agent_subject
        && left.request_context.agent_session_id == right.request_context.agent_session_id
        && left.request_context.agent_turn_id == right.request_context.agent_turn_id
        && left.request_context.decision_request_id == right.request_context.decision_request_id
        && left.request_context.request_digest == right.request_context.request_digest
        && payer_support::provider_payer_id(&left.request_context).ok()
            == payer_support::provider_payer_id(&right.request_context).ok()
}

pub(super) fn validate_provider_lease_identity(
    agent_id: &str,
    request: &crate::simulator::ContinuousAgentRequestContextV1,
    lease: &crate::runtime::CognitionLeaseV1,
) -> Result<(), String> {
    request
        .validate()
        .map_err(|error| format!("provider cognition request invalid: {error}"))?;
    lease
        .validate()
        .map_err(|error| format!("provider cognition lease invalid: {error}"))?;
    let expected_account = payer_support::provider_payer_id(request)?;
    if lease.status != crate::runtime::CognitionLeaseStatusV1::Reserved {
        return Err(format!(
            "provider cognition lease is not reserved for {agent_id}"
        ));
    }
    let expected_invocation_key = request.provider_invocation_key().to_string();
    let mismatches = [
        ("agent_id", lease.agent_id != agent_id),
        ("agent_subject", request.agent_subject != agent_id),
        ("account_id", lease.account_id != expected_account),
        (
            "idempotency_key",
            lease.idempotency_key != expected_invocation_key,
        ),
        (
            "agent_session_id",
            lease.agent_session_id != request.agent_session_id,
        ),
        (
            "agent_turn_id",
            lease.agent_turn_id != request.agent_turn_id,
        ),
        (
            "decision_request_id",
            lease.decision_request_id != request.decision_request_id,
        ),
        (
            "request_digest",
            lease.request_digest != request.request_digest.to_string(),
        ),
        ("quote.resource", lease.quote.resource != "cognition_units"),
        ("reserved_amount", lease.reserved_amount != 1),
        ("quote.payer_id", lease.quote.payer_id != lease.account_id),
        (
            "quote.resource_version",
            lease.quote.resource_version != crate::runtime::COGNITION_RESOURCE_VERSION_V1,
        ),
        ("quote.purpose", lease.quote.purpose != "provider_cognition"),
        ("quote.scope", lease.quote.scope != "agent_turn"),
        (
            "quote.policy_revision",
            lease.quote.policy_revision
                != crate::runtime::COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
        ),
        (
            "quote.authority_context",
            lease.quote.authority_context
                != request.capability_invocation_context_digest.to_string(),
        ),
        (
            "quote.world_binding",
            lease.quote.world_binding != request.runtime_binding.base_world_hash.to_string(),
        ),
    ]
    .into_iter()
    .filter_map(|(field, mismatched)| mismatched.then_some(field))
    .collect::<Vec<_>>();
    if !mismatches.is_empty() {
        return Err(format!(
            "provider cognition lease identity mismatch for {agent_id}: fields={}",
            mismatches.join(",")
        ));
    }
    Ok(())
}

pub(super) fn validate_provider_lease_binding(
    world: &RuntimeWorld,
    agent_id: &str,
    request: &crate::simulator::ContinuousAgentRequestContextV1,
    lease: &crate::runtime::CognitionLeaseV1,
    operation: &str,
) -> Result<(), String> {
    validate_provider_lease_identity(agent_id, request, lease)?;
    payer_support::runtime_authorized_provider_payer_id(world, request)?;
    validate_provider_lease_runtime_record(world, agent_id, lease, operation)?;
    if operation == "dispatch"
        && (lease.reserved_at_tick > world.state().time
            || lease
                .quote
                .valid_until_tick
                .is_some_and(|expires| world.state().time > expires))
    {
        return Err(format!(
            "provider cognition lease dispatch is stale for {agent_id}"
        ));
    }
    Ok(())
}

/// Validate the exact Runtime-owned lease record without projecting its
/// request through the current capability authority. Generation rotation
/// deliberately makes an old request's grant obsolete, but the old Reserved
/// lease still needs a durable release during recovery.
pub(super) fn validate_provider_lease_runtime_record(
    world: &RuntimeWorld,
    agent_id: &str,
    lease: &crate::runtime::CognitionLeaseV1,
    operation: &str,
) -> Result<(), String> {
    let economy = world.cognition_economy().map_err(|error| {
        format!("provider cognition lease {operation} economy read failed: {error:?}")
    })?;
    let runtime_lease = economy.leases.get(lease.lease_id.as_str()).ok_or_else(|| {
        format!(
            "provider cognition lease {operation} missing from Runtime: {}",
            lease.lease_id
        )
    })?;
    if runtime_lease.idempotency_key != lease.idempotency_key
        || runtime_lease.account_id != lease.account_id
        || runtime_lease.agent_id != lease.agent_id
        || runtime_lease.agent_session_id != lease.agent_session_id
        || runtime_lease.agent_turn_id != lease.agent_turn_id
        || runtime_lease.decision_request_id != lease.decision_request_id
        || runtime_lease.request_digest != lease.request_digest
        || runtime_lease.quote != lease.quote
        || runtime_lease.reserved_amount != lease.reserved_amount
    {
        return Err(format!(
            "provider cognition lease {operation} Runtime identity mismatch for {agent_id}"
        ));
    }
    if operation == "dispatch"
        && runtime_lease.status != crate::runtime::CognitionLeaseStatusV1::Reserved
    {
        return Err(format!(
            "provider cognition lease dispatch is already closed for {agent_id}"
        ));
    }
    Ok(())
}

pub(super) fn provider_request_capability_identity(
    request: &crate::simulator::ContinuousAgentRequestContextV1,
) -> Option<crate::runtime::CapabilityAgentIdentity> {
    let invocation = request
        .base_decision_request
        .capability_invocation_context
        .as_ref()?;
    let oasis7_wasm_abi::CapabilitySubject::Agent {
        agent_id: _,
        owner_binding,
        generation,
    } = &invocation.subject
    else {
        return None;
    };
    Some(crate::runtime::CapabilityAgentIdentity {
        owner_binding: owner_binding.clone(),
        generation: *generation,
    })
}
