pub use oasis7_client_api::legacy_world_service::*;
pub use oasis7_client_api::world_service::*;
pub use oasis7_client_api::world_service_wire::{
    AgentSignerDelegationChangeV1, FeedbackAckIntentV1, SignedReadRequest,
};
pub mod correlation {
    use super::*;
    pub fn key_digest(key: &RequestKey) -> Result<String, String> {
        request_digest("request-key", key)
    }
    pub fn validate_result_record_uncached(
        key: &str,
        value: &serde_json::Value,
    ) -> Result<CanonicalIntentResultV1, String> {
        let result: CanonicalIntentResultV1 =
            serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        oasis7_client_api::legacy_world_service::validate_canonical_result_fields(key, &result)?;
        Ok(result)
    }
    pub fn validate_result(key: &str, value: &serde_json::Value) -> Result<(), String> {
        validate_result_record_uncached(key, value).map(|_| ())
    }
}

pub mod participant;
pub mod participant_policy;
