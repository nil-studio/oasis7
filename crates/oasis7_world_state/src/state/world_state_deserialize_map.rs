//! Map-access field readers for the grouped `WorldState` decoder.

use super::*;

pub(super) fn read<'de, A>(
    field: Field,
    map: &mut A,
    builder: &mut WorldStateBuilder,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match field.index() {
        0..=19 => read_map_group_0(field, map, builder)?,
        20..=39 => read_map_group_1(field, map, builder)?,
        40..=59 => read_map_group_2(field, map, builder)?,
        60..=79 => read_map_group_3(field, map, builder)?,
        80..=99 => read_map_group_4(field, map, builder)?,
        _ => unreachable!("unknown fields are consumed before map dispatch"),
    }
    Ok(())
}

fn read_map_group_0<'de, A>(
    field: Field,
    map: &mut A,
    builder: &mut WorldStateBuilder,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match field {
        Field::WorldRules => builder.world_rules = Some(map.next_value()?),
        Field::Time => builder.time = Some(map.next_value()?),
        Field::RetainedHistoryRoot => builder.retained_history_root = Some(map.next_value()?),
        Field::Agents => builder.agents = Some(map.next_value()?),
        Field::AgentIntentLedger => builder.agent_intent_ledger = Some(map.next_value()?),
        Field::Resources => builder.resources = Some(map.next_value()?),
        Field::Materials => builder.materials = Some(map.next_value()?),
        Field::MaterialLedgers => builder.material_ledgers = Some(map.next_value()?),
        Field::MaterialProfiles => builder.material_profiles = Some(map.next_value()?),
        Field::LogisticsRoutes => builder.logistics_routes = Some(map.next_value()?),
        Field::CompletedLogisticsRouteIds => {
            builder.completed_logistics_route_ids = Some(map.next_value()?)
        }
        Field::CompletedLogisticsPaths => {
            builder.completed_logistics_paths = Some(map.next_value()?)
        }
        Field::SettledLogisticsTransitIds => {
            builder.settled_logistics_transit_ids = Some(map.next_value()?)
        }
        Field::LogisticsSettlementReceipts => {
            builder.logistics_settlement_receipts = Some(map.next_value()?)
        }
        Field::FactoryProductionFailureDispositions => {
            builder.factory_production_failure_dispositions = Some(map.next_value_seed(
                U64KeyMapSeed::<FactoryProductionFailureDispositionV1>(PhantomData),
            )?);
        }
        Field::DirectMaterialTransferReceipts => {
            builder.direct_material_transfer_receipts = Some(map.next_value()?)
        }
        Field::ProductValidationReceipts => {
            builder.product_validation_receipts = Some(map.next_value_seed(U64KeyMapSeed::<
                Vec<ProductValidationReceiptV1>,
            >(
                PhantomData
            ))?);
        }
        Field::ProductProfiles => builder.product_profiles = Some(map.next_value()?),
        Field::LatestProductValidation => {
            builder.latest_product_validation = Some(map.next_value()?)
        }
        Field::RecipeProfiles => builder.recipe_profiles = Some(map.next_value()?),
        _ => unreachable!("field routed to the wrong map group"),
    }
    Ok(())
}

fn read_map_group_1<'de, A>(
    field: Field,
    map: &mut A,
    builder: &mut WorldStateBuilder,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match field {
        Field::FactoryProfiles => builder.factory_profiles = Some(map.next_value()?),
        Field::AgentLocationAuthorities => {
            builder.agent_location_authorities = Some(map.next_value()?)
        }
        Field::LocationAnchors => builder.location_anchors = Some(map.next_value()?),
        Field::FactorySiteAuthorities => builder.factory_site_authorities = Some(map.next_value()?),
        Field::FactoryConstructionPowerProfiles => {
            builder.factory_construction_power_profiles = Some(map.next_value()?)
        }
        Field::Factories => builder.factories = Some(map.next_value()?),
        Field::RetiredFactoryIds => builder.retired_factory_ids = Some(map.next_value()?),
        Field::SettledFactoryBuildIds => {
            builder.settled_factory_build_ids = Some(map.next_value()?)
        }
        Field::FactoryConstructionReceipts => {
            builder.factory_construction_receipts = Some(map.next_value()?)
        }
        Field::ProductValidationAttempts => {
            builder.product_validation_attempts = Some(map.next_value_seed(U64KeyMapSeed::<
                Vec<ProductValidationAttemptV1>,
            >(
                PhantomData
            ))?);
        }
        Field::ProductValidationDeliveryCursor => {
            builder.product_validation_delivery_cursor = Some(map.next_value()?)
        }
        Field::PendingFactoryBuilds => {
            builder.pending_factory_builds =
                Some(map.next_value_seed(U64KeyMapSeed::<FactoryBuildJobState>(PhantomData))?);
        }
        Field::PendingRecipeJobs => {
            builder.pending_recipe_jobs =
                Some(map.next_value_seed(U64KeyMapSeed::<RecipeJobState>(PhantomData))?);
        }
        Field::SettledRecipeJobIds => builder.settled_recipe_job_ids = Some(map.next_value()?),
        Field::NextIndustrySettlementOrder => {
            builder.next_industry_settlement_order = Some(map.next_value()?)
        }
        Field::IndustrySettlementOrders => {
            builder.industry_settlement_orders =
                Some(map.next_value_seed(U64KeyMapSeed::<u64>(PhantomData))?);
        }
        Field::RecipeCompletionReceipts => {
            builder.recipe_completion_receipts =
                Some(map.next_value_seed(U64KeyMapSeed::<RecipeCompletionReceiptV1>(PhantomData))?);
        }
        Field::FactoryRecycleReceipts => builder.factory_recycle_receipts = Some(map.next_value()?),
        Field::PendingMaterialTransits => {
            builder.pending_material_transits =
                Some(map.next_value_seed(U64KeyMapSeed::<MaterialTransitJobState>(PhantomData))?);
        }
        Field::IndustryProgress => builder.industry_progress = Some(map.next_value()?),
        _ => unreachable!("field routed to the wrong map group"),
    }
    Ok(())
}

fn read_map_group_2<'de, A>(
    field: Field,
    map: &mut A,
    builder: &mut WorldStateBuilder,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match field {
        Field::Alliances => builder.alliances = Some(map.next_value()?),
        Field::GameplayPolicy => builder.gameplay_policy = Some(map.next_value()?),
        Field::DataAccessPermissions => builder.data_access_permissions = Some(map.next_value()?),
        Field::EconomicContracts => builder.economic_contracts = Some(map.next_value()?),
        Field::AgentClaims => builder.agent_claims = Some(map.next_value()?),
        Field::AgentClaimAcquisitionSequences => {
            builder.agent_claim_acquisition_sequences = Some(map.next_value()?)
        }
        Field::StarterOcClaims => builder.starter_oc_claims = Some(map.next_value()?),
        Field::AuthenticatedCollectDataLastNonces => {
            builder.authenticated_collect_data_last_nonces = Some(map.next_value()?)
        }
        Field::AgentClaimLastProcessedEpoch => {
            builder.agent_claim_last_processed_epoch = Some(map.next_value()?)
        }
        Field::ContractPairLastSuccessSettledAt => {
            builder.contract_pair_last_success_settled_at = Some(map.next_value()?)
        }
        Field::ReputationRewardWindowStartedAt => {
            builder.reputation_reward_window_started_at = Some(map.next_value()?)
        }
        Field::ReputationRewardWindowAccumulated => {
            builder.reputation_reward_window_accumulated = Some(map.next_value()?)
        }
        Field::ReputationScores => builder.reputation_scores = Some(map.next_value()?),
        Field::Wars => builder.wars = Some(map.next_value()?),
        Field::GovernanceVotes => builder.governance_votes = Some(map.next_value()?),
        Field::GovernanceProposals => builder.governance_proposals = Some(map.next_value()?),
        Field::GovernanceIdentityProfiles => {
            builder.governance_identity_profiles = Some(map.next_value()?)
        }
        Field::Crises => builder.crises = Some(map.next_value()?),
        Field::MetaProgress => builder.meta_progress = Some(map.next_value()?),
        Field::ModuleStates => builder.module_states = Some(map.next_value()?),
        _ => unreachable!("field routed to the wrong map group"),
    }
    Ok(())
}

fn read_map_group_3<'de, A>(
    field: Field,
    map: &mut A,
    builder: &mut WorldStateBuilder,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match field {
        Field::ModuleVisualEntities => builder.module_visual_entities = Some(map.next_value()?),
        Field::ModuleArtifactOwners => builder.module_artifact_owners = Some(map.next_value()?),
        Field::ModuleArtifactListings => builder.module_artifact_listings = Some(map.next_value()?),
        Field::ModuleArtifactBids => builder.module_artifact_bids = Some(map.next_value()?),
        Field::ModuleInstances => builder.module_instances = Some(map.next_value()?),
        Field::ModuleAdmissionFreezes => builder.module_admission_freezes = Some(map.next_value()?),
        Field::ModuleReleaseRequests => {
            builder.module_release_requests =
                Some(map.next_value_seed(U64KeyMapSeed::<ModuleReleaseRequestState>(PhantomData))?);
        }
        Field::ModuleReleaseManifestMappings => {
            builder.module_release_manifest_mappings = Some(map.next_value_seed(
                U64KeyMapSeed::<ModuleReleaseManifestMappingState>(PhantomData),
            )?);
        }
        Field::NextModuleReleaseRequestId => {
            builder.next_module_release_request_id = Some(map.next_value()?)
        }
        Field::ModuleReleaseRoleBindings => {
            builder.module_release_role_bindings = Some(map.next_value()?)
        }
        Field::InstalledModuleTargets => builder.installed_module_targets = Some(map.next_value()?),
        Field::NextModuleInstanceId => builder.next_module_instance_id = Some(map.next_value()?),
        Field::NextModuleMarketOrderId => {
            builder.next_module_market_order_id = Some(map.next_value()?)
        }
        Field::NextModuleMarketSaleId => {
            builder.next_module_market_sale_id = Some(map.next_value()?)
        }
        Field::MainTokenConfig => builder.main_token_config = Some(map.next_value()?),
        Field::MainTokenSupply => builder.main_token_supply = Some(map.next_value()?),
        Field::MainTokenBalances => builder.main_token_balances = Some(map.next_value()?),
        Field::RestrictedStarterClaimGrants => {
            builder.restricted_starter_claim_grants = Some(map.next_value()?)
        }
        Field::MainTokenGenesisBuckets => {
            builder.main_token_genesis_buckets = Some(map.next_value()?)
        }
        Field::MainTokenEpochIssuanceRecords => {
            builder.main_token_epoch_issuance_records = Some(
                map.next_value_seed(U64KeyMapSeed::<MainTokenEpochIssuanceRecord>(PhantomData))?,
            );
        }
        _ => unreachable!("field routed to the wrong map group"),
    }
    Ok(())
}

fn read_map_group_4<'de, A>(
    field: Field,
    map: &mut A,
    builder: &mut WorldStateBuilder,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match field {
        Field::MainTokenTreasuryBalances => {
            builder.main_token_treasury_balances = Some(map.next_value()?)
        }
        Field::MainTokenClaimNonces => builder.main_token_claim_nonces = Some(map.next_value()?),
        Field::MainTokenTransferNonces => {
            builder.main_token_transfer_nonces = Some(map.next_value()?)
        }
        Field::MainTokenScheduledPolicyUpdates => {
            builder.main_token_scheduled_policy_updates = Some(
                map.next_value_seed(U64KeyMapSeed::<MainTokenScheduledPolicyUpdate>(PhantomData))?,
            );
        }
        Field::MainTokenNodePointsBridgeRecords => {
            builder.main_token_node_points_bridge_records = Some(map.next_value_seed(
                U64KeyMapSeed::<MainTokenNodePointsBridgeEpochRecord>(PhantomData),
            )?);
        }
        Field::MainTokenTreasuryDistributionRecords => {
            builder.main_token_treasury_distribution_records = Some(map.next_value()?)
        }
        Field::RestrictedStarterClaimLiveopsPoolTopUpRecords => {
            builder.restricted_starter_claim_liveops_pool_top_up_records = Some(map.next_value()?)
        }
        Field::RewardAssetConfig => builder.reward_asset_config = Some(map.next_value()?),
        Field::NodeAssetBalances => builder.node_asset_balances = Some(map.next_value()?),
        Field::ProtocolPowerReserve => builder.protocol_power_reserve = Some(map.next_value()?),
        Field::RewardMintRecords => builder.reward_mint_records = Some(map.next_value()?),
        Field::NodeRedeemNonces => builder.node_redeem_nonces = Some(map.next_value()?),
        Field::SystemOrderPoolBudgets => {
            builder.system_order_pool_budgets =
                Some(map.next_value_seed(U64KeyMapSeed::<SystemOrderPoolBudget>(PhantomData))?);
        }
        Field::NodeIdentityBindings => builder.node_identity_bindings = Some(map.next_value()?),
        Field::NodeMainTokenAccountBindings => {
            builder.node_main_token_account_bindings = Some(map.next_value()?)
        }
        Field::GovernanceFinalitySignerRegistry => {
            builder.governance_finality_signer_registry = Some(map.next_value()?)
        }
        Field::GovernanceValidatorAdmissions => {
            builder.governance_validator_admissions = Some(map.next_value()?)
        }
        Field::GovernanceMainTokenControllerRegistry => {
            builder.governance_main_token_controller_registry = Some(map.next_value()?)
        }
        Field::GovernanceAuthorizationNonces => {
            builder.governance_authorization_nonces = Some(map.next_value()?)
        }
        Field::RewardSignatureGovernancePolicy => {
            builder.reward_signature_governance_policy = Some(map.next_value()?)
        }
        _ => unreachable!("field routed to the wrong map group"),
    }
    Ok(())
}
