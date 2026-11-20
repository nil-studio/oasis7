//! Provider traits and the shared response identity checks.

use std::error::Error;
use std::fmt;

use oasis7_agent_api::{
    CognitionError, ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    FeedbackEnvelopeV1, h_v1,
};
use serde::Serialize;
use serde_json::Value;

pub const COGNITION_RESPONSE_DIGEST_DOMAIN: &str = "oasis7.cognition.response.v2";
pub const COGNITION_LEGACY_RESPONSE_DIGEST_DOMAIN: &str = "oasis7.cognition.response.v1";
const CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR: &str = "oasis7.continuous-agent-context";
const CONTINUOUS_AGENT_CONTEXT_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionResponseDigestDisposition {
    Current,
    Legacy,
    Mismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionProviderError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl DecisionProviderError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }

    fn cognition(error: CognitionError) -> Self {
        Self::new(error.code(), error.message(), false)
    }
}

impl fmt::Display for DecisionProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for DecisionProviderError {}

pub fn cognition_response_digest<A: Serialize, Q: Serialize>(
    response: &oasis7_agent_api::DecisionResponse<A, Q>,
) -> oasis7_agent_api::Digest32 {
    h_v1(
        COGNITION_RESPONSE_DIGEST_DOMAIN,
        &(
            &response.decision,
            &response.module_command,
            &response.provider_error,
            &response.memory_write_intents,
        ),
    )
}

pub fn cognition_legacy_response_digest<A: Serialize, Q: Serialize>(
    response: &oasis7_agent_api::DecisionResponse<A, Q>,
) -> oasis7_agent_api::Digest32 {
    h_v1(COGNITION_LEGACY_RESPONSE_DIGEST_DOMAIN, response)
}

pub fn classify_cognition_response_digest<A: Serialize, Q: Serialize>(
    response: &oasis7_agent_api::DecisionResponse<A, Q>,
    digest: &oasis7_agent_api::Digest32,
) -> CognitionResponseDigestDisposition {
    if digest == &cognition_response_digest(response) {
        CognitionResponseDigestDisposition::Current
    } else if digest == &cognition_legacy_response_digest(response) {
        CognitionResponseDigestDisposition::Legacy
    } else {
        CognitionResponseDigestDisposition::Mismatch
    }
}

pub fn validate_response_artifact_identity<A: Serialize, Q: Serialize>(
    response_context: &ContinuousAgentResponseContextV1<A, Q>,
    identity: &oasis7_agent_api::ResponseArtifactIdentityV1,
) -> Result<(), CognitionError> {
    response_context.validate_response_artifact_identity(identity)
}

/// Portable DecisionProvider seam. Application adapters retain the mapping
/// between typed game actions and this shared proposal DTO; a provider cannot
/// write World state through this interface.
pub trait DecisionProvider<A = Value, Q = Value>: Send
where
    A: Serialize,
    Q: Serialize,
{
    fn provider_id(&self) -> &str;

    fn decide(
        &mut self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<ContinuousAgentResponseContextV1<A, Q>, DecisionProviderError>;

    fn push_feedback(
        &mut self,
        _feedback: &FeedbackEnvelopeV1,
    ) -> Result<(), DecisionProviderError> {
        Ok(())
    }

    /// Validate the full request before provider work and reject response
    /// lineage/content drift before it reaches the host adapter.
    fn decide_checked(
        &mut self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<ContinuousAgentResponseContextV1<A, Q>, DecisionProviderError> {
        request
            .validate_production_lane()
            .map_err(DecisionProviderError::cognition)?;
        let response = self.decide(request)?;
        validate_response_for_request(&response, request)?;
        Ok(response)
    }
}

pub fn validate_response_for_request<A: Serialize, Q: Serialize>(
    response: &ContinuousAgentResponseContextV1<A, Q>,
    request: &ContinuousAgentRequestContextV1,
) -> Result<(), DecisionProviderError> {
    if response.context_discriminator != CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR
        || response.context_version != CONTINUOUS_AGENT_CONTEXT_VERSION
        || response.agent_session_id != request.agent_session_id
        || response.agent_turn_id != request.agent_turn_id
        || response.decision_request_id != request.decision_request_id
        || response.retry_seq != request.retry_seq
        || response.transport_attempt != request.transport_attempt
        || response.request_digest != request.request_digest
    {
        return Err(DecisionProviderError::new(
            "response_identity_mismatch",
            "provider response does not preserve the complete outer request lineage",
            false,
        ));
    }
    match classify_cognition_response_digest(
        &response.base_decision_response,
        &response.response_digest,
    ) {
        CognitionResponseDigestDisposition::Current => {}
        CognitionResponseDigestDisposition::Legacy => {
            return Err(DecisionProviderError::new(
                "legacy_response_digest_unsupported",
                "legacy full-response digest requires an explicit compatibility migration",
                false,
            ));
        }
        CognitionResponseDigestDisposition::Mismatch => {
            return Err(DecisionProviderError::new(
                "response_digest_mismatch",
                "provider response digest does not match its content",
                false,
            ));
        }
    }
    let identity = response.response_artifact_identity();
    identity
        .validate()
        .map_err(DecisionProviderError::cognition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_agent_api::{
        DecisionResponse, Digest32, ProviderDecision, ProviderDiagnostics, ProviderTraceEnvelope,
    };
    use serde_json::json;

    fn wait_response() -> DecisionResponse {
        DecisionResponse {
            decision: ProviderDecision::Wait,
            module_command: None,
            provider_error: None,
            diagnostics: ProviderDiagnostics::default(),
            trace_payload: ProviderTraceEnvelope::default(),
            memory_write_intents: Vec::new(),
        }
    }

    #[test]
    fn response_digest_uses_semantic_fields_and_ignores_trace_latency() {
        let mut first = wait_response();
        let mut second = first.clone();
        first.diagnostics.latency_ms = Some(5);
        first.trace_payload.latency_ms = Some(5);
        second.diagnostics.latency_ms = Some(1);
        second.trace_payload.latency_ms = Some(1);
        assert_eq!(
            cognition_response_digest(&first),
            cognition_response_digest(&second)
        );
        second.decision = ProviderDecision::WaitTicks { ticks: 2 };
        assert_ne!(
            cognition_response_digest(&first),
            cognition_response_digest(&second)
        );
    }

    #[test]
    fn legacy_digest_remains_classified_but_is_not_current() {
        let response = wait_response();
        let legacy = cognition_legacy_response_digest(&response);
        assert_ne!(cognition_response_digest(&response), legacy);
        assert_eq!(
            classify_cognition_response_digest(&response, &legacy),
            CognitionResponseDigestDisposition::Legacy
        );
    }

    #[test]
    fn provider_response_digest_matches_shared_golden() {
        let response = wait_response();
        assert_eq!(
            serde_json::to_value(&response).unwrap()["decision"],
            json!({"decision":"wait"})
        );
        // Literal generated from the established v2 domain and canonical
        // semantic tuple; guards extraction against a digest-domain change.
        assert_eq!(
            cognition_response_digest(&response).as_str(),
            "blake3:1be5141984fcd3b521d54c5a2d763ea5da7502f6c4c0e23be85095abd57a6c02"
        );
        assert!(
            Digest32::from(
                "blake3:0000000000000000000000000000000000000000000000000000000000000000"
            )
            .is_canonical_blake3()
        );
    }
}
