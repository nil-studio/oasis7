//! Legacy adjacent-tag wire encoding split into bounded borrowed payloads.
use super::*;
use serde::ser::SerializeStruct as _;

mod data_power;
mod governance;
mod logistics;
mod module_lifecycle;
mod production;
mod token_claims;
mod validation;
mod world_agent;

#[derive(serde::Serialize)]
#[serde(rename = "DomainEvent")]
enum Tag {
    WorldRuleCommitted,
    AgentRegistered,
    AgentMoved,
    AgentLocationAuthorityUpdated,
    LocationAnchorUpdated,
    FactorySiteAuthorityUpdated,
    FactoryConstructionPowerProfileUpdated,
    ActionAccepted,
    ActionRejected,
    AgentIntentProposed,
    AgentIntentSubmitted,
    AgentIntentAccepted,
    AgentIntentReplaced,
    AgentIntentTransitioned,
    Observation,
    BodyAttributesUpdated,
    BodyAttributesRejected,
    BodyInterfaceExpanded,
    BodyInterfaceExpandRejected,
    ModuleArtifactDeployed,
    ModuleInstalled,
    ModuleUpgraded,
    ModuleReleaseRequested,
    ModuleReleaseShadowed,
    ModuleReleaseAttested,
    ModuleReleaseRoleApproved,
    ModuleReleaseRolesBound,
    ModuleReleaseRejected,
    ModuleReleaseApplied,
    ModuleRollbackApplied,
    ModuleArtifactListed,
    ModuleArtifactDelisted,
    ModuleArtifactDestroyed,
    ModuleArtifactBidPlaced,
    ModuleArtifactBidCancelled,
    ModuleArtifactSaleCompleted,
    ResourceTransferred,
    DataCollected,
    DataCollectedAuthenticated,
    DataAccessGranted,
    DataAccessRevoked,
    PowerRedeemed,
    PowerRedeemRejected,
    NodePointsSettlementApplied,
    MainTokenGenesisInitialized,
    MainTokenVestingClaimed,
    MainTokenTransferred,
    MainTokenEpochIssued,
    MainTokenFeeSettled,
    MainTokenPolicyUpdateScheduled,
    MainTokenTreasuryDistributed,
    RestrictedStarterClaimLiveopsPoolToppedUp,
    RestrictedStarterClaimGrantIssued,
    RestrictedStarterClaimGrantExpired,
    RestrictedStarterClaimGrantRevoked,
    StarterOcClaimed,
    AgentClaimed,
    AgentClaimReleaseRequested,
    AgentClaimUpkeepSettled,
    AgentClaimEnteredGrace,
    AgentClaimIdleWarning,
    AgentClaimReleased,
    AgentClaimReclaimed,
    MaterialTransferred,
    LogisticsRouteRegistered,
    LogisticsRouteAvailabilityChanged,
    LogisticsPathRerouted,
    MaterialTransitStarted,
    MaterialTransitCompleted,
    FactoryBuildStarted,
    FactoryBuilt,
    FactoryDurabilityChanged,
    FactoryMaintained,
    FactoryRecycled,
    RecipeStarted,
    RecipeCompleted,
    FactoryProductionBlocked,
    FactoryProductionResumed,
    FactoryProductionPaused,
    GameplayPolicyUpdated,
    EconomicContractOpened,
    EconomicContractAccepted,
    EconomicContractSettled,
    EconomicContractExpired,
    AllianceFormed,
    AllianceJoined,
    AllianceLeft,
    AllianceDissolved,
    WarDeclared,
    WarConcluded,
    GovernanceProposalOpened,
    GovernanceVoteCast,
    GovernanceProposalFinalized,
    CrisisSpawned,
    CrisisResolved,
    CrisisTimedOut,
    MetaProgressGranted,
    ProductValidated,
    ProductValidationRecorded,
    ProductValidationAttemptStarted,
    MaterialProfileGoverned,
    ProductProfileGoverned,
    RecipeProfileGoverned,
    FactoryProfileGoverned,
}

fn serialize_adjacent<S: serde::Serializer, T: serde::Serialize>(
    serializer: S,
    tag: Tag,
    payload: &T,
) -> Result<S::Ok, S::Error> {
    let mut output = serializer.serialize_struct("DomainEvent", 2)?;
    output.serialize_field("type", &tag)?;
    output.serialize_field("data", payload)?;
    output.end()
}

impl serde::Serialize for DomainEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::WorldRuleCommitted { .. }
            | Self::AgentRegistered { .. }
            | Self::AgentMoved { .. }
            | Self::AgentLocationAuthorityUpdated { .. }
            | Self::LocationAnchorUpdated { .. }
            | Self::FactorySiteAuthorityUpdated { .. }
            | Self::FactoryConstructionPowerProfileUpdated { .. }
            | Self::ActionAccepted { .. }
            | Self::ActionRejected { .. }
            | Self::AgentIntentProposed { .. }
            | Self::AgentIntentSubmitted { .. }
            | Self::AgentIntentAccepted { .. }
            | Self::AgentIntentReplaced { .. }
            | Self::AgentIntentTransitioned { .. }
            | Self::Observation { .. }
            | Self::BodyAttributesUpdated { .. }
            | Self::BodyAttributesRejected { .. }
            | Self::BodyInterfaceExpanded { .. }
            | Self::BodyInterfaceExpandRejected { .. } => world_agent::serialize(self, serializer),
            Self::ModuleArtifactDeployed { .. }
            | Self::ModuleInstalled { .. }
            | Self::ModuleUpgraded { .. }
            | Self::ModuleReleaseRequested { .. }
            | Self::ModuleReleaseShadowed { .. }
            | Self::ModuleReleaseAttested { .. }
            | Self::ModuleReleaseRoleApproved { .. }
            | Self::ModuleReleaseRolesBound { .. }
            | Self::ModuleReleaseRejected { .. }
            | Self::ModuleReleaseApplied { .. }
            | Self::ModuleRollbackApplied { .. }
            | Self::ModuleArtifactListed { .. }
            | Self::ModuleArtifactDelisted { .. }
            | Self::ModuleArtifactDestroyed { .. }
            | Self::ModuleArtifactBidPlaced { .. }
            | Self::ModuleArtifactBidCancelled { .. }
            | Self::ModuleArtifactSaleCompleted { .. } => {
                module_lifecycle::serialize(self, serializer)
            }
            Self::ResourceTransferred { .. }
            | Self::DataCollected { .. }
            | Self::DataCollectedAuthenticated { .. }
            | Self::DataAccessGranted { .. }
            | Self::DataAccessRevoked { .. }
            | Self::PowerRedeemed { .. }
            | Self::PowerRedeemRejected { .. } => data_power::serialize(self, serializer),
            Self::NodePointsSettlementApplied { .. }
            | Self::MainTokenGenesisInitialized { .. }
            | Self::MainTokenVestingClaimed { .. }
            | Self::MainTokenTransferred { .. }
            | Self::MainTokenEpochIssued { .. }
            | Self::MainTokenFeeSettled { .. }
            | Self::MainTokenPolicyUpdateScheduled { .. }
            | Self::MainTokenTreasuryDistributed { .. }
            | Self::RestrictedStarterClaimLiveopsPoolToppedUp { .. }
            | Self::RestrictedStarterClaimGrantIssued { .. }
            | Self::RestrictedStarterClaimGrantExpired { .. }
            | Self::RestrictedStarterClaimGrantRevoked { .. }
            | Self::StarterOcClaimed { .. }
            | Self::AgentClaimed { .. }
            | Self::AgentClaimReleaseRequested { .. }
            | Self::AgentClaimUpkeepSettled { .. }
            | Self::AgentClaimEnteredGrace { .. }
            | Self::AgentClaimIdleWarning { .. }
            | Self::AgentClaimReleased { .. }
            | Self::AgentClaimReclaimed { .. } => token_claims::serialize(self, serializer),
            Self::MaterialTransferred { .. }
            | Self::LogisticsRouteRegistered { .. }
            | Self::LogisticsRouteAvailabilityChanged { .. }
            | Self::LogisticsPathRerouted { .. }
            | Self::MaterialTransitStarted { .. }
            | Self::MaterialTransitCompleted { .. } => logistics::serialize(self, serializer),
            Self::FactoryBuildStarted { .. }
            | Self::FactoryBuilt { .. }
            | Self::FactoryDurabilityChanged { .. }
            | Self::FactoryMaintained { .. }
            | Self::FactoryRecycled { .. }
            | Self::RecipeStarted { .. }
            | Self::RecipeCompleted { .. }
            | Self::FactoryProductionBlocked { .. }
            | Self::FactoryProductionResumed { .. }
            | Self::FactoryProductionPaused { .. } => production::serialize(self, serializer),
            Self::GameplayPolicyUpdated { .. }
            | Self::EconomicContractOpened { .. }
            | Self::EconomicContractAccepted { .. }
            | Self::EconomicContractSettled { .. }
            | Self::EconomicContractExpired { .. }
            | Self::AllianceFormed { .. }
            | Self::AllianceJoined { .. }
            | Self::AllianceLeft { .. }
            | Self::AllianceDissolved { .. }
            | Self::WarDeclared { .. }
            | Self::WarConcluded { .. }
            | Self::GovernanceProposalOpened { .. }
            | Self::GovernanceVoteCast { .. }
            | Self::GovernanceProposalFinalized { .. }
            | Self::CrisisSpawned { .. }
            | Self::CrisisResolved { .. }
            | Self::CrisisTimedOut { .. }
            | Self::MetaProgressGranted { .. } => governance::serialize(self, serializer),
            Self::ProductValidated { .. }
            | Self::ProductValidationRecorded { .. }
            | Self::ProductValidationAttemptStarted { .. }
            | Self::MaterialProfileGoverned { .. }
            | Self::ProductProfileGoverned { .. }
            | Self::RecipeProfileGoverned { .. }
            | Self::FactoryProfileGoverned { .. } => validation::serialize(self, serializer),
        }
    }
}

#[cfg(test)]
#[path = "serialization_tests.rs"]
mod tests;
