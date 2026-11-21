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

#[cfg(test)]
mod contract_regression_tests {
    use super::*;

    fn input(height: u64) -> NodeConsensusAction {
        NodeConsensusAction::from_payload(
            REPLICATED_EXECUTION_INPUT_ACTION_ID,
            REPLICATED_EXECUTION_INPUT_SUBMITTER,
            NodeReplicatedExecutionInputV1::new("test", height, vec![1])
                .encode()
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn ordinary_payload_is_not_a_reserved_envelope() {
        assert!(
            NodeReplicatedExecutionInputV1::decode(b"not cbor")
                .unwrap()
                .is_none()
        );
        assert!(
            decode_replicated_execution_input_action(
                &NodeConsensusAction::from_payload(1, "player", vec![1]).unwrap()
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn half_reserved_identity_is_rejected_not_fallen_through() {
        let mut action = input(2);
        action.submitter_player_id = "player".into();
        assert!(
            validate_replicated_execution_input_actions(&[action], 2)
                .unwrap_err()
                .contains("identity is malformed")
        );
        let mut action = input(2);
        action.action_id = 7;
        assert!(
            validate_replicated_execution_input_actions(&[action], 2)
                .unwrap_err()
                .contains("identity is malformed")
        );
    }

    #[test]
    fn duplicate_unbound_and_mismatched_heights_are_rejected() {
        assert!(
            validate_replicated_execution_input_actions(&[input(2), input(2)], 2)
                .unwrap_err()
                .contains("multiple replicated execution")
        );
        assert!(
            validate_replicated_execution_input_actions(&[input(0)], 2)
                .unwrap_err()
                .contains("bound to a height")
        );
        assert!(
            validate_replicated_execution_input_actions(&[input(3)], 2)
                .unwrap_err()
                .contains("height mismatch")
        );
        assert!(validate_replicated_execution_input_actions(&[input(2)], 2).is_ok());
    }

    #[test]
    fn fatal_paid_decode_is_not_a_non_matching_payload() {
        let action = input(2);
        let result = decode_replicated_execution_input_action_with::<
            NodeReplicatedExecutionInputV1,
            (),
            &'static str,
        >(&action, |_| Err("cancelled"));
        assert_eq!(result.unwrap_err(), "cancelled");
    }

    #[test]
    fn continuation_cbor_and_default_action_field_remain_compatible() {
        let raw = serde_cbor::to_vec(&input(4)).unwrap();
        let restored: NodeConsensusAction = serde_cbor::from_slice(&raw).unwrap();
        assert_eq!(restored, input(4));
    }
}
