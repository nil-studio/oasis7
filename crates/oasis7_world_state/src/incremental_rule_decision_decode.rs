//! Bounded VM emit payload projection, never an original or permission proof.
use crate::rules::{RuleDecision, RuleDecisionRecord};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::{
    cbor_decode_preflight::{
        CborPreflightErrorV1, CborPreflightWorkV1, preflight_definite_cbor_v1,
    },
    memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue},
};
const _: () =
    assert!(std::mem::size_of::<RuleDecision>() <= std::mem::size_of::<RuleDecisionRecord>());
pub struct RuleDecisionPayloadDecodeWorkV1 {
    pub structure: CborPreflightWorkV1,
    pub max_payload_bytes: usize,
    pub max_heap_bytes: usize,
}
/// Owned plain payload with its admission; never a qualification or fact.
/// Callers moving subfields must retain the fee until all moved data is
/// destroyed. Merge-created containers need independently admitted scratch.
pub struct OwnedRuleDecisionPayloadV1 {
    decision: RuleDecision,
    fee: MemoryReservation,
}
impl OwnedRuleDecisionPayloadV1 {
    pub(crate) fn from_admitted_parts(decision: RuleDecision, fee: MemoryReservation) -> Self {
        Self { decision, fee }
    }
    pub fn into_parts(self) -> (RuleDecision, MemoryReservation) {
        (self.decision, self.fee)
    }
}
pub fn decode_rule_decision_payload_v1(
    raw: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut RuleDecisionPayloadDecodeWorkV1,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuleDecision>, ValueTreeError> {
    let (decision, fee) =
        decode_rule_decision_payload_owned_v1(raw, memory, class, work, cancelled)?.into_parts();
    Ok(fee.retain(decision))
}
pub fn decode_rule_decision_payload_owned_v1(
    raw: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut RuleDecisionPayloadDecodeWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<OwnedRuleDecisionPayloadV1, ValueTreeError> {
    let charge = admit_rule_decision_payload_v1(raw, memory, class, work, &mut cancelled)?;
    let value = serde_cbor::from_slice::<RuleDecision>(raw)
        .map_err(|_| ValueTreeError::Invalid("rule payload type"))?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(OwnedRuleDecisionPayloadV1 {
        decision: value,
        fee: charge,
    })
}
pub(crate) fn admit_rule_decision_payload_v1(
    raw: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut RuleDecisionPayloadDecodeWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<MemoryReservation, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if class != MemoryClass::Ordinary {
        return Err(ValueTreeError::Invalid("rule payload class"));
    }
    crate::incremental_world_event_decode::verify_module_output_layout_inventory_v1(memory, class)?;
    if raw.len() > work.max_payload_bytes {
        return Err(ValueTreeError::ResourceLimited("rule payload bytes"));
    }
    let shape = preflight_definite_cbor_v1(raw, &mut work.structure, &mut cancelled).map_err(
        |e| match e {
            CborPreflightErrorV1::Cancelled => ValueTreeError::Cancelled,
            CborPreflightErrorV1::Invalid => ValueTreeError::Invalid("rule payload CBOR"),
            CborPreflightErrorV1::ResourceLimited => {
                ValueTreeError::ResourceLimited("rule payload structure")
            }
        },
    )?;
    work.structure.values_remaining = work
        .structure
        .values_remaining
        .checked_sub(shape.values)
        .ok_or(ValueTreeError::ResourceLimited("rule typed visits"))?;
    work.structure.bytes_remaining = work
        .structure
        .bytes_remaining
        .checked_sub(raw.len() as u64)
        .ok_or(ValueTreeError::ResourceLimited("rule typed bytes"))?;
    let bytes = crate::incremental_world_event_decode::shared_owned_heap_bound_v1(
        &shape,
        std::mem::size_of::<RuleDecision>(),
    )?;
    if bytes > work.max_heap_bytes {
        return Err(ValueTreeError::ResourceLimited("rule payload heap"));
    }
    let charge = memory.reserve(bytes, class)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge)
}
#[cfg(test)]
#[path = "incremental_rule_decision_decode_tests.rs"]
mod tests;
