use super::*;
use crate::incremental_projection::{FieldDestination, FieldOwner};
pub(super) const FIELD_FAMILIES: [(&str, &str); 98] = [
    ("world_rules", "world_rules13"),
    ("time", "primitive29"),
    ("agent_intent_ledger", "agent_authority3"),
    ("resources", "inventory3"),
    ("materials", "inventory3"),
    ("material_ledgers", "inventory3"),
    ("material_profiles", "material2"),
    ("logistics_routes", "logistics2"),
    ("completed_logistics_route_ids", "primitive29"),
    ("completed_logistics_paths", "logistics2"),
    ("settled_logistics_transit_ids", "primitive29"),
    ("logistics_settlement_receipts", "industry5"),
    ("factory_production_failure_dispositions", "industry5"),
    ("direct_material_transfer_receipts", "material2"),
    ("product_validation_receipts", "native24"),
    ("product_profiles", "native24"),
    ("latest_product_validation", "native24"),
    ("recipe_profiles", "native24"),
    ("factory_profiles", "native24"),
    ("agent_location_authorities", "agent_authority3"),
    ("location_anchors", "agent_authority3"),
    ("factory_site_authorities", "factory_authority2"),
    ("factory_construction_power_profiles", "factory_authority2"),
    ("factories", "factory2"),
    ("retired_factory_ids", "primitive29"),
    ("settled_factory_build_ids", "primitive29"),
    ("factory_construction_receipts", "native24"),
    ("product_validation_attempts", "native24"),
    ("pending_factory_builds", "factory2"),
    ("pending_recipe_jobs", "industry5"),
    ("settled_recipe_job_ids", "primitive29"),
    ("next_industry_settlement_order", "primitive29"),
    ("industry_settlement_orders", "primitive29"),
    ("recipe_completion_receipts", "industry5"),
    ("factory_recycle_receipts", "native24"),
    ("pending_material_transits", "industry5"),
    ("industry_progress", "native24"),
    ("alliances", "diplomacy4"),
    ("gameplay_policy", "native24"),
    ("economic_contracts", "diplomacy4"),
    ("agent_claims", "native24"),
    ("agent_claim_acquisition_sequences", "primitive29"),
    ("starter_oc_claims", "native24"),
    ("agent_claim_last_processed_epoch", "primitive29"),
    ("contract_pair_last_success_settled_at", "primitive29"),
    ("reputation_reward_window_started_at", "primitive29"),
    ("reputation_reward_window_accumulated", "primitive29"),
    ("reputation_scores", "primitive29"),
    ("wars", "diplomacy4"),
    ("governance_votes", "governance3"),
    ("governance_proposals", "governance3"),
    ("governance_identity_profiles", "governance3"),
    ("crises", "diplomacy4"),
    ("meta_progress", "native24"),
    ("module_visual_entities", "market3"),
    ("module_artifact_owners", "primitive29"),
    ("module_artifact_listings", "market3"),
    ("module_artifact_bids", "market3"),
    ("module_admission_freezes", "native24"),
    ("module_release_requests", "native24"),
    ("module_release_manifest_mappings", "native24"),
    ("next_module_release_request_id", "primitive29"),
    ("installed_module_targets", "primitive29"),
    ("next_module_instance_id", "primitive29"),
    ("next_module_market_order_id", "primitive29"),
    ("next_module_market_sale_id", "primitive29"),
    ("main_token_config", "main_token_policy5"),
    ("main_token_supply", "fixed_economy4"),
    ("main_token_balances", "primitive29"),
    ("restricted_starter_claim_grants", "main_token_policy5"),
    ("main_token_genesis_buckets", "main_token_policy5"),
    ("main_token_epoch_issuance_records", "main_token_policy5"),
    ("main_token_treasury_balances", "primitive29"),
    ("main_token_claim_nonces", "primitive29"),
    ("main_token_transfer_nonces", "primitive29"),
    ("main_token_scheduled_policy_updates", "main_token_policy5"),
    ("main_token_node_points_bridge_records", "native24"),
    ("main_token_treasury_distribution_records", "native24"),
    (
        "restricted_starter_claim_liveops_pool_top_up_records",
        "native24",
    ),
    ("reward_asset_config", "fixed_economy4"),
    ("node_asset_balances", "primitive29"),
    ("protocol_power_reserve", "fixed_economy4"),
    ("reward_mint_records", "reward2"),
    ("node_redeem_nonces", "primitive29"),
    ("system_order_pool_budgets", "reward2"),
    ("node_identity_bindings", "primitive29"),
    ("node_main_token_account_bindings", "primitive29"),
    ("governance_finality_signer_registry", "native24"),
    ("governance_validator_admissions", "native24"),
    ("governance_finality_key_rotations", "native24"),
    ("governance_bootstrap_finality_key_rotations", "native24"),
    ("governance_main_token_controller_registry", "native24"),
    ("governance_authorization_nonces", "primitive29"),
    ("reward_signature_governance_policy", "fixed_economy4"),
    ("data_access_permissions", "controls4"),
    ("module_release_role_bindings", "controls4"),
    ("authenticated_collect_data_last_nonces", "controls4"),
    ("product_validation_delivery_cursor", "controls4"),
];
pub(super) fn check() -> Result<(), ValueTreeError> {
    let fields = crate::incremental_projection::WORLD_STATE_FIELDS;
    let mut native = 0;
    for field in fields {
        if matches!(
            field.name,
            "agents" | "module_states" | "module_instances" | "retained_history_root"
        ) {
            continue;
        }
        native += 1;
        if field.owner != FieldOwner::WorldState
            || !matches!(field.destination, FieldDestination::State(_))
            || FIELD_FAMILIES
                .iter()
                .filter(|(name, _)| *name == field.name)
                .count()
                != 1
        {
            return Err(invalid());
        }
    }
    if native != 98 {
        return Err(invalid());
    }
    for (i, (name, _)) in FIELD_FAMILIES.iter().enumerate() {
        if FIELD_FAMILIES[..i].iter().any(|(old, _)| old == name)
            || fields
                .iter()
                .filter(|f| f.name == *name && f.owner == FieldOwner::WorldState)
                .count()
                != 1
        {
            return Err(invalid());
        }
    }
    Ok(())
}
