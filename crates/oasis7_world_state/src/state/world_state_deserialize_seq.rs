//! Sequence-access field readers for the grouped `WorldState` decoder.

use super::*;

pub(super) fn read_all<'de, A>(seq: &mut A, builder: &mut WorldStateBuilder) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
{
    read_seq_group_0(seq, builder)?;
    read_seq_group_1(seq, builder)?;
    read_seq_group_2(seq, builder)?;
    read_seq_group_3(seq, builder)?;
    read_seq_group_4(seq, builder)?;
    Ok(())
}

fn read_seq_group_0<'de, A>(seq: &mut A, builder: &mut WorldStateBuilder) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
{
    builder.world_rules = Some(seq.next_element()?.unwrap_or_default());
    let value = seq
        .next_element()?
        .ok_or_else(|| WorldStateVisitor::missing_sequence_field(1))?;
    builder.time = Some(value);
    builder.retained_history_root = Some(seq.next_element()?.unwrap_or_default());
    let value = seq
        .next_element()?
        .ok_or_else(|| WorldStateVisitor::missing_sequence_field(3))?;
    builder.agents = Some(value);
    builder.agent_intent_ledger = Some(seq.next_element()?.unwrap_or_default());
    builder.resources = Some(seq.next_element()?.unwrap_or_default());
    builder.materials = Some(seq.next_element()?.unwrap_or_default());
    builder.material_ledgers = Some(seq.next_element()?.unwrap_or_else(default_material_ledgers));
    builder.material_profiles = Some(seq.next_element()?.unwrap_or_default());
    builder.logistics_routes = Some(seq.next_element()?.unwrap_or_default());
    builder.completed_logistics_route_ids = Some(seq.next_element()?.unwrap_or_default());
    builder.completed_logistics_paths = Some(seq.next_element()?.unwrap_or_default());
    builder.settled_logistics_transit_ids = Some(seq.next_element()?.unwrap_or_default());
    builder.logistics_settlement_receipts = Some(seq.next_element()?.unwrap_or_default());
    builder.factory_production_failure_dispositions = Some(
        seq.next_element_seed(U64KeyMapSeed::<FactoryProductionFailureDispositionV1>(
            PhantomData,
        ))?
        .unwrap_or_default(),
    );
    builder.direct_material_transfer_receipts = Some(seq.next_element()?.unwrap_or_default());
    builder.product_validation_receipts = Some(
        seq.next_element_seed(U64KeyMapSeed::<Vec<ProductValidationReceiptV1>>(
            PhantomData,
        ))?
        .unwrap_or_default(),
    );
    builder.product_profiles = Some(seq.next_element()?.unwrap_or_default());
    builder.latest_product_validation = Some(seq.next_element()?.unwrap_or_default());
    builder.recipe_profiles = Some(seq.next_element()?.unwrap_or_default());
    Ok(())
}

fn read_seq_group_1<'de, A>(seq: &mut A, builder: &mut WorldStateBuilder) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
{
    builder.factory_profiles = Some(seq.next_element()?.unwrap_or_default());
    builder.agent_location_authorities = Some(seq.next_element()?.unwrap_or_default());
    builder.location_anchors = Some(seq.next_element()?.unwrap_or_default());
    builder.factory_site_authorities = Some(seq.next_element()?.unwrap_or_default());
    builder.factory_construction_power_profiles = Some(seq.next_element()?.unwrap_or_default());
    builder.factories = Some(seq.next_element()?.unwrap_or_default());
    builder.retired_factory_ids = Some(seq.next_element()?.unwrap_or_default());
    builder.settled_factory_build_ids = Some(seq.next_element()?.unwrap_or_default());
    builder.factory_construction_receipts = Some(seq.next_element()?.unwrap_or_default());
    builder.product_validation_attempts = Some(
        seq.next_element_seed(U64KeyMapSeed::<Vec<ProductValidationAttemptV1>>(
            PhantomData,
        ))?
        .unwrap_or_default(),
    );
    builder.product_validation_delivery_cursor = Some(seq.next_element()?.unwrap_or_default());
    builder.pending_factory_builds = Some(
        seq.next_element_seed(U64KeyMapSeed::<FactoryBuildJobState>(PhantomData))?
            .unwrap_or_default(),
    );
    builder.pending_recipe_jobs = Some(
        seq.next_element_seed(U64KeyMapSeed::<RecipeJobState>(PhantomData))?
            .unwrap_or_default(),
    );
    builder.settled_recipe_job_ids = Some(seq.next_element()?.unwrap_or_default());
    builder.next_industry_settlement_order = Some(
        seq.next_element()?
            .unwrap_or_else(state_defaults::default_next_industry_settlement_order),
    );
    builder.industry_settlement_orders = Some(
        seq.next_element_seed(U64KeyMapSeed::<u64>(PhantomData))?
            .unwrap_or_default(),
    );
    builder.recipe_completion_receipts = Some(
        seq.next_element_seed(U64KeyMapSeed::<RecipeCompletionReceiptV1>(PhantomData))?
            .unwrap_or_default(),
    );
    builder.factory_recycle_receipts = Some(seq.next_element()?.unwrap_or_default());
    builder.pending_material_transits = Some(
        seq.next_element_seed(U64KeyMapSeed::<MaterialTransitJobState>(PhantomData))?
            .unwrap_or_default(),
    );
    builder.industry_progress = Some(seq.next_element()?.unwrap_or_default());
    Ok(())
}

fn read_seq_group_2<'de, A>(seq: &mut A, builder: &mut WorldStateBuilder) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
{
    builder.alliances = Some(seq.next_element()?.unwrap_or_default());
    builder.gameplay_policy = Some(seq.next_element()?.unwrap_or_default());
    builder.data_access_permissions = Some(seq.next_element()?.unwrap_or_default());
    builder.economic_contracts = Some(seq.next_element()?.unwrap_or_default());
    builder.agent_claims = Some(seq.next_element()?.unwrap_or_default());
    builder.agent_claim_acquisition_sequences = Some(seq.next_element()?.unwrap_or_default());
    builder.starter_oc_claims = Some(seq.next_element()?.unwrap_or_default());
    builder.authenticated_collect_data_last_nonces = Some(seq.next_element()?.unwrap_or_default());
    builder.agent_claim_last_processed_epoch = Some(seq.next_element()?.unwrap_or_default());
    builder.contract_pair_last_success_settled_at = Some(seq.next_element()?.unwrap_or_default());
    builder.reputation_reward_window_started_at = Some(seq.next_element()?.unwrap_or_default());
    builder.reputation_reward_window_accumulated = Some(seq.next_element()?.unwrap_or_default());
    builder.reputation_scores = Some(seq.next_element()?.unwrap_or_default());
    builder.wars = Some(seq.next_element()?.unwrap_or_default());
    builder.governance_votes = Some(seq.next_element()?.unwrap_or_default());
    builder.governance_proposals = Some(seq.next_element()?.unwrap_or_default());
    builder.governance_identity_profiles = Some(seq.next_element()?.unwrap_or_default());
    builder.crises = Some(seq.next_element()?.unwrap_or_default());
    builder.meta_progress = Some(seq.next_element()?.unwrap_or_default());
    builder.module_states = Some(seq.next_element()?.unwrap_or_default());
    Ok(())
}

fn read_seq_group_3<'de, A>(seq: &mut A, builder: &mut WorldStateBuilder) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
{
    builder.module_visual_entities = Some(seq.next_element()?.unwrap_or_default());
    builder.module_artifact_owners = Some(seq.next_element()?.unwrap_or_default());
    builder.module_artifact_listings = Some(seq.next_element()?.unwrap_or_default());
    builder.module_artifact_bids = Some(seq.next_element()?.unwrap_or_default());
    builder.module_instances = Some(seq.next_element()?.unwrap_or_default());
    builder.module_admission_freezes = Some(seq.next_element()?.unwrap_or_default());
    builder.module_release_requests = Some(
        seq.next_element_seed(U64KeyMapSeed::<ModuleReleaseRequestState>(PhantomData))?
            .unwrap_or_default(),
    );
    builder.module_release_manifest_mappings = Some(
        seq.next_element_seed(U64KeyMapSeed::<ModuleReleaseManifestMappingState>(
            PhantomData,
        ))?
        .unwrap_or_default(),
    );
    builder.next_module_release_request_id = Some(
        seq.next_element()?
            .unwrap_or_else(default_next_module_release_request_id),
    );
    builder.module_release_role_bindings = Some(seq.next_element()?.unwrap_or_default());
    builder.installed_module_targets = Some(seq.next_element()?.unwrap_or_default());
    builder.next_module_instance_id = Some(
        seq.next_element()?
            .unwrap_or_else(default_next_module_instance_id),
    );
    builder.next_module_market_order_id = Some(
        seq.next_element()?
            .unwrap_or_else(default_module_market_order_id),
    );
    builder.next_module_market_sale_id = Some(
        seq.next_element()?
            .unwrap_or_else(default_module_market_sale_id),
    );
    builder.main_token_config = Some(seq.next_element()?.unwrap_or_default());
    builder.main_token_supply = Some(seq.next_element()?.unwrap_or_default());
    builder.main_token_balances = Some(seq.next_element()?.unwrap_or_default());
    builder.restricted_starter_claim_grants = Some(seq.next_element()?.unwrap_or_default());
    builder.main_token_genesis_buckets = Some(seq.next_element()?.unwrap_or_default());
    builder.main_token_epoch_issuance_records = Some(
        seq.next_element_seed(U64KeyMapSeed::<MainTokenEpochIssuanceRecord>(PhantomData))?
            .unwrap_or_default(),
    );
    Ok(())
}

fn read_seq_group_4<'de, A>(seq: &mut A, builder: &mut WorldStateBuilder) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
{
    builder.main_token_treasury_balances = Some(seq.next_element()?.unwrap_or_default());
    builder.main_token_claim_nonces = Some(seq.next_element()?.unwrap_or_default());
    builder.main_token_transfer_nonces = Some(seq.next_element()?.unwrap_or_default());
    builder.main_token_scheduled_policy_updates = Some(
        seq.next_element_seed(U64KeyMapSeed::<MainTokenScheduledPolicyUpdate>(PhantomData))?
            .unwrap_or_default(),
    );
    builder.main_token_node_points_bridge_records = Some(
        seq.next_element_seed(U64KeyMapSeed::<MainTokenNodePointsBridgeEpochRecord>(
            PhantomData,
        ))?
        .unwrap_or_default(),
    );
    builder.main_token_treasury_distribution_records =
        Some(seq.next_element()?.unwrap_or_default());
    builder.restricted_starter_claim_liveops_pool_top_up_records =
        Some(seq.next_element()?.unwrap_or_default());
    builder.reward_asset_config = Some(seq.next_element()?.unwrap_or_default());
    builder.node_asset_balances = Some(seq.next_element()?.unwrap_or_default());
    builder.protocol_power_reserve = Some(seq.next_element()?.unwrap_or_default());
    builder.reward_mint_records = Some(seq.next_element()?.unwrap_or_default());
    builder.node_redeem_nonces = Some(seq.next_element()?.unwrap_or_default());
    builder.system_order_pool_budgets = Some(
        seq.next_element_seed(U64KeyMapSeed::<SystemOrderPoolBudget>(PhantomData))?
            .unwrap_or_default(),
    );
    builder.node_identity_bindings = Some(seq.next_element()?.unwrap_or_default());
    builder.node_main_token_account_bindings = Some(seq.next_element()?.unwrap_or_default());
    builder.governance_finality_signer_registry = Some(seq.next_element()?.unwrap_or_default());
    builder.governance_validator_admissions = Some(seq.next_element()?.unwrap_or_default());
    builder.governance_main_token_controller_registry =
        Some(seq.next_element()?.unwrap_or_default());
    builder.governance_authorization_nonces = Some(seq.next_element()?.unwrap_or_default());
    builder.reward_signature_governance_policy = Some(seq.next_element()?.unwrap_or_default());
    Ok(())
}
