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

#[derive(Serialize)]
#[serde(rename = "Action")]
enum ActionTag {
    WorldRule,
    WorldServiceIntent,
    RegisterAgent,
    MoveAgent,
    QueryObservation,
    EmitObservation,
    BodyAction,
    EmitBodyAttributes,
    ExpandBodyInterface,
    DeployModuleArtifact,
    CompileModuleArtifactFromSource,
    InstallModuleFromArtifact,
    InstallModuleFromArtifactWithFinality,
    InstallModuleToTargetFromArtifact,
    InstallModuleToTargetFromArtifactWithFinality,
    UpgradeModuleFromArtifact,
    UpgradeModuleFromArtifactWithFinality,
    ModuleReleaseSubmit,
    ModuleReleaseShadow,
    ModuleReleaseSubmitAttestation,
    ModuleReleaseApproveRole,
    ModuleReleaseBindRoles,
    ModuleReleaseReject,
    ModuleReleaseApply,
    ModuleReleaseApplyWithFinality,
    RollbackModuleInstance,
    RollbackModuleInstanceWithFinality,
    ListModuleArtifactForSale,
    BuyModuleArtifact,
    DelistModuleArtifact,
    DestroyModuleArtifact,
    PlaceModuleArtifactBid,
    CancelModuleArtifactBid,
    TransferResource,
    CollectData,
    CollectDataAuthenticated,
    GrantDataAccess,
    RevokeDataAccess,
    RedeemPower,
    RedeemPowerSigned,
    ApplyNodePointsSettlementSigned,
    InitializeMainTokenGenesis,
    ClaimMainTokenVesting,
    TransferMainToken,
    ApplyMainTokenEpochIssuance,
    SettleMainTokenFee,
    UpdateMainTokenPolicy,
    DistributeMainTokenTreasury,
    TopUpRestrictedStarterClaimLiveopsPool,
    IssueRestrictedStarterClaimGrant,
    RevokeRestrictedStarterClaimGrant,
    ClaimAgent,
    ClaimStarterOc,
    ReleaseAgentClaim,
    TransferMaterial,
    RegisterLogisticsRoute,
    SetLogisticsRouteAvailability,
    FormAlliance,
    JoinAlliance,
    LeaveAlliance,
    DissolveAlliance,
    DeclareWar,
    OpenGovernanceProposal,
    CastGovernanceVote,
    ResolveCrisis,
    GrantMetaProgress,
    UpdateGameplayPolicy,
    UpdateRestrictedStarterClaimAdminRegistry,
    SubmitGovernanceValidatorAdmission,
    ApproveGovernanceValidatorAdmission,
    ActivateGovernanceValidatorAdmission,
    RevokeGovernanceValidatorAdmission,
    OpenEconomicContract,
    AcceptEconomicContract,
    SettleEconomicContract,
    EmitResourceTransfer,
    BuildFactory,
    BuildFactoryWithModule,
    MaintainFactory,
    RecycleFactory,
    ScheduleEssentialRecipe,
    ScheduleRecipe,
    PauseFactoryProduction,
    ScheduleRecipeWithModule,
    ValidateProduct,
    ValidateProductWithModule,
    GovernMaterialProfile,
    GovernProductProfile,
    GovernRecipeProfile,
    GovernFactoryProfile,
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
