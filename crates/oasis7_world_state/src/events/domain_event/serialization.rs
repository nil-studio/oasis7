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

struct Tag {
    index: u32,
    name: &'static str,
}

impl Tag {
    const WORLD_RULE_COMMITTED: Self = Self {
        index: 0,
        name: "WorldRuleCommitted",
    };
    const AGENT_REGISTERED: Self = Self {
        index: 1,
        name: "AgentRegistered",
    };
    const AGENT_MOVED: Self = Self {
        index: 2,
        name: "AgentMoved",
    };
    const AGENT_LOCATION_AUTHORITY_UPDATED: Self = Self {
        index: 3,
        name: "AgentLocationAuthorityUpdated",
    };
    const LOCATION_ANCHOR_UPDATED: Self = Self {
        index: 4,
        name: "LocationAnchorUpdated",
    };
    const FACTORY_SITE_AUTHORITY_UPDATED: Self = Self {
        index: 5,
        name: "FactorySiteAuthorityUpdated",
    };
    const FACTORY_CONSTRUCTION_POWER_PROFILE_UPDATED: Self = Self {
        index: 6,
        name: "FactoryConstructionPowerProfileUpdated",
    };
    const ACTION_ACCEPTED: Self = Self {
        index: 7,
        name: "ActionAccepted",
    };
    const ACTION_REJECTED: Self = Self {
        index: 8,
        name: "ActionRejected",
    };
    const AGENT_INTENT_PROPOSED: Self = Self {
        index: 9,
        name: "AgentIntentProposed",
    };
    const AGENT_INTENT_SUBMITTED: Self = Self {
        index: 10,
        name: "AgentIntentSubmitted",
    };
    const AGENT_INTENT_ACCEPTED: Self = Self {
        index: 11,
        name: "AgentIntentAccepted",
    };
    const AGENT_INTENT_REPLACED: Self = Self {
        index: 12,
        name: "AgentIntentReplaced",
    };
    const AGENT_INTENT_TRANSITIONED: Self = Self {
        index: 13,
        name: "AgentIntentTransitioned",
    };
    const OBSERVATION: Self = Self {
        index: 14,
        name: "Observation",
    };
    const BODY_ATTRIBUTES_UPDATED: Self = Self {
        index: 15,
        name: "BodyAttributesUpdated",
    };
    const BODY_ATTRIBUTES_REJECTED: Self = Self {
        index: 16,
        name: "BodyAttributesRejected",
    };
    const BODY_INTERFACE_EXPANDED: Self = Self {
        index: 17,
        name: "BodyInterfaceExpanded",
    };
    const BODY_INTERFACE_EXPAND_REJECTED: Self = Self {
        index: 18,
        name: "BodyInterfaceExpandRejected",
    };
    const MODULE_ARTIFACT_DEPLOYED: Self = Self {
        index: 19,
        name: "ModuleArtifactDeployed",
    };
    const MODULE_INSTALLED: Self = Self {
        index: 20,
        name: "ModuleInstalled",
    };
    const MODULE_UPGRADED: Self = Self {
        index: 21,
        name: "ModuleUpgraded",
    };
    const MODULE_RELEASE_REQUESTED: Self = Self {
        index: 22,
        name: "ModuleReleaseRequested",
    };
    const MODULE_RELEASE_SHADOWED: Self = Self {
        index: 23,
        name: "ModuleReleaseShadowed",
    };
    const MODULE_RELEASE_ATTESTED: Self = Self {
        index: 24,
        name: "ModuleReleaseAttested",
    };
    const MODULE_RELEASE_ROLE_APPROVED: Self = Self {
        index: 25,
        name: "ModuleReleaseRoleApproved",
    };
    const MODULE_RELEASE_ROLES_BOUND: Self = Self {
        index: 26,
        name: "ModuleReleaseRolesBound",
    };
    const MODULE_RELEASE_REJECTED: Self = Self {
        index: 27,
        name: "ModuleReleaseRejected",
    };
    const MODULE_RELEASE_APPLIED: Self = Self {
        index: 28,
        name: "ModuleReleaseApplied",
    };
    const MODULE_ROLLBACK_APPLIED: Self = Self {
        index: 29,
        name: "ModuleRollbackApplied",
    };
    const MODULE_ARTIFACT_LISTED: Self = Self {
        index: 30,
        name: "ModuleArtifactListed",
    };
    const MODULE_ARTIFACT_DELISTED: Self = Self {
        index: 31,
        name: "ModuleArtifactDelisted",
    };
    const MODULE_ARTIFACT_DESTROYED: Self = Self {
        index: 32,
        name: "ModuleArtifactDestroyed",
    };
    const MODULE_ARTIFACT_BID_PLACED: Self = Self {
        index: 33,
        name: "ModuleArtifactBidPlaced",
    };
    const MODULE_ARTIFACT_BID_CANCELLED: Self = Self {
        index: 34,
        name: "ModuleArtifactBidCancelled",
    };
    const MODULE_ARTIFACT_SALE_COMPLETED: Self = Self {
        index: 35,
        name: "ModuleArtifactSaleCompleted",
    };
    const RESOURCE_TRANSFERRED: Self = Self {
        index: 36,
        name: "ResourceTransferred",
    };
    const DATA_COLLECTED: Self = Self {
        index: 37,
        name: "DataCollected",
    };
    const DATA_COLLECTED_AUTHENTICATED: Self = Self {
        index: 38,
        name: "DataCollectedAuthenticated",
    };
    const DATA_ACCESS_GRANTED: Self = Self {
        index: 39,
        name: "DataAccessGranted",
    };
    const DATA_ACCESS_REVOKED: Self = Self {
        index: 40,
        name: "DataAccessRevoked",
    };
    const POWER_REDEEMED: Self = Self {
        index: 41,
        name: "PowerRedeemed",
    };
    const POWER_REDEEM_REJECTED: Self = Self {
        index: 42,
        name: "PowerRedeemRejected",
    };
    const NODE_POINTS_SETTLEMENT_APPLIED: Self = Self {
        index: 43,
        name: "NodePointsSettlementApplied",
    };
    const MAIN_TOKEN_GENESIS_INITIALIZED: Self = Self {
        index: 44,
        name: "MainTokenGenesisInitialized",
    };
    const MAIN_TOKEN_VESTING_CLAIMED: Self = Self {
        index: 45,
        name: "MainTokenVestingClaimed",
    };
    const MAIN_TOKEN_TRANSFERRED: Self = Self {
        index: 46,
        name: "MainTokenTransferred",
    };
    const MAIN_TOKEN_EPOCH_ISSUED: Self = Self {
        index: 47,
        name: "MainTokenEpochIssued",
    };
    const MAIN_TOKEN_FEE_SETTLED: Self = Self {
        index: 48,
        name: "MainTokenFeeSettled",
    };
    const MAIN_TOKEN_POLICY_UPDATE_SCHEDULED: Self = Self {
        index: 49,
        name: "MainTokenPolicyUpdateScheduled",
    };
    const MAIN_TOKEN_TREASURY_DISTRIBUTED: Self = Self {
        index: 50,
        name: "MainTokenTreasuryDistributed",
    };
    const RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL_TOPPED_UP: Self = Self {
        index: 51,
        name: "RestrictedStarterClaimLiveopsPoolToppedUp",
    };
    const RESTRICTED_STARTER_CLAIM_GRANT_ISSUED: Self = Self {
        index: 52,
        name: "RestrictedStarterClaimGrantIssued",
    };
    const RESTRICTED_STARTER_CLAIM_GRANT_EXPIRED: Self = Self {
        index: 53,
        name: "RestrictedStarterClaimGrantExpired",
    };
    const RESTRICTED_STARTER_CLAIM_GRANT_REVOKED: Self = Self {
        index: 54,
        name: "RestrictedStarterClaimGrantRevoked",
    };
    const STARTER_OC_CLAIMED: Self = Self {
        index: 55,
        name: "StarterOcClaimed",
    };
    const AGENT_CLAIMED: Self = Self {
        index: 56,
        name: "AgentClaimed",
    };
    const AGENT_CLAIM_RELEASE_REQUESTED: Self = Self {
        index: 57,
        name: "AgentClaimReleaseRequested",
    };
    const AGENT_CLAIM_UPKEEP_SETTLED: Self = Self {
        index: 58,
        name: "AgentClaimUpkeepSettled",
    };
    const AGENT_CLAIM_ENTERED_GRACE: Self = Self {
        index: 59,
        name: "AgentClaimEnteredGrace",
    };
    const AGENT_CLAIM_IDLE_WARNING: Self = Self {
        index: 60,
        name: "AgentClaimIdleWarning",
    };
    const AGENT_CLAIM_RELEASED: Self = Self {
        index: 61,
        name: "AgentClaimReleased",
    };
    const AGENT_CLAIM_RECLAIMED: Self = Self {
        index: 62,
        name: "AgentClaimReclaimed",
    };
    const MATERIAL_TRANSFERRED: Self = Self {
        index: 63,
        name: "MaterialTransferred",
    };
    const LOGISTICS_ROUTE_REGISTERED: Self = Self {
        index: 64,
        name: "LogisticsRouteRegistered",
    };
    const LOGISTICS_ROUTE_AVAILABILITY_CHANGED: Self = Self {
        index: 65,
        name: "LogisticsRouteAvailabilityChanged",
    };
    const LOGISTICS_PATH_REROUTED: Self = Self {
        index: 66,
        name: "LogisticsPathRerouted",
    };
    const MATERIAL_TRANSIT_STARTED: Self = Self {
        index: 67,
        name: "MaterialTransitStarted",
    };
    const MATERIAL_TRANSIT_COMPLETED: Self = Self {
        index: 68,
        name: "MaterialTransitCompleted",
    };
    const FACTORY_BUILD_STARTED: Self = Self {
        index: 69,
        name: "FactoryBuildStarted",
    };
    const FACTORY_BUILT: Self = Self {
        index: 70,
        name: "FactoryBuilt",
    };
    const FACTORY_DURABILITY_CHANGED: Self = Self {
        index: 71,
        name: "FactoryDurabilityChanged",
    };
    const FACTORY_MAINTAINED: Self = Self {
        index: 72,
        name: "FactoryMaintained",
    };
    const FACTORY_RECYCLED: Self = Self {
        index: 73,
        name: "FactoryRecycled",
    };
    const RECIPE_STARTED: Self = Self {
        index: 74,
        name: "RecipeStarted",
    };
    const RECIPE_COMPLETED: Self = Self {
        index: 75,
        name: "RecipeCompleted",
    };
    const FACTORY_PRODUCTION_BLOCKED: Self = Self {
        index: 76,
        name: "FactoryProductionBlocked",
    };
    const FACTORY_PRODUCTION_RESUMED: Self = Self {
        index: 77,
        name: "FactoryProductionResumed",
    };
    const FACTORY_PRODUCTION_PAUSED: Self = Self {
        index: 78,
        name: "FactoryProductionPaused",
    };
    const GAMEPLAY_POLICY_UPDATED: Self = Self {
        index: 79,
        name: "GameplayPolicyUpdated",
    };
    const ECONOMIC_CONTRACT_OPENED: Self = Self {
        index: 80,
        name: "EconomicContractOpened",
    };
    const ECONOMIC_CONTRACT_ACCEPTED: Self = Self {
        index: 81,
        name: "EconomicContractAccepted",
    };
    const ECONOMIC_CONTRACT_SETTLED: Self = Self {
        index: 82,
        name: "EconomicContractSettled",
    };
    const ECONOMIC_CONTRACT_EXPIRED: Self = Self {
        index: 83,
        name: "EconomicContractExpired",
    };
    const ALLIANCE_FORMED: Self = Self {
        index: 84,
        name: "AllianceFormed",
    };
    const ALLIANCE_JOINED: Self = Self {
        index: 85,
        name: "AllianceJoined",
    };
    const ALLIANCE_LEFT: Self = Self {
        index: 86,
        name: "AllianceLeft",
    };
    const ALLIANCE_DISSOLVED: Self = Self {
        index: 87,
        name: "AllianceDissolved",
    };
    const WAR_DECLARED: Self = Self {
        index: 88,
        name: "WarDeclared",
    };
    const WAR_CONCLUDED: Self = Self {
        index: 89,
        name: "WarConcluded",
    };
    const GOVERNANCE_PROPOSAL_OPENED: Self = Self {
        index: 90,
        name: "GovernanceProposalOpened",
    };
    const GOVERNANCE_VOTE_CAST: Self = Self {
        index: 91,
        name: "GovernanceVoteCast",
    };
    const GOVERNANCE_PROPOSAL_FINALIZED: Self = Self {
        index: 92,
        name: "GovernanceProposalFinalized",
    };
    const CRISIS_SPAWNED: Self = Self {
        index: 93,
        name: "CrisisSpawned",
    };
    const CRISIS_RESOLVED: Self = Self {
        index: 94,
        name: "CrisisResolved",
    };
    const CRISIS_TIMED_OUT: Self = Self {
        index: 95,
        name: "CrisisTimedOut",
    };
    const META_PROGRESS_GRANTED: Self = Self {
        index: 96,
        name: "MetaProgressGranted",
    };
    const PRODUCT_VALIDATED: Self = Self {
        index: 97,
        name: "ProductValidated",
    };
    const PRODUCT_VALIDATION_RECORDED: Self = Self {
        index: 98,
        name: "ProductValidationRecorded",
    };
    const PRODUCT_VALIDATION_ATTEMPT_STARTED: Self = Self {
        index: 99,
        name: "ProductValidationAttemptStarted",
    };
    const MATERIAL_PROFILE_GOVERNED: Self = Self {
        index: 100,
        name: "MaterialProfileGoverned",
    };
    const PRODUCT_PROFILE_GOVERNED: Self = Self {
        index: 101,
        name: "ProductProfileGoverned",
    };
    const RECIPE_PROFILE_GOVERNED: Self = Self {
        index: 102,
        name: "RecipeProfileGoverned",
    };
    const FACTORY_PROFILE_GOVERNED: Self = Self {
        index: 103,
        name: "FactoryProfileGoverned",
    };
}

impl serde::Serialize for Tag {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit_variant("DomainEvent", self.index, self.name)
    }
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
