//! Explicit finite settings for the actual complete Current consumer.
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3,
    incremental_world_current_limits::WorldCurrentStateLimitsV1,
};
/// Explicit production allowances. No default or implicit quota expansion.
#[derive(Debug, Clone, Copy)]
pub struct IwsCurrentLimitsV1 {
    pub current: WorldCurrentStateLimitsV1,
    pub scratch_pages: usize,
    pub cognition: CognitionCurrentPresenceV3,
    pub decode_bytes: u64,
    pub decode_values: u64,
    pub decode_depth: usize,
    pub decode_max_original_bytes: usize,
    pub decode_max_heap_bytes: usize,
    pub canonical_visits: u64,
    pub canonical_bytes: u64,
    pub canonical_map_keys: usize,
    pub canonical_key_bytes: usize,
}
impl IwsCurrentLimitsV1 {
    pub(super) fn candidate_mailbox(
        &self,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        class: oasis7_proto::memory_budget::MemoryClass,
    ) -> Result<super::mailbox::CandidateMailboxReader, ValueTreeError> {
        self.validate()?;
        super::mailbox::CandidateMailboxReader::new(
            oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
                structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                    bytes_remaining: self.decode_bytes,
                    values_remaining: self.decode_values,
                    max_depth: self.decode_depth,
                },
                canonical: oasis7_wasm_abi::CanonicalCborWorkV1 {
                    visits_remaining: self.canonical_visits,
                    bytes_remaining: self.canonical_bytes,
                    max_depth: self.decode_depth,
                    max_map_keys: self.canonical_map_keys,
                    max_key_bytes: self.canonical_key_bytes,
                },
                max_original_bytes: self.decode_max_original_bytes,
                max_heap_bytes: self.decode_max_heap_bytes,
            },
            self.current.mailbox_occurrences_remaining,
            self.current.records_remaining,
            memory,
            class,
        )
    }
    pub(super) fn validate(&self) -> Result<(), ValueTreeError> {
        self.current.validate()?;
        self.scratch_pages
            .checked_mul(256)
            .and_then(|n| n.checked_add(512))
            .ok_or(ValueTreeError::Invalid("endpoint scratch limit overflow"))?;
        Ok(())
    }
}
