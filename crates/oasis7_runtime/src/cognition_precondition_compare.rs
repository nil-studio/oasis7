//! Canonical comparison of observed Current values against encoded preconditions.
use super::{CognitionValidationError, PreconditionV1};
use serde_json::Value as JsonValue;

pub(super) fn compare_precondition(
    condition: &PreconditionV1,
    actual: JsonValue,
) -> Result<(), CognitionValidationError> {
    let is_unsigned = matches!(
        condition.path_or_rule.as_str(),
        "world.logical_tick" | "world.reorg_epoch"
    );
    let is_numeric = is_unsigned || is_supported_resource_path(condition.path_or_rule.as_str());
    if is_numeric {
        let left = if is_unsigned {
            actual.as_u64().map(i128::from)
        } else {
            actual.as_i64().map(i128::from)
        };
        let right = if is_unsigned {
            serde_cbor::from_slice::<u64>(condition.expected_value_bytes.as_slice())
                .ok()
                .map(i128::from)
        } else {
            serde_cbor::from_slice::<i64>(condition.expected_value_bytes.as_slice())
                .ok()
                .map(i128::from)
        };
        let (Some(left), Some(right)) = (left, right) else {
            return Err(CognitionValidationError::new("precondition_failed"));
        };
        let matches = match condition.operator.as_str() {
            "eq" => left == right,
            "neq" => left != right,
            "lt" => left < right,
            "lte" => left <= right,
            "gt" => left > right,
            "gte" => left >= right,
            _ => false,
        };
        if matches {
            return Ok(());
        }
    } else {
        let expected: JsonValue = serde_cbor::from_slice(condition.expected_value_bytes.as_slice())
            .map_err(|_| CognitionValidationError::new("precondition_failed"))?;
        let matches = match condition.operator.as_str() {
            "eq" => actual == expected,
            "neq" => actual != expected,
            _ => false,
        };
        if matches {
            return Ok(());
        }
    }
    Err(CognitionValidationError::new("precondition_failed"))
}

pub(crate) fn is_supported_resource_path(path: &str) -> bool {
    matches!(path, "agent.resource.electricity" | "agent.resource.data")
}
