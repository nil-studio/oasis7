//! Normal projection-root producer's readonly source material. Neither a
//! column seed nor a digest is execution/accepted/durable qualification.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::incremental_projection::{
    FieldDescriptor, FieldOwner, GenesisFieldVisitor,
};
#[path = "incremental_normal_projection_commitment_inventory.rs"]
mod inventory;
pub(in crate::world) use inventory::*;
#[path = "incremental_normal_projection_commitment_initial.rs"]
mod initial;
pub(in crate::world) use initial::{
    NormalProjectionColumnInitializerV1, NormalProjectionColumnSeedDataV1,
    ProjectionOriginalColumnFieldV1, compare_projection_original_keys_v1,
};
fn invalid(note: &'static str) -> BodyMutationError {
    BodyMutationError::Invalid(note)
}
fn paid(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}

#[path = "incremental_normal_projection_commitment_json_fragment.rs"]
mod json_fragment;
#[path = "incremental_normal_projection_commitment_original_rows.rs"]
mod original_rows;
#[path = "incremental_normal_projection_commitment_owner.rs"]
mod owner;
#[path = "incremental_normal_projection_commitment_raw_hash.rs"]
mod raw_hash;
#[path = "incremental_normal_projection_commitment_state_columns.rs"]
mod state_columns;
#[path = "incremental_normal_projection_commitment_template.rs"]
mod template;
pub use owner::AcceptedNormalProjectionCacheV1;
pub(in crate::world) use owner::{
    ActualCurrentStateHashSnapshotV1, FrozenNormalProjectionCommitmentV1,
};
#[path = "incremental_normal_projection_commitment_work.rs"]
mod work;
pub use work::{NormalProjectionCurrentWorkV1, NormalProjectionWorkBankV1};

#[path = "incremental_normal_projection_commitment_nested_rows.rs"]
mod nested_rows;

#[path = "incremental_normal_projection_commitment_electricity.rs"]
mod electricity;

#[path = "incremental_normal_projection_commitment_world_rule_rows.rs"]
mod world_rule_rows;

#[path = "incremental_normal_projection_commitment_journal.rs"]
mod journal;
pub(in crate::world) use journal::{
    ActualCurrentJournalProjectionV1, prepare_cold_journal_projection_v1,
    prepare_current_journal_projection_v1,
};

#[path = "incremental_normal_projection_commitment_fold.rs"]
mod fold;
#[path = "incremental_normal_projection_commitment_sources.rs"]
mod sources;

#[path = "incremental_normal_projection_commitment_observation.rs"]
mod observation;
#[path = "incremental_normal_projection_commitment_shared_json.rs"]
mod shared_json;

#[path = "incremental_normal_projection_commitment_fact_decode.rs"]
mod fact_decode;
#[path = "incremental_normal_projection_commitment_facts.rs"]
mod facts;

#[path = "incremental_normal_projection_commitment_coverage.rs"]
mod coverage;

/// Pure paid JSON DATA from the unique count-first original encoder. Source
/// qualification and its exact object/output limits stay with the fixed caller.
pub(in crate::world) fn capture_projection_json_data_v1<T: serde::Serialize + ?Sized>(
    value: &T,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
    initial::capture(common, |out| {
        serde_json::to_writer(out, value).map_err(paid)
    })
}

#[path = "incremental_normal_projection_commitment_governance_rows.rs"]
mod governance_rows;

#[path = "incremental_normal_projection_commitment_governance_fold.rs"]
mod governance_fold;
#[path = "incremental_normal_projection_commitment_governance_observation.rs"]
mod governance_observation;

#[path = "incremental_normal_projection_commitment_audiences.rs"]
mod audiences;
#[path = "incremental_normal_projection_commitment_format.rs"]
mod format;
#[path = "incremental_normal_projection_commitment_leaf.rs"]
mod leaf;
pub use leaf::genesis::PaidGenesisProjectionCommitmentV1;
pub(in crate::world) use leaf::genesis::{
    GenesisProjectionOriginalsV1, capture_genesis_projection_originals_v1,
    prepare_genesis_projection_commitment_v1,
};
#[path = "incremental_normal_projection_commitment_leaf_hash.rs"]
mod leaf_hash;
#[path = "incremental_normal_projection_commitment_visible_state.rs"]
mod visible_state;

#[path = "incremental_normal_projection_commitment_diplomacy_fold.rs"]
mod diplomacy_fold;
#[path = "incremental_normal_projection_commitment_diplomacy_observation.rs"]
mod diplomacy_observation;
#[path = "incremental_normal_projection_commitment_diplomacy_rows.rs"]
mod diplomacy_rows;

#[path = "incremental_normal_projection_commitment_reward_rows.rs"]
mod reward_rows;

#[path = "incremental_normal_projection_commitment_reward_fold.rs"]
mod reward_fold;

#[path = "incremental_normal_projection_commitment_market_index.rs"]
mod market_index;

#[path = "incremental_normal_projection_commitment_intents.rs"]
mod intents;

#[path = "incremental_normal_projection_commitment_market_rows.rs"]
mod market_rows;

#[path = "incremental_normal_projection_commitment_market_fold.rs"]
mod market_fold;
#[path = "incremental_normal_projection_commitment_market_observation.rs"]
mod market_observation;

#[path = "incremental_normal_projection_commitment_cold_record_maps.rs"]
mod cold_record_maps;

#[path = "incremental_normal_projection_commitment_cold.rs"]
mod cold;
#[path = "incremental_normal_projection_commitment_cold_assemble.rs"]
mod cold_assemble;
#[path = "incremental_normal_projection_commitment_cold_sources.rs"]
mod cold_sources;
pub(in crate::world) use cold::{
    QualifiedColdProjectionSeedV1, QualifiedColdProjectionSourceFeesV1,
};
