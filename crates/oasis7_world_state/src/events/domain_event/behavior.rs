use super::DomainEvent;
use std::fmt;

mod data_power;
mod governance;
mod logistics;
mod module_lifecycle;
mod production;
mod token_claims;
mod validation;
mod world_agent;

enum DomainEventView<'a> {
    WorldAgent(world_agent::WorldAgentView<'a>),
    ModuleLifecycle(module_lifecycle::ModuleLifecycleView<'a>),
    DataPower(data_power::DataPowerView<'a>),
    TokenClaims(token_claims::TokenClaimsView<'a>),
    Logistics(logistics::LogisticsView<'a>),
    Production(production::ProductionView<'a>),
    Governance(governance::GovernanceView<'a>),
    Validation(validation::ValidationView<'a>),
}

impl<'a> From<&'a DomainEvent> for DomainEventView<'a> {
    fn from(event: &'a DomainEvent) -> Self {
        match event {
            DomainEvent::WorldRuleCommitted { .. }
            | DomainEvent::AgentRegistered { .. }
            | DomainEvent::AgentMoved { .. }
            | DomainEvent::AgentLocationAuthorityUpdated { .. }
            | DomainEvent::LocationAnchorUpdated { .. }
            | DomainEvent::FactorySiteAuthorityUpdated { .. }
            | DomainEvent::FactoryConstructionPowerProfileUpdated { .. }
            | DomainEvent::ActionAccepted { .. }
            | DomainEvent::ActionRejected { .. }
            | DomainEvent::AgentIntentProposed { .. }
            | DomainEvent::AgentIntentSubmitted { .. }
            | DomainEvent::AgentIntentAccepted { .. }
            | DomainEvent::AgentIntentReplaced { .. }
            | DomainEvent::AgentIntentTransitioned { .. }
            | DomainEvent::Observation { .. }
            | DomainEvent::BodyAttributesUpdated { .. }
            | DomainEvent::BodyAttributesRejected { .. }
            | DomainEvent::BodyInterfaceExpanded { .. }
            | DomainEvent::BodyInterfaceExpandRejected { .. } => {
                Self::WorldAgent(world_agent::WorldAgentView::from_event(event))
            }
            DomainEvent::ModuleArtifactDeployed { .. }
            | DomainEvent::ModuleInstalled { .. }
            | DomainEvent::ModuleUpgraded { .. }
            | DomainEvent::ModuleReleaseRequested { .. }
            | DomainEvent::ModuleReleaseShadowed { .. }
            | DomainEvent::ModuleReleaseAttested { .. }
            | DomainEvent::ModuleReleaseRoleApproved { .. }
            | DomainEvent::ModuleReleaseRolesBound { .. }
            | DomainEvent::ModuleReleaseRejected { .. }
            | DomainEvent::ModuleReleaseApplied { .. }
            | DomainEvent::ModuleRollbackApplied { .. }
            | DomainEvent::ModuleArtifactListed { .. }
            | DomainEvent::ModuleArtifactDelisted { .. }
            | DomainEvent::ModuleArtifactDestroyed { .. }
            | DomainEvent::ModuleArtifactBidPlaced { .. }
            | DomainEvent::ModuleArtifactBidCancelled { .. }
            | DomainEvent::ModuleArtifactSaleCompleted { .. } => {
                Self::ModuleLifecycle(module_lifecycle::ModuleLifecycleView::from_event(event))
            }
            DomainEvent::ResourceTransferred { .. }
            | DomainEvent::DataCollected { .. }
            | DomainEvent::DataCollectedAuthenticated { .. }
            | DomainEvent::DataAccessGranted { .. }
            | DomainEvent::DataAccessRevoked { .. }
            | DomainEvent::PowerRedeemed { .. }
            | DomainEvent::PowerRedeemRejected { .. } => {
                Self::DataPower(data_power::DataPowerView::from_event(event))
            }
            DomainEvent::NodePointsSettlementApplied { .. }
            | DomainEvent::MainTokenGenesisInitialized { .. }
            | DomainEvent::MainTokenVestingClaimed { .. }
            | DomainEvent::MainTokenTransferred { .. }
            | DomainEvent::MainTokenEpochIssued { .. }
            | DomainEvent::MainTokenFeeSettled { .. }
            | DomainEvent::MainTokenPolicyUpdateScheduled { .. }
            | DomainEvent::MainTokenTreasuryDistributed { .. }
            | DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { .. }
            | DomainEvent::RestrictedStarterClaimGrantIssued { .. }
            | DomainEvent::RestrictedStarterClaimGrantExpired { .. }
            | DomainEvent::RestrictedStarterClaimGrantRevoked { .. }
            | DomainEvent::StarterOcClaimed { .. }
            | DomainEvent::AgentClaimed { .. }
            | DomainEvent::AgentClaimReleaseRequested { .. }
            | DomainEvent::AgentClaimUpkeepSettled { .. }
            | DomainEvent::AgentClaimEnteredGrace { .. }
            | DomainEvent::AgentClaimIdleWarning { .. }
            | DomainEvent::AgentClaimReleased { .. }
            | DomainEvent::AgentClaimReclaimed { .. } => {
                Self::TokenClaims(token_claims::TokenClaimsView::from_event(event))
            }
            DomainEvent::MaterialTransferred { .. }
            | DomainEvent::LogisticsRouteRegistered { .. }
            | DomainEvent::LogisticsRouteAvailabilityChanged { .. }
            | DomainEvent::LogisticsPathRerouted { .. }
            | DomainEvent::MaterialTransitStarted { .. }
            | DomainEvent::MaterialTransitCompleted { .. } => {
                Self::Logistics(logistics::LogisticsView::from_event(event))
            }
            DomainEvent::FactoryBuildStarted { .. }
            | DomainEvent::FactoryBuilt { .. }
            | DomainEvent::FactoryDurabilityChanged { .. }
            | DomainEvent::FactoryMaintained { .. }
            | DomainEvent::FactoryRecycled { .. }
            | DomainEvent::RecipeStarted { .. }
            | DomainEvent::RecipeCompleted { .. }
            | DomainEvent::FactoryProductionBlocked { .. }
            | DomainEvent::FactoryProductionResumed { .. }
            | DomainEvent::FactoryProductionPaused { .. } => {
                Self::Production(production::ProductionView::from_event(event))
            }
            DomainEvent::GameplayPolicyUpdated { .. }
            | DomainEvent::EconomicContractOpened { .. }
            | DomainEvent::EconomicContractAccepted { .. }
            | DomainEvent::EconomicContractSettled { .. }
            | DomainEvent::EconomicContractExpired { .. }
            | DomainEvent::AllianceFormed { .. }
            | DomainEvent::AllianceJoined { .. }
            | DomainEvent::AllianceLeft { .. }
            | DomainEvent::AllianceDissolved { .. }
            | DomainEvent::WarDeclared { .. }
            | DomainEvent::WarConcluded { .. }
            | DomainEvent::GovernanceProposalOpened { .. }
            | DomainEvent::GovernanceVoteCast { .. }
            | DomainEvent::GovernanceProposalFinalized { .. }
            | DomainEvent::CrisisSpawned { .. }
            | DomainEvent::CrisisResolved { .. }
            | DomainEvent::CrisisTimedOut { .. }
            | DomainEvent::MetaProgressGranted { .. } => {
                Self::Governance(governance::GovernanceView::from_event(event))
            }
            DomainEvent::ProductValidated { .. }
            | DomainEvent::ProductValidationRecorded { .. }
            | DomainEvent::ProductValidationAttemptStarted { .. }
            | DomainEvent::MaterialProfileGoverned { .. }
            | DomainEvent::ProductProfileGoverned { .. }
            | DomainEvent::RecipeProfileGoverned { .. }
            | DomainEvent::FactoryProfileGoverned { .. } => {
                Self::Validation(validation::ValidationView::from_event(event))
            }
        }
    }
}

impl fmt::Debug for DomainEventView<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorldAgent(view) => fmt::Debug::fmt(view, f),
            Self::ModuleLifecycle(view) => fmt::Debug::fmt(view, f),
            Self::DataPower(view) => fmt::Debug::fmt(view, f),
            Self::TokenClaims(view) => fmt::Debug::fmt(view, f),
            Self::Logistics(view) => fmt::Debug::fmt(view, f),
            Self::Production(view) => fmt::Debug::fmt(view, f),
            Self::Governance(view) => fmt::Debug::fmt(view, f),
            Self::Validation(view) => fmt::Debug::fmt(view, f),
        }
    }
}

impl PartialEq for DomainEventView<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::WorldAgent(left), Self::WorldAgent(right)) => left == right,
            (Self::ModuleLifecycle(left), Self::ModuleLifecycle(right)) => left == right,
            (Self::DataPower(left), Self::DataPower(right)) => left == right,
            (Self::TokenClaims(left), Self::TokenClaims(right)) => left == right,
            (Self::Logistics(left), Self::Logistics(right)) => left == right,
            (Self::Production(left), Self::Production(right)) => left == right,
            (Self::Governance(left), Self::Governance(right)) => left == right,
            (Self::Validation(left), Self::Validation(right)) => left == right,
            _ => false,
        }
    }
}

pub(super) fn fmt(event: &DomainEvent, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&DomainEventView::from(event), f)
}

pub(super) fn eq(left: &DomainEvent, right: &DomainEvent) -> bool {
    DomainEventView::from(left) == DomainEventView::from(right)
}
