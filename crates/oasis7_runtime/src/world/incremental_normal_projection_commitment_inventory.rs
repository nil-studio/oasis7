//! Complete original read inventory. These descriptors select codecs, never
//! authenticate bytes. The actual GEN/cold/Current source token is separate.
use oasis7_world_state::incremental_projection::{FieldDescriptor,SnapshotFieldLink,FieldDestination,WORLD_STATE_FIELDS,SNAPSHOT_FIELDS};
/// All semantic State fields are needed by RuntimeBinding.base_world_hash,
/// including fields deliberately omitted from the visible state allowlist.
pub(in crate::world) fn semantic_state_columns_v1()->&'static[FieldDescriptor]{WORLD_STATE_FIELDS}
/// Original explicit public/state allowlist. Agent/site/order visibility is
/// applied by the fixed committed formatter; this list never grants access.
pub(in crate::world) const VISIBLE_STATE_COLUMNS_V1:&[&str]=&[
 "world_rules","time","agents","resources","materials","material_ledgers","material_profiles","logistics_routes","factories","recipe_profiles","factory_profiles","product_profiles","latest_product_validation","pending_material_transits","pending_factory_builds","factory_site_authorities","agent_location_authorities","industry_progress","alliances","gameplay_policy","agent_claims","agent_claim_acquisition_sequences","starter_oc_claims","reputation_scores","wars","governance_votes","governance_proposals","crises","meta_progress","module_visual_entities","main_token_config","main_token_supply","factory_construction_power_profiles","location_anchors",
];
/// The link names are selected from the original exhaustive Snapshot registry.
/// module bytecode/process credentials and full historical Tick arrays are not
/// formatting inputs and must not be hydrated into the column cache.
const RUNTIME_READ_LINKS:&[&str]=&[
 "manifest","cognition","chain_resource_manifest","module_registry","capability_grants_v2","capability_revocation_state","capability_invocation_contexts","capability_authorization_root","policies","next_event_id","event_id_era","governance_execution_policy","governance_finality_epoch_snapshots","governance_emergency_brake_until_tick","governance_identity_penalties","tick_consensus_authority_source",
];
pub(in crate::world) fn runtime_projection_columns_v1()->impl Iterator<Item=&'static FieldDescriptor>{
 crate::World::iws_runtime_fields().iter().filter(|field|!matches!(field.destination,FieldDestination::Process)&&(runtime_projection_links_v1().any(|link|link.runtime_field==field.name)||matches!(field.name,"journal"|"next_event_id"|"next_event_id_era")))
}
pub(in crate::world) fn runtime_projection_links_v1()->impl Iterator<Item=&'static SnapshotFieldLink>{SNAPSHOT_FIELDS.iter().filter(|link|RUNTIME_READ_LINKS.contains(&link.name))}
/// Whole Cognition input registry: actual disabled/absent profile is retained.
/// Economy Journal/Provision and regular Journal are independent actual lanes.
pub(in crate::world) fn required_cognition_columns_v1()->&'static[&'static str]{&oasis7_world_state::incremental_cognition_fields::REQUIRED_COGNITION_FIELDS}
pub(in crate::world) fn optional_cognition_columns_v1()->&'static[&'static str]{&oasis7_world_state::incremental_cognition_fields::OPTIONAL_COGNITION_FIELDS}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub(in crate::world) enum ProjectionHistoryInputV1{
 /// Retained hot WorldJournal plus authenticated retention/cold prefixes. The
 /// formatter's latest-era suffix and domain-agent filter stay in one kernel.
 WorldJournal,
 /// Actual retained canonical world_service_results, including original
 /// correlation keys, receipt positions and game-task/Agent-chat lookups.
 CanonicalServiceResults,
 /// ClosedTask/current-task rows are matched exactly, not inferred from the
 /// latest lifecycle record or from an Agent's current intent alone.
 TaskTerminalAndCurrent,
 /// Receipt-lineage and feedback originals remain genuine separate inputs.
 FeedbackAndReceiptLineage,
}
pub(in crate::world) const HISTORY_INPUTS_V1:&[ProjectionHistoryInputV1]=&[ProjectionHistoryInputV1::WorldJournal,ProjectionHistoryInputV1::CanonicalServiceResults,ProjectionHistoryInputV1::TaskTerminalAndCurrent,ProjectionHistoryInputV1::FeedbackAndReceiptLineage];
