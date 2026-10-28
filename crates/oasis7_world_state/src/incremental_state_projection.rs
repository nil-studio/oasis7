//! Exhaustive WorldState field ownership for borrowed genesis traversal.
use super::*;
use crate::state::WorldState;
use oasis7_formal_protocol::incremental_state::StateNamespaceV1 as Namespace;
macro_rules! fields {
    ($($name:ident => $mode:ident, $shape:ident, $destination:expr;)*) => {
        pub const WORLD_STATE_FIELDS: &[FieldDescriptor] = &[$(FieldDescriptor { owner: FieldOwner::WorldState, name: stringify!($name), destination: $destination, shape: FieldShape::$shape },)*];
        impl WorldState {
            pub fn visit_iws_genesis_fields<S: GenesisFieldVisitor>(&self, sink: &mut S) -> Result<(), ProjectionError<S::Error>> {
                // No `..`: a newly added field must be explicitly classified,
                // including fields omitted by serde skip_serializing_if.
                let WorldState { $($name: _,)* } = self;
                $(let field = FieldDescriptor { owner: FieldOwner::WorldState, name: stringify!($name), destination: $destination, shape: FieldShape::$shape };
                  crate::visit_iws_field!(sink, field, &self.$name, $mode)?;)*
                Ok(())
            }
        }
    }
}
fields! {
    world_rules => value, Value, FieldDestination::State(Namespace::Authorization);
    time => value, Value, FieldDestination::State(Namespace::Allocator);
    retained_history_root => side, History, FieldDestination::ProofLedger;
    agents => agents, Children, FieldDestination::Children;
    agent_intent_ledger => map, Map, FieldDestination::State(Namespace::Authorization);
    resources => map, Map, FieldDestination::State(Namespace::Business);
    materials => map, Map, FieldDestination::State(Namespace::Business);
    material_ledgers => nested, NestedMap, FieldDestination::State(Namespace::Business);
    material_profiles => map, Map, FieldDestination::State(Namespace::Business);
    logistics_routes => map, Map, FieldDestination::State(Namespace::Business);
    completed_logistics_route_ids => set, Set, FieldDestination::State(Namespace::Authorization);
    completed_logistics_paths => map, Map, FieldDestination::State(Namespace::Authorization);
    settled_logistics_transit_ids => set, Set, FieldDestination::State(Namespace::Authorization);
    logistics_settlement_receipts => map, Map, FieldDestination::State(Namespace::Authorization);
    factory_production_failure_dispositions => map, Map, FieldDestination::State(Namespace::Authorization);
    direct_material_transfer_receipts => map, Map, FieldDestination::State(Namespace::Authorization);
    product_validation_receipts => map, Map, FieldDestination::State(Namespace::Authorization);
    product_profiles => map, Map, FieldDestination::State(Namespace::Business);
    latest_product_validation => value, Value, FieldDestination::State(Namespace::Business);
    recipe_profiles => map, Map, FieldDestination::State(Namespace::Business);
    factory_profiles => map, Map, FieldDestination::State(Namespace::Business);
    agent_location_authorities => map, Map, FieldDestination::State(Namespace::Business);
    location_anchors => map, Map, FieldDestination::State(Namespace::Business);
    factory_site_authorities => map, Map, FieldDestination::State(Namespace::Business);
    factory_construction_power_profiles => map, Map, FieldDestination::State(Namespace::Business);
    factories => map, Map, FieldDestination::State(Namespace::Business);
    retired_factory_ids => set, Set, FieldDestination::State(Namespace::Authorization);
    settled_factory_build_ids => set, Set, FieldDestination::State(Namespace::Authorization);
    factory_construction_receipts => map, Map, FieldDestination::State(Namespace::Authorization);
    product_validation_attempts => map, Map, FieldDestination::State(Namespace::Effects);
    product_validation_delivery_cursor => value, Value, FieldDestination::State(Namespace::Scheduler);
    pending_factory_builds => map, Map, FieldDestination::State(Namespace::Scheduler);
    pending_recipe_jobs => map, Map, FieldDestination::State(Namespace::Scheduler);
    settled_recipe_job_ids => set, Set, FieldDestination::State(Namespace::Authorization);
    next_industry_settlement_order => value, Value, FieldDestination::State(Namespace::Allocator);
    industry_settlement_orders => map, Map, FieldDestination::State(Namespace::Allocator);
    recipe_completion_receipts => map, Map, FieldDestination::State(Namespace::Authorization);
    factory_recycle_receipts => map, Map, FieldDestination::State(Namespace::Authorization);
    pending_material_transits => map, Map, FieldDestination::State(Namespace::Scheduler);
    industry_progress => value, Value, FieldDestination::State(Namespace::Business);
    alliances => map, Map, FieldDestination::State(Namespace::Business);
    gameplay_policy => value, Value, FieldDestination::State(Namespace::Business);
    data_access_permissions => mapset, MapSet, FieldDestination::State(Namespace::Authorization);
    economic_contracts => map, Map, FieldDestination::State(Namespace::Business);
    agent_claims => map, Map, FieldDestination::State(Namespace::Business);
    agent_claim_acquisition_sequences => map, Map, FieldDestination::State(Namespace::Allocator);
    starter_oc_claims => map, Map, FieldDestination::State(Namespace::Authorization);
    authenticated_collect_data_last_nonces => nested, NestedMap, FieldDestination::State(Namespace::Authorization);
    agent_claim_last_processed_epoch => value, Value, FieldDestination::State(Namespace::Scheduler);
    contract_pair_last_success_settled_at => map, Map, FieldDestination::State(Namespace::Business);
    reputation_reward_window_started_at => map, Map, FieldDestination::State(Namespace::Business);
    reputation_reward_window_accumulated => map, Map, FieldDestination::State(Namespace::Business);
    reputation_scores => map, Map, FieldDestination::State(Namespace::Business);
    wars => map, Map, FieldDestination::State(Namespace::Business);
    governance_votes => map, Map, FieldDestination::State(Namespace::Authorization);
    governance_proposals => map, Map, FieldDestination::State(Namespace::Authorization);
    governance_identity_profiles => map, Map, FieldDestination::State(Namespace::Authorization);
    crises => map, Map, FieldDestination::State(Namespace::Business);
    meta_progress => map, Map, FieldDestination::State(Namespace::Business);
    module_states => bytes, ByteMap, FieldDestination::State(Namespace::Business);
    module_visual_entities => map, Map, FieldDestination::State(Namespace::Business);
    module_artifact_owners => map, Map, FieldDestination::State(Namespace::Business);
    module_artifact_listings => map, Map, FieldDestination::State(Namespace::Business);
    module_artifact_bids => map, Map, FieldDestination::State(Namespace::Business);
    module_instances => map, Map, FieldDestination::State(Namespace::Business);
    module_admission_freezes => map, Map, FieldDestination::State(Namespace::Authorization);
    module_release_requests => map, Map, FieldDestination::State(Namespace::Authorization);
    module_release_manifest_mappings => map, Map, FieldDestination::State(Namespace::Authorization);
    next_module_release_request_id => value, Value, FieldDestination::State(Namespace::Allocator);
    module_release_role_bindings => mapset, MapSet, FieldDestination::State(Namespace::Authorization);
    installed_module_targets => map, Map, FieldDestination::State(Namespace::Artifacts);
    next_module_instance_id => value, Value, FieldDestination::State(Namespace::Allocator);
    next_module_market_order_id => value, Value, FieldDestination::State(Namespace::Allocator);
    next_module_market_sale_id => value, Value, FieldDestination::State(Namespace::Allocator);
    main_token_config => value, Value, FieldDestination::State(Namespace::Business);
    main_token_supply => value, Value, FieldDestination::State(Namespace::Business);
    main_token_balances => map, Map, FieldDestination::State(Namespace::Business);
    restricted_starter_claim_grants => map, Map, FieldDestination::State(Namespace::Authorization);
    main_token_genesis_buckets => map, Map, FieldDestination::State(Namespace::Business);
    main_token_epoch_issuance_records => map, Map, FieldDestination::State(Namespace::Authorization);
    main_token_treasury_balances => map, Map, FieldDestination::State(Namespace::Business);
    main_token_claim_nonces => map, Map, FieldDestination::State(Namespace::Authorization);
    main_token_transfer_nonces => map, Map, FieldDestination::State(Namespace::Authorization);
    main_token_scheduled_policy_updates => map, Map, FieldDestination::State(Namespace::Scheduler);
    main_token_node_points_bridge_records => map, Map, FieldDestination::State(Namespace::Authorization);
    main_token_treasury_distribution_records => map, Map, FieldDestination::State(Namespace::Authorization);
    restricted_starter_claim_liveops_pool_top_up_records => map, Map, FieldDestination::State(Namespace::Authorization);
    reward_asset_config => value, Value, FieldDestination::State(Namespace::Business);
    node_asset_balances => map, Map, FieldDestination::State(Namespace::Business);
    protocol_power_reserve => value, Value, FieldDestination::State(Namespace::Business);
    reward_mint_records => ordered, Ordered, FieldDestination::State(Namespace::Authorization);
    node_redeem_nonces => map, Map, FieldDestination::State(Namespace::Authorization);
    system_order_pool_budgets => map, Map, FieldDestination::State(Namespace::Business);
    node_identity_bindings => map, Map, FieldDestination::State(Namespace::Authorization);
    node_main_token_account_bindings => map, Map, FieldDestination::State(Namespace::Authorization);
    governance_finality_signer_registry => value, Value, FieldDestination::State(Namespace::Authorization);
    governance_validator_admissions => map, Map, FieldDestination::State(Namespace::Authorization);
    governance_main_token_controller_registry => value, Value, FieldDestination::State(Namespace::Authorization);
    governance_authorization_nonces => map, Map, FieldDestination::State(Namespace::Authorization);
    reward_signature_governance_policy => value, Value, FieldDestination::State(Namespace::Authorization);
}
