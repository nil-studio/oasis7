//! JSON-semantics projection of a borrowed VM emit payload; no authority proof.
use crate::incremental_rule_decision_decode::{
    OwnedRuleDecisionPayloadV1, RuleDecisionPayloadDecodeWorkV1, admit_rule_decision_payload_v1,
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use oasis7_wasm_abi::{
    CanonicalCborStreamContextV1, CanonicalCborStreamError, CanonicalCborWorkV1,
    stream_canonical_cbor_v1,
};
use serde::Deserialize;
use std::io::Write;
pub struct OwnedRuleDecisionJsonErrorV1 {
    error: serde_json::Error,
    _fee: MemoryReservation,
}
impl std::fmt::Display for OwnedRuleDecisionJsonErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::fmt::Debug for OwnedRuleDecisionJsonErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.error, f)
    }
}
#[derive(Debug)]
pub enum RuleDecisionJsonDecodeErrorV1 {
    Host(ValueTreeError),
    Payload(OwnedRuleDecisionJsonErrorV1),
}
impl From<ValueTreeError> for RuleDecisionJsonDecodeErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Host(e)
    }
}
struct Raw {
    bytes: Vec<u8>,
    max: usize,
}
impl Write for Raw {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(b.len())
            .is_none_or(|n| n > self.max)
        {
            return Err(std::io::Error::other("rule payload bytes"));
        }
        self.bytes.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn decode_rule_decision_json_payload_owned_v1(
    payload: &serde_json::Value,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut RuleDecisionPayloadDecodeWorkV1,
    canonical: &mut CanonicalCborWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<OwnedRuleDecisionPayloadV1, RuleDecisionJsonDecodeErrorV1> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if class != MemoryClass::Ordinary {
        return Err(ValueTreeError::Invalid("rule payload class").into());
    }
    let _raw_fee = memory
        .reserve(work.max_payload_bytes, class)
        .map_err(ValueTreeError::Memory)?;
    let mut raw = Raw {
        bytes: Vec::new(),
        max: work.max_payload_bytes,
    };
    raw.bytes
        .try_reserve_exact(raw.max)
        .map_err(|_| ValueTreeError::ResourceLimited("rule transport allocation"))?;
    if raw.bytes.capacity() > raw.max {
        return Err(ValueTreeError::ResourceLimited("rule transport capacity").into());
    }
    let allocator = crate::incremental_original_decode::Allocator::new(memory, class);
    let empty = CanonicalCborWorkV1 {
        visits_remaining: 0,
        bytes_remaining: 0,
        max_depth: canonical.max_depth,
        max_map_keys: canonical.max_map_keys,
        max_key_bytes: canonical.max_key_bytes,
    };
    let (result, spent) = {
        let mut context = CanonicalCborStreamContextV1::new(
            &allocator,
            std::mem::replace(canonical, empty),
            &mut cancelled,
        );
        let result = stream_canonical_cbor_v1(payload, &mut context, &mut raw);
        (result, context.work)
    };
    *canonical = spent;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(e) = allocator.take_error() {
        return Err(ValueTreeError::Memory(e).into());
    }
    match result {
        Ok(summary) if summary.encoded_bytes == raw.bytes.len() as u64 => {}
        Err(CanonicalCborStreamError::ResourceLimited(s)) => {
            return Err(ValueTreeError::ResourceLimited(s).into());
        }
        Err(CanonicalCborStreamError::Cancelled) => return Err(ValueTreeError::Cancelled.into()),
        _ => return Err(ValueTreeError::ResourceLimited("rule transport bytes").into()),
    }
    let fee = admit_rule_decision_payload_v1(&raw.bytes, memory, class, work, &mut cancelled)?;
    let decoded = crate::rules::RuleDecision::deserialize(payload);
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    match decoded {
        Ok(decision) => Ok(OwnedRuleDecisionPayloadV1::from_admitted_parts(
            decision, fee,
        )),
        Err(error) => Err(RuleDecisionJsonDecodeErrorV1::Payload(
            OwnedRuleDecisionJsonErrorV1 { error, _fee: fee },
        )),
    }
}

#[cfg(test)]
#[path = "incremental_rule_decision_json_decode_tests.rs"]
mod tests;
