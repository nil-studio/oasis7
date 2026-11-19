//! Original Node decision wire shared without a Runtime/Node dependency cycle.
//! These serializable inputs alone confer no execution or acceptance permission.
use crate::node_consensus_action::NodeConsensusAction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeExecutionCommitContext {
    pub world_id: String,
    pub node_id: String,
    pub proposer_id: String,
    pub height: u64,
    pub slot: u64,
    pub epoch: u64,
    pub node_block_hash: String,
    pub action_root: String,
    #[serde(default)]
    pub committed_actions: Vec<NodeConsensusAction>,
    pub committed_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeLocalExecutionContinuation {
    pub schema_version: u32,
    pub context: NodeExecutionCommitContext,
    pub predecessor_committed_height: u64,
    pub predecessor_node_block_hash: Option<String>,
    pub predecessor_execution_height: u64,
    pub predecessor_execution_block_hash: Option<String>,
    pub predecessor_execution_state_root: Option<String>,
    pub next_height: u64,
    pub next_slot: u64,
    pub reserved_action_bytes: usize,
    pub approved_stake: u64,
    pub rejected_stake: u64,
    pub required_stake: u64,
    pub total_stake: u64,
}

#[path = "normal_tick_input_read.rs"]
mod read;
pub use read::{PaidNormalContextOriginalsV1, read_normal_context_originals_bounded};

#[path = "normal_tick_input_bind.rs"]
mod bind;
pub use bind::{PaidBoundNormalInputDataV1, bind_normal_input_data_bounded};
