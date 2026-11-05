//! Explicit one-candidate work ownership. Exhausted accounts are never reset.
use crate::incremental_page_store::PageIoBudget;
use oasis7_formal_protocol::incremental_value::ValueIoBudget;
use oasis7_world_state::{
    incremental_cognition_history::CognitionHistoryIndexWorkV1,
    incremental_cognition_history_cbor::CognitionHistoryCborWorkV1,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_cognition_lifecycle_index_v2::CognitionLifecycleWorkV2,
    incremental_field_member::MemberRecordLimitsV1,
    incremental_runtime_history_source::RuntimeHistoryCandidateReadWorkV1,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_result_router::WorldResultRouterWorkV1,
};
/// Caller supplies every allowance. This owns work only, never a proof or
/// authority. A retry requires newly explicit accounts, not an internal reset.
pub struct IwsCandidateSourceWorkV1 {
    pub router: WorldResultRouterWorkV1,
    pub runtime: RuntimeHistoryCandidateReadWorkV1,
    pub cognition_index: CognitionHistoryIndexWorkV1,
    pub cognition_cbor: CognitionHistoryCborWorkV1,
    pub cognition_hash: CognitionHeadHashWorkV1,
    pub cognition_lifecycle: CognitionLifecycleWorkV2,
    pub history_physical: PageIoBudget,
    pub history_values: ValueIoBudget,
    pub persist_physical: PageIoBudget,
    pub persist_values: ValueIoBudget,
    pub write_physical: PageIoBudget,
    pub write_values: ValueIoBudget,
    pub write_keys: SemanticKeyBudget,
    pub write_limits: MemberRecordLimitsV1,
    pub history_occurrences: u64,
    pub history_hash_bytes: u64,
    pub persist_occurrences: u64,
    /// Native/artifact phase runs before Current's large read/sync memos.
    pub artifacts: oasis7_formal_protocol::incremental_material::ExecutionArtifactBudget,
    pub native_chunk_occurrences: u64,
    pub native_chunk_bytes: u64,
    pub frozen_wire_bytes: u64,
    /// All retained originals share these two continuous logical counters,
    /// including Cognition pull reads; physical accounts are explicit by lane.
    pub original_sync_occurrences: u64,
    pub original_sync_bytes: u64,
    pub original_sync_physical: PageIoBudget,
    pub original_sync_values: ValueIoBudget,
    pub cognition_sync_physical: PageIoBudget,
    pub cognition_sync_values: ValueIoBudget,
    pub publish_physical: PageIoBudget,
    pub max_synced_pages: usize,
    pub sync_memo_steps: u64,
    pub catalog_read_physical: PageIoBudget,
    pub catalog_publish_physical: PageIoBudget,
    pub catalog_occurrences: u64,
    pub transition_scan: oasis7_formal_protocol::incremental_state::StateScanBudget,
    pub transition_occurrences: u64,
}
