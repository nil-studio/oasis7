//! Closed SafeAlternative owned layout admission. No allocation or permission
//! is inferred from a caller's byte count or boolean.
use super::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use serde_json::Value;
use std::{
    mem::{align_of, size_of},
    ptr::NonNull,
};
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("SafeAlternative owned layout").into()
}
#[derive(Default)]
struct Shape {
    values: usize,
    seq: usize,
    map: usize,
    strings: usize,
    depth: usize,
}
fn visit(
    value: &Value,
    depth: usize,
    s: &mut Shape,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    common
        .consume_steps(1)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    if depth > 64 {
        return Err(limited());
    }
    s.depth = s.depth.max(depth);
    s.values = s.values.checked_add(1).ok_or_else(limited)?;
    match value {
        Value::String(v) => {
            common
                .consume_read_bytes(v.len() as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            s.strings = s.strings.checked_add(v.len()).ok_or_else(limited)?;
        }
        Value::Array(v) => {
            s.seq = s.seq.checked_add(v.len()).ok_or_else(limited)?;
            for child in v {
                visit(child, depth + 1, s, common)?;
            }
        }
        Value::Object(v) => {
            s.map = s.map.checked_add(v.len()).ok_or_else(limited)?;
            for (key, child) in v {
                common
                    .consume_read_bytes(key.len() as u64)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                s.strings = s.strings.checked_add(key.len()).ok_or_else(limited)?;
                visit(child, depth + 1, s, common)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn node<K, V>() -> Option<usize> {
    let a = align_of::<K>()
        .max(align_of::<V>())
        .max(align_of::<Option<NonNull<()>>>())
        .max(align_of::<u16>());
    (size_of::<K>() + size_of::<V>())
        .checked_mul(11)?
        .checked_add(
            size_of::<Option<NonNull<()>>>() + 2 * size_of::<u16>() + 12 * size_of::<NonNull<()>>(),
        )?
        .checked_add((a - 1) * 7)
}
pub(super) fn safe_alternative_owned_bound_v1(
    value: &Value,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, BodyMutationError> {
    let mut s = Shape::default();
    visit(value, 0, &mut s, common)?;
    // serde229's private Content is deliberately named: upgrading its layout
    // cannot silently keep this closed bound. Only Vec<GeoPos> is reachable in
    // the signed command graph; Value and Content additionally buffer JSON.
    let content = size_of::<serde::__private229::de::Content<'static>>();
    let seq = size_of::<oasis7_agent_api::world_action::GeoPos>()
        .max(size_of::<Value>())
        .max(content)
        .checked_mul(4)
        .ok_or_else(limited)?;
    let pair = size_of::<(
        serde::__private229::de::Content<'static>,
        serde::__private229::de::Content<'static>,
    )>();
    let map = node::<String, Value>()
        .and_then(|n| pair.checked_mul(4).and_then(|p| n.checked_add(p)))
        .ok_or_else(limited)?;
    let token = size_of::<oasis7_client_api::safe_alternative::SafeAlternativeCommandV1>()
        .checked_add(content)
        .and_then(|n| n.checked_add(size_of::<Value>()))
        .ok_or_else(limited)?;
    // Two simultaneous JSON escaping buffers use at most six bytes per input
    // byte each, plus two owned UTF8 copies. Each nested Content clone stage is
    // separately admitted, as are the original target and two boundary stages.
    let phase = s
        .seq
        .checked_mul(seq)
        .and_then(|n| s.map.checked_mul(map).and_then(|m| n.checked_add(m)))
        .and_then(|n| s.values.checked_mul(token).and_then(|v| n.checked_add(v)))
        .and_then(|n| {
            s.strings
                .checked_mul(6 + 6 + 2)
                .and_then(|v| n.checked_add(v))
        })
        .ok_or_else(limited)?;
    phase
        .checked_mul(s.depth.checked_add(4).ok_or_else(limited)?)
        .and_then(|n| n.checked_add(node::<u64, ()>()?))
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(limited)
}
