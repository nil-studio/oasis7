//! The field table remains the projection authority; native assembly is closed.
use super::*;
const NATIVE: &[&str] = &[
    "world_rules",
    "time",
    "agent_intent_ledger",
    "resources",
    "materials",
    "material_ledgers",
    "material_profiles",
    "logistics_routes",
    "completed_logistics_route_ids",
    "completed_logistics_paths",
    "settled_logistics_transit_ids",
    "logistics_settlement_receipts",
    "factory_production_failure_dispositions",
    "direct_material_transfer_receipts",
    "product_validation_receipts",
    "product_profiles",
    "latest_product_validation",
    "recipe_profiles",
    "factory_profiles",
    "agent_location_authorities",
    "location_anchors",
    "factory_site_authorities",
    "factory_construction_power_profiles",
    "factories",
    "retired_factory_ids",
    "settled_factory_build_ids",
    "factory_construction_receipts",
    "product_validation_attempts",
    "product_validation_delivery_cursor",
    "pending_factory_builds",
    "pending_recipe_jobs",
    "settled_recipe_job_ids",
    "next_industry_settlement_order",
    "industry_settlement_orders",
    "recipe_completion_receipts",
    "factory_recycle_receipts",
    "pending_material_transits",
    "industry_progress",
    "alliances",
    "gameplay_policy",
    "data_access_permissions",
    "economic_contracts",
    "agent_claims",
    "agent_claim_acquisition_sequences",
    "starter_oc_claims",
    "authenticated_collect_data_last_nonces",
    "agent_claim_last_processed_epoch",
    "contract_pair_last_success_settled_at",
    "reputation_reward_window_started_at",
    "reputation_reward_window_accumulated",
    "reputation_scores",
    "wars",
    "governance_votes",
    "governance_proposals",
    "governance_identity_profiles",
    "crises",
    "meta_progress",
    "module_visual_entities",
    "module_artifact_owners",
    "module_artifact_listings",
    "module_artifact_bids",
    "module_admission_freezes",
    "module_release_requests",
    "module_release_manifest_mappings",
    "next_module_release_request_id",
    "module_release_role_bindings",
    "installed_module_targets",
    "next_module_instance_id",
    "next_module_market_order_id",
    "next_module_market_sale_id",
    "main_token_config",
    "main_token_supply",
    "main_token_balances",
    "restricted_starter_claim_grants",
    "main_token_genesis_buckets",
    "main_token_epoch_issuance_records",
    "main_token_treasury_balances",
    "main_token_claim_nonces",
    "main_token_transfer_nonces",
    "main_token_scheduled_policy_updates",
    "main_token_node_points_bridge_records",
    "main_token_treasury_distribution_records",
    "restricted_starter_claim_liveops_pool_top_up_records",
    "reward_asset_config",
    "node_asset_balances",
    "protocol_power_reserve",
    "reward_mint_records",
    "node_redeem_nonces",
    "system_order_pool_budgets",
    "node_identity_bindings",
    "node_main_token_account_bindings",
    "governance_finality_signer_registry",
    "governance_validator_admissions",
    "governance_finality_key_rotations",
    "governance_main_token_controller_registry",
    "governance_authorization_nonces",
    "reward_signature_governance_policy",
];
fn delegated(field: &crate::incremental_projection::FieldDescriptor) -> bool {
    use crate::incremental_projection::{FieldDestination as D, FieldShape as S};
    matches!(
        (field.name, field.shape, field.destination),
        (
            "module_states",
            S::ByteMap,
            D::State(StateNamespaceV1::Business)
        ) | ("agents" | "module_instances", S::Children, D::Children)
            | ("retained_history_root", S::History, D::ProofLedger)
    )
}
pub(super) fn validate() -> Result<(), MemberRecordError> {
    if NATIVE.len().checked_add(4) != Some(WORLD_STATE_FIELDS.len()) {
        return Err(invalid());
    }
    for (index, field) in WORLD_STATE_FIELDS.iter().enumerate() {
        if field.owner != FieldOwner::WorldState
            || WORLD_STATE_FIELDS[..index]
                .iter()
                .any(|old| old.name == field.name)
        {
            return Err(invalid());
        }
        let native = NATIVE.iter().filter(|name| **name == field.name).count();
        if delegated(field) {
            if native != 0 {
                return Err(invalid());
            }
        } else if native != 1 || !matches!(field.destination, FieldDestination::State(_)) {
            return Err(invalid());
        }
    }
    Ok(())
}
pub(super) struct Coverage {
    seen: [bool; WORLD_STATE_FIELDS.len()],
    writes: u64,
}
impl Coverage {
    pub(super) fn new() -> Self {
        Self {
            seen: [false; WORLD_STATE_FIELDS.len()],
            writes: 0,
        }
    }
    pub(super) fn write(&mut self, row: &ExecutionWriteV1) -> Result<(), ValueTreeError> {
        let parsed =
            WorldFieldKeyV1::from_bytes(&row.mutation.key).map_err(ValueTreeError::Invalid)?;
        let (index, field) = WORLD_STATE_FIELDS
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == parsed.name)
            .ok_or_else(|| ValueTreeError::Invalid("WorldState native GEN unknown field"))?;
        if parsed.owner != FieldOwner::WorldState
            || parsed.namespace != row.namespace
            || field.destination != FieldDestination::State(row.namespace)
            || delegated(field)
            || row.mutation.before.is_some()
            || row.mutation.after.is_none()
        {
            return Err(ValueTreeError::Invalid("WorldState native GEN ownership"));
        }
        let expected = WorldFieldKeyV1::new(*field, parsed.kind, parsed.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        if expected.as_bytes() != row.mutation.key {
            return Err(ValueTreeError::Invalid("WorldState native GEN key"));
        }
        if parsed.kind == WorldFieldRecordKindV1::Metadata {
            if parsed.semantic_key.is_some() || self.seen[index] {
                return Err(ValueTreeError::Invalid(
                    "WorldState native GEN duplicate metadata",
                ));
            }
            self.seen[index] = true;
        }
        self.writes = self
            .writes
            .checked_add(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "WorldState native GEN writes",
            ))?;
        Ok(())
    }
    pub(super) fn finish(self) -> Result<WorldStateGenesisSummaryV1, MemberRecordError> {
        for (index, field) in WORLD_STATE_FIELDS.iter().enumerate() {
            if self.seen[index] == delegated(field) {
                return Err(MemberRecordError::Invalid(field.name));
            }
        }
        Ok(WorldStateGenesisSummaryV1 {
            native_fields: NATIVE.len() as u64,
            delegated_fields: 4,
            writes: self.writes,
        })
    }
}
/// Exhaustive source guards retain projection ownership and all rule branches.
pub(super) fn source_guard(source: &WorldState) {
    let WorldState {
        world_rules: _,
        time: _,
        retained_history_root: _,
        agents: _,
        agent_intent_ledger: _,
        resources: _,
        materials: _,
        material_ledgers: _,
        material_profiles: _,
        logistics_routes: _,
        completed_logistics_route_ids: _,
        completed_logistics_paths: _,
        settled_logistics_transit_ids: _,
        logistics_settlement_receipts: _,
        factory_production_failure_dispositions: _,
        direct_material_transfer_receipts: _,
        product_validation_receipts: _,
        product_profiles: _,
        latest_product_validation: _,
        recipe_profiles: _,
        factory_profiles: _,
        agent_location_authorities: _,
        location_anchors: _,
        factory_site_authorities: _,
        factory_construction_power_profiles: _,
        factories: _,
        retired_factory_ids: _,
        settled_factory_build_ids: _,
        factory_construction_receipts: _,
        product_validation_attempts: _,
        product_validation_delivery_cursor: _,
        pending_factory_builds: _,
        pending_recipe_jobs: _,
        settled_recipe_job_ids: _,
        next_industry_settlement_order: _,
        industry_settlement_orders: _,
        recipe_completion_receipts: _,
        factory_recycle_receipts: _,
        pending_material_transits: _,
        industry_progress: _,
        alliances: _,
        gameplay_policy: _,
        data_access_permissions: _,
        economic_contracts: _,
        agent_claims: _,
        agent_claim_acquisition_sequences: _,
        starter_oc_claims: _,
        authenticated_collect_data_last_nonces: _,
        agent_claim_last_processed_epoch: _,
        contract_pair_last_success_settled_at: _,
        reputation_reward_window_started_at: _,
        reputation_reward_window_accumulated: _,
        reputation_scores: _,
        wars: _,
        governance_votes: _,
        governance_proposals: _,
        governance_identity_profiles: _,
        crises: _,
        meta_progress: _,
        module_states: _,
        module_visual_entities: _,
        module_artifact_owners: _,
        module_artifact_listings: _,
        module_artifact_bids: _,
        module_instances: _,
        module_admission_freezes: _,
        module_release_requests: _,
        module_release_manifest_mappings: _,
        next_module_release_request_id: _,
        module_release_role_bindings: _,
        installed_module_targets: _,
        next_module_instance_id: _,
        next_module_market_order_id: _,
        next_module_market_sale_id: _,
        main_token_config: _,
        main_token_supply: _,
        main_token_balances: _,
        restricted_starter_claim_grants: _,
        main_token_genesis_buckets: _,
        main_token_epoch_issuance_records: _,
        main_token_treasury_balances: _,
        main_token_claim_nonces: _,
        main_token_transfer_nonces: _,
        main_token_scheduled_policy_updates: _,
        main_token_node_points_bridge_records: _,
        main_token_treasury_distribution_records: _,
        restricted_starter_claim_liveops_pool_top_up_records: _,
        reward_asset_config: _,
        node_asset_balances: _,
        protocol_power_reserve: _,
        reward_mint_records: _,
        node_redeem_nonces: _,
        system_order_pool_budgets: _,
        node_identity_bindings: _,
        node_main_token_account_bindings: _,
        governance_finality_signer_registry: _,
        governance_validator_admissions: _,
        governance_finality_key_rotations: _,
        governance_main_token_controller_registry: _,
        governance_authorization_nonces: _,
        reward_signature_governance_policy: _,
    } = source;
    if let Some(rules) = &source.world_rules {
        let crate::WorldRulesStateV1 {
            requests: _,
            governance_effects: _,
            organization_cases: _,
            organization_estate: _,
            supply_authorizations: _,
            supply_batches: _,
            supply_lineages: _,
            supply_shipments: _,
            organization_continuity: _,
            emergency_supply: _,
            agent_orders: _,
            industrial_work: _,
            substitution: _,
        } = rules;
    }
}
