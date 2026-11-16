use super::*;
#[path = "replication_local_recovery.rs"]
mod local_recovery;
use oasis7_proto::distributed_checkpoint_lineage::CheckpointLineageEnvelopeV1;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ReplicationCommitPayloadView {
    pub(super) height: u64,
    pub(super) block_hash: String,
    pub(super) committed_at_ms: i64,
    #[serde(default)]
    pub(super) execution_block_hash: Option<String>,
    #[serde(default)]
    pub(super) execution_state_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ReplicationCommitPayload {
    pub(super) world_id: String,
    pub(super) node_id: String,
    #[serde(default)]
    pub(super) proposer_id: Option<String>,
    pub(super) height: u64,
    pub(super) slot: u64,
    pub(super) epoch: u64,
    pub(super) block_hash: String,
    pub(super) action_root: String,
    pub(super) actions: Vec<NodeConsensusAction>,
    pub(super) committed_at_ms: i64,
    #[serde(default)]
    pub(super) execution_block_hash: Option<String>,
    #[serde(default)]
    pub(super) execution_state_root: Option<String>,
    #[serde(default)]
    pub(super) execution_checkpoint: Option<NodeExecutionCheckpointDescriptor>,
    #[serde(default)]
    pub(super) lineage_envelope: Option<CheckpointLineageEnvelopeV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NodeEngineTickResult {
    pub(super) consensus_snapshot: NodeConsensusSnapshot,
    pub(super) committed_action_batch: Option<NodeCommittedActionBatch>,
}

pub(super) fn parse_replication_commit_payload_view(
    payload: &[u8],
) -> Option<ReplicationCommitPayloadView> {
    serde_json::from_slice::<ReplicationCommitPayloadView>(payload).ok()
}

pub(super) fn parse_replication_commit_payload(payload: &[u8]) -> Option<ReplicationCommitPayload> {
    serde_json::from_slice::<ReplicationCommitPayload>(payload).ok()
}

pub(super) fn reconcile_engine_with_persisted_replication(
    engine: &mut PosNodeEngine,
    replication: &ReplicationRuntime,
    world_id: &str,
    mut execution_hook: Option<&mut dyn NodeExecutionHook>,
) -> Result<(), NodeError> {
    if execution_hook
        .as_ref()
        .is_some_and(|hook| hook.guarded_local_execution())
    {
        return Err(NodeError::Execution {
            reason:
                "guarded engineering execution rejects legacy replication startup reconciliation"
                    .into(),
        });
    }
    let latest_persisted_height = replication.latest_persisted_commit_height(world_id)?;
    if local_recovery::reconcile_recovered_publication(
        engine,
        replication,
        world_id,
        latest_persisted_height,
        &mut execution_hook,
    )? {
        return Ok(());
    }
    if latest_persisted_height < engine.committed_height {
        if latest_persisted_height == 0 {
            return Ok(());
        }
        let message =
            load_validated_persisted_commit(replication, world_id, latest_persisted_height)?;
        let payload =
            parse_validated_persisted_commit_payload(world_id, latest_persisted_height, &message)?;
        engine.rollback_to_replicated_commit_boundary(
            latest_persisted_height,
            payload.block_hash,
            payload.committed_at_ms,
            payload.execution_block_hash,
            payload.execution_state_root,
        )?;
        if let Some(hook) = execution_hook.as_deref_mut() {
            let restored = hook
                .restore_to_height(world_id, latest_persisted_height)
                .map_err(|reason| NodeError::Execution {
                    reason: format!(
                        "persisted replication reconcile rollback to height {} failed to restore execution head: {}",
                        latest_persisted_height, reason
                    ),
                })?;
            if !restored {
                return Err(NodeError::Execution {
                    reason: format!(
                        "persisted replication reconcile rollback record for height {} is unavailable",
                        latest_persisted_height
                    ),
                });
            }
        }
        return Ok(());
    }
    engine.replication_persisted_height = engine
        .replication_persisted_height
        .max(latest_persisted_height);
    let replay_start_height = if execution_hook.is_some() {
        engine
            .last_execution_height
            .checked_add(1)
            .ok_or_else(|| NodeError::Replication {
                reason: format!(
                    "persisted replication reconcile execution replay overflow after last_execution_height={}",
                    engine.last_execution_height
                ),
            })?
    } else {
        engine
            .committed_height
            .checked_add(1)
            .ok_or_else(|| NodeError::Replication {
                reason: format!(
                    "persisted replication reconcile height overflow after committed_height={}",
                    engine.committed_height
                ),
            })?
    };
    if replay_start_height > latest_persisted_height {
        return Ok(());
    }
    replay_persisted_replication_commits(
        engine,
        replication,
        world_id,
        replay_start_height,
        latest_persisted_height,
        &mut execution_hook,
    )
}

fn replay_persisted_replication_commits(
    engine: &mut PosNodeEngine,
    replication: &ReplicationRuntime,
    world_id: &str,
    start_height: u64,
    latest_persisted_height: u64,
    execution_hook: &mut Option<&mut dyn NodeExecutionHook>,
) -> Result<(), NodeError> {
    if execution_hook
        .as_ref()
        .is_some_and(|hook| hook.guarded_local_execution())
    {
        return Err(NodeError::Execution {
            reason: "guarded engineering execution rejects legacy replicated checkpoints".into(),
        });
    }
    let mut height = start_height;
    while height <= latest_persisted_height {
        let message = load_validated_persisted_commit(replication, world_id, height)?;
        let payload = parse_validated_persisted_commit_payload(world_id, height, &message)?;
        if let (Some(hook), Some(descriptor)) = (
            execution_hook.as_deref_mut(),
            payload.execution_checkpoint.as_ref(),
        ) && descriptor.height == payload.height
            && payload.height > engine.last_execution_height.saturating_add(1)
        {
            let Some(bundle) = replication.load_execution_checkpoint_bundle(descriptor)? else {
                return Err(NodeError::Replication {
                    reason: format!(
                        "persisted checkpoint bundle missing for height {}",
                        payload.height
                    ),
                });
            };
            let result = hook
                .install_checkpoint_bundle(
                    NodeExecutionCheckpointInstallContext {
                        world_id: world_id.to_string(),
                        node_id: payload.node_id.clone(),
                        height: payload.height,
                        node_block_hash: payload.block_hash.clone(),
                        execution_block_hash: descriptor.execution_block_hash.clone(),
                        execution_state_root: descriptor.execution_state_root.clone(),
                        committed_at_unix_ms: payload.committed_at_ms,
                    },
                    bundle,
                )
                .map_err(|reason| NodeError::Execution { reason })?;
            if result.execution_height != payload.height
                || Some(result.execution_block_hash.as_str())
                    != payload.execution_block_hash.as_deref()
                || Some(result.execution_state_root.as_str())
                    != payload.execution_state_root.as_deref()
            {
                return Err(NodeError::Execution {
                    reason: format!(
                        "persisted checkpoint install returned mismatched binding at height {}",
                        payload.height
                    ),
                });
            }
            engine.last_execution_height = result.execution_height;
            engine.last_execution_block_hash = Some(result.execution_block_hash);
            engine.last_execution_state_root = Some(result.execution_state_root);
            engine.remember_execution_binding_for_height(payload.height);
        }
        with_execution_hook(execution_hook, |hook| {
            engine.apply_synced_replication_commit(world_id, &payload, hook)
        })?;
        if height == latest_persisted_height {
            break;
        }
        height = height
            .checked_add(1)
            .ok_or_else(|| NodeError::Replication {
                reason: format!(
                    "persisted replication reconcile cursor overflow at height={height}"
                ),
            })?;
    }
    Ok(())
}

fn load_validated_persisted_commit(
    replication: &ReplicationRuntime,
    world_id: &str,
    height: u64,
) -> Result<super::replication::GossipReplicationMessage, NodeError> {
    let message = replication
        .load_commit_message_by_height(world_id, height)?
        .ok_or_else(|| NodeError::Replication {
            reason: format!(
                "persisted commit missing for world={} height={}",
                world_id, height
            ),
        })?;
    if message.world_id != world_id || message.record.world_id != world_id {
        return Err(NodeError::Replication {
            reason: format!(
                "persisted commit world mismatch at height {} expected={} actual_message={} actual_record={}",
                height, world_id, message.world_id, message.record.world_id
            ),
        });
    }
    Ok(message)
}

fn parse_validated_persisted_commit_payload(
    world_id: &str,
    height: u64,
    message: &super::replication::GossipReplicationMessage,
) -> Result<ReplicationCommitPayload, NodeError> {
    let payload =
        parse_replication_commit_payload(message.payload.as_slice()).ok_or_else(|| {
            NodeError::Replication {
                reason: format!(
                    "persisted commit payload decode failed for world={} height={}",
                    world_id, height
                ),
            }
        })?;
    if payload.world_id != world_id {
        return Err(NodeError::Replication {
            reason: format!(
                "persisted commit payload world mismatch at height {} expected={} actual={}",
                height, world_id, payload.world_id
            ),
        });
    }
    if payload.node_id != message.node_id {
        return Err(NodeError::Replication {
            reason: format!(
                "persisted commit payload node mismatch at height {} expected={} actual={}",
                height, message.node_id, payload.node_id
            ),
        });
    }
    if payload.height != height {
        return Err(NodeError::Replication {
            reason: format!(
                "persisted commit payload height mismatch expected={} actual={}",
                height, payload.height
            ),
        });
    }
    if payload.block_hash.trim().is_empty() {
        return Err(NodeError::Replication {
            reason: format!(
                "persisted commit payload block_hash is empty at height={}",
                height
            ),
        });
    }
    validate_consensus_action_root(payload.action_root.as_str(), payload.actions.as_slice())
        .map_err(|err| NodeError::Replication {
            reason: format!(
                "persisted commit action_root validation failed at height {}: {:?}",
                height, err
            ),
        })?;
    validate_replicated_execution_input_actions(payload.actions.as_slice(), payload.height)
        .map_err(|err| NodeError::Replication {
            reason: format!(
                "persisted commit replicated execution input validation failed at height {}: {}",
                payload.height, err
            ),
        })?;
    Ok(payload)
}
