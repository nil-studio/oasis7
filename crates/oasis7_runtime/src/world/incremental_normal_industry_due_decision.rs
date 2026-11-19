//! Bounded original ProductValidationDecision deserialization from one real
//! worker emit. This produces only data; actual receipt/state/cap gates remain
//! in the fixed due dispatcher. No Value clone or replacement JSON parser.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::MemoryClass;
use serde::Deserialize;
use std::mem::size_of;
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("due product decision layout").into()
}
fn charge(common: &mut FormalBoundedContextV1<'_>, bytes: usize) -> Result<(), BodyMutationError> {
    common
        .charge(1, u64::try_from(bytes).map_err(|_| limited())?)
        .map_err(|error| BodyMutationError::Original(Box::new(error)))
}
fn walk(
    value: &serde_json::Value,
    common: &mut FormalBoundedContextV1<'_>,
    text: &mut usize,
) -> Result<(), BodyMutationError> {
    charge(common, 0)?;
    match value {
        serde_json::Value::String(value) => {
            charge(common, value.len())?;
            *text = text.checked_add(value.len()).ok_or_else(limited)?;
        }
        serde_json::Value::Array(values) => {
            for value in values {
                walk(value, common, text)?;
            }
        }
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                charge(common, key.len())?;
                *text = text.checked_add(key.len()).ok_or_else(limited)?;
                walk(value, common, text)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(super) enum ProductDecisionPayloadV1 {
    ForbiddenEffects,
    Missing,
    Duplicate,
    Decoded(ReservedValue<Result<oasis7_wasm_abi::ProductValidationDecision, serde_json::Error>>),
}
pub(super) fn decode_actual_product_decision_v1(
    output: &oasis7_wasm_abi::ModuleOutput,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ProductDecisionPayloadV1, BodyMutationError> {
    charge(common, 0)?;
    if !output.effects.is_empty() {
        return Ok(ProductDecisionPayloadV1::ForbiddenEffects);
    }
    let mut selected = None;
    for emit in &output.emits {
        charge(common, emit.kind.len())?;
        if emit.kind != crate::world::economy::PRODUCT_VALIDATION_EMIT_KIND {
            continue;
        }
        if selected.is_some() {
            return Ok(ProductDecisionPayloadV1::Duplicate);
        }
        selected = Some(&emit.payload);
    }
    let Some(payload) = selected else {
        return Ok(ProductDecisionPayloadV1::Missing);
    };
    let mut text = 0usize;
    walk(payload, common, &mut text)?;
    let mut slots = 0usize;
    for field in ["notes", "quality_levels"] {
        let count = payload
            .get(field)
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        // Locked serde cautious initial Vec cap plus both buffers live at
        // geometric growth: initial<=n and old+new<3n. No framecap is used as
        // the DTO heap allowance, and wrong types still use the old codec.
        slots = slots
            .checked_add(count.checked_mul(3).ok_or_else(limited)?)
            .ok_or_else(limited)?;
    }
    // All copied string payload is bounded by actual input text. A serde
    // invalid-string diagnostic may quote/escape one actual string (6x),
    // overlap old/new growable buffers, and coexist with partial typed
    // strings. The established 24n bridge bound dominates that peak.
    // Static scalar/type/missing-field templates plus
    // decoder/error/carrier headers are admitted independently.
    let bytes = text
        .checked_mul(24)
        .and_then(|n| n.checked_add(slots.checked_mul(size_of::<String>())?))
        .and_then(|n| {
            n.checked_add(
                4096 + size_of::<
                    ReservedValue<
                        Result<oasis7_wasm_abi::ProductValidationDecision, serde_json::Error>,
                    >,
                >() + size_of::<serde_json::Error>(),
            )
        })
        .ok_or_else(limited)?;
    charge(common, bytes)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let decoded = oasis7_wasm_abi::ProductValidationDecision::deserialize(payload);
    Ok(ProductDecisionPayloadV1::Decoded(fee.retain(decoded)))
}
