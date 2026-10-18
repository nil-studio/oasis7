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
    let result: CanonicalIntentResultV1 =
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    result.request.validate().map_err(|e| e.to_string())?;
    if derive_correlation(
        result.request.correlation.key.world.clone(),
        &result.request.signed_payload,
    )? != result.request.correlation
        || key_digest(&result.request.correlation.key)? != key
        || result.committed_height == 0
    {
        return Err("invalid canonical intent result identity".into());
    }
    super::game_task::validate_result(&result)?;
    Ok(result)
}

#[cfg(test)]
mod result_cache_tests;

pub use oasis7_client_api::legacy_world_service::derive_correlation;
