//! Grouped public-Serde decoding for `WorldState`.
//!
//! Field order follows the struct declaration. Defaults are evaluated only for missing fields.

use super::*;
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::marker::PhantomData;

const FIELDS: &[&str] = &[
    "world_rules",
    "time",
    "retained_history_root",
    "agents",
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
    "module_states",
    "module_visual_entities",
    "module_artifact_owners",
    "module_artifact_listings",
    "module_artifact_bids",
    "module_instances",
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
    "governance_main_token_controller_registry",
    "governance_authorization_nonces",
    "reward_signature_governance_policy",
];

#[derive(Clone, Copy, Deserialize)]
#[serde(field_identifier)]
enum Field {
    #[serde(rename = "world_rules")]
    WorldRules,
    #[serde(rename = "time")]
    Time,
    #[serde(rename = "retained_history_root")]
    RetainedHistoryRoot,
    #[serde(rename = "agents")]
    Agents,
    #[serde(rename = "agent_intent_ledger")]
    AgentIntentLedger,
    #[serde(rename = "resources")]
    Resources,
    #[serde(rename = "materials")]
    Materials,
    #[serde(rename = "material_ledgers")]
    MaterialLedgers,
    #[serde(rename = "material_profiles")]
    MaterialProfiles,
    #[serde(rename = "logistics_routes")]
    LogisticsRoutes,
    #[serde(rename = "completed_logistics_route_ids")]
    CompletedLogisticsRouteIds,
    #[serde(rename = "completed_logistics_paths")]
    CompletedLogisticsPaths,
    #[serde(rename = "settled_logistics_transit_ids")]
    SettledLogisticsTransitIds,
    #[serde(rename = "logistics_settlement_receipts")]
    LogisticsSettlementReceipts,
    #[serde(rename = "factory_production_failure_dispositions")]
    FactoryProductionFailureDispositions,
    #[serde(rename = "direct_material_transfer_receipts")]
    DirectMaterialTransferReceipts,
    #[serde(rename = "product_validation_receipts")]
    ProductValidationReceipts,
    #[serde(rename = "product_profiles")]
    ProductProfiles,
    #[serde(rename = "latest_product_validation")]
    LatestProductValidation,
    #[serde(rename = "recipe_profiles")]
    RecipeProfiles,
    #[serde(rename = "factory_profiles")]
    FactoryProfiles,
    #[serde(rename = "agent_location_authorities")]
    AgentLocationAuthorities,
    #[serde(rename = "location_anchors")]
    LocationAnchors,
    #[serde(rename = "factory_site_authorities")]
    FactorySiteAuthorities,
    #[serde(rename = "factory_construction_power_profiles")]
    FactoryConstructionPowerProfiles,
    #[serde(rename = "factories")]
    Factories,
    #[serde(rename = "retired_factory_ids")]
    RetiredFactoryIds,
    #[serde(rename = "settled_factory_build_ids")]
    SettledFactoryBuildIds,
    #[serde(rename = "factory_construction_receipts")]
    FactoryConstructionReceipts,
    #[serde(rename = "product_validation_attempts")]
    ProductValidationAttempts,
    #[serde(rename = "product_validation_delivery_cursor")]
    ProductValidationDeliveryCursor,
    #[serde(rename = "pending_factory_builds")]
    PendingFactoryBuilds,
    #[serde(rename = "pending_recipe_jobs")]
    PendingRecipeJobs,
    #[serde(rename = "settled_recipe_job_ids")]
    SettledRecipeJobIds,
    #[serde(rename = "next_industry_settlement_order")]
    NextIndustrySettlementOrder,
    #[serde(rename = "industry_settlement_orders")]
    IndustrySettlementOrders,
    #[serde(rename = "recipe_completion_receipts")]
    RecipeCompletionReceipts,
    #[serde(rename = "factory_recycle_receipts")]
    FactoryRecycleReceipts,
    #[serde(rename = "pending_material_transits")]
    PendingMaterialTransits,
    #[serde(rename = "industry_progress")]
    IndustryProgress,
    #[serde(rename = "alliances")]
    Alliances,
    #[serde(rename = "gameplay_policy")]
    GameplayPolicy,
    #[serde(rename = "data_access_permissions")]
    DataAccessPermissions,
    #[serde(rename = "economic_contracts")]
    EconomicContracts,
    #[serde(rename = "agent_claims")]
    AgentClaims,
    #[serde(rename = "agent_claim_acquisition_sequences")]
    AgentClaimAcquisitionSequences,
    #[serde(rename = "starter_oc_claims")]
    StarterOcClaims,
    #[serde(rename = "authenticated_collect_data_last_nonces")]
    AuthenticatedCollectDataLastNonces,
    #[serde(rename = "agent_claim_last_processed_epoch")]
    AgentClaimLastProcessedEpoch,
    #[serde(rename = "contract_pair_last_success_settled_at")]
    ContractPairLastSuccessSettledAt,
    #[serde(rename = "reputation_reward_window_started_at")]
    ReputationRewardWindowStartedAt,
    #[serde(rename = "reputation_reward_window_accumulated")]
    ReputationRewardWindowAccumulated,
    #[serde(rename = "reputation_scores")]
    ReputationScores,
    #[serde(rename = "wars")]
    Wars,
    #[serde(rename = "governance_votes")]
    GovernanceVotes,
    #[serde(rename = "governance_proposals")]
    GovernanceProposals,
    #[serde(rename = "governance_identity_profiles")]
    GovernanceIdentityProfiles,
    #[serde(rename = "crises")]
    Crises,
    #[serde(rename = "meta_progress")]
    MetaProgress,
    #[serde(rename = "module_states")]
    ModuleStates,
    #[serde(rename = "module_visual_entities")]
    ModuleVisualEntities,
    #[serde(rename = "module_artifact_owners")]
    ModuleArtifactOwners,
    #[serde(rename = "module_artifact_listings")]
    ModuleArtifactListings,
    #[serde(rename = "module_artifact_bids")]
    ModuleArtifactBids,
    #[serde(rename = "module_instances")]
    ModuleInstances,
    #[serde(rename = "module_admission_freezes")]
    ModuleAdmissionFreezes,
    #[serde(rename = "module_release_requests")]
    ModuleReleaseRequests,
    #[serde(rename = "module_release_manifest_mappings")]
    ModuleReleaseManifestMappings,
    #[serde(rename = "next_module_release_request_id")]
    NextModuleReleaseRequestId,
    #[serde(rename = "module_release_role_bindings")]
    ModuleReleaseRoleBindings,
    #[serde(rename = "installed_module_targets")]
    InstalledModuleTargets,
    #[serde(rename = "next_module_instance_id")]
    NextModuleInstanceId,
    #[serde(rename = "next_module_market_order_id")]
    NextModuleMarketOrderId,
    #[serde(rename = "next_module_market_sale_id")]
    NextModuleMarketSaleId,
    #[serde(rename = "main_token_config")]
    MainTokenConfig,
    #[serde(rename = "main_token_supply")]
    MainTokenSupply,
    #[serde(rename = "main_token_balances")]
    MainTokenBalances,
    #[serde(rename = "restricted_starter_claim_grants")]
    RestrictedStarterClaimGrants,
    #[serde(rename = "main_token_genesis_buckets")]
    MainTokenGenesisBuckets,
    #[serde(rename = "main_token_epoch_issuance_records")]
    MainTokenEpochIssuanceRecords,
    #[serde(rename = "main_token_treasury_balances")]
    MainTokenTreasuryBalances,
    #[serde(rename = "main_token_claim_nonces")]
    MainTokenClaimNonces,
    #[serde(rename = "main_token_transfer_nonces")]
    MainTokenTransferNonces,
    #[serde(rename = "main_token_scheduled_policy_updates")]
    MainTokenScheduledPolicyUpdates,
    #[serde(rename = "main_token_node_points_bridge_records")]
    MainTokenNodePointsBridgeRecords,
    #[serde(rename = "main_token_treasury_distribution_records")]
    MainTokenTreasuryDistributionRecords,
    #[serde(rename = "restricted_starter_claim_liveops_pool_top_up_records")]
    RestrictedStarterClaimLiveopsPoolTopUpRecords,
    #[serde(rename = "reward_asset_config")]
    RewardAssetConfig,
    #[serde(rename = "node_asset_balances")]
    NodeAssetBalances,
    #[serde(rename = "protocol_power_reserve")]
    ProtocolPowerReserve,
    #[serde(rename = "reward_mint_records")]
    RewardMintRecords,
    #[serde(rename = "node_redeem_nonces")]
    NodeRedeemNonces,
    #[serde(rename = "system_order_pool_budgets")]
    SystemOrderPoolBudgets,
    #[serde(rename = "node_identity_bindings")]
    NodeIdentityBindings,
    #[serde(rename = "node_main_token_account_bindings")]
    NodeMainTokenAccountBindings,
    #[serde(rename = "governance_finality_signer_registry")]
    GovernanceFinalitySignerRegistry,
    #[serde(rename = "governance_validator_admissions")]
    GovernanceValidatorAdmissions,
    #[serde(rename = "governance_main_token_controller_registry")]
    GovernanceMainTokenControllerRegistry,
    #[serde(rename = "governance_authorization_nonces")]
    GovernanceAuthorizationNonces,
    #[serde(rename = "reward_signature_governance_policy")]
    RewardSignatureGovernancePolicy,
    #[serde(other)]
    Other,
}

impl Field {
    fn index(self) -> usize {
        match self {
            Field::WorldRules => 0,
            Field::Time => 1,
            Field::RetainedHistoryRoot => 2,
            Field::Agents => 3,
            Field::AgentIntentLedger => 4,
            Field::Resources => 5,
            Field::Materials => 6,
            Field::MaterialLedgers => 7,
            Field::MaterialProfiles => 8,
            Field::LogisticsRoutes => 9,
            Field::CompletedLogisticsRouteIds => 10,
            Field::CompletedLogisticsPaths => 11,
            Field::SettledLogisticsTransitIds => 12,
            Field::LogisticsSettlementReceipts => 13,
            Field::FactoryProductionFailureDispositions => 14,
            Field::DirectMaterialTransferReceipts => 15,
            Field::ProductValidationReceipts => 16,
            Field::ProductProfiles => 17,
            Field::LatestProductValidation => 18,
            Field::RecipeProfiles => 19,
            Field::FactoryProfiles => 20,
            Field::AgentLocationAuthorities => 21,
            Field::LocationAnchors => 22,
            Field::FactorySiteAuthorities => 23,
            Field::FactoryConstructionPowerProfiles => 24,
            Field::Factories => 25,
            Field::RetiredFactoryIds => 26,
            Field::SettledFactoryBuildIds => 27,
            Field::FactoryConstructionReceipts => 28,
            Field::ProductValidationAttempts => 29,
            Field::ProductValidationDeliveryCursor => 30,
            Field::PendingFactoryBuilds => 31,
            Field::PendingRecipeJobs => 32,
            Field::SettledRecipeJobIds => 33,
            Field::NextIndustrySettlementOrder => 34,
            Field::IndustrySettlementOrders => 35,
            Field::RecipeCompletionReceipts => 36,
            Field::FactoryRecycleReceipts => 37,
            Field::PendingMaterialTransits => 38,
            Field::IndustryProgress => 39,
            Field::Alliances => 40,
            Field::GameplayPolicy => 41,
            Field::DataAccessPermissions => 42,
            Field::EconomicContracts => 43,
            Field::AgentClaims => 44,
            Field::AgentClaimAcquisitionSequences => 45,
            Field::StarterOcClaims => 46,
            Field::AuthenticatedCollectDataLastNonces => 47,
            Field::AgentClaimLastProcessedEpoch => 48,
            Field::ContractPairLastSuccessSettledAt => 49,
            Field::ReputationRewardWindowStartedAt => 50,
            Field::ReputationRewardWindowAccumulated => 51,
            Field::ReputationScores => 52,
            Field::Wars => 53,
            Field::GovernanceVotes => 54,
            Field::GovernanceProposals => 55,
            Field::GovernanceIdentityProfiles => 56,
            Field::Crises => 57,
            Field::MetaProgress => 58,
            Field::ModuleStates => 59,
            Field::ModuleVisualEntities => 60,
            Field::ModuleArtifactOwners => 61,
            Field::ModuleArtifactListings => 62,
            Field::ModuleArtifactBids => 63,
            Field::ModuleInstances => 64,
            Field::ModuleAdmissionFreezes => 65,
            Field::ModuleReleaseRequests => 66,
            Field::ModuleReleaseManifestMappings => 67,
            Field::NextModuleReleaseRequestId => 68,
            Field::ModuleReleaseRoleBindings => 69,
            Field::InstalledModuleTargets => 70,
            Field::NextModuleInstanceId => 71,
            Field::NextModuleMarketOrderId => 72,
            Field::NextModuleMarketSaleId => 73,
            Field::MainTokenConfig => 74,
            Field::MainTokenSupply => 75,
            Field::MainTokenBalances => 76,
            Field::RestrictedStarterClaimGrants => 77,
            Field::MainTokenGenesisBuckets => 78,
            Field::MainTokenEpochIssuanceRecords => 79,
            Field::MainTokenTreasuryBalances => 80,
            Field::MainTokenClaimNonces => 81,
            Field::MainTokenTransferNonces => 82,
            Field::MainTokenScheduledPolicyUpdates => 83,
            Field::MainTokenNodePointsBridgeRecords => 84,
            Field::MainTokenTreasuryDistributionRecords => 85,
            Field::RestrictedStarterClaimLiveopsPoolTopUpRecords => 86,
            Field::RewardAssetConfig => 87,
            Field::NodeAssetBalances => 88,
            Field::ProtocolPowerReserve => 89,
            Field::RewardMintRecords => 90,
            Field::NodeRedeemNonces => 91,
            Field::SystemOrderPoolBudgets => 92,
            Field::NodeIdentityBindings => 93,
            Field::NodeMainTokenAccountBindings => 94,
            Field::GovernanceFinalitySignerRegistry => 95,
            Field::GovernanceValidatorAdmissions => 96,
            Field::GovernanceMainTokenControllerRegistry => 97,
            Field::GovernanceAuthorizationNonces => 98,
            Field::RewardSignatureGovernancePolicy => 99,
            Field::Other => FIELDS.len(),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Field::WorldRules => "world_rules",
            Field::Time => "time",
            Field::RetainedHistoryRoot => "retained_history_root",
            Field::Agents => "agents",
            Field::AgentIntentLedger => "agent_intent_ledger",
            Field::Resources => "resources",
            Field::Materials => "materials",
            Field::MaterialLedgers => "material_ledgers",
            Field::MaterialProfiles => "material_profiles",
            Field::LogisticsRoutes => "logistics_routes",
            Field::CompletedLogisticsRouteIds => "completed_logistics_route_ids",
            Field::CompletedLogisticsPaths => "completed_logistics_paths",
            Field::SettledLogisticsTransitIds => "settled_logistics_transit_ids",
            Field::LogisticsSettlementReceipts => "logistics_settlement_receipts",
            Field::FactoryProductionFailureDispositions => {
                "factory_production_failure_dispositions"
            }
            Field::DirectMaterialTransferReceipts => "direct_material_transfer_receipts",
            Field::ProductValidationReceipts => "product_validation_receipts",
            Field::ProductProfiles => "product_profiles",
            Field::LatestProductValidation => "latest_product_validation",
            Field::RecipeProfiles => "recipe_profiles",
            Field::FactoryProfiles => "factory_profiles",
            Field::AgentLocationAuthorities => "agent_location_authorities",
            Field::LocationAnchors => "location_anchors",
            Field::FactorySiteAuthorities => "factory_site_authorities",
            Field::FactoryConstructionPowerProfiles => "factory_construction_power_profiles",
            Field::Factories => "factories",
            Field::RetiredFactoryIds => "retired_factory_ids",
            Field::SettledFactoryBuildIds => "settled_factory_build_ids",
            Field::FactoryConstructionReceipts => "factory_construction_receipts",
            Field::ProductValidationAttempts => "product_validation_attempts",
            Field::ProductValidationDeliveryCursor => "product_validation_delivery_cursor",
            Field::PendingFactoryBuilds => "pending_factory_builds",
            Field::PendingRecipeJobs => "pending_recipe_jobs",
            Field::SettledRecipeJobIds => "settled_recipe_job_ids",
            Field::NextIndustrySettlementOrder => "next_industry_settlement_order",
            Field::IndustrySettlementOrders => "industry_settlement_orders",
            Field::RecipeCompletionReceipts => "recipe_completion_receipts",
            Field::FactoryRecycleReceipts => "factory_recycle_receipts",
            Field::PendingMaterialTransits => "pending_material_transits",
            Field::IndustryProgress => "industry_progress",
            Field::Alliances => "alliances",
            Field::GameplayPolicy => "gameplay_policy",
            Field::DataAccessPermissions => "data_access_permissions",
            Field::EconomicContracts => "economic_contracts",
            Field::AgentClaims => "agent_claims",
            Field::AgentClaimAcquisitionSequences => "agent_claim_acquisition_sequences",
            Field::StarterOcClaims => "starter_oc_claims",
            Field::AuthenticatedCollectDataLastNonces => "authenticated_collect_data_last_nonces",
            Field::AgentClaimLastProcessedEpoch => "agent_claim_last_processed_epoch",
            Field::ContractPairLastSuccessSettledAt => "contract_pair_last_success_settled_at",
            Field::ReputationRewardWindowStartedAt => "reputation_reward_window_started_at",
            Field::ReputationRewardWindowAccumulated => "reputation_reward_window_accumulated",
            Field::ReputationScores => "reputation_scores",
            Field::Wars => "wars",
            Field::GovernanceVotes => "governance_votes",
            Field::GovernanceProposals => "governance_proposals",
            Field::GovernanceIdentityProfiles => "governance_identity_profiles",
            Field::Crises => "crises",
            Field::MetaProgress => "meta_progress",
            Field::ModuleStates => "module_states",
            Field::ModuleVisualEntities => "module_visual_entities",
            Field::ModuleArtifactOwners => "module_artifact_owners",
            Field::ModuleArtifactListings => "module_artifact_listings",
            Field::ModuleArtifactBids => "module_artifact_bids",
            Field::ModuleInstances => "module_instances",
            Field::ModuleAdmissionFreezes => "module_admission_freezes",
            Field::ModuleReleaseRequests => "module_release_requests",
            Field::ModuleReleaseManifestMappings => "module_release_manifest_mappings",
            Field::NextModuleReleaseRequestId => "next_module_release_request_id",
            Field::ModuleReleaseRoleBindings => "module_release_role_bindings",
            Field::InstalledModuleTargets => "installed_module_targets",
            Field::NextModuleInstanceId => "next_module_instance_id",
            Field::NextModuleMarketOrderId => "next_module_market_order_id",
            Field::NextModuleMarketSaleId => "next_module_market_sale_id",
            Field::MainTokenConfig => "main_token_config",
            Field::MainTokenSupply => "main_token_supply",
            Field::MainTokenBalances => "main_token_balances",
            Field::RestrictedStarterClaimGrants => "restricted_starter_claim_grants",
            Field::MainTokenGenesisBuckets => "main_token_genesis_buckets",
            Field::MainTokenEpochIssuanceRecords => "main_token_epoch_issuance_records",
            Field::MainTokenTreasuryBalances => "main_token_treasury_balances",
            Field::MainTokenClaimNonces => "main_token_claim_nonces",
            Field::MainTokenTransferNonces => "main_token_transfer_nonces",
            Field::MainTokenScheduledPolicyUpdates => "main_token_scheduled_policy_updates",
            Field::MainTokenNodePointsBridgeRecords => "main_token_node_points_bridge_records",
            Field::MainTokenTreasuryDistributionRecords => {
                "main_token_treasury_distribution_records"
            }
            Field::RestrictedStarterClaimLiveopsPoolTopUpRecords => {
                "restricted_starter_claim_liveops_pool_top_up_records"
            }
            Field::RewardAssetConfig => "reward_asset_config",
            Field::NodeAssetBalances => "node_asset_balances",
            Field::ProtocolPowerReserve => "protocol_power_reserve",
            Field::RewardMintRecords => "reward_mint_records",
            Field::NodeRedeemNonces => "node_redeem_nonces",
            Field::SystemOrderPoolBudgets => "system_order_pool_budgets",
            Field::NodeIdentityBindings => "node_identity_bindings",
            Field::NodeMainTokenAccountBindings => "node_main_token_account_bindings",
            Field::GovernanceFinalitySignerRegistry => "governance_finality_signer_registry",
            Field::GovernanceValidatorAdmissions => "governance_validator_admissions",
            Field::GovernanceMainTokenControllerRegistry => {
                "governance_main_token_controller_registry"
            }
            Field::GovernanceAuthorizationNonces => "governance_authorization_nonces",
            Field::RewardSignatureGovernancePolicy => "reward_signature_governance_policy",
            Field::Other => "<unknown>",
        }
    }
}

#[derive(Default)]
struct WorldStateBuilder {
    world_rules: Option<Option<crate::world_rules::WorldRulesStateV1>>,
    time: Option<WorldTime>,
    retained_history_root: Option<Option<FormalRetainedHistoryRootV1>>,
    agents: Option<BTreeMap<String, AgentCell>>,
    agent_intent_ledger: Option<BTreeMap<String, AgentIntentV2>>,
    resources: Option<BTreeMap<ResourceKind, i64>>,
    materials: Option<BTreeMap<String, i64>>,
    material_ledgers: Option<BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>>,
    material_profiles: Option<BTreeMap<String, MaterialProfileV1>>,
    logistics_routes: Option<BTreeMap<String, LogisticsRouteV1>>,
    completed_logistics_route_ids: Option<BTreeSet<String>>,
    completed_logistics_paths: Option<BTreeMap<String, LogisticsPathAuthorityV1>>,
    settled_logistics_transit_ids: Option<BTreeSet<ActionId>>,
    logistics_settlement_receipts: Option<BTreeMap<ActionId, LogisticsSettlementReceiptV1>>,
    factory_production_failure_dispositions:
        Option<BTreeMap<ActionId, FactoryProductionFailureDispositionV1>>,
    direct_material_transfer_receipts: Option<BTreeMap<ActionId, MaterialTransferReceiptV1>>,
    product_validation_receipts: Option<BTreeMap<ActionId, Vec<ProductValidationReceiptV1>>>,
    product_profiles: Option<BTreeMap<String, ProductProfileV1>>,
    latest_product_validation: Option<Option<LastProductValidationState>>,
    recipe_profiles: Option<BTreeMap<String, RecipeProfileV1>>,
    factory_profiles: Option<BTreeMap<String, FactoryProfileV1>>,
    agent_location_authorities: Option<BTreeMap<String, AgentLocationAuthorityV1>>,
    location_anchors: Option<BTreeMap<String, LocationAnchorV1>>,
    factory_site_authorities: Option<BTreeMap<String, FactorySiteAuthorityV1>>,
    factory_construction_power_profiles:
        Option<BTreeMap<String, FactoryConstructionPowerProfileV1>>,
    factories: Option<BTreeMap<String, FactoryState>>,
    retired_factory_ids: Option<BTreeSet<String>>,
    settled_factory_build_ids: Option<BTreeSet<ActionId>>,
    factory_construction_receipts: Option<BTreeMap<String, FactoryBuildPowerObligationV1>>,
    product_validation_attempts: Option<BTreeMap<ActionId, Vec<ProductValidationAttemptV1>>>,
    product_validation_delivery_cursor: Option<ProductValidationDeliveryCursor>,
    pending_factory_builds: Option<BTreeMap<ActionId, FactoryBuildJobState>>,
    pending_recipe_jobs: Option<BTreeMap<ActionId, RecipeJobState>>,
    settled_recipe_job_ids: Option<BTreeSet<ActionId>>,
    next_industry_settlement_order: Option<u64>,
    industry_settlement_orders: Option<BTreeMap<ActionId, u64>>,
    recipe_completion_receipts: Option<BTreeMap<ActionId, RecipeCompletionReceiptV1>>,
    factory_recycle_receipts: Option<BTreeMap<String, FactoryRecycleReceiptV1>>,
    pending_material_transits: Option<BTreeMap<ActionId, MaterialTransitJobState>>,
    industry_progress: Option<IndustryProgressState>,
    alliances: Option<BTreeMap<String, AllianceState>>,
    gameplay_policy: Option<GameplayPolicyState>,
    data_access_permissions: Option<BTreeMap<String, BTreeSet<String>>>,
    economic_contracts: Option<BTreeMap<String, EconomicContractState>>,
    agent_claims: Option<BTreeMap<String, AgentClaimState>>,
    agent_claim_acquisition_sequences: Option<BTreeMap<String, u64>>,
    starter_oc_claims: Option<BTreeMap<String, StarterOcClaimState>>,
    authenticated_collect_data_last_nonces: Option<BTreeMap<String, BTreeMap<String, u64>>>,
    agent_claim_last_processed_epoch: Option<u64>,
    contract_pair_last_success_settled_at: Option<BTreeMap<String, WorldTime>>,
    reputation_reward_window_started_at: Option<BTreeMap<String, WorldTime>>,
    reputation_reward_window_accumulated: Option<BTreeMap<String, i64>>,
    reputation_scores: Option<BTreeMap<String, i64>>,
    wars: Option<BTreeMap<String, WarState>>,
    governance_votes: Option<BTreeMap<String, GovernanceVoteState>>,
    governance_proposals: Option<BTreeMap<String, GovernanceProposalState>>,
    governance_identity_profiles: Option<BTreeMap<String, GovernanceIdentityProfileState>>,
    crises: Option<BTreeMap<String, CrisisState>>,
    meta_progress: Option<BTreeMap<String, MetaProgressState>>,
    module_states: Option<BTreeMap<String, Vec<u8>>>,
    module_visual_entities: Option<BTreeMap<String, ModuleVisualEntity>>,
    module_artifact_owners: Option<BTreeMap<String, String>>,
    module_artifact_listings: Option<BTreeMap<String, ModuleArtifactListingState>>,
    module_artifact_bids: Option<BTreeMap<String, Vec<ModuleArtifactBidState>>>,
    module_instances: Option<BTreeMap<String, ModuleInstanceState>>,
    module_admission_freezes: Option<BTreeMap<String, ModuleAdmissionFreeze>>,
    module_release_requests: Option<BTreeMap<u64, ModuleReleaseRequestState>>,
    module_release_manifest_mappings: Option<BTreeMap<u64, ModuleReleaseManifestMappingState>>,
    next_module_release_request_id: Option<u64>,
    module_release_role_bindings: Option<BTreeMap<String, BTreeSet<String>>>,
    installed_module_targets: Option<BTreeMap<String, ModuleInstallTarget>>,
    next_module_instance_id: Option<u64>,
    next_module_market_order_id: Option<u64>,
    next_module_market_sale_id: Option<u64>,
    main_token_config: Option<MainTokenConfig>,
    main_token_supply: Option<MainTokenSupplyState>,
    main_token_balances: Option<BTreeMap<String, MainTokenAccountBalance>>,
    restricted_starter_claim_grants: Option<BTreeMap<String, RestrictedStarterClaimGrantState>>,
    main_token_genesis_buckets: Option<BTreeMap<String, MainTokenGenesisAllocationBucketState>>,
    main_token_epoch_issuance_records: Option<BTreeMap<u64, MainTokenEpochIssuanceRecord>>,
    main_token_treasury_balances: Option<BTreeMap<String, u64>>,
    main_token_claim_nonces: Option<BTreeMap<String, u64>>,
    main_token_transfer_nonces: Option<BTreeMap<String, u64>>,
    main_token_scheduled_policy_updates: Option<BTreeMap<u64, MainTokenScheduledPolicyUpdate>>,
    main_token_node_points_bridge_records:
        Option<BTreeMap<u64, MainTokenNodePointsBridgeEpochRecord>>,
    main_token_treasury_distribution_records:
        Option<BTreeMap<String, MainTokenTreasuryDistributionRecord>>,
    restricted_starter_claim_liveops_pool_top_up_records:
        Option<BTreeMap<String, RestrictedStarterClaimLiveopsPoolTopUpRecord>>,
    reward_asset_config: Option<RewardAssetConfig>,
    node_asset_balances: Option<BTreeMap<String, NodeAssetBalance>>,
    protocol_power_reserve: Option<ProtocolPowerReserve>,
    reward_mint_records: Option<Vec<NodeRewardMintRecord>>,
    node_redeem_nonces: Option<BTreeMap<String, u64>>,
    system_order_pool_budgets: Option<BTreeMap<u64, SystemOrderPoolBudget>>,
    node_identity_bindings: Option<BTreeMap<String, String>>,
    node_main_token_account_bindings: Option<BTreeMap<String, String>>,
    governance_finality_signer_registry: Option<Option<GovernanceFinalitySignerRegistry>>,
    governance_validator_admissions: Option<BTreeMap<String, GovernanceValidatorAdmissionRecord>>,
    governance_main_token_controller_registry:
        Option<Option<GovernanceMainTokenControllerRegistry>>,
    governance_authorization_nonces: Option<BTreeMap<String, u64>>,
    reward_signature_governance_policy: Option<RewardSignatureGovernancePolicy>,
}

fn finish_state<E: de::Error>(builder: WorldStateBuilder) -> Result<WorldState, E> {
    let world_rules = builder.world_rules.unwrap_or_default();
    let time = builder.time.ok_or_else(|| E::missing_field("time"))?;
    let retained_history_root = builder.retained_history_root.unwrap_or_default();
    let agents = builder.agents.ok_or_else(|| E::missing_field("agents"))?;
    let agent_intent_ledger = builder.agent_intent_ledger.unwrap_or_default();
    let resources = builder.resources.unwrap_or_default();
    let materials = builder.materials.unwrap_or_default();
    let material_ledgers = builder
        .material_ledgers
        .unwrap_or_else(default_material_ledgers);
    let material_profiles = builder.material_profiles.unwrap_or_default();
    let logistics_routes = builder.logistics_routes.unwrap_or_default();
    let completed_logistics_route_ids = builder.completed_logistics_route_ids.unwrap_or_default();
    let completed_logistics_paths = builder.completed_logistics_paths.unwrap_or_default();
    let settled_logistics_transit_ids = builder.settled_logistics_transit_ids.unwrap_or_default();
    let logistics_settlement_receipts = builder.logistics_settlement_receipts.unwrap_or_default();
    let factory_production_failure_dispositions = builder
        .factory_production_failure_dispositions
        .unwrap_or_default();
    let direct_material_transfer_receipts = builder
        .direct_material_transfer_receipts
        .unwrap_or_default();
    let product_validation_receipts = builder.product_validation_receipts.unwrap_or_default();
    let product_profiles = builder.product_profiles.unwrap_or_default();
    let latest_product_validation = builder.latest_product_validation.unwrap_or_default();
    let recipe_profiles = builder.recipe_profiles.unwrap_or_default();
    let factory_profiles = builder.factory_profiles.unwrap_or_default();
    let agent_location_authorities = builder.agent_location_authorities.unwrap_or_default();
    let location_anchors = builder.location_anchors.unwrap_or_default();
    let factory_site_authorities = builder.factory_site_authorities.unwrap_or_default();
    let factory_construction_power_profiles = builder
        .factory_construction_power_profiles
        .unwrap_or_default();
    let factories = builder.factories.unwrap_or_default();
    let retired_factory_ids = builder.retired_factory_ids.unwrap_or_default();
    let settled_factory_build_ids = builder.settled_factory_build_ids.unwrap_or_default();
    let factory_construction_receipts = builder.factory_construction_receipts.unwrap_or_default();
    let product_validation_attempts = builder.product_validation_attempts.unwrap_or_default();
    let product_validation_delivery_cursor = builder
        .product_validation_delivery_cursor
        .unwrap_or_default();
    let pending_factory_builds = builder.pending_factory_builds.unwrap_or_default();
    let pending_recipe_jobs = builder.pending_recipe_jobs.unwrap_or_default();
    let settled_recipe_job_ids = builder.settled_recipe_job_ids.unwrap_or_default();
    let next_industry_settlement_order = builder
        .next_industry_settlement_order
        .unwrap_or_else(state_defaults::default_next_industry_settlement_order);
    let industry_settlement_orders = builder.industry_settlement_orders.unwrap_or_default();
    let recipe_completion_receipts = builder.recipe_completion_receipts.unwrap_or_default();
    let factory_recycle_receipts = builder.factory_recycle_receipts.unwrap_or_default();
    let pending_material_transits = builder.pending_material_transits.unwrap_or_default();
    let industry_progress = builder.industry_progress.unwrap_or_default();
    let alliances = builder.alliances.unwrap_or_default();
    let gameplay_policy = builder.gameplay_policy.unwrap_or_default();
    let data_access_permissions = builder.data_access_permissions.unwrap_or_default();
    let economic_contracts = builder.economic_contracts.unwrap_or_default();
    let agent_claims = builder.agent_claims.unwrap_or_default();
    let agent_claim_acquisition_sequences = builder
        .agent_claim_acquisition_sequences
        .unwrap_or_default();
    let starter_oc_claims = builder.starter_oc_claims.unwrap_or_default();
    let authenticated_collect_data_last_nonces = builder
        .authenticated_collect_data_last_nonces
        .unwrap_or_default();
    let agent_claim_last_processed_epoch =
        builder.agent_claim_last_processed_epoch.unwrap_or_default();
    let contract_pair_last_success_settled_at = builder
        .contract_pair_last_success_settled_at
        .unwrap_or_default();
    let reputation_reward_window_started_at = builder
        .reputation_reward_window_started_at
        .unwrap_or_default();
    let reputation_reward_window_accumulated = builder
        .reputation_reward_window_accumulated
        .unwrap_or_default();
    let reputation_scores = builder.reputation_scores.unwrap_or_default();
    let wars = builder.wars.unwrap_or_default();
    let governance_votes = builder.governance_votes.unwrap_or_default();
    let governance_proposals = builder.governance_proposals.unwrap_or_default();
    let governance_identity_profiles = builder.governance_identity_profiles.unwrap_or_default();
    let crises = builder.crises.unwrap_or_default();
    let meta_progress = builder.meta_progress.unwrap_or_default();
    let module_states = builder.module_states.unwrap_or_default();
    let module_visual_entities = builder.module_visual_entities.unwrap_or_default();
    let module_artifact_owners = builder.module_artifact_owners.unwrap_or_default();
    let module_artifact_listings = builder.module_artifact_listings.unwrap_or_default();
    let module_artifact_bids = builder.module_artifact_bids.unwrap_or_default();
    let module_instances = builder.module_instances.unwrap_or_default();
    let module_admission_freezes = builder.module_admission_freezes.unwrap_or_default();
    let module_release_requests = builder.module_release_requests.unwrap_or_default();
    let module_release_manifest_mappings =
        builder.module_release_manifest_mappings.unwrap_or_default();
    let next_module_release_request_id = builder
        .next_module_release_request_id
        .unwrap_or_else(default_next_module_release_request_id);
    let module_release_role_bindings = builder.module_release_role_bindings.unwrap_or_default();
    let installed_module_targets = builder.installed_module_targets.unwrap_or_default();
    let next_module_instance_id = builder
        .next_module_instance_id
        .unwrap_or_else(default_next_module_instance_id);
    let next_module_market_order_id = builder
        .next_module_market_order_id
        .unwrap_or_else(default_module_market_order_id);
    let next_module_market_sale_id = builder
        .next_module_market_sale_id
        .unwrap_or_else(default_module_market_sale_id);
    let main_token_config = builder.main_token_config.unwrap_or_default();
    let main_token_supply = builder.main_token_supply.unwrap_or_default();
    let main_token_balances = builder.main_token_balances.unwrap_or_default();
    let restricted_starter_claim_grants =
        builder.restricted_starter_claim_grants.unwrap_or_default();
    let main_token_genesis_buckets = builder.main_token_genesis_buckets.unwrap_or_default();
    let main_token_epoch_issuance_records = builder
        .main_token_epoch_issuance_records
        .unwrap_or_default();
    let main_token_treasury_balances = builder.main_token_treasury_balances.unwrap_or_default();
    let main_token_claim_nonces = builder.main_token_claim_nonces.unwrap_or_default();
    let main_token_transfer_nonces = builder.main_token_transfer_nonces.unwrap_or_default();
    let main_token_scheduled_policy_updates = builder
        .main_token_scheduled_policy_updates
        .unwrap_or_default();
    let main_token_node_points_bridge_records = builder
        .main_token_node_points_bridge_records
        .unwrap_or_default();
    let main_token_treasury_distribution_records = builder
        .main_token_treasury_distribution_records
        .unwrap_or_default();
    let restricted_starter_claim_liveops_pool_top_up_records = builder
        .restricted_starter_claim_liveops_pool_top_up_records
        .unwrap_or_default();
    let reward_asset_config = builder.reward_asset_config.unwrap_or_default();
    let node_asset_balances = builder.node_asset_balances.unwrap_or_default();
    let protocol_power_reserve = builder.protocol_power_reserve.unwrap_or_default();
    let reward_mint_records = builder.reward_mint_records.unwrap_or_default();
    let node_redeem_nonces = builder.node_redeem_nonces.unwrap_or_default();
    let system_order_pool_budgets = builder.system_order_pool_budgets.unwrap_or_default();
    let node_identity_bindings = builder.node_identity_bindings.unwrap_or_default();
    let node_main_token_account_bindings =
        builder.node_main_token_account_bindings.unwrap_or_default();
    let governance_finality_signer_registry = builder
        .governance_finality_signer_registry
        .unwrap_or_default();
    let governance_validator_admissions =
        builder.governance_validator_admissions.unwrap_or_default();
    let governance_main_token_controller_registry = builder
        .governance_main_token_controller_registry
        .unwrap_or_default();
    let governance_authorization_nonces =
        builder.governance_authorization_nonces.unwrap_or_default();
    let reward_signature_governance_policy = builder
        .reward_signature_governance_policy
        .unwrap_or_default();
    Ok(WorldState {
        world_rules,
        time,
        retained_history_root,
        agents,
        agent_intent_ledger,
        resources,
        materials,
        material_ledgers,
        material_profiles,
        logistics_routes,
        completed_logistics_route_ids,
        completed_logistics_paths,
        settled_logistics_transit_ids,
        logistics_settlement_receipts,
        factory_production_failure_dispositions,
        direct_material_transfer_receipts,
        product_validation_receipts,
        product_profiles,
        latest_product_validation,
        recipe_profiles,
        factory_profiles,
        agent_location_authorities,
        location_anchors,
        factory_site_authorities,
        factory_construction_power_profiles,
        factories,
        retired_factory_ids,
        settled_factory_build_ids,
        factory_construction_receipts,
        product_validation_attempts,
        product_validation_delivery_cursor,
        pending_factory_builds,
        pending_recipe_jobs,
        settled_recipe_job_ids,
        next_industry_settlement_order,
        industry_settlement_orders,
        recipe_completion_receipts,
        factory_recycle_receipts,
        pending_material_transits,
        industry_progress,
        alliances,
        gameplay_policy,
        data_access_permissions,
        economic_contracts,
        agent_claims,
        agent_claim_acquisition_sequences,
        starter_oc_claims,
        authenticated_collect_data_last_nonces,
        agent_claim_last_processed_epoch,
        contract_pair_last_success_settled_at,
        reputation_reward_window_started_at,
        reputation_reward_window_accumulated,
        reputation_scores,
        wars,
        governance_votes,
        governance_proposals,
        governance_identity_profiles,
        crises,
        meta_progress,
        module_states,
        module_visual_entities,
        module_artifact_owners,
        module_artifact_listings,
        module_artifact_bids,
        module_instances,
        module_admission_freezes,
        module_release_requests,
        module_release_manifest_mappings,
        next_module_release_request_id,
        module_release_role_bindings,
        installed_module_targets,
        next_module_instance_id,
        next_module_market_order_id,
        next_module_market_sale_id,
        main_token_config,
        main_token_supply,
        main_token_balances,
        restricted_starter_claim_grants,
        main_token_genesis_buckets,
        main_token_epoch_issuance_records,
        main_token_treasury_balances,
        main_token_claim_nonces,
        main_token_transfer_nonces,
        main_token_scheduled_policy_updates,
        main_token_node_points_bridge_records,
        main_token_treasury_distribution_records,
        restricted_starter_claim_liveops_pool_top_up_records,
        reward_asset_config,
        node_asset_balances,
        protocol_power_reserve,
        reward_mint_records,
        node_redeem_nonces,
        system_order_pool_budgets,
        node_identity_bindings,
        node_main_token_account_bindings,
        governance_finality_signer_registry,
        governance_validator_admissions,
        governance_main_token_controller_registry,
        governance_authorization_nonces,
        reward_signature_governance_policy,
    })
}

struct U64KeyMapSeed<V>(PhantomData<V>);

impl<'de, V> DeserializeSeed<'de> for U64KeyMapSeed<V>
where
    V: Deserialize<'de>,
{
    type Value = BTreeMap<u64, V>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_btreemap_u64_keys(deserializer)
    }
}

struct WorldStateVisitor;

impl WorldStateVisitor {
    fn missing_sequence_field<E: de::Error>(index: usize) -> E {
        E::invalid_length(index, &WorldStateVisitor)
    }
}

impl<'de> Visitor<'de> for WorldStateVisitor {
    type Value = WorldState;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("struct WorldState with 100 elements")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut builder = WorldStateBuilder::default();
        let mut seen = [false; 100];
        while let Some(field) = map.next_key::<Field>()? {
            let index = field.index();
            if index == FIELDS.len() {
                let _ = map.next_value::<IgnoredAny>()?;
                continue;
            }
            if seen[index] {
                return Err(de::Error::duplicate_field(field.name()));
            }
            seen[index] = true;
            map_groups::read(field, &mut map, &mut builder)?;
        }
        finish_state::<A::Error>(builder)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut builder = WorldStateBuilder::default();
        seq_groups::read_all(&mut seq, &mut builder)?;
        if seq.next_element::<IgnoredAny>()?.is_some() {
            return Err(de::Error::invalid_length(101, &"fewer elements in array"));
        }
        finish_state::<A::Error>(builder)
    }
}

impl<'de> Deserialize<'de> for WorldState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct("WorldState", FIELDS, WorldStateVisitor)
    }
}

#[cfg(test)]
#[path = "world_state_deserialize_tests.rs"]
mod tests;

#[path = "world_state_deserialize_map.rs"]
mod map_groups;
#[path = "world_state_deserialize_seq.rs"]
mod seq_groups;
