use oasis7_consensus::tendermint_v2::{
    BftHeightProposerScheduleV2, BftSchedulePolicyV2, BftValidatorSetV2,
};
use oasis7_proto::distributed_bft::v2::{BftScopeV2, BftValidatorV2};

use crate::NodeConsensusAction;

/// Fixed genesis inputs, never a request to replace the current active set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftExecutionConfigV2 {
    pub scope: BftScopeV2,
    pub authority_epoch: u64,
    pub initial_validators: Vec<BftValidatorV2>,
    pub schedule_policy: BftSchedulePolicyV2,
    pub genesis_state_root: String,
    pub genesis_manifest_hash: String,
}

impl NodeBftExecutionConfigV2 {
    pub fn validate(&self) -> Result<BftHeightProposerScheduleV2, String> {
        let set = BftValidatorSetV2::new(self.scope.clone(), self.initial_validators.clone())?;
        BftHeightProposerScheduleV2::genesis(
            set,
            self.schedule_policy.clone(),
            self.authority_epoch,
            self.genesis_state_root.clone(),
            self.genesis_manifest_hash.clone(),
        )
    }
}

/// The exact parent and protocol cursor the execution driver must independently
/// compare with its own durable state before building or previewing a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftExecutionContextV2 {
    pub scope: BftScopeV2,
    pub height: u64,
    pub parent_commit_ref: String,
    pub authority_epoch: u64,
    pub parent_state_root: String,
    pub parent_tick: u64,
    pub active_validator_set_hash: String,
    pub schedule_policy_hash: String,
    pub manifest_hash: String,
}

impl NodeBftExecutionContextV2 {
    pub fn from_schedule(schedule: &BftHeightProposerScheduleV2) -> Self {
        Self {
            scope: schedule.scope().clone(),
            height: schedule.height(),
            parent_commit_ref: schedule.parent_commit_ref().into(),
            authority_epoch: schedule.authority_epoch(),
            parent_state_root: schedule.parent_state_root().into(),
            parent_tick: schedule.parent_tick(),
            active_validator_set_hash: schedule.active_set().set_hash().into(),
            schedule_policy_hash: schedule.policy_hash().into(),
            manifest_hash: schedule.parent_manifest_hash().into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftBuildContextV2 {
    pub execution: NodeBftExecutionContextV2,
    pub actions: Vec<NodeConsensusAction>,
}

/// Reported only after durable V2 configuration/history verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftExecutionHeadV2 {
    pub height: u64,
    pub block_id: String,
    pub state_root: String,
    pub manifest_hash: String,
}

/// A successful actual execution install. This is not by itself a provider or
/// local consensus/WAL set-install acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftExecutionCommitResultV2 {
    pub height: u64,
    pub bft_commit_ref: String,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub state_root: String,
    pub post_manifest_hash: String,
    /// Domain-separated digest of the exact execution record durably installed
    /// by the driver, including its certificate and persisted state references.
    /// This local software receipt does not attest hardware persistence.
    pub driver_install_digest: String,
}
