use oasis7_node::NodeExecutionCommitContext;

use super::driver::NodeRuntimeExecutionDriver;
use super::external_effect::load_execution_external_effect_materialization;
use super::{
    EXECUTION_BRIDGE_RECORD_SCHEMA_V3, ExecutionBridgeRecord, ExecutionCommittedActionAnchor,
};

impl NodeRuntimeExecutionDriver {
    pub(super) fn validate_equal_height_replay_identity(
        &self,
        record: &ExecutionBridgeRecord,
        context: &NodeExecutionCommitContext,
    ) -> Result<(), String> {
        if record.schema_version < EXECUTION_BRIDGE_RECORD_SCHEMA_V3 {
            return Ok(());
        }
        let checkpoint_install_record = record.checkpoint_ref.is_some()
            && record.proposer_id.is_none()
            && record.action_root.is_none();
        if checkpoint_install_record {
            return Err(format!(
                "execution driver equal-height checkpoint-install record cannot authenticate consensus replay at height {}",
                context.height
            ));
        }

        let node_block_hash = record.node_block_hash.as_deref().ok_or_else(|| {
            format!(
                "execution driver equal-height V3 record missing node_block_hash at height {}",
                context.height
            )
        })?;
        if node_block_hash != context.node_block_hash {
            return Err(format!(
                "execution driver equal-height V3 node_block_hash mismatch at height {}: expected={} actual={}",
                context.height, node_block_hash, context.node_block_hash
            ));
        }
        let proposer_id = record.proposer_id.as_deref().ok_or_else(|| {
            format!(
                "execution driver equal-height V3 record missing proposer_id at height {}",
                context.height
            )
        })?;
        if proposer_id != context.proposer_id {
            return Err(format!(
                "execution driver equal-height V3 proposer_id mismatch at height {}: expected={} actual={}",
                context.height, proposer_id, context.proposer_id
            ));
        }
        let action_root = record.action_root.as_deref().ok_or_else(|| {
            format!(
                "execution driver equal-height V3 record missing action_root at height {}",
                context.height
            )
        })?;
        if action_root != context.action_root {
            return Err(format!(
                "execution driver equal-height V3 action_root mismatch at height {}: expected={} actual={}",
                context.height, action_root, context.action_root
            ));
        }
        if record.timestamp_ms != context.committed_at_unix_ms {
            return Err(format!(
                "execution driver equal-height V3 committed_at_unix_ms mismatch at height {}: expected={} actual={}",
                context.height, record.timestamp_ms, context.committed_at_unix_ms
            ));
        }

        let external_effect_ref = record.external_effect_ref.as_deref().ok_or_else(|| {
            format!(
                "execution driver equal-height V3 record missing authoritative external effect at height {}",
                context.height
            )
        })?;
        let external_effect = load_execution_external_effect_materialization(
            &self.execution_store,
            external_effect_ref,
        )?;
        if external_effect.world_id != context.world_id {
            return Err(format!(
                "execution driver equal-height V3 authoritative effect world_id mismatch at height {}: expected={} actual={}",
                context.height, context.world_id, external_effect.world_id
            ));
        }
        if external_effect.node_id != context.node_id {
            return Err(format!(
                "execution driver equal-height V3 authoritative effect node_id mismatch at height {}: expected={} actual={}",
                context.height, context.node_id, external_effect.node_id
            ));
        }
        if external_effect.height != context.height {
            return Err(format!(
                "execution driver equal-height V3 authoritative effect height mismatch: expected={} actual={}",
                context.height, external_effect.height
            ));
        }
        if external_effect.slot != context.slot {
            return Err(format!(
                "execution driver equal-height V3 authoritative effect slot mismatch at height {}: expected={} actual={}",
                context.height, context.slot, external_effect.slot
            ));
        }
        if external_effect.epoch != context.epoch {
            return Err(format!(
                "execution driver equal-height V3 authoritative effect epoch mismatch at height {}: expected={} actual={}",
                context.height, context.epoch, external_effect.epoch
            ));
        }
        if external_effect.node_block_hash != context.node_block_hash {
            return Err(format!(
                "execution driver equal-height V3 authoritative effect node_block_hash mismatch at height {}: expected={} actual={}",
                context.height, context.node_block_hash, external_effect.node_block_hash
            ));
        }
        if external_effect.action_root != context.action_root {
            return Err(format!(
                "execution driver equal-height V3 authoritative effect action_root mismatch at height {}: expected={} actual={}",
                context.height, context.action_root, external_effect.action_root
            ));
        }
        if external_effect.committed_at_unix_ms != context.committed_at_unix_ms {
            return Err(format!(
                "execution driver equal-height V3 authoritative effect committed_at_unix_ms mismatch at height {}: expected={} actual={}",
                context.height, context.committed_at_unix_ms, external_effect.committed_at_unix_ms
            ));
        }
        let mut committed_actions: Vec<_> = context
            .committed_actions
            .iter()
            .map(|action| ExecutionCommittedActionAnchor {
                action_id: action.action_id,
                submitter_player_id: action.submitter_player_id.clone(),
                payload_hash: action.payload_hash.clone(),
            })
            .collect();
        committed_actions.sort_by_key(|left| left.action_id);
        if external_effect.committed_actions != committed_actions {
            return Err(format!(
                "execution driver equal-height V3 committed_actions mismatch at height {}",
                context.height
            ));
        }
        Ok(())
    }
}
