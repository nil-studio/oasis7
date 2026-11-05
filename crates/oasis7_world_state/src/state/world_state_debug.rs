//! Preserve legacy Debug output when optional rotation history is empty.
use super::WorldState;
impl std::fmt::Debug for WorldState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut output = f.debug_struct("WorldState");
        output.field("world_rules", &self.world_rules);
        output.field("time", &self.time);
        output.field("retained_history_root", &self.retained_history_root);
        output.field("agents", &self.agents);
        output.field("agent_intent_ledger", &self.agent_intent_ledger);
        output.field("resources", &self.resources);
        output.field("materials", &self.materials);
        output.field("material_ledgers", &self.material_ledgers);
        output.field("material_profiles", &self.material_profiles);
        output.field("logistics_routes", &self.logistics_routes);
        output.field(
            "completed_logistics_route_ids",
            &self.completed_logistics_route_ids,
        );
        output.field("completed_logistics_paths", &self.completed_logistics_paths);
        output.field(
            "settled_logistics_transit_ids",
            &self.settled_logistics_transit_ids,
        );
        output.field(
            "logistics_settlement_receipts",
            &self.logistics_settlement_receipts,
        );
        output.field(
            "factory_production_failure_dispositions",
            &self.factory_production_failure_dispositions,
        );
        output.field(
            "direct_material_transfer_receipts",
            &self.direct_material_transfer_receipts,
        );
        output.field(
            "product_validation_receipts",
            &self.product_validation_receipts,
        );
        output.field("product_profiles", &self.product_profiles);
        output.field("latest_product_validation", &self.latest_product_validation);
        output.field("recipe_profiles", &self.recipe_profiles);
        output.field("factory_profiles", &self.factory_profiles);
        output.field(
            "agent_location_authorities",
            &self.agent_location_authorities,
        );
        output.field("location_anchors", &self.location_anchors);
        output.field("factory_site_authorities", &self.factory_site_authorities);
        output.field(
            "factory_construction_power_profiles",
            &self.factory_construction_power_profiles,
        );
        output.field("factories", &self.factories);
        output.field("retired_factory_ids", &self.retired_factory_ids);
        output.field("settled_factory_build_ids", &self.settled_factory_build_ids);
        output.field(
            "factory_construction_receipts",
            &self.factory_construction_receipts,
        );
        output.field(
            "product_validation_attempts",
            &self.product_validation_attempts,
        );
        output.field(
            "product_validation_delivery_cursor",
            &self.product_validation_delivery_cursor,
        );
        output.field("pending_factory_builds", &self.pending_factory_builds);
        output.field("pending_recipe_jobs", &self.pending_recipe_jobs);
        output.field("settled_recipe_job_ids", &self.settled_recipe_job_ids);
        output.field(
            "next_industry_settlement_order",
            &self.next_industry_settlement_order,
        );
        output.field(
            "industry_settlement_orders",
            &self.industry_settlement_orders,
        );
        output.field(
            "recipe_completion_receipts",
            &self.recipe_completion_receipts,
        );
        output.field("factory_recycle_receipts", &self.factory_recycle_receipts);
        output.field("pending_material_transits", &self.pending_material_transits);
        output.field("industry_progress", &self.industry_progress);
        output.field("alliances", &self.alliances);
        output.field("gameplay_policy", &self.gameplay_policy);
        output.field("data_access_permissions", &self.data_access_permissions);
        output.field("economic_contracts", &self.economic_contracts);
        output.field("agent_claims", &self.agent_claims);
        output.field(
            "agent_claim_acquisition_sequences",
            &self.agent_claim_acquisition_sequences,
        );
        output.field("starter_oc_claims", &self.starter_oc_claims);
        output.field(
            "authenticated_collect_data_last_nonces",
            &self.authenticated_collect_data_last_nonces,
        );
        output.field(
            "agent_claim_last_processed_epoch",
            &self.agent_claim_last_processed_epoch,
        );
        output.field(
            "contract_pair_last_success_settled_at",
            &self.contract_pair_last_success_settled_at,
        );
        output.field(
            "reputation_reward_window_started_at",
            &self.reputation_reward_window_started_at,
        );
        output.field(
            "reputation_reward_window_accumulated",
            &self.reputation_reward_window_accumulated,
        );
        output.field("reputation_scores", &self.reputation_scores);
        output.field("wars", &self.wars);
        output.field("governance_votes", &self.governance_votes);
        output.field("governance_proposals", &self.governance_proposals);
        output.field(
            "governance_identity_profiles",
            &self.governance_identity_profiles,
        );
        output.field("crises", &self.crises);
        output.field("meta_progress", &self.meta_progress);
        output.field("module_states", &self.module_states);
        output.field("module_visual_entities", &self.module_visual_entities);
        output.field("module_artifact_owners", &self.module_artifact_owners);
        output.field("module_artifact_listings", &self.module_artifact_listings);
        output.field("module_artifact_bids", &self.module_artifact_bids);
        output.field("module_instances", &self.module_instances);
        output.field("module_admission_freezes", &self.module_admission_freezes);
        output.field("module_release_requests", &self.module_release_requests);
        output.field(
            "module_release_manifest_mappings",
            &self.module_release_manifest_mappings,
        );
        output.field(
            "next_module_release_request_id",
            &self.next_module_release_request_id,
        );
        output.field(
            "module_release_role_bindings",
            &self.module_release_role_bindings,
        );
        output.field("installed_module_targets", &self.installed_module_targets);
        output.field("next_module_instance_id", &self.next_module_instance_id);
        output.field(
            "next_module_market_order_id",
            &self.next_module_market_order_id,
        );
        output.field(
            "next_module_market_sale_id",
            &self.next_module_market_sale_id,
        );
        output.field("main_token_config", &self.main_token_config);
        output.field("main_token_supply", &self.main_token_supply);
        output.field("main_token_balances", &self.main_token_balances);
        output.field(
            "restricted_starter_claim_grants",
            &self.restricted_starter_claim_grants,
        );
        output.field(
            "main_token_genesis_buckets",
            &self.main_token_genesis_buckets,
        );
        output.field(
            "main_token_epoch_issuance_records",
            &self.main_token_epoch_issuance_records,
        );
        output.field(
            "main_token_treasury_balances",
            &self.main_token_treasury_balances,
        );
        output.field("main_token_claim_nonces", &self.main_token_claim_nonces);
        output.field(
            "main_token_transfer_nonces",
            &self.main_token_transfer_nonces,
        );
        output.field(
            "main_token_scheduled_policy_updates",
            &self.main_token_scheduled_policy_updates,
        );
        output.field(
            "main_token_node_points_bridge_records",
            &self.main_token_node_points_bridge_records,
        );
        output.field(
            "main_token_treasury_distribution_records",
            &self.main_token_treasury_distribution_records,
        );
        output.field(
            "restricted_starter_claim_liveops_pool_top_up_records",
            &self.restricted_starter_claim_liveops_pool_top_up_records,
        );
        output.field("reward_asset_config", &self.reward_asset_config);
        output.field("node_asset_balances", &self.node_asset_balances);
        output.field("protocol_power_reserve", &self.protocol_power_reserve);
        output.field("reward_mint_records", &self.reward_mint_records);
        output.field("node_redeem_nonces", &self.node_redeem_nonces);
        output.field("system_order_pool_budgets", &self.system_order_pool_budgets);
        output.field("node_identity_bindings", &self.node_identity_bindings);
        output.field(
            "node_main_token_account_bindings",
            &self.node_main_token_account_bindings,
        );
        output.field(
            "governance_finality_signer_registry",
            &self.governance_finality_signer_registry,
        );
        output.field(
            "governance_validator_admissions",
            &self.governance_validator_admissions,
        );
        if !self.governance_finality_key_rotations.is_empty() {
            output.field(
                "governance_finality_key_rotations",
                &self.governance_finality_key_rotations,
            );
        }
        output.field(
            "governance_main_token_controller_registry",
            &self.governance_main_token_controller_registry,
        );
        output.field(
            "governance_authorization_nonces",
            &self.governance_authorization_nonces,
        );
        output.field(
            "reward_signature_governance_policy",
            &self.reward_signature_governance_policy,
        );
        output.finish()
    }
}
