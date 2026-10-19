use super::continuous_agent_harness::{
    CognitionError, ContinuousAgentResponseContextV1, Digest32, ResponseArtifactIdentityV1,
};
use super::decision_provider::DecisionResponse;

pub use oasis7_agent_api::{
    COGNITION_LEGACY_RESPONSE_DIGEST_DOMAIN, COGNITION_RESPONSE_DIGEST_DOMAIN,
    CognitionResponseDigestDisposition,
};

pub fn cognition_response_digest(response: &DecisionResponse) -> Digest32 {
    oasis7_agent_api::cognition_response_digest(response)
}
pub fn cognition_legacy_response_digest(response: &DecisionResponse) -> Digest32 {
    oasis7_agent_api::cognition_legacy_response_digest(response)
}
pub fn classify_cognition_response_digest(
    response: &DecisionResponse,
    digest: &Digest32,
) -> CognitionResponseDigestDisposition {
    oasis7_agent_api::classify_cognition_response_digest(response, digest)
}

impl ContinuousAgentResponseContextV1 {
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
