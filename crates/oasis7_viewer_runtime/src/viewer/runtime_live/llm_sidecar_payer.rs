use super::*;
use crate::simulator::ContinuousAgentRequestContextV1;
use oasis7_wasm_abi::CapabilitySubject;

/// Return the payer/account bound by Runtime to this provider invocation.
/// `agent_subject` identifies the execution subject, but it is not an
/// economic authority. The capability subject's owner binding is the
/// Runtime-authorized payer for the experimental fixed-unit policy.
pub(super) fn provider_payer_id(
    request: &ContinuousAgentRequestContextV1,
) -> Result<String, String> {
    let invocation = request
        .base_decision_request
        .capability_invocation_context
        .as_ref()
        .ok_or_else(|| {
            "Runtime provider invocation is missing its payer authority context".to_string()
        })?;
    let CapabilitySubject::Agent {
        agent_id,
        owner_binding,
        ..
    } = &invocation.subject
    else {
        return Err("Runtime provider invocation subject is not an Agent".to_string());
    };
    if agent_id != &request.agent_subject {
        return Err(
            "Runtime provider invocation subject does not match the request Agent".to_string(),
        );
    }
    if owner_binding.trim().is_empty() {
        return Err("Runtime provider invocation owner binding is empty".to_string());
    }
    Ok(owner_binding.clone())
}

/// Re-project the invocation through Runtime before using its owner binding
/// for an economic mutation. A persisted or provider-modified DTO may carry
/// a valid digest while naming another owner, so the live Runtime projection
/// must match before admission or recovery.
pub(super) fn runtime_authorized_provider_payer_id(
    world: &RuntimeWorld,
    request: &ContinuousAgentRequestContextV1,
) -> Result<String, String> {
    request
        .validate_production_lane()
        .map_err(|error| format!("Runtime provider request context invalid: {error}"))?;
    let invocation = request
        .base_decision_request
        .capability_invocation_context
        .as_ref()
        .ok_or_else(|| {
            "Runtime provider invocation is missing its payer authority context".to_string()
        })?;
    let (_, authoritative) = world
        .capability_context_for_agent(
            request.agent_subject.as_str(),
            invocation.presenter.clone(),
            invocation.response_nonce.clone(),
        )
        .map_err(|error| format!("Runtime provider payer authority unavailable: {error:?}"))?;
    // `catalog_snapshot_id` includes the Runtime world head and can advance
    // when the durable invocation is installed. Compare the stable authority
    // tuple while allowing that snapshot lineage to move between projection
    // and admission.
    if authoritative.subject != invocation.subject
        || authoritative.presenter != invocation.presenter
        || authoritative.audience != invocation.audience
        || authoritative.grant_id != invocation.grant_id
        || authoritative.module_id != invocation.module_id
        || authoritative.module_version != invocation.module_version
        || authoritative.response_nonce != invocation.response_nonce
    {
        return Err(
            "Runtime provider invocation payer authority does not match the live Runtime"
                .to_string(),
        );
    }
    provider_payer_id(request)
}
