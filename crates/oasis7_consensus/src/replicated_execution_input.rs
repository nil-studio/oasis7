//! Reserved execution-input wire and its original pure predicates.
//! Node transports these bytes; Runtime may inspect them without a Node/DistFS
//! dependency. None of these DTOs creates a verified Node decision or lease.
use crate::node_consensus_action::NodeConsensusAction;
use serde::{Deserialize, Serialize};
/// Reserved consensus action identity for execution inputs that are authored
/// by the block proposer and then replayed by every materializing node.
///
/// These inputs deliberately travel through `NodeConsensusAction`: the action
/// payload is covered by the consensus action root and by the replicated
/// commit message.  A node-local file can therefore only provide an input to
/// its own proposal; it cannot mutate a committed block after the fact.
pub const REPLICATED_EXECUTION_INPUT_ACTION_ID: u64 = u64::MAX;
pub const REPLICATED_EXECUTION_INPUT_SUBMITTER: &str = "system:replicated-execution";
pub const REPLICATED_EXECUTION_INPUT_VERSION: u8 = 1;
pub const PROVIDER_BACKED_BOOTSTRAP_EXECUTION_INPUT_KIND: &str = "provider_backed_bootstrap.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeReplicatedExecutionInputV1 {
    pub version: u8,
    pub kind: String,
    /// Zero is an unbound producer-side input. The node binds it to its next
    /// canonical proposal height before the worker starts. Replicated inputs
    /// must carry the concrete committed height when they reach a hook.
    pub target_height: u64,
    /// Kind-specific canonical CBOR payload. The chain runtime owns decoding
    /// this opaque field; the node layer only transports and commits it.
    pub payload_cbor: Vec<u8>,
}

impl NodeReplicatedExecutionInputV1 {
    pub fn new(kind: impl Into<String>, target_height: u64, payload_cbor: Vec<u8>) -> Self {
        Self {
            version: REPLICATED_EXECUTION_INPUT_VERSION,
            kind: kind.into(),
            target_height,
            payload_cbor,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.version != REPLICATED_EXECUTION_INPUT_VERSION {
            return Err(format!(
                "unsupported replicated execution input version {}",
                self.version
            ));
        }
        if self.kind.trim().is_empty() {
            return Err("replicated execution input kind cannot be empty".to_string());
        }
        if self.payload_cbor.is_empty() {
            return Err("replicated execution input payload cannot be empty".to_string());
        }
        serde_cbor::to_vec(self)
            .map_err(|err| format!("encode replicated execution input failed: {err}"))
    }

    /// Decode only the reserved input envelope. Ordinary runtime/simulator
    /// action payloads return `Ok(None)` so existing action admission remains
    /// unchanged.
    pub fn decode(bytes: &[u8]) -> Result<Option<Self>, String> {
        match Self::decode_with_owned(bytes, |bytes| {
            Ok::<_, std::convert::Infallible>(serde_cbor::from_slice::<Self>(bytes))
        }) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    pub fn bind_target_height(&mut self, target_height: u64) -> Result<(), String> {
        if target_height == 0 {
            return Err("replicated execution input target height must be > 0".to_string());
        }
        if self.target_height == 0 {
            self.target_height = target_height;
        } else if self.target_height != target_height {
            return Err(format!(
                "replicated execution input target height mismatch: expected={} actual={}",
                target_height, self.target_height
            ));
        }
        Ok(())
    }
}

pub fn decode_replicated_execution_input_action(
    action: &NodeConsensusAction,
) -> Result<Option<NodeReplicatedExecutionInputV1>, String> {
    if action.action_id != REPLICATED_EXECUTION_INPUT_ACTION_ID
        || action.submitter_player_id != REPLICATED_EXECUTION_INPUT_SUBMITTER
    {
        return Ok(None);
    }
    NodeReplicatedExecutionInputV1::decode(action.payload_cbor.as_slice())
}

/// Validate the reserved execution-input identity and its committed height.
/// A mismatched reserved identity must be rejected as malformed input instead
/// of falling through to ordinary runtime-action decoding.
pub fn validate_replicated_execution_input_actions(
    actions: &[NodeConsensusAction],
    expected_height: u64,
) -> Result<(), String> {
    match validate_replicated_execution_input_actions_with(
        actions,
        expected_height,
        &mut replicated_input::Legacy,
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

pub fn bind_replicated_execution_input_action(
    action: &mut NodeConsensusAction,
    target_height: u64,
) -> Result<bool, String> {
    let Some(mut input) = decode_replicated_execution_input_action(action)? else {
        return Ok(false);
    };
    input.bind_target_height(target_height)?;
    let payload_cbor = input.encode()?;
    *action = NodeConsensusAction::from_payload(
        REPLICATED_EXECUTION_INPUT_ACTION_ID,
        REPLICATED_EXECUTION_INPUT_SUBMITTER,
        payload_cbor,
    )
    .map_err(|err| format!("rebuild replicated execution input action failed: {err}"))?;
    Ok(true)
}
#[path = "replicated_execution_input_ops.rs"]
mod replicated_input;
pub use replicated_input::{
    ReplicatedExecutionInputValidationOpsV1, decode_replicated_execution_input_action_with,
    validate_replicated_execution_input_actions_with,
};
