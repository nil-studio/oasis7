//! Action and domain event types.

use oasis7_agent_api::world_action::GeoPos;
use crate::agent_state::{BodyKernelView, BodySlotType};
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{
    FactoryModuleSpec, FactoryProfileV1, MaterialProfileV1, MaterialStack, ModuleManifest,
    ProductProfileV1, RecipeProfileV1,
};
use serde::{Deserialize, Serialize};

use super::agent_cell::AgentIntentV2;
use super::gameplay_state::{EconomicContractFulfillmentKind, WarParticipantOutcome};
use super::main_token::{
    MainTokenConfig, MainTokenGenesisAllocationBucketState, MainTokenNodePointsBridgeDistribution,
    MainTokenTreasuryDistribution, RestrictedStarterClaimRefundSink,
};
use super::node_points::EpochSettlementReport;
use super::reward_asset::NodeRewardMintRecord;
use super::state::{
    AgentLocationAuthorityV1, FactoryBuildPowerObligationV1, FactoryConstructionPowerProfileV1,
    FactorySiteAuthorityV1, LocationAnchorV1, ProductValidationAttemptV1,
    ProductValidationReceiptV1,
};
use super::types::{ActionId, MaterialLedgerId, ProposalId, WorldTime};

fn default_world_material_ledger() -> MaterialLedgerId {
    MaterialLedgerId::world()
}

fn default_module_action_fee_kind() -> ResourceKind {
    ResourceKind::Electricity
}

fn default_material_transit_priority() -> MaterialTransitPriority {
    MaterialTransitPriority::Standard
}

fn default_logistics_route_available() -> bool {
    true
}

fn default_logistics_capacity_units() -> i64 {
    i64::MAX
}

mod domain_event;

pub use self::domain_event::{CausedBy, DomainEvent, LogisticsOwnerPayout, RejectReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IndustryStage {
    #[default]
    Bootstrap,
    ScaleOut,
    Governance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialMarketQuote {
    pub kind: String,
    pub requested_amount: i64,
    pub local_available_amount: i64,
    pub world_available_amount: i64,
    pub local_deficit_amount: i64,
    pub transit_loss_bps: i64,
    pub governance_tax_bps: u16,
    pub effective_cost_index_ppm: i64,
}

pub use oasis7_agent_api::world_action::MainTokenFeeKind;

pub use oasis7_agent_api::world_action::MaterialTransitPriority;

pub use oasis7_proto::gameplay_submission_origin::GameplaySubmissionOrigin;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedRecipeOrigin {
    pub submission: GameplaySubmissionOrigin,
    pub consensus_action_id: u64,
    pub consensus_submitter_player_id: String,
    pub action_payload_hash: String,
    pub committed_height: u64,
    pub action_root: String,
}

impl CommittedRecipeOrigin {
    pub fn matches_recipe(&self, requester: &str, factory: &str, recipe: &str) -> bool {
        self.submission.matches_recipe(requester, factory, recipe)
            && self.consensus_action_id > 0
            && self.committed_height > 0
            && !self.consensus_submitter_player_id.trim().is_empty()
            && [self.action_payload_hash.as_str(), self.action_root.as_str()]
                .iter()
                .all(|hash| {
                    hash.len() == 64
                        && hash
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                })
    }
}

/// An envelope wrapping an action with its ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionEnvelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_recipe_origin: Option<CommittedRecipeOrigin>,
    pub id: ActionId,
    pub action: Action,
}

pub use oasis7_agent_api::world_action::Observation;

#[expect(
    unused_imports,
    reason = "Preserve the existing typed observation compatibility path"
)]
pub use oasis7_agent_api::world_action::ObservedAgent;

pub use oasis7_agent_api::world_action::ModuleSourcePackage;

pub use oasis7_agent_api::world_action::ModuleProfileChanges;

pub use oasis7_agent_api::world_action::Action;

#[cfg(test)]
#[path = "events/recipe_origin_tests.rs"]
pub(crate) mod recipe_origin_tests;
