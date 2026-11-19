//! Read-only lease views projected after complete Runtime validation.

use oasis7_agent_api::CognitionLeaseConsumptionViewV1;

use super::{
    AsyncAgentRunnerError, CognitionLeaseV1, ContinuousAgentRequestContextV1, WorldTime,
    validate_cognition_lease_for_request,
};

/// Runtime-to-Harness projection after complete lease validation. This is a
/// correlation view only; the durable lease remains the only accounting and
/// admission authority.
///
/// Produce the read-only API correlation view only after the same complete
/// Runtime lease admission checks used by the provider-dispatch boundary.
/// The view intentionally omits quote/accounting fields and cannot authorize
/// reservation, settlement, or world effects.
#[allow(dead_code)]
pub(crate) fn validated_cognition_lease_consumption_view(
    agent_id: &str,
    request_context: &ContinuousAgentRequestContextV1,
    lease: &CognitionLeaseV1,
    logical_tick: WorldTime,
) -> Result<CognitionLeaseConsumptionViewV1, AsyncAgentRunnerError> {
    validate_cognition_lease_for_request(agent_id, request_context, lease, logical_tick)?;
    Ok(CognitionLeaseConsumptionViewV1 {
        schema_version: CognitionLeaseConsumptionViewV1::SCHEMA_VERSION.to_string(),
        lease_id: lease.lease_id.clone(),
        idempotency_key: lease.idempotency_key.clone(),
        agent_id: lease.agent_id.clone(),
        agent_session_id: lease.agent_session_id.clone(),
        agent_turn_id: lease.agent_turn_id.clone(),
        decision_request_id: lease.decision_request_id.clone(),
        request_digest: lease.request_digest.clone(),
        status: lease.status,
        reserved_amount: lease.reserved_amount,
        reserved_at_tick: lease.reserved_at_tick,
    })
}
