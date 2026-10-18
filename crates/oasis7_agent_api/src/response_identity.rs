use crate::{
    CognitionError, ContinuousAgentResponseContextV1, DecisionResponse, Digest32,
    ResponseArtifactIdentityV1, h_v1,
};
use serde::Serialize;

/// Versioned identity domains for provider response content.
///
/// V2 is the target contract: only semantic response fields are hashed. V1
/// is retained solely to classify old persisted or external payloads so they
/// can be rejected explicitly instead of being accepted under a changed V1
/// meaning.
pub const COGNITION_RESPONSE_DIGEST_DOMAIN: &str = "oasis7.cognition.response.v2";
pub const COGNITION_LEGACY_RESPONSE_DIGEST_DOMAIN: &str = "oasis7.cognition.response.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionResponseDigestDisposition {
    Current,
    Legacy,
    Mismatch,
}

/// Hash the provider response fields that describe the logical cognition
/// outcome. Diagnostics and trace payloads are observability data; their
/// measured latency may vary across a transport retry.
pub fn cognition_response_digest<A: Serialize, Q: Serialize>(
    response: &DecisionResponse<A, Q>,
) -> Digest32 {
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

/// Compute the pre-fix full-DTO digest for explicit legacy classification.
/// This value is never accepted by the target response validation path.
pub fn cognition_legacy_response_digest<A: Serialize, Q: Serialize>(
    response: &DecisionResponse<A, Q>,
) -> Digest32 {
    h_v1(COGNITION_LEGACY_RESPONSE_DIGEST_DOMAIN, response)
}

pub fn classify_cognition_response_digest<A: Serialize, Q: Serialize>(
    response: &DecisionResponse<A, Q>,
    digest: &Digest32,
) -> CognitionResponseDigestDisposition {
    if digest == &cognition_response_digest(response) {
        CognitionResponseDigestDisposition::Current
    } else if digest == &cognition_legacy_response_digest(response) {
        CognitionResponseDigestDisposition::Legacy
    } else {
        CognitionResponseDigestDisposition::Mismatch
    }
}

impl<A: Serialize, Q: Serialize> ContinuousAgentResponseContextV1<A, Q> {
    pub fn validate_response_artifact_identity(
        &self,
        identity: &ResponseArtifactIdentityV1,
    ) -> Result<(), CognitionError> {
        match classify_cognition_response_digest(
            &self.base_decision_response,
            &self.response_digest,
        ) {
            CognitionResponseDigestDisposition::Current => {}
            CognitionResponseDigestDisposition::Legacy => {
                return Err(CognitionError::new(
                    "legacy_response_digest_unsupported",
                    "legacy full-response digest requires an explicit compatibility migration",
                ));
            }
            CognitionResponseDigestDisposition::Mismatch => {
                return Err(CognitionError::new(
                    "response_digest_mismatch",
                    "provider response digest does not match its content",
                ));
            }
        }
        identity.validate()?;
        if identity != &self.response_artifact_identity() {
            return Err(CognitionError::new(
                "response_artifact_identity_mismatch",
                "response artifact identity does not match the response context",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProviderDecision, ProviderDiagnostics, ProviderTraceEnvelope};

    fn wait_response() -> DecisionResponse<serde_json::Value, serde_json::Value> {
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
    fn semantic_digest_ignores_observability_fields() {
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
    }

    #[test]
    fn legacy_full_dto_digest_is_explicitly_classified_and_not_current() {
        let response = wait_response();
        let legacy = cognition_legacy_response_digest(&response);

        assert_ne!(cognition_response_digest(&response), legacy);
        assert_eq!(
            classify_cognition_response_digest(&response, &legacy),
            CognitionResponseDigestDisposition::Legacy
        );
    }

    #[test]
    fn semantic_digest_changes_for_authority_fields() {
        let first = wait_response();
        let mut second = first.clone();
        second.decision = ProviderDecision::WaitTicks { ticks: 2 };

        assert_ne!(
            cognition_response_digest(&first),
            cognition_response_digest(&second)
        );
    }
}
