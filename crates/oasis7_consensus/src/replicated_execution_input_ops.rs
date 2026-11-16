//! Original reserved-input predicates shared by ordinary and paid owners.
//! A codec refusal is still None; fatal work/storage/cancellation never is.
use super::*;
use std::{borrow::Borrow, convert::Infallible};
impl NodeReplicatedExecutionInputV1 {
    pub fn decode_with_owned<Owner: Borrow<Self>, CodecError, Fatal>(
        bytes: &[u8],
        decode: impl FnOnce(&[u8]) -> Result<Result<Owner, CodecError>, Fatal>,
    ) -> Result<Result<Option<Owner>, String>, Fatal> {
        let Ok(owner) = decode(bytes)? else {
            return Ok(Ok(None));
        };
        let input = owner.borrow();
        if input.version != REPLICATED_EXECUTION_INPUT_VERSION {
            return Ok(Err(format!(
                "unsupported replicated execution input version {}",
                input.version
            )));
        }
        if input.kind.trim().is_empty() {
            return Ok(Err(
                "replicated execution input kind cannot be empty".to_string()
            ));
        }
        if input.payload_cbor.is_empty() {
            return Ok(Err(
                "replicated execution input payload cannot be empty".to_string()
            ));
        }
        Ok(Ok(Some(owner)))
    }
}

pub fn decode_replicated_execution_input_action_with<
    Owner: Borrow<NodeReplicatedExecutionInputV1>,
    CodecError,
    Fatal,
>(
    action: &NodeConsensusAction,
    decode: impl FnOnce(&[u8]) -> Result<Result<Owner, CodecError>, Fatal>,
) -> Result<Result<Option<Owner>, String>, Fatal> {
    if action.action_id != REPLICATED_EXECUTION_INPUT_ACTION_ID
        || action.submitter_player_id != REPLICATED_EXECUTION_INPUT_SUBMITTER
    {
        return Ok(Ok(None));
    }
    NodeReplicatedExecutionInputV1::decode_with_owned(&action.payload_cbor, decode)
}

/// before_action pays the actual traversal. Neither callback can replace the
/// original identity, duplicate or target-height predicates below.
pub trait ReplicatedExecutionInputValidationOpsV1 {
    type Owner: Borrow<NodeReplicatedExecutionInputV1>;
    type Fatal;
    fn before_action(&mut self, action: &NodeConsensusAction) -> Result<(), Self::Fatal>;
    fn decode_action(
        &mut self,
        action: &NodeConsensusAction,
    ) -> Result<Result<Option<Self::Owner>, String>, Self::Fatal>;
}
pub fn validate_replicated_execution_input_actions_with<
    O: ReplicatedExecutionInputValidationOpsV1,
>(
    actions: &[NodeConsensusAction],
    expected_height: u64,
    ops: &mut O,
) -> Result<Result<(), String>, O::Fatal> {
    let mut found_input = false;
    for action in actions {
        ops.before_action(action)?;
        let reserved_identity = action.action_id == REPLICATED_EXECUTION_INPUT_ACTION_ID
            || action.submitter_player_id == REPLICATED_EXECUTION_INPUT_SUBMITTER;
        if !reserved_identity {
            continue;
        }
        let owner = match ops.decode_action(action)? {
            Err(error) => return Ok(Err(error)),
            Ok(None) => {
                return Ok(Err(format!(
                    "reserved replicated execution input identity is malformed action_id={} submitter={}",
                    action.action_id, action.submitter_player_id
                )));
            }
            Ok(Some(owner)) => owner,
        };
        let input = owner.borrow();
        if found_input {
            return Ok(Err(format!(
                "multiple replicated execution inputs are not allowed at height {}",
                expected_height
            )));
        }
        found_input = true;
        if input.target_height == 0 {
            return Ok(Err(
                "committed replicated execution input must be bound to a height".to_string(),
            ));
        }
        if expected_height != 0 && input.target_height != expected_height {
            return Ok(Err(format!(
                "replicated execution input height mismatch: expected={} actual={}",
                expected_height, input.target_height
            )));
        }
    }
    Ok(Ok(()))
}
pub(super) struct Legacy;
impl ReplicatedExecutionInputValidationOpsV1 for Legacy {
    type Owner = NodeReplicatedExecutionInputV1;
    type Fatal = Infallible;
    fn before_action(&mut self, _: &NodeConsensusAction) -> Result<(), Infallible> {
        Ok(())
    }
    fn decode_action(
        &mut self,
        action: &NodeConsensusAction,
    ) -> Result<Result<Option<Self::Owner>, String>, Infallible> {
        Ok(decode_replicated_execution_input_action(action))
    }
}
