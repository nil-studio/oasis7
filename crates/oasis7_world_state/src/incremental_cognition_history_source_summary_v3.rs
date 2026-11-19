//! Unqualified input schema for the explicit v3 source attestation.
//! Shape validity is neither a source proof nor accepted History authority.
use super::*;
use oasis7_formal_protocol::{
    incremental_material::FactLocationV1, incremental_value::ValueTreeError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionHistorySourceModeV3 {
    Bootstrap,
    Delta,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitionHeadSourceLastV3 {
    pub sequence: u64,
    pub semantic_digest: [u8; 32],
    pub location: FactLocationV1,
    pub original_bytes: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitionHistorySourceSummaryV3 {
    pub mode: CognitionHistorySourceModeV3,
    pub cache_state_digests: [Option<[u8; 32]>; 3],
    pub last: [Option<CognitionHeadSourceLastV3>; 3],
    pub lifecycle_keys: u64,
    pub lifecycle_journal_records: u64,
}
impl CognitionHistorySourceSummaryV3 {
    pub fn validate_shape(
        &self,
        view: &CognitionHistoryViewV1,
        derived: &CognitionHistoryDerivedRefsV2,
    ) -> Result<(), ValueTreeError> {
        let invalid = || ValueTreeError::Invalid("Cognition History source summary shape");
        derived.validate(view)?;
        if self.mode == CognitionHistorySourceModeV3::Delta && view.previous_view.is_none() {
            return Err(invalid());
        }
        for (slot, index) in [0, 2, 3].into_iter().enumerate() {
            let declared = &view.indexes[index];
            if self.cache_state_digests[slot].is_some() != derived.caches[slot].is_some()
                || self.last[slot].is_some() != (declared.present && declared.records != 0)
            {
                return Err(invalid());
            }
            if let Some(last) = &self.last[slot]
                && (last.sequence != declared.records || last.original_bytes == 0)
            {
                return Err(invalid());
            }
        }
        if self.lifecycle_journal_records != view.indexes[0].records
            || self.lifecycle_keys > self.lifecycle_journal_records
            || (!view.enabled && (self.lifecycle_keys != 0 || self.lifecycle_journal_records != 0))
        {
            return Err(invalid());
        }
        Ok(())
    }
}

pub const COGNITION_HISTORY_SOURCE_SUMMARY_ADMISSION_V3_BYTES: usize = std::mem::size_of::<
    CognitionHistorySourceSummaryV3,
>() + std::mem::size_of::<
    oasis7_proto::memory_budget::ReservedValue<Box<CognitionHistorySourceSummaryV3>>,
>();
