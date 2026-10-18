//! Lossless adapters between Oasis7's source-compatible Harness wrappers and
//! the portable `oasis7_agent_api` DTOs.
//!
//! These conversions carry data only. Runtime admission, lease accounting,
//! durable receipt derivation, and world-readback verification stay in their
//! existing Oasis7-owned paths.

use crate::simulator::{DecisionRequest, DecisionResponse};
use oasis7_simulator::{Action, AgentQuery};

#[cfg(test)]
use super::continuous_agent_harness::MemoryWriteIntentV1;
use super::continuous_agent_harness::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1, FeedbackEnvelopeV1,
};

pub(crate) fn request_context_to_api(
    context: &ContinuousAgentRequestContextV1,
) -> oasis7_agent_api::ContinuousAgentRequestContextV1 {
    oasis7_agent_api::ContinuousAgentRequestContextV1 {
        base_decision_request: decision_request_to_api(&context.base_decision_request),
        context_discriminator: context.context_discriminator.clone(),
        context_version: context.context_version,
        protocol_version: context.protocol_version.clone(),
        agent_session_id: context.agent_session_id.clone(),
        agent_turn_id: context.agent_turn_id.clone(),
        decision_request_id: context.decision_request_id.clone(),
        retry_seq: context.retry_seq,
        transport_attempt: context.transport_attempt,
        agent_subject: context.agent_subject.clone(),
        runtime_binding: context.runtime_binding.clone(),
        observation_digest: context.observation_digest.clone(),
        capability_catalog_digest: context.capability_catalog_digest.clone(),
        capability_invocation_context_digest: context.capability_invocation_context_digest.clone(),
        memory_snapshot_digest: context.memory_snapshot_digest.clone(),
        goal_snapshot_digest: context.goal_snapshot_digest.clone(),
        continuation_digest: context.continuation_digest.clone(),
        adapter_protocol_version: context.adapter_protocol_version.clone(),
        budget_contract: context.budget_contract.clone(),
        request_digest: context.request_digest.clone(),
    }
}

pub(crate) fn request_context_from_api(
    context: oasis7_agent_api::ContinuousAgentRequestContextV1,
) -> ContinuousAgentRequestContextV1 {
    ContinuousAgentRequestContextV1 {
        base_decision_request: decision_request_from_api(context.base_decision_request),
        context_discriminator: context.context_discriminator,
        context_version: context.context_version,
        protocol_version: context.protocol_version,
        agent_session_id: context.agent_session_id,
        agent_turn_id: context.agent_turn_id,
        decision_request_id: context.decision_request_id,
        retry_seq: context.retry_seq,
        transport_attempt: context.transport_attempt,
        agent_subject: context.agent_subject,
        runtime_binding: context.runtime_binding,
        observation_digest: context.observation_digest,
        capability_catalog_digest: context.capability_catalog_digest,
        capability_invocation_context_digest: context.capability_invocation_context_digest,
        memory_snapshot_digest: context.memory_snapshot_digest,
        goal_snapshot_digest: context.goal_snapshot_digest,
        continuation_digest: context.continuation_digest,
        adapter_protocol_version: context.adapter_protocol_version,
        budget_contract: context.budget_contract,
        request_digest: context.request_digest,
    }
}

pub(crate) fn response_context_to_api(
    context: &ContinuousAgentResponseContextV1,
) -> oasis7_agent_api::ContinuousAgentResponseContextV1<Action, AgentQuery> {
    oasis7_agent_api::ContinuousAgentResponseContextV1 {
        base_decision_response: decision_response_to_api(&context.base_decision_response),
        context_discriminator: context.context_discriminator.clone(),
        context_version: context.context_version,
        agent_session_id: context.agent_session_id.clone(),
        agent_turn_id: context.agent_turn_id.clone(),
        decision_request_id: context.decision_request_id.clone(),
        retry_seq: context.retry_seq,
        transport_attempt: context.transport_attempt,
        request_digest: context.request_digest.clone(),
        response_digest: context.response_digest.clone(),
    }
}

#[cfg(test)]
pub(crate) fn response_context_from_api(
    context: oasis7_agent_api::ContinuousAgentResponseContextV1<Action, AgentQuery>,
) -> ContinuousAgentResponseContextV1 {
    ContinuousAgentResponseContextV1 {
        base_decision_response: decision_response_from_api(context.base_decision_response),
        context_discriminator: context.context_discriminator,
        context_version: context.context_version,
        agent_session_id: context.agent_session_id,
        agent_turn_id: context.agent_turn_id,
        decision_request_id: context.decision_request_id,
        retry_seq: context.retry_seq,
        transport_attempt: context.transport_attempt,
        request_digest: context.request_digest,
        response_digest: context.response_digest,
    }
}

pub(crate) fn feedback_to_api(
    feedback: &FeedbackEnvelopeV1,
) -> oasis7_agent_api::FeedbackEnvelopeV1 {
    oasis7_agent_api::FeedbackEnvelopeV1 {
        feedback_id: feedback.feedback_id.clone(),
        feedback_seq: feedback.feedback_seq,
        agent_subject: feedback.agent_subject.clone(),
        agent_session_id: feedback.agent_session_id.clone(),
        agent_turn_id: feedback.agent_turn_id.clone(),
        decision_request_id: feedback.decision_request_id.clone(),
        candidate_action_id: feedback.candidate_action_id,
        runtime_receipt_id: feedback.runtime_receipt_id.clone(),
        status: feedback.status.clone(),
        request_digest: feedback.request_digest.clone(),
        reject_reason: feedback.reject_reason.clone(),
        provenance: feedback.provenance.clone(),
    }
}

pub(crate) fn feedback_from_api(
    feedback: oasis7_agent_api::FeedbackEnvelopeV1,
) -> FeedbackEnvelopeV1 {
    FeedbackEnvelopeV1 {
        feedback_id: feedback.feedback_id,
        feedback_seq: feedback.feedback_seq,
        agent_subject: feedback.agent_subject,
        agent_session_id: feedback.agent_session_id,
        agent_turn_id: feedback.agent_turn_id,
        decision_request_id: feedback.decision_request_id,
        candidate_action_id: feedback.candidate_action_id,
        runtime_receipt_id: feedback.runtime_receipt_id,
        status: feedback.status,
        request_digest: feedback.request_digest,
        reject_reason: feedback.reject_reason,
        provenance: feedback.provenance,
    }
}

#[cfg(test)]
pub(crate) fn memory_write_intent_to_api(
    intent: &MemoryWriteIntentV1,
) -> oasis7_agent_api::MemoryWriteIntentV1 {
    oasis7_agent_api::MemoryWriteIntentV1 {
        schema_version: intent.schema_version,
        scope: intent.scope.clone(),
        summary: intent.summary.clone(),
        tags: intent.tags.clone(),
        compatibility_reason: intent.compatibility_reason.clone(),
    }
}

fn decision_request_to_api(request: &DecisionRequest) -> oasis7_agent_api::DecisionRequest {
    oasis7_agent_api::DecisionRequest {
        observation: request.observation.clone(),
        provider_config_ref: request.provider_config_ref.clone(),
        agent_profile: request.agent_profile.clone(),
        fixture_id: request.fixture_id.clone(),
        replay_id: request.replay_id.clone(),
        capability_catalog: request.capability_catalog.clone(),
        capability_invocation_context: request.capability_invocation_context.clone(),
        timeout_budget_ms: request.timeout_budget_ms,
    }
}

fn decision_request_from_api(request: oasis7_agent_api::DecisionRequest) -> DecisionRequest {
    DecisionRequest {
        observation: request.observation,
        provider_config_ref: request.provider_config_ref,
        agent_profile: request.agent_profile,
        fixture_id: request.fixture_id,
        replay_id: request.replay_id,
        capability_catalog: request.capability_catalog,
        capability_invocation_context: request.capability_invocation_context,
        timeout_budget_ms: request.timeout_budget_ms,
    }
}

pub(crate) fn decision_response_to_api(
    response: &DecisionResponse,
) -> oasis7_agent_api::DecisionResponse<Action, AgentQuery> {
    oasis7_agent_api::DecisionResponse {
        decision: response.decision.clone(),
        module_command: response.module_command.clone(),
        provider_error: response.provider_error.clone(),
        diagnostics: response.diagnostics.clone(),
        trace_payload: response.trace_payload.clone(),
        memory_write_intents: response.memory_write_intents.clone(),
    }
}

#[cfg(test)]
fn decision_response_from_api(
    response: oasis7_agent_api::DecisionResponse<Action, AgentQuery>,
) -> DecisionResponse {
    DecisionResponse {
        decision: response.decision,
        module_command: response.module_command,
        provider_error: response.provider_error,
        diagnostics: response.diagnostics,
        trace_payload: response.trace_payload,
        memory_write_intents: response.memory_write_intents,
    }
}
