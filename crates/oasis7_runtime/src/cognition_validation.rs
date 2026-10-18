use super::cognition::{
    CognitionValidationError, MAX_EXPECTED_VALUE_BYTES, MAX_IDENTIFIER_BYTES, MAX_PATH_BYTES,
    MAX_PRECONDITION_BYTES, MAX_PRECONDITIONS, PreconditionV1, is_canonical_identifier,
    is_supported_resource_path,
};
use std::collections::BTreeSet;

pub(crate) fn validate_preconditions(
    conditions: &[PreconditionV1],
) -> Result<(), CognitionValidationError> {
    if conditions.len() > MAX_PRECONDITIONS {
        return Err(CognitionValidationError::new("precondition_failed"));
    }
    let mut total_bytes = 0usize;
    let mut seen = BTreeSet::new();
    for condition in conditions {
        validate_precondition_shape(condition)?;
        validate_precondition_value(condition)?;
        let canonical = serde_json::to_vec(condition)
            .map_err(|_| CognitionValidationError::new("precondition_failed"))?;
        total_bytes = total_bytes.saturating_add(canonical.len());
        if total_bytes > MAX_PRECONDITION_BYTES || !seen.insert(canonical) {
            return Err(CognitionValidationError::new("precondition_failed"));
        }
    }
    Ok(())
}

pub(crate) fn validate_precondition_shape(
    condition: &PreconditionV1,
) -> Result<(), CognitionValidationError> {
    if condition.schema_version != 1
        || condition.missing_behavior != "fail"
        || condition.expected_value_bytes.is_empty()
        || condition.expected_value_bytes.len() > MAX_EXPECTED_VALUE_BYTES
        || condition.path_or_rule.is_empty()
        || condition.path_or_rule.len() > MAX_PATH_BYTES
        || !is_canonical_identifier(&condition.subject.id, MAX_IDENTIFIER_BYTES)
        || !matches!(
            condition.subject.kind.as_str(),
            "world" | "agent" | "intent"
        )
    {
        return Err(CognitionValidationError::new("precondition_failed"));
    }

    let numeric = matches!(
        condition.path_or_rule.as_str(),
        "world.logical_tick" | "world.reorg_epoch"
    ) || is_supported_resource_path(condition.path_or_rule.as_str());
    let equality = matches!(
        condition.path_or_rule.as_str(),
        "world.state_root"
            | "world.runtime_manifest_hash"
            | "agent.status"
            | "agent.position"
            | "agent.inventory_digest"
            | "agent.capability_snapshot_hash"
            | "intent.status"
    );
    if !numeric && !equality {
        return Err(CognitionValidationError::new("precondition_failed"));
    }
    let operator_valid = matches!(
        condition.operator.as_str(),
        "eq" | "neq" | "lt" | "lte" | "gt" | "gte"
    );
    let equality_operator = matches!(condition.operator.as_str(), "eq" | "neq");
    if !operator_valid || (equality && !equality_operator) {
        return Err(CognitionValidationError::new("precondition_failed"));
    }
    Ok(())
}

fn validate_precondition_value(condition: &PreconditionV1) -> Result<(), CognitionValidationError> {
    let bytes = condition.expected_value_bytes.as_slice();
    let valid = if matches!(
        condition.path_or_rule.as_str(),
        "world.logical_tick" | "world.reorg_epoch"
    ) {
        serde_cbor::from_slice::<u64>(bytes).is_ok()
    } else if is_supported_resource_path(condition.path_or_rule.as_str()) {
        serde_cbor::from_slice::<i64>(bytes).is_ok()
    } else {
        serde_cbor::from_slice::<serde_cbor::Value>(bytes).is_ok()
    };
    valid
        .then_some(())
        .ok_or_else(|| CognitionValidationError::new("precondition_failed"))
}
