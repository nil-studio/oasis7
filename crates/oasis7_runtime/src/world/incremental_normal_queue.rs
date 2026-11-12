//! Exhaustive routing is inventory, never evidence that a reducer completed.
//! The fixed queue owner selects from the genuine selected original and Rule
//! outcome; no caller-supplied route/family or replacement identity is accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::world) enum NormalActionFamilyV1 {
    Core,
    Crisis,
    ContractAccept,
    MetaProgress,
    FactoryLifecycle,
    RouteAvailability,
    MaterialTransfer,
    Body,
    DataAccess,
    CollectData,
    AuthenticatedCollectData,
    TransferResource,
    WorldRule,
    WorldServiceIntent,
    CoreExtension,
    Power,
    Claims,
    Logistics,
    Diplomacy,
    Governance,
    Industry,
    MainToken,
    Module,
}
pub(in crate::world) fn normal_action_family_v1(action: &crate::Action) -> NormalActionFamilyV1 {
    match action {
        crate::Action::RegisterAgent { .. } | crate::Action::MoveAgent { .. } => {
            NormalActionFamilyV1::Core
        }
        crate::Action::ResolveCrisis { .. } => NormalActionFamilyV1::Crisis,
        crate::Action::AcceptEconomicContract { .. } => NormalActionFamilyV1::ContractAccept,
        crate::Action::GrantMetaProgress { .. } => NormalActionFamilyV1::MetaProgress,
        crate::Action::MaintainFactory { .. } | crate::Action::RecycleFactory { .. } => {
            NormalActionFamilyV1::FactoryLifecycle
        }
        crate::Action::SetLogisticsRouteAvailability { .. } => {
            NormalActionFamilyV1::RouteAvailability
        }
        crate::Action::TransferMaterial { .. } => NormalActionFamilyV1::MaterialTransfer,
        crate::Action::EmitBodyAttributes { .. }
        | crate::Action::QueryObservation { .. }
        | crate::Action::BodyAction { .. } => NormalActionFamilyV1::Body,
        crate::Action::GrantDataAccess { .. } | crate::Action::RevokeDataAccess { .. } => {
            NormalActionFamilyV1::DataAccess
        }
        crate::Action::CollectData { .. } => NormalActionFamilyV1::CollectData,
        crate::Action::CollectDataAuthenticated { .. } => {
            NormalActionFamilyV1::AuthenticatedCollectData
        }
        crate::Action::TransferResource { .. } | crate::Action::EmitResourceTransfer { .. } => {
            NormalActionFamilyV1::TransferResource
        }
        crate::Action::WorldRule(..) => NormalActionFamilyV1::WorldRule,
        crate::Action::WorldServiceIntent(..) => NormalActionFamilyV1::WorldServiceIntent,
        crate::Action::EmitObservation { .. } | crate::Action::ExpandBodyInterface { .. } => {
            NormalActionFamilyV1::CoreExtension
        }
        crate::Action::RedeemPower { .. }
        | crate::Action::RedeemPowerSigned { .. }
        | crate::Action::ApplyNodePointsSettlementSigned { .. } => NormalActionFamilyV1::Power,
        crate::Action::ClaimAgent { .. }
        | crate::Action::ClaimStarterOc { .. }
        | crate::Action::ReleaseAgentClaim { .. }
        | crate::Action::TopUpRestrictedStarterClaimLiveopsPool { .. }
        | crate::Action::IssueRestrictedStarterClaimGrant { .. }
        | crate::Action::RevokeRestrictedStarterClaimGrant { .. }
        | crate::Action::UpdateRestrictedStarterClaimAdminRegistry { .. } => {
            NormalActionFamilyV1::Claims
        }
        crate::Action::RegisterLogisticsRoute { .. } => NormalActionFamilyV1::Logistics,
        crate::Action::FormAlliance { .. }
        | crate::Action::JoinAlliance { .. }
        | crate::Action::LeaveAlliance { .. }
        | crate::Action::DissolveAlliance { .. }
        | crate::Action::DeclareWar { .. }
        | crate::Action::OpenEconomicContract { .. }
        | crate::Action::SettleEconomicContract { .. } => NormalActionFamilyV1::Diplomacy,
        crate::Action::OpenGovernanceProposal { .. }
        | crate::Action::CastGovernanceVote { .. }
        | crate::Action::UpdateGameplayPolicy { .. }
        | crate::Action::SubmitGovernanceValidatorAdmission { .. }
        | crate::Action::ApproveGovernanceValidatorAdmission { .. }
        | crate::Action::ActivateGovernanceValidatorAdmission { .. }
        | crate::Action::RevokeGovernanceValidatorAdmission { .. }
        | crate::Action::RotateGovernanceValidatorFinalityKeyV1 { .. }
        | crate::Action::RotateGovernanceBootstrapFinalityKeyV1 { .. } => {
            NormalActionFamilyV1::Governance
        }
        crate::Action::BuildFactory { .. }
        | crate::Action::BuildFactoryWithModule { .. }
        | crate::Action::ScheduleEssentialRecipe { .. }
        | crate::Action::ScheduleRecipe { .. }
        | crate::Action::PauseFactoryProduction { .. }
        | crate::Action::ScheduleRecipeWithModule { .. }
        | crate::Action::ValidateProduct { .. }
        | crate::Action::ValidateProductWithModule { .. }
        | crate::Action::GovernMaterialProfile { .. }
        | crate::Action::GovernProductProfile { .. }
        | crate::Action::GovernRecipeProfile { .. }
        | crate::Action::GovernFactoryProfile { .. } => NormalActionFamilyV1::Industry,
        crate::Action::InitializeMainTokenGenesis { .. }
        | crate::Action::ClaimMainTokenVesting { .. }
        | crate::Action::TransferMainToken { .. }
        | crate::Action::ApplyMainTokenEpochIssuance { .. }
        | crate::Action::SettleMainTokenFee { .. }
        | crate::Action::UpdateMainTokenPolicy { .. }
        | crate::Action::DistributeMainTokenTreasury { .. } => NormalActionFamilyV1::MainToken,
        crate::Action::DeployModuleArtifact { .. }
        | crate::Action::CompileModuleArtifactFromSource { .. }
        | crate::Action::InstallModuleFromArtifact { .. }
        | crate::Action::InstallModuleFromArtifactWithFinality { .. }
        | crate::Action::InstallModuleToTargetFromArtifact { .. }
        | crate::Action::InstallModuleToTargetFromArtifactWithFinality { .. }
        | crate::Action::UpgradeModuleFromArtifact { .. }
        | crate::Action::UpgradeModuleFromArtifactWithFinality { .. }
        | crate::Action::ModuleReleaseSubmit { .. }
        | crate::Action::ModuleReleaseShadow { .. }
        | crate::Action::ModuleReleaseSubmitAttestation { .. }
        | crate::Action::ModuleReleaseApproveRole { .. }
        | crate::Action::ModuleReleaseBindRoles { .. }
        | crate::Action::ModuleReleaseReject { .. }
        | crate::Action::ModuleReleaseApply { .. }
        | crate::Action::ModuleReleaseApplyWithFinality { .. }
        | crate::Action::RollbackModuleInstance { .. }
        | crate::Action::RollbackModuleInstanceWithFinality { .. }
        | crate::Action::ListModuleArtifactForSale { .. }
        | crate::Action::BuyModuleArtifact { .. }
        | crate::Action::DelistModuleArtifact { .. }
        | crate::Action::DestroyModuleArtifact { .. }
        | crate::Action::PlaceModuleArtifactBid { .. }
        | crate::Action::CancelModuleArtifactBid { .. } => NormalActionFamilyV1::Module,
    }
}
