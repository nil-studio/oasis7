use super::*;

pub(super) fn default_world_material_ledger() -> MaterialLedgerId {
    MaterialLedgerId::world()
}

pub(super) fn default_logistics_route_available() -> bool {
    true
}

pub(super) fn default_logistics_capacity_units() -> i64 {
    i64::MAX
}

pub(super) fn default_material_ledgers() -> BTreeMap<MaterialLedgerId, BTreeMap<String, i64>> {
    let mut ledgers = BTreeMap::new();
    ledgers.insert(MaterialLedgerId::world(), BTreeMap::new());
    ledgers
}

pub(super) fn default_material_transit_priority() -> MaterialTransitPriority {
    MaterialTransitPriority::Standard
}

pub(super) fn default_module_market_order_id() -> u64 {
    1
}

pub(super) fn default_module_market_sale_id() -> u64 {
    1
}

pub(super) fn default_next_module_instance_id() -> u64 {
    1
}

pub(super) fn default_next_module_release_request_id() -> u64 {
    1
}

pub(super) fn default_factory_durability_ppm() -> i64 {
    1_000_000
}

pub(super) fn default_factory_production_state() -> FactoryProductionState {
    FactoryProductionState::default()
}

pub(super) fn default_next_industry_settlement_order() -> u64 {
    1
}

pub(super) fn default_module_release_required_roles() -> Vec<String> {
    vec![
        "security".to_string(),
        "economy".to_string(),
        "runtime".to_string(),
    ]
}

impl Default for WorldState {
    fn default() -> Self {
        Self {
            time: 0,
            retained_history_root: None,
            agents: BTreeMap::new(),
            agent_intent_ledger: BTreeMap::new(),
            resources: BTreeMap::new(),
            materials: BTreeMap::new(),
            material_ledgers: default_material_ledgers(),
            material_profiles: BTreeMap::new(),
            logistics_routes: BTreeMap::new(),
            completed_logistics_route_ids: BTreeSet::new(),
            completed_logistics_paths: BTreeMap::new(),
            settled_logistics_transit_ids: BTreeSet::new(),
            logistics_settlement_receipts: BTreeMap::new(),
            factory_production_failure_dispositions: BTreeMap::new(),
            direct_material_transfer_receipts: BTreeMap::new(),
            product_validation_receipts: BTreeMap::new(),
            product_validation_attempts: BTreeMap::new(),
            product_validation_delivery_cursor: ProductValidationDeliveryCursor::default(),
            product_profiles: BTreeMap::new(),
            latest_product_validation: None,
            recipe_profiles: BTreeMap::new(),
            factory_profiles: BTreeMap::new(),
            agent_location_authorities: BTreeMap::new(),
            location_anchors: BTreeMap::new(),
            factory_site_authorities: BTreeMap::new(),
            factory_construction_power_profiles: BTreeMap::new(),
            factories: BTreeMap::new(),
            retired_factory_ids: BTreeSet::new(),
            settled_factory_build_ids: BTreeSet::new(),
            factory_construction_receipts: BTreeMap::new(),
            pending_factory_builds: BTreeMap::new(),
            pending_recipe_jobs: BTreeMap::new(),
            settled_recipe_job_ids: BTreeSet::new(),
            next_industry_settlement_order: default_next_industry_settlement_order(),
            industry_settlement_orders: BTreeMap::new(),
            recipe_completion_receipts: BTreeMap::new(),
            factory_recycle_receipts: BTreeMap::new(),
            pending_material_transits: BTreeMap::new(),
            industry_progress: IndustryProgressState::default(),
            alliances: BTreeMap::new(),
            gameplay_policy: GameplayPolicyState::default(),
            data_access_permissions: BTreeMap::new(),
            economic_contracts: BTreeMap::new(),
            agent_claims: BTreeMap::new(),
            starter_oc_claims: BTreeMap::new(),
            authenticated_collect_data_last_nonces: BTreeMap::new(),
            agent_claim_last_processed_epoch: 0,
            contract_pair_last_success_settled_at: BTreeMap::new(),
            reputation_reward_window_started_at: BTreeMap::new(),
            reputation_reward_window_accumulated: BTreeMap::new(),
            reputation_scores: BTreeMap::new(),
            wars: BTreeMap::new(),
            governance_votes: BTreeMap::new(),
            governance_proposals: BTreeMap::new(),
            governance_identity_profiles: BTreeMap::new(),
            crises: BTreeMap::new(),
            meta_progress: BTreeMap::new(),
            module_states: BTreeMap::new(),
            module_visual_entities: BTreeMap::new(),
            module_artifact_owners: BTreeMap::new(),
            module_artifact_listings: BTreeMap::new(),
            module_artifact_bids: BTreeMap::new(),
            module_instances: BTreeMap::new(),
            module_admission_freezes: BTreeMap::new(),
            module_release_requests: BTreeMap::new(),
            module_release_manifest_mappings: BTreeMap::new(),
            next_module_release_request_id: default_next_module_release_request_id(),
            module_release_role_bindings: BTreeMap::new(),
            installed_module_targets: BTreeMap::new(),
            next_module_instance_id: default_next_module_instance_id(),
            next_module_market_order_id: default_module_market_order_id(),
            next_module_market_sale_id: default_module_market_sale_id(),
            main_token_config: MainTokenConfig::default(),
            main_token_supply: MainTokenSupplyState::default(),
            main_token_balances: BTreeMap::new(),
            restricted_starter_claim_grants: BTreeMap::new(),
            main_token_genesis_buckets: BTreeMap::new(),
            main_token_epoch_issuance_records: BTreeMap::new(),
            main_token_treasury_balances: BTreeMap::new(),
            main_token_claim_nonces: BTreeMap::new(),
            main_token_transfer_nonces: BTreeMap::new(),
            main_token_scheduled_policy_updates: BTreeMap::new(),
            main_token_node_points_bridge_records: BTreeMap::new(),
            main_token_treasury_distribution_records: BTreeMap::new(),
            restricted_starter_claim_liveops_pool_top_up_records: BTreeMap::new(),
            reward_asset_config: RewardAssetConfig::default(),
            node_asset_balances: BTreeMap::new(),
            protocol_power_reserve: ProtocolPowerReserve::default(),
            reward_mint_records: Vec::new(),
            node_redeem_nonces: BTreeMap::new(),
            system_order_pool_budgets: BTreeMap::new(),
            node_identity_bindings: BTreeMap::new(),
            node_main_token_account_bindings: BTreeMap::new(),
            governance_finality_signer_registry: None,
            governance_validator_admissions: BTreeMap::new(),
            governance_main_token_controller_registry: None,
            reward_signature_governance_policy: RewardSignatureGovernancePolicy::default(),
        }
    }
}
