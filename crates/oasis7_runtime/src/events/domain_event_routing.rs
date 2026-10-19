//! Domain-event mailbox recipient routing.
use super::DomainEvent;

impl DomainEvent {
    pub fn agent_id(&self) -> Option<&str> {
        match self {
            DomainEvent::WorldRuleCommitted {
                action_id: 0,
                command,
                ..
            } if command.is_runtime_observation() => None,
            DomainEvent::WorldRuleCommitted { actor_id, .. } => Some(actor_id),
            DomainEvent::AgentRegistered { agent_id, .. } => Some(agent_id.as_str()),
            DomainEvent::AgentMoved { agent_id, .. } => Some(agent_id.as_str()),
            DomainEvent::AgentLocationAuthorityUpdated { authority } => {
                Some(authority.agent_id.as_str())
            }
            DomainEvent::LocationAnchorUpdated { .. } => None,
            DomainEvent::FactorySiteAuthorityUpdated { authority } => {
                Some(authority.owner_agent_id.as_str())
            }
            DomainEvent::FactoryConstructionPowerProfileUpdated { .. } => None,
            DomainEvent::ActionAccepted { actor_id, .. } => Some(actor_id.as_str()),
            DomainEvent::AgentIntentProposed { intent }
            | DomainEvent::AgentIntentSubmitted { intent }
            | DomainEvent::AgentIntentAccepted { intent }
            | DomainEvent::AgentIntentReplaced { intent }
            | DomainEvent::AgentIntentTransitioned { intent } => Some(intent.agent_id.as_str()),
            DomainEvent::Observation { observation } => Some(observation.agent_id.as_str()),
            DomainEvent::BodyAttributesUpdated { agent_id, .. } => Some(agent_id.as_str()),
            DomainEvent::BodyAttributesRejected { agent_id, .. } => Some(agent_id.as_str()),
            DomainEvent::BodyInterfaceExpanded { agent_id, .. } => Some(agent_id.as_str()),
            DomainEvent::BodyInterfaceExpandRejected { agent_id, .. } => Some(agent_id.as_str()),
            DomainEvent::ModuleArtifactDeployed {
                publisher_agent_id, ..
            } => Some(publisher_agent_id.as_str()),
            DomainEvent::ModuleInstalled {
                installer_agent_id, ..
            } => Some(installer_agent_id.as_str()),
            DomainEvent::ModuleUpgraded {
                upgrader_agent_id, ..
            } => Some(upgrader_agent_id.as_str()),
            DomainEvent::ModuleReleaseRequested {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::ModuleReleaseShadowed {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::ModuleReleaseAttested {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::ModuleReleaseRoleApproved {
                approver_agent_id, ..
            } => Some(approver_agent_id.as_str()),
            DomainEvent::ModuleReleaseRolesBound {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::ModuleReleaseRejected {
                rejector_agent_id, ..
            } => Some(rejector_agent_id.as_str()),
            DomainEvent::ModuleReleaseApplied {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::ModuleRollbackApplied {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::ModuleArtifactListed {
                seller_agent_id, ..
            } => Some(seller_agent_id.as_str()),
            DomainEvent::ModuleArtifactDelisted {
                seller_agent_id, ..
            } => Some(seller_agent_id.as_str()),
            DomainEvent::ModuleArtifactDestroyed { owner_agent_id, .. } => {
                Some(owner_agent_id.as_str())
            }
            DomainEvent::ModuleArtifactBidPlaced {
                bidder_agent_id, ..
            } => Some(bidder_agent_id.as_str()),
            DomainEvent::ModuleArtifactBidCancelled {
                bidder_agent_id, ..
            } => Some(bidder_agent_id.as_str()),
            DomainEvent::ModuleArtifactSaleCompleted { buyer_agent_id, .. } => {
                Some(buyer_agent_id.as_str())
            }
            DomainEvent::ActionRejected { .. } => None,
            DomainEvent::ResourceTransferred { from_agent_id, .. } => Some(from_agent_id.as_str()),
            DomainEvent::DataCollected {
                collector_agent_id, ..
            }
            | DomainEvent::DataCollectedAuthenticated {
                collector_agent_id, ..
            } => Some(collector_agent_id.as_str()),
            DomainEvent::DataAccessGranted { owner_agent_id, .. } => Some(owner_agent_id.as_str()),
            DomainEvent::DataAccessRevoked { owner_agent_id, .. } => Some(owner_agent_id.as_str()),
            DomainEvent::PowerRedeemed {
                target_agent_id, ..
            } => Some(target_agent_id.as_str()),
            DomainEvent::PowerRedeemRejected {
                target_agent_id, ..
            } => Some(target_agent_id.as_str()),
            DomainEvent::NodePointsSettlementApplied { .. } => None,
            DomainEvent::MainTokenGenesisInitialized { .. } => None,
            DomainEvent::MainTokenVestingClaimed { beneficiary, .. } => Some(beneficiary.as_str()),
            DomainEvent::MainTokenTransferred {
                from_account_id, ..
            } => Some(from_account_id.as_str()),
            DomainEvent::MainTokenEpochIssued { .. } => None,
            DomainEvent::MainTokenFeeSettled { .. } => None,
            DomainEvent::MainTokenPolicyUpdateScheduled { .. } => None,
            DomainEvent::MainTokenTreasuryDistributed { .. } => None,
            DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp {
                controller_account_id,
                ..
            } => Some(controller_account_id.as_str()),
            DomainEvent::RestrictedStarterClaimGrantIssued { issuer_id, .. }
            | DomainEvent::RestrictedStarterClaimGrantRevoked { issuer_id, .. } => {
                Some(issuer_id.as_str())
            }
            DomainEvent::RestrictedStarterClaimGrantExpired { .. } => None,
            DomainEvent::StarterOcClaimed { agent_id, .. } => Some(agent_id.as_str()),
            DomainEvent::AgentClaimed {
                claimer_agent_id, ..
            }
            | DomainEvent::AgentClaimReleaseRequested {
                claimer_agent_id, ..
            }
            | DomainEvent::AgentClaimUpkeepSettled {
                claimer_agent_id, ..
            }
            | DomainEvent::AgentClaimEnteredGrace {
                claimer_agent_id, ..
            }
            | DomainEvent::AgentClaimIdleWarning {
                claimer_agent_id, ..
            }
            | DomainEvent::AgentClaimReleased {
                claimer_agent_id, ..
            }
            | DomainEvent::AgentClaimReclaimed {
                claimer_agent_id, ..
            } => Some(claimer_agent_id.as_str()),
            DomainEvent::MaterialTransferred {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::LogisticsRouteRegistered {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::LogisticsRouteAvailabilityChanged {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::LogisticsPathRerouted {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::MaterialTransitStarted {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::MaterialTransitCompleted {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::FactoryBuildStarted {
                builder_agent_id, ..
            } => Some(builder_agent_id.as_str()),
            DomainEvent::FactoryBuilt {
                builder_agent_id, ..
            } => Some(builder_agent_id.as_str()),
            DomainEvent::FactoryDurabilityChanged { .. } => None,
            DomainEvent::FactoryMaintained {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::FactoryRecycled {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::RecipeStarted {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::RecipeCompleted {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::FactoryProductionBlocked {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::FactoryProductionResumed {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::FactoryProductionPaused {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::GameplayPolicyUpdated {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::EconomicContractOpened {
                creator_agent_id, ..
            } => Some(creator_agent_id.as_str()),
            DomainEvent::EconomicContractAccepted {
                accepter_agent_id, ..
            } => Some(accepter_agent_id.as_str()),
            DomainEvent::EconomicContractSettled {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::EconomicContractExpired {
                creator_agent_id, ..
            } => Some(creator_agent_id.as_str()),
            DomainEvent::AllianceFormed {
                proposer_agent_id, ..
            } => Some(proposer_agent_id.as_str()),
            DomainEvent::AllianceJoined {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::AllianceLeft {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::AllianceDissolved {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::WarDeclared {
                initiator_agent_id, ..
            } => Some(initiator_agent_id.as_str()),
            DomainEvent::WarConcluded { .. } => None,
            DomainEvent::GovernanceProposalOpened {
                proposer_agent_id, ..
            } => Some(proposer_agent_id.as_str()),
            DomainEvent::GovernanceVoteCast { voter_agent_id, .. } => Some(voter_agent_id.as_str()),
            DomainEvent::GovernanceProposalFinalized { .. } => None,
            DomainEvent::CrisisSpawned { .. } => None,
            DomainEvent::CrisisResolved {
                resolver_agent_id, ..
            } => Some(resolver_agent_id.as_str()),
            DomainEvent::CrisisTimedOut { .. } => None,
            DomainEvent::MetaProgressGranted {
                target_agent_id, ..
            } => Some(target_agent_id.as_str()),
            DomainEvent::ProductValidated {
                requester_agent_id, ..
            } => Some(requester_agent_id.as_str()),
            DomainEvent::ProductValidationRecorded { receipt } => {
                Some(receipt.requester_agent_id.as_str())
            }
            DomainEvent::ProductValidationAttemptStarted { attempt } => {
                Some(attempt.requester_agent_id.as_str())
            }
            DomainEvent::MaterialProfileGoverned {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::ProductProfileGoverned {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::RecipeProfileGoverned {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
            DomainEvent::FactoryProfileGoverned {
                operator_agent_id, ..
            } => Some(operator_agent_id.as_str()),
        }
    }
}

