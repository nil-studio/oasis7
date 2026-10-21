use super::*;
mod result_cache;

#[cfg(test)]
std::thread_local! {
    static RESULT_VALIDATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub fn key_digest(key: &RequestKey) -> Result<String, String> {
    request_digest("request-key", key)
}

pub fn validate_result(key: &str, value: &serde_json::Value) -> Result<(), String> {
    validate_result_record(key, value).map(|_| ())
}

pub(crate) fn validate_result_record(
    key: &str,
    value: &serde_json::Value,
) -> Result<CanonicalIntentResultV1, String> {
    result_cache::validate(key, value)
}

fn validate_result_record_uncached(
    key: &str,
    value: &serde_json::Value,
) -> Result<CanonicalIntentResultV1, String> {
    #[cfg(test)]
    RESULT_VALIDATIONS.with(|count| count.set(count.get() + 1));
    oasis7_world_state::service_admission::correlation::validate_result_record_uncached(key, value)
}

#[cfg(test)]
mod result_cache_tests;

pub use oasis7_client_api::legacy_world_service::derive_correlation;
