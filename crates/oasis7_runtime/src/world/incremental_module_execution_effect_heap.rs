//! Owned JSON clone admission for one module effect, never a source proof.
use oasis7_formal_protocol::incremental_value::ValueTreeError;
pub(super) struct EffectCloneWorkV1 {
    pub nodes: u64,
    pub bytes: u64,
    pub max_depth: usize,
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("module effect clone bound")
}
pub(super) fn json_clone_bound_v1(
    value: &serde_json::Value,
    work: &mut EffectCloneWorkV1,
    depth: usize,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<usize, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if depth > work.max_depth.min(64) {
        return Err(limited());
    }
    work.nodes = work.nodes.checked_sub(1).ok_or_else(limited)?;
    let mut cost = std::mem::size_of::<serde_json::Value>() + 128;
    let mut add = |n: usize| -> Result<(), ValueTreeError> {
        cost = cost.checked_add(n).ok_or_else(limited)?;
        Ok(())
    };
    match value {
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
        serde_json::Value::String(s) => {
            work.bytes = work.bytes.checked_sub(s.len() as u64).ok_or_else(limited)?;
            add(s.len().checked_mul(4).ok_or_else(limited)?)?;
        }
        serde_json::Value::Array(values) => {
            add(values
                .len()
                .checked_mul(std::mem::size_of::<serde_json::Value>())
                .and_then(|n| n.checked_mul(2))
                .ok_or_else(limited)?)?;
            for value in values {
                add(json_clone_bound_v1(value, work, depth + 1, cancelled)?)?;
            }
        }
        serde_json::Value::Object(values) => {
            for (k, v) in values {
                work.bytes = work.bytes.checked_sub(k.len() as u64).ok_or_else(limited)?;
                add(k
                    .len()
                    .checked_mul(4)
                    .and_then(|n| {
                        n.checked_add(
                            (std::mem::size_of::<String>()
                                + std::mem::size_of::<serde_json::Value>()
                                + 16)
                                * 16
                                + 256,
                        )
                    })
                    .ok_or_else(limited)?)?;
                add(json_clone_bound_v1(v, work, depth + 1, cancelled)?)?;
            }
        }
    }
    Ok(cost)
}
