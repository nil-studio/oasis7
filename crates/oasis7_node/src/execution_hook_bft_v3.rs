//! Private execution material stays out of public proposal and provider codecs.
use crate::NodeConsensusAction;
use oasis7_consensus::tendermint_v3::{BftHeaderHeightScheduleV3, BftHeaderValueV3};
/// The exact parent and protocol cursor the execution driver must independently
/// compare with its own durable state before building or previewing a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftExecutionContextV3 {
    pub scope: oasis7_proto::distributed_bft::v2::BftScopeV2,
    pub activation_digest: String,
    pub height: u64,
    pub parent_commit_ref: String,
    pub authority_epoch: u64,
    pub parent_state_root: String,
    pub parent_tick: u64,
    pub active_validator_set_hash: String,
    pub schedule_policy_hash: String,
    pub manifest_hash: String,
}

impl NodeBftExecutionContextV3 {
    pub fn from_schedule(schedule: &BftHeaderHeightScheduleV3) -> Self {
        Self {
            scope: schedule.scope().clone(),
            activation_digest: schedule.activation_digest().into(),
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

#[derive(Clone, PartialEq, Eq)]
pub struct NodeBftBuildContextV3 {
    pub execution: NodeBftExecutionContextV3,
    pub actions: Vec<NodeConsensusAction>,
}

/// Reported only after durable V3 configuration/history verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftExecutionHeadV3 {
    pub height: u64,
    pub header_id: String,
    pub state_root: String,
    pub manifest_hash: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct NodeBftPrivateCandidateV3 {
    pub public_value: BftHeaderValueV3,
    pub actions_cbor: Vec<u8>,
}
impl std::fmt::Debug for NodeBftPrivateCandidateV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeBftPrivateCandidateV3")
            .field("public_value", &self.public_value)
            .finish_non_exhaustive()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftExecutionCommitResultV3 {
    pub height: u64,
    pub header_id: String,
    pub execution_commitment_hash: String,
    pub execution_state_root: String,
    pub state_root: String,
    pub post_manifest_hash: String,
    pub driver_install_digest: String,
}

impl std::fmt::Debug for NodeBftBuildContextV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeBftBuildContextV3")
            .field("execution", &self.execution)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_build_debug_omits_exact_input() {
        let context = NodeBftExecutionContextV3 {
            scope: oasis7_proto::distributed_bft::v2::BftScopeV2 {
                world_id: "world".into(),
                chain_id: "chain".into(),
                genesis_hash: "11".repeat(32),
            },
            activation_digest: "22".repeat(32),
            height: 3,
            parent_commit_ref: "33".repeat(32),
            authority_epoch: 1,
            parent_state_root: "44".repeat(32),
            parent_tick: 2,
            active_validator_set_hash: "55".repeat(32),
            schedule_policy_hash: "66".repeat(32),
            manifest_hash: "77".repeat(32),
        };
        let action =
            NodeConsensusAction::from_payload(1, "reader", b"private-node-wire3-canary".to_vec())
                .unwrap();
        let build = NodeBftBuildContextV3 {
            execution: context,
            actions: vec![action],
        };
        let debug = format!("{build:?}");
        assert!(!debug.contains("payload_cbor"));
        assert!(!debug.contains("private-node-wire3-canary"));
        assert!(!debug.contains("actions"));
    }
}
