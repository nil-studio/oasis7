//! Native Host cognition response-admission boundary, without a duplicated digest algorithm.
//!
//! This is an inert owner-first slice. Live runner/provider consumers are moved
//! only after their own success, refusal, retry and recovery acceptance.
use serde::Serialize;

use oasis7_agent_api::{
    CognitionError, ContinuousAgentResponseContextV1, DecisionResponse, Digest32,
    ResponseArtifactIdentityV1,
};

pub use oasis7_agent_api::CognitionResponseDigestDisposition;

/// Classify the canonical semantic digest without accepting a historical
/// full-DTO digest as a valid current response.
pub fn classify_response_digest<A: Serialize, Q: Serialize>(
    response: &DecisionResponse<A, Q>,
    digest: &Digest32,
) -> CognitionResponseDigestDisposition {
    oasis7_agent_api::classify_cognition_response_digest(response, digest)
}

/// Require both current response content and a matching authenticated artifact
/// identity before a Host caller is allowed to use an alleged provider result.
///
/// AgentAPI owns the versioned wire and digest semantics. The Harness owns the
/// portable host-seam validation; this module introduces no alternative hash.
pub fn validate_host_response_artifact<A: Serialize, Q: Serialize>(
    response: &ContinuousAgentResponseContextV1<A, Q>,
    identity: &ResponseArtifactIdentityV1,
) -> Result<(), CognitionError> {
    oasis7_agent_harness::validate_response_artifact_identity(response, identity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_agent_api::{
        CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR, CONTINUOUS_AGENT_CONTEXT_VERSION, ProviderDecision,
        ProviderDiagnostics, ProviderTraceEnvelope, h_v1,
    };

    fn response() -> DecisionResponse<serde_json::Value, serde_json::Value> {
        DecisionResponse {
            decision: ProviderDecision::Wait,
            module_command: None,
            provider_error: None,
            diagnostics: ProviderDiagnostics::default(),
            trace_payload: ProviderTraceEnvelope::default(),
            memory_write_intents: Vec::new(),
        }
    }

    fn context() -> ContinuousAgentResponseContextV1 {
        let response = response();
        ContinuousAgentResponseContextV1 {
            response_digest: oasis7_agent_api::cognition_response_digest(&response),
            base_decision_response: response,
            context_discriminator: CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.into(),
            context_version: CONTINUOUS_AGENT_CONTEXT_VERSION,
            agent_session_id: "host-session".into(),
            agent_turn_id: "turn-1".into(),
            decision_request_id: "request-1".into(),
            retry_seq: 0,
            transport_attempt: 1,
            request_digest: h_v1("test.native-host.request", &"request"),
        }
    }

    #[test]
    fn accepted_identity_is_exact_and_observability_does_not_change_semantic_digest() {
        let original = context();
        let identity = original.response_artifact_identity();
        validate_host_response_artifact(&original, &identity).unwrap();
        assert_eq!(
            classify_response_digest(&original.base_decision_response, &original.response_digest),
            CognitionResponseDigestDisposition::Current,
        );

        let mut changed = original.base_decision_response.clone();
        changed.diagnostics.latency_ms = Some(100);
        changed.trace_payload.latency_ms = Some(100);
        assert_eq!(
            oasis7_agent_api::cognition_response_digest(&changed),
            original.response_digest,
        );
    }

    #[test]
    fn legacy_full_response_hash_is_rejected_even_with_self_consistent_artifact() {
        let mut old = context();
        old.response_digest =
            oasis7_agent_api::cognition_legacy_response_digest(&old.base_decision_response);
        let identity = old.response_artifact_identity();
        assert_eq!(
            classify_response_digest(&old.base_decision_response, &old.response_digest),
            CognitionResponseDigestDisposition::Legacy,
        );
        assert_eq!(
            validate_host_response_artifact(&old, &identity)
                .unwrap_err()
                .code(),
            "legacy_response_digest_unsupported",
        );
    }

    #[test]
    fn forged_digest_or_another_request_identity_cannot_cross_host_boundary() {
        let correct = context();
        let identity = correct.response_artifact_identity();

        let mut corrupted = correct.clone();
        corrupted.response_digest = h_v1("test.forger", &"forged");
        assert_eq!(
            validate_host_response_artifact(&corrupted, &identity)
                .unwrap_err()
                .code(),
            "response_digest_mismatch",
        );

        let mut another_request = correct.clone();
        another_request.request_digest = h_v1("test.native-host.request", &"different");
        let forged_identity = another_request.response_artifact_identity();
        assert_eq!(
            validate_host_response_artifact(&correct, &forged_identity)
                .unwrap_err()
                .code(),
            "response_artifact_identity_mismatch",
        );
    }
}
