//! Narrow same-height repair after exact local publication recovery, never general fork choice.
use super::*;
fn error(reason: &str) -> NodeError {
    NodeError::Replication {
        reason: format!("recovered local publication: {reason}"),
    }
}
pub(super) fn reconcile_recovered_publication(
    engine: &mut PosNodeEngine,
    replication: &ReplicationRuntime,
    world_id: &str,
    height: u64,
    hook: &mut Option<&mut dyn NodeExecutionHook>,
) -> Result<bool, NodeError> {
    if height == 0 || height != engine.committed_height {
        return Ok(false);
    }
    let message = load_validated_persisted_commit(replication, world_id, height)?;
    let payload = parse_validated_persisted_commit_payload(world_id, height, &message)?;
    let conflict = engine.last_committed_block_hash.as_deref() != Some(payload.block_hash.as_str())
        || (engine.last_execution_height == height
            && (engine.last_execution_block_hash != payload.execution_block_hash
                || engine.last_execution_state_root != payload.execution_state_root));
    if !conflict {
        return Ok(false);
    }
    if engine.last_execution_height != height {
        return Err(error(
            "same-height repair requires the same execution height",
        ));
    }
    if payload
        .execution_block_hash
        .as_deref()
        .is_none_or(str::is_empty)
        || payload
            .execution_state_root
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return Err(error("original execution binding is incomplete"));
    }
    let node = engine.local_validator_id.clone();
    if !replication.recovered_publication_matches(&node, &message)? {
        return Err(error(
            "same-height conflict has no exact local recovery proof",
        ));
    }
    replication.validate_local_recovery_commit(&node, &message)?;
    let previous = height
        .checked_sub(1)
        .filter(|h| *h > 0)
        .ok_or_else(|| error("verified predecessor unavailable"))?;
    let predecessor = load_validated_persisted_commit(replication, world_id, previous)?;
    replication.validate_local_recovery_commit(&node, &predecessor)?;
    let prior = parse_validated_persisted_commit_payload(world_id, previous, &predecessor)?;
    let expected = NodeExecutionCommitResult {
        execution_height: previous,
        execution_block_hash: prior
            .execution_block_hash
            .clone()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| error("predecessor execution hash missing"))?,
        execution_state_root: prior
            .execution_state_root
            .clone()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| error("predecessor execution root missing"))?,
    };
    let context = NodeExecutionCommitContext {
        world_id: world_id.into(),
        node_id: prior.node_id.clone(),
        proposer_id: prior
            .proposer_id
            .clone()
            .ok_or_else(|| error("predecessor proposer missing"))?,
        height: previous,
        slot: prior.slot,
        epoch: prior.epoch,
        node_block_hash: prior.block_hash.clone(),
        action_root: prior.action_root.clone(),
        committed_actions: prior.actions.clone(),
        committed_at_unix_ms: prior.committed_at_ms,
    };
    let verified = hook
        .as_deref_mut()
        .ok_or_else(|| error("qualified execution hook unavailable"))?
        .restore_to_verified_commit(&context, &expected)
        .map_err(|e| error(&e))?;
    if !verified {
        return Err(error("verified predecessor restoration unavailable"));
    }
    engine.rollback_to_replicated_commit_boundary(
        previous,
        prior.block_hash,
        prior.committed_at_ms,
        prior.execution_block_hash,
        prior.execution_state_root,
    )?;
    replay_persisted_replication_commits(engine, replication, world_id, height, height, hook)?;
    if engine.committed_height != height
        || engine.last_committed_block_hash.as_deref() != Some(payload.block_hash.as_str())
        || engine.last_execution_height != height
        || engine.last_execution_block_hash != payload.execution_block_hash
        || engine.last_execution_state_root != payload.execution_state_root
    {
        return Err(error("replayed original execution binding mismatch"));
    }
    Ok(true)
}
