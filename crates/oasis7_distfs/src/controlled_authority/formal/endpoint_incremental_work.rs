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
}
