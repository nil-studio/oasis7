use super::continuous_agent_harness::Digest32;
use super::decision_provider::DecisionResponse;

pub use oasis7_agent_harness::{
    COGNITION_LEGACY_RESPONSE_DIGEST_DOMAIN, COGNITION_RESPONSE_DIGEST_DOMAIN,
    CognitionResponseDigestDisposition,
};

pub fn cognition_response_digest(response: &DecisionResponse) -> Digest32 {
    oasis7_agent_harness::cognition_response_digest(response)
}
pub fn cognition_legacy_response_digest(response: &DecisionResponse) -> Digest32 {
    oasis7_agent_harness::cognition_legacy_response_digest(response)
}
pub fn classify_cognition_response_digest(
    response: &DecisionResponse,
    digest: &Digest32,
) -> CognitionResponseDigestDisposition {
    oasis7_agent_harness::classify_cognition_response_digest(response, digest)
}
