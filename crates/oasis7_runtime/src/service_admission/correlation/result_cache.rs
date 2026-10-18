//! Memoizes only pure canonical receipt validation, never current World authority.
use super::{CanonicalIntentResultV1, validate_result_record_uncached};
use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

const MAX_ENTRIES: usize = 64;
const MAX_ENTRY_INPUT_BYTES: usize = 64 * 1024;
const MAX_RETAINED_INPUT_BYTES: usize = 4 * 1024 * 1024;

struct Entry {
    key: String,
    input: Vec<u8>,
    result: CanonicalIntentResultV1,
}

#[derive(Default)]
pub(super) struct Cache {
    entries: VecDeque<Entry>,
    // Encoded input lengths bound retention; this is not an exact heap metric.
    retained_input_bytes: usize,
}

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

struct BoundedInput {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for BoundedInput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.limit {
            return Err(std::io::Error::other("receipt cache input bound"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn validate(
    key: &str,
    value: &serde_json::Value,
) -> Result<CanonicalIntentResultV1, String> {
    validate_with(
        CACHE.get_or_init(|| Mutex::new(Cache::default())),
        key,
        value,
    )
}

pub(super) fn validate_with(
    cache: &Mutex<Cache>,
    key: &str,
    value: &serde_json::Value,
) -> Result<CanonicalIntentResultV1, String> {
    let Some(limit) = MAX_ENTRY_INPUT_BYTES.checked_sub(key.len()) else {
        return validate_result_record_uncached(key, value);
    };
    let mut encoded = BoundedInput {
        bytes: Vec::new(),
        limit,
    };
    if serde_json::to_writer(&mut encoded, value).is_err() {
        return validate_result_record_uncached(key, value);
    }
    let input = encoded.bytes;
    let input_bytes = input.len() + key.len();
    if let Ok(state) = cache.lock()
        && let Some(entry) = state
            .entries
            .iter()
            .find(|entry| entry.key == key && entry.input == input)
    {
        return Ok(entry.result.clone());
    }
    // Never hold the mutex through decoding or cryptography. Concurrent misses
    // may repeat work; neither in-progress nor failed validation is reusable.
    let result = validate_result_record_uncached(key, value)?;
    if let Ok(mut state) = cache.lock()
        && !state
            .entries
            .iter()
            .any(|entry| entry.key == key && entry.input == input)
    {
        while state.entries.len() >= MAX_ENTRIES
            || state.retained_input_bytes.saturating_add(input_bytes) > MAX_RETAINED_INPUT_BYTES
        {
            let Some(entry) = state.entries.pop_front() else {
                break;
            };
            state.retained_input_bytes -= entry.key.len() + entry.input.len();
        }
        state.retained_input_bytes += input_bytes;
        state.entries.push_back(Entry {
            key: key.into(),
            input,
            result: result.clone(),
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_admission::correlation::RESULT_VALIDATIONS;
    use crate::service_admission::correlation::result_cache_tests::fixture;

    #[test]
    fn bounds_and_oversize_bypass_preserve_cold_validation() {
        let cache = Mutex::new(Cache::default());
        for index in 0..MAX_ENTRIES + 1 {
            let (key, value) = fixture(&format!("result-cache-bound-{index}"));
            validate_with(&cache, &key, &value).unwrap();
        }
        {
            let state = cache.lock().unwrap();
            assert_eq!(state.entries.len(), MAX_ENTRIES);
            assert!(state.retained_input_bytes <= MAX_RETAINED_INPUT_BYTES);
        }
        let (key, mut value) = fixture("result-cache-oversize");
        value["rejected"] = "x".repeat(MAX_ENTRY_INPUT_BYTES).into();
        RESULT_VALIDATIONS.with(|count| count.set(0));
        validate_with(&cache, &key, &value).unwrap();
        validate_with(&cache, &key, &value).unwrap();
        assert_eq!(RESULT_VALIDATIONS.with(std::cell::Cell::get), 2);
        assert_eq!(cache.lock().unwrap().entries.len(), MAX_ENTRIES);
    }

    #[test]
    fn failures_are_not_retained_and_poisoned_cache_revalidates() {
        let cache = Mutex::new(Cache::default());
        let (key, mut value) = fixture("result-cache-failure");
        value["committed_height"] = 0.into();
        RESULT_VALIDATIONS.with(|count| count.set(0));
        assert!(validate_with(&cache, &key, &value).is_err());
        assert!(validate_with(&cache, &key, &value).is_err());
        assert_eq!(RESULT_VALIDATIONS.with(std::cell::Cell::get), 2);
        assert!(cache.lock().unwrap().entries.is_empty());
        let _ = std::panic::catch_unwind(|| {
            let _guard = cache.lock().unwrap();
            panic!("poison disposable cache");
        });
        let (key, value) = fixture("result-cache-poisoned");
        RESULT_VALIDATIONS.with(|count| count.set(0));
        validate_with(&cache, &key, &value).unwrap();
        validate_with(&cache, &key, &value).unwrap();
        assert_eq!(RESULT_VALIDATIONS.with(std::cell::Cell::get), 2);
    }
}
