// Action's adjacent-tag Serialize contract, split by responsibility and domain.
use super::Action;
use serde::Serialize;
use serde::ser::Serializer;

#[path = "action_serialize_core_world.rs"]
mod core_world;
#[path = "action_serialize_economic_contracts.rs"]
mod economic_contracts;
#[path = "action_serialize_governance.rs"]
mod governance;
#[path = "action_serialize_industry.rs"]
mod industry;
#[path = "action_serialize_main_token.rs"]
mod main_token;
#[path = "action_serialize_material_logistics_alliance.rs"]
mod material_logistics_alliance;
#[path = "action_serialize_module_artifacts.rs"]
mod module_artifacts;
#[path = "action_serialize_payloads.rs"]
mod payloads;
#[path = "action_serialize_resource_data_power.rs"]
mod resource_data_power;
#[path = "action_serialize_starter_claims.rs"]
mod starter_claims;

struct ActionTag {
    index: u32,
    name: &'static str,
}

impl ActionTag {
    const WORLD_RULE: Self = Self {
        index: 0,
        name: "WorldRule",
    };
    const WORLD_SERVICE_INTENT: Self = Self {
        index: 1,
        name: "WorldServiceIntent",
    };
    const REGISTER_AGENT: Self = Self {
        index: 2,
        name: "RegisterAgent",
    };
    const MOVE_AGENT: Self = Self {
        index: 3,
        name: "MoveAgent",
    };
    const QUERY_OBSERVATION: Self = Self {
        index: 4,
        name: "QueryObservation",
    };
    const EMIT_OBSERVATION: Self = Self {
        index: 5,
        name: "EmitObservation",
    };
    const BODY_ACTION: Self = Self {
        index: 6,
        name: "BodyAction",
    };
    const EMIT_BODY_ATTRIBUTES: Self = Self {
        index: 7,
        name: "EmitBodyAttributes",
    };
    const EXPAND_BODY_INTERFACE: Self = Self {
        index: 8,
        name: "ExpandBodyInterface",
    };
    const DEPLOY_MODULE_ARTIFACT: Self = Self {
        index: 9,
        name: "DeployModuleArtifact",
    };
    const COMPILE_MODULE_ARTIFACT_FROM_SOURCE: Self = Self {
        index: 10,
        name: "CompileModuleArtifactFromSource",
    };
    const INSTALL_MODULE_FROM_ARTIFACT: Self = Self {
        index: 11,
        name: "InstallModuleFromArtifact",
    };
    const INSTALL_MODULE_FROM_ARTIFACT_WITH_FINALITY: Self = Self {
        index: 12,
        name: "InstallModuleFromArtifactWithFinality",
    };
    const INSTALL_MODULE_TO_TARGET_FROM_ARTIFACT: Self = Self {
        index: 13,
        name: "InstallModuleToTargetFromArtifact",
    };
    const INSTALL_MODULE_TO_TARGET_FROM_ARTIFACT_WITH_FINALITY: Self = Self {
        index: 14,
        name: "InstallModuleToTargetFromArtifactWithFinality",
    };
    const UPGRADE_MODULE_FROM_ARTIFACT: Self = Self {
        index: 15,
        name: "UpgradeModuleFromArtifact",
    };
    const UPGRADE_MODULE_FROM_ARTIFACT_WITH_FINALITY: Self = Self {
        index: 16,
        name: "UpgradeModuleFromArtifactWithFinality",
    };
    const MODULE_RELEASE_SUBMIT: Self = Self {
        index: 17,
        name: "ModuleReleaseSubmit",
    };
    const MODULE_RELEASE_SHADOW: Self = Self {
        index: 18,
        name: "ModuleReleaseShadow",
    };
    const MODULE_RELEASE_SUBMIT_ATTESTATION: Self = Self {
        index: 19,
        name: "ModuleReleaseSubmitAttestation",
    };
    const MODULE_RELEASE_APPROVE_ROLE: Self = Self {
        index: 20,
        name: "ModuleReleaseApproveRole",
    };
    const MODULE_RELEASE_BIND_ROLES: Self = Self {
        index: 21,
        name: "ModuleReleaseBindRoles",
    };
    const MODULE_RELEASE_REJECT: Self = Self {
        index: 22,
        name: "ModuleReleaseReject",
    };
    const MODULE_RELEASE_APPLY: Self = Self {
        index: 23,
        name: "ModuleReleaseApply",
    };
    const MODULE_RELEASE_APPLY_WITH_FINALITY: Self = Self {
        index: 24,
        name: "ModuleReleaseApplyWithFinality",
    };
    const ROLLBACK_MODULE_INSTANCE: Self = Self {
        index: 25,
        name: "RollbackModuleInstance",
    };
    const ROLLBACK_MODULE_INSTANCE_WITH_FINALITY: Self = Self {
        index: 26,
        name: "RollbackModuleInstanceWithFinality",
    };
    const LIST_MODULE_ARTIFACT_FOR_SALE: Self = Self {
        index: 27,
        name: "ListModuleArtifactForSale",
    };
    const BUY_MODULE_ARTIFACT: Self = Self {
        index: 28,
        name: "BuyModuleArtifact",
    };
    const DELIST_MODULE_ARTIFACT: Self = Self {
        index: 29,
        name: "DelistModuleArtifact",
    };
    const DESTROY_MODULE_ARTIFACT: Self = Self {
        index: 30,
        name: "DestroyModuleArtifact",
    };
    const PLACE_MODULE_ARTIFACT_BID: Self = Self {
        index: 31,
        name: "PlaceModuleArtifactBid",
    };
    const CANCEL_MODULE_ARTIFACT_BID: Self = Self {
        index: 32,
        name: "CancelModuleArtifactBid",
    };
    const TRANSFER_RESOURCE: Self = Self {
        index: 33,
        name: "TransferResource",
    };
    const COLLECT_DATA: Self = Self {
        index: 34,
        name: "CollectData",
    };
    const COLLECT_DATA_AUTHENTICATED: Self = Self {
        index: 35,
        name: "CollectDataAuthenticated",
    };
    const GRANT_DATA_ACCESS: Self = Self {
        index: 36,
        name: "GrantDataAccess",
    };
    const REVOKE_DATA_ACCESS: Self = Self {
        index: 37,
        name: "RevokeDataAccess",
    };
    const REDEEM_POWER: Self = Self {
        index: 38,
        name: "RedeemPower",
    };
    const REDEEM_POWER_SIGNED: Self = Self {
        index: 39,
        name: "RedeemPowerSigned",
    };
    const APPLY_NODE_POINTS_SETTLEMENT_SIGNED: Self = Self {
        index: 40,
        name: "ApplyNodePointsSettlementSigned",
    };
    const INITIALIZE_MAIN_TOKEN_GENESIS: Self = Self {
        index: 41,
        name: "InitializeMainTokenGenesis",
    };
    const CLAIM_MAIN_TOKEN_VESTING: Self = Self {
        index: 42,
        name: "ClaimMainTokenVesting",
    };
    const TRANSFER_MAIN_TOKEN: Self = Self {
        index: 43,
        name: "TransferMainToken",
    };
    const APPLY_MAIN_TOKEN_EPOCH_ISSUANCE: Self = Self {
        index: 44,
        name: "ApplyMainTokenEpochIssuance",
    };
    const SETTLE_MAIN_TOKEN_FEE: Self = Self {
        index: 45,
        name: "SettleMainTokenFee",
    };
    const UPDATE_MAIN_TOKEN_POLICY: Self = Self {
        index: 46,
        name: "UpdateMainTokenPolicy",
    };
    const DISTRIBUTE_MAIN_TOKEN_TREASURY: Self = Self {
        index: 47,
        name: "DistributeMainTokenTreasury",
    };
    const TOP_UP_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL: Self = Self {
        index: 48,
        name: "TopUpRestrictedStarterClaimLiveopsPool",
    };
    const ISSUE_RESTRICTED_STARTER_CLAIM_GRANT: Self = Self {
        index: 49,
        name: "IssueRestrictedStarterClaimGrant",
    };
    const REVOKE_RESTRICTED_STARTER_CLAIM_GRANT: Self = Self {
        index: 50,
        name: "RevokeRestrictedStarterClaimGrant",
    };
    const CLAIM_AGENT: Self = Self {
        index: 51,
        name: "ClaimAgent",
    };
    const CLAIM_STARTER_OC: Self = Self {
        index: 52,
        name: "ClaimStarterOc",
    };
    const RELEASE_AGENT_CLAIM: Self = Self {
        index: 53,
        name: "ReleaseAgentClaim",
    };
    const TRANSFER_MATERIAL: Self = Self {
        index: 54,
        name: "TransferMaterial",
    };
    const REGISTER_LOGISTICS_ROUTE: Self = Self {
        index: 55,
        name: "RegisterLogisticsRoute",
    };
    const SET_LOGISTICS_ROUTE_AVAILABILITY: Self = Self {
        index: 56,
        name: "SetLogisticsRouteAvailability",
    };
    const FORM_ALLIANCE: Self = Self {
        index: 57,
        name: "FormAlliance",
    };
    const JOIN_ALLIANCE: Self = Self {
        index: 58,
        name: "JoinAlliance",
    };
    const LEAVE_ALLIANCE: Self = Self {
        index: 59,
        name: "LeaveAlliance",
    };
    const DISSOLVE_ALLIANCE: Self = Self {
        index: 60,
        name: "DissolveAlliance",
    };
    const DECLARE_WAR: Self = Self {
        index: 61,
        name: "DeclareWar",
    };
    const OPEN_GOVERNANCE_PROPOSAL: Self = Self {
        index: 62,
        name: "OpenGovernanceProposal",
    };
    const CAST_GOVERNANCE_VOTE: Self = Self {
        index: 63,
        name: "CastGovernanceVote",
    };
    const RESOLVE_CRISIS: Self = Self {
        index: 64,
        name: "ResolveCrisis",
    };
    const GRANT_META_PROGRESS: Self = Self {
        index: 65,
        name: "GrantMetaProgress",
    };
    const UPDATE_GAMEPLAY_POLICY: Self = Self {
        index: 66,
        name: "UpdateGameplayPolicy",
    };
    const UPDATE_RESTRICTED_STARTER_CLAIM_ADMIN_REGISTRY: Self = Self {
        index: 67,
        name: "UpdateRestrictedStarterClaimAdminRegistry",
    };
    const SUBMIT_GOVERNANCE_VALIDATOR_ADMISSION: Self = Self {
        index: 68,
        name: "SubmitGovernanceValidatorAdmission",
    };
    const APPROVE_GOVERNANCE_VALIDATOR_ADMISSION: Self = Self {
        index: 69,
        name: "ApproveGovernanceValidatorAdmission",
    };
    const ACTIVATE_GOVERNANCE_VALIDATOR_ADMISSION: Self = Self {
        index: 70,
        name: "ActivateGovernanceValidatorAdmission",
    };
    const REVOKE_GOVERNANCE_VALIDATOR_ADMISSION: Self = Self {
        index: 71,
        name: "RevokeGovernanceValidatorAdmission",
    };
    const OPEN_ECONOMIC_CONTRACT: Self = Self {
        index: 72,
        name: "OpenEconomicContract",
    };
    const ACCEPT_ECONOMIC_CONTRACT: Self = Self {
        index: 73,
        name: "AcceptEconomicContract",
    };
    const SETTLE_ECONOMIC_CONTRACT: Self = Self {
        index: 74,
        name: "SettleEconomicContract",
    };
    const EMIT_RESOURCE_TRANSFER: Self = Self {
        index: 75,
        name: "EmitResourceTransfer",
    };
    const BUILD_FACTORY: Self = Self {
        index: 76,
        name: "BuildFactory",
    };
    const BUILD_FACTORY_WITH_MODULE: Self = Self {
        index: 77,
        name: "BuildFactoryWithModule",
    };
    const MAINTAIN_FACTORY: Self = Self {
        index: 78,
        name: "MaintainFactory",
    };
    const RECYCLE_FACTORY: Self = Self {
        index: 79,
        name: "RecycleFactory",
    };
    const SCHEDULE_ESSENTIAL_RECIPE: Self = Self {
        index: 80,
        name: "ScheduleEssentialRecipe",
    };
    const SCHEDULE_RECIPE: Self = Self {
        index: 81,
        name: "ScheduleRecipe",
    };
    const PAUSE_FACTORY_PRODUCTION: Self = Self {
        index: 82,
        name: "PauseFactoryProduction",
    };
    const SCHEDULE_RECIPE_WITH_MODULE: Self = Self {
        index: 83,
        name: "ScheduleRecipeWithModule",
    };
    const VALIDATE_PRODUCT: Self = Self {
        index: 84,
        name: "ValidateProduct",
    };
    const VALIDATE_PRODUCT_WITH_MODULE: Self = Self {
        index: 85,
        name: "ValidateProductWithModule",
    };
    const GOVERN_MATERIAL_PROFILE: Self = Self {
        index: 86,
        name: "GovernMaterialProfile",
    };
    const GOVERN_PRODUCT_PROFILE: Self = Self {
        index: 87,
        name: "GovernProductProfile",
    };
    const GOVERN_RECIPE_PROFILE: Self = Self {
        index: 88,
        name: "GovernRecipeProfile",
    };
    const GOVERN_FACTORY_PROFILE: Self = Self {
        index: 89,
        name: "GovernFactoryProfile",
    };
}

impl serde::Serialize for ActionTag {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit_variant("Action", self.index, self.name)
    }
}

#[derive(Clone, Copy)]
enum ActionSerializeGroup {
    CoreWorld,
    ModuleArtifacts,
    ResourceDataPower,
    MainToken,
    StarterClaims,
    MaterialLogisticsAlliance,
    Governance,
    EconomicContracts,
    Industry,
}

#[derive(Serialize)]
#[serde(rename = "Action")]
struct ActionEnvelope<T> {
    #[serde(rename = "type")]
    tag: ActionTag,
    #[serde(rename = "data")]
    data: T,
}

fn serialize_action_envelope<S, T>(
    serializer: S,
    tag: ActionTag,
    data: T,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    ActionEnvelope { tag, data }.serialize(serializer)
}

fn action_serialize_group(action: &Action) -> ActionSerializeGroup {
    match action {
        Action::WorldRule { .. }
        | Action::WorldServiceIntent { .. }
        | Action::RegisterAgent { .. }
        | Action::MoveAgent { .. }
        | Action::QueryObservation { .. }
        | Action::EmitObservation { .. }
        | Action::BodyAction { .. }
        | Action::EmitBodyAttributes { .. }
        | Action::ExpandBodyInterface { .. } => ActionSerializeGroup::CoreWorld,
        Action::DeployModuleArtifact { .. }
        | Action::CompileModuleArtifactFromSource { .. }
        | Action::InstallModuleFromArtifact { .. }
        | Action::InstallModuleFromArtifactWithFinality { .. }
        | Action::InstallModuleToTargetFromArtifact { .. }
        | Action::InstallModuleToTargetFromArtifactWithFinality { .. }
        | Action::UpgradeModuleFromArtifact { .. }
        | Action::UpgradeModuleFromArtifactWithFinality { .. }
        | Action::ModuleReleaseSubmit { .. }
        | Action::ModuleReleaseShadow { .. }
        | Action::ModuleReleaseSubmitAttestation { .. }
        | Action::ModuleReleaseApproveRole { .. }
        | Action::ModuleReleaseBindRoles { .. }
        | Action::ModuleReleaseReject { .. }
        | Action::ModuleReleaseApply { .. }
        | Action::ModuleReleaseApplyWithFinality { .. }
        | Action::RollbackModuleInstance { .. }
        | Action::RollbackModuleInstanceWithFinality { .. }
        | Action::ListModuleArtifactForSale { .. }
        | Action::BuyModuleArtifact { .. }
        | Action::DelistModuleArtifact { .. }
        | Action::DestroyModuleArtifact { .. }
        | Action::PlaceModuleArtifactBid { .. }
        | Action::CancelModuleArtifactBid { .. } => ActionSerializeGroup::ModuleArtifacts,
        Action::TransferResource { .. }
        | Action::CollectData { .. }
        | Action::CollectDataAuthenticated { .. }
        | Action::GrantDataAccess { .. }
        | Action::RevokeDataAccess { .. }
        | Action::RedeemPower { .. }
        | Action::RedeemPowerSigned { .. }
        | Action::ApplyNodePointsSettlementSigned { .. } => ActionSerializeGroup::ResourceDataPower,
        Action::InitializeMainTokenGenesis { .. }
        | Action::ClaimMainTokenVesting { .. }
        | Action::TransferMainToken { .. }
        | Action::ApplyMainTokenEpochIssuance { .. }
        | Action::SettleMainTokenFee { .. }
        | Action::UpdateMainTokenPolicy { .. }
        | Action::DistributeMainTokenTreasury { .. } => ActionSerializeGroup::MainToken,
        Action::TopUpRestrictedStarterClaimLiveopsPool { .. }
        | Action::IssueRestrictedStarterClaimGrant { .. }
        | Action::RevokeRestrictedStarterClaimGrant { .. }
        | Action::ClaimAgent { .. }
        | Action::ClaimStarterOc { .. }
        | Action::ReleaseAgentClaim { .. } => ActionSerializeGroup::StarterClaims,
        Action::TransferMaterial { .. }
        | Action::RegisterLogisticsRoute { .. }
        | Action::SetLogisticsRouteAvailability { .. }
        | Action::FormAlliance { .. }
        | Action::JoinAlliance { .. }
        | Action::LeaveAlliance { .. }
        | Action::DissolveAlliance { .. }
        | Action::DeclareWar { .. } => ActionSerializeGroup::MaterialLogisticsAlliance,
        Action::OpenGovernanceProposal { .. }
        | Action::CastGovernanceVote { .. }
        | Action::ResolveCrisis { .. }
        | Action::GrantMetaProgress { .. }
        | Action::UpdateGameplayPolicy { .. }
        | Action::UpdateRestrictedStarterClaimAdminRegistry { .. }
        | Action::SubmitGovernanceValidatorAdmission { .. }
        | Action::ApproveGovernanceValidatorAdmission { .. }
        | Action::ActivateGovernanceValidatorAdmission { .. }
        | Action::RevokeGovernanceValidatorAdmission { .. } => ActionSerializeGroup::Governance,
        Action::OpenEconomicContract { .. }
        | Action::AcceptEconomicContract { .. }
        | Action::SettleEconomicContract { .. } => ActionSerializeGroup::EconomicContracts,
        Action::EmitResourceTransfer { .. }
        | Action::BuildFactory { .. }
        | Action::BuildFactoryWithModule { .. }
        | Action::MaintainFactory { .. }
        | Action::RecycleFactory { .. }
        | Action::ScheduleEssentialRecipe { .. }
        | Action::ScheduleRecipe { .. }
        | Action::PauseFactoryProduction { .. }
        | Action::ScheduleRecipeWithModule { .. }
        | Action::ValidateProduct { .. }
        | Action::ValidateProductWithModule { .. }
        | Action::GovernMaterialProfile { .. }
        | Action::GovernProductProfile { .. }
        | Action::GovernRecipeProfile { .. }
        | Action::GovernFactoryProfile { .. } => ActionSerializeGroup::Industry,
    }
}

impl Serialize for Action {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match action_serialize_group(self) {
            ActionSerializeGroup::CoreWorld => core_world::serialize(self, serializer),
            ActionSerializeGroup::ModuleArtifacts => module_artifacts::serialize(self, serializer),
            ActionSerializeGroup::ResourceDataPower => {
                resource_data_power::serialize(self, serializer)
            }
            ActionSerializeGroup::MainToken => main_token::serialize(self, serializer),
            ActionSerializeGroup::StarterClaims => starter_claims::serialize(self, serializer),
            ActionSerializeGroup::MaterialLogisticsAlliance => {
                material_logistics_alliance::serialize(self, serializer)
            }
            ActionSerializeGroup::Governance => governance::serialize(self, serializer),
            ActionSerializeGroup::EconomicContracts => {
                economic_contracts::serialize(self, serializer)
            }
            ActionSerializeGroup::Industry => industry::serialize(self, serializer),
        }
    }
}
