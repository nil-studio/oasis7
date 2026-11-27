use super::LlmConfigError;

pub(super) use oasis7_model_provider::goal_value;
pub(super) fn required_env<F>(getter: &mut F, key: &'static str) -> Result<String, LlmConfigError>
where
    F: FnMut(&str) -> Option<String>,
{
    let value = getter(key).ok_or(LlmConfigError::MissingEnv { key })?;
    if value.trim().is_empty() {
        return Err(LlmConfigError::EmptyEnv { key });
    }
    Ok(value)
}

pub(super) fn parse_positive_usize<F>(
    getter: &mut F,
    key: &'static str,
    default: usize,
    error: fn(String) -> LlmConfigError,
) -> Result<usize, LlmConfigError>
where
    F: FnMut(&str) -> Option<String>,
{
    match getter(key) {
        Some(value) => value
            .parse::<usize>()
            .ok()
            .filter(|parsed| *parsed > 0)
            .ok_or_else(|| error(value)),
        None => Ok(default),
    }
}

pub(super) fn parse_positive_i64<F>(
    getter: &mut F,
    key: &'static str,
    default: i64,
    error: fn(String) -> LlmConfigError,
) -> Result<i64, LlmConfigError>
where
    F: FnMut(&str) -> Option<String>,
{
    match getter(key) {
        Some(value) => value
            .parse::<i64>()
            .ok()
            .filter(|parsed| *parsed > 0)
            .ok_or_else(|| error(value)),
        None => Ok(default),
    }
}

pub(super) fn parse_non_negative_usize<F>(
    getter: &mut F,
    key: &'static str,
    default: usize,
    error: fn(String) -> LlmConfigError,
) -> Result<usize, LlmConfigError>
where
    F: FnMut(&str) -> Option<String>,
{
    match getter(key) {
        Some(value) => value.parse::<usize>().map_err(|_| error(value.clone())),
        None => Ok(default),
    }
}
