//! Exact borrowed JSON equality follows the persisted node types and float bits.
use crate::incremental_json_records::JsonReadWorkV1;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use serde_json::Value;
fn cell(
    work: &mut JsonReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    work.cells_remaining =
        work.cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "authorization JSON comparison cells",
            ))?;
    Ok(())
}
fn bytes(
    a: &[u8],
    b: &[u8],
    work: &mut JsonReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<bool, ValueTreeError> {
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.chunks(4096).zip(b.chunks(4096)) {
        cell(work, cancelled)?;
        if a != b {
            return Ok(false);
        }
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(true)
}
fn equal(
    a: &Value,
    b: &Value,
    depth: usize,
    work: &mut JsonReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<bool, ValueTreeError> {
    cell(work, cancelled)?;
    work.json_nodes_remaining =
        work.json_nodes_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "authorization JSON comparison nodes",
            ))?;
    if depth > 64 || depth > work.max_depth {
        return Err(ValueTreeError::ResourceLimited(
            "authorization JSON comparison depth",
        ));
    }
    match (a, b) {
        (Value::Null, Value::Null) => Ok(true),
        (Value::Bool(a), Value::Bool(b)) => Ok(a == b),
        (Value::Number(a), Value::Number(b)) => {
            if a.is_f64() != b.is_f64() {
                return Ok(false);
            }
            if a.is_f64() {
                return Ok(a.as_f64().map(f64::to_bits) == b.as_f64().map(f64::to_bits));
            }
            if a.as_u64().is_some() || b.as_u64().is_some() {
                return Ok(a.as_u64() == b.as_u64());
            }
            Ok(a.as_i64() == b.as_i64())
        }
        (Value::String(a), Value::String(b)) => bytes(a.as_bytes(), b.as_bytes(), work, cancelled),
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (a, b) in a.iter().zip(b) {
                if !equal(a, b, depth + 1, work, cancelled)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Value::Object(a), Value::Object(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for ((ak, av), (bk, bv)) in a.iter().zip(b.iter()) {
                if !bytes(ak.as_bytes(), bk.as_bytes(), work, cancelled)?
                    || !equal(av, bv, depth + 1, work, cancelled)?
                {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}
pub(crate) fn exact_authorization_json_match_v1(
    a: &Value,
    b: &Value,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut JsonReadWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<bool, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let _stack = memory.reserve(128 << 10, class)?;
    let matched = equal(a, b, 0, work, &mut cancelled)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(matched)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn work() -> JsonReadWorkV1 {
        JsonReadWorkV1 {
            cells_remaining: 1000,
            json_nodes_remaining: 1000,
            max_depth: 16,
            max_payload_bytes: 1 << 20,
        }
    }
    #[test]
    fn persisted_float_bits_and_types_are_exact() {
        let memory = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
        let negative = serde_json::json!({"n":[-0.0]});
        let positive = serde_json::json!({"n":[0.0]});
        assert_eq!(negative, positive);
        assert!(
            !exact_authorization_json_match_v1(
                &negative,
                &positive,
                &memory,
                MemoryClass::Ordinary,
                &mut work(),
                || false
            )
            .unwrap()
        );
        assert!(
            !exact_authorization_json_match_v1(
                &serde_json::json!(1),
                &serde_json::json!(1.0),
                &memory,
                MemoryClass::Ordinary,
                &mut work(),
                || false
            )
            .unwrap()
        );
        assert!(
            exact_authorization_json_match_v1(
                &negative,
                &negative,
                &memory,
                MemoryClass::Ordinary,
                &mut work(),
                || false
            )
            .unwrap()
        );
        let mut short = work();
        short.json_nodes_remaining = 0;
        assert!(
            exact_authorization_json_match_v1(
                &negative,
                &negative,
                &memory,
                MemoryClass::Ordinary,
                &mut short,
                || false
            )
            .is_err()
        );
        assert!(matches!(
            exact_authorization_json_match_v1(
                &negative,
                &negative,
                &memory,
                MemoryClass::Ordinary,
                &mut work(),
                || true
            ),
            Err(ValueTreeError::Cancelled)
        ));
    }
}
