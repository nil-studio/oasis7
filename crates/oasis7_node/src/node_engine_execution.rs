//! Typed local execution outcome and exact retained qualified replay.
use super::*;

impl PosNodeEngine {
    pub(super) fn apply_committed_execution(
        &mut self,
        node_id: &str,
        world_id: &str,
        now_ms: i64,
        decision: &PosDecision,
        execution_hook: Option<&mut dyn NodeExecutionHook>,
    ) -> Result<bool, NodeError> {
        self.apply_committed_execution_with_expected(
            node_id,
            world_id,
            now_ms,
            decision,
            execution_hook,
            None,
            None,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable commit execution seam keeps expected peer bindings explicit for recovery validation"
    )]
    pub(super) fn apply_committed_execution_with_expected(
        &mut self,
        node_id: &str,
        world_id: &str,
        now_ms: i64,
        decision: &PosDecision,
        execution_hook: Option<&mut dyn NodeExecutionHook>,
        expected_execution_block_hash: Option<&str>,
        expected_execution_state_root: Option<&str>,
    ) -> Result<bool, NodeError> {
        if !matches!(decision.status, PosConsensusStatus::Committed) {
            return Ok(true);
        }
        let guarded = execution_hook
            .as_ref()
            .is_some_and(|hook| hook.guarded_local_execution());
        if decision.height < self.last_execution_height && guarded {
            return Err(NodeError::Execution {
                reason: "guarded stale decision cannot replace original continuation".into(),
            });
        }
        if decision.height <= self.last_execution_height && !guarded {
            return Ok(true);
        }
        let Some(execution_hook) = execution_hook else {
            if self.require_execution_on_commit {
                return Err(NodeError::Execution {
                    reason: format!(
                        "execution hook is required before committing height {}",
                        decision.height
                    ),
                });
            }
            return Ok(true);
        };

        let context = NodeExecutionCommitContext {
            world_id: world_id.to_string(),
            node_id: node_id.to_string(),
            proposer_id: decision.proposer_id.clone(),
            height: decision.height,
            slot: decision.slot,
            epoch: decision.epoch,
            node_block_hash: decision.block_hash.clone(),
            action_root: decision.action_root.clone(),
            committed_actions: decision.committed_actions.clone(),
            committed_at_unix_ms: now_ms,
        };
        if guarded
            && (decision.proposer_id != node_id
                || expected_execution_block_hash.is_some()
                || expected_execution_state_root.is_some())
        {
            return Err(NodeError::Execution {
                reason: "guarded engineering execution requires an original local decision".into(),
            });
        }
        if guarded && decision.height > self.last_execution_height {
            let continuation = NodeLocalExecutionContinuation {
                schema_version: 1,
                context: context.clone(),
                predecessor_committed_height: self.committed_height,
                predecessor_node_block_hash: self.last_committed_block_hash.clone(),
                predecessor_execution_height: self.last_execution_height,
                predecessor_execution_block_hash: self.last_execution_block_hash.clone(),
                predecessor_execution_state_root: self.last_execution_state_root.clone(),
                next_height: self.next_height,
                next_slot: self.next_slot,
                reserved_action_bytes: self.pending_action_reservation_bytes(),
                approved_stake: decision.approved_stake,
                rejected_stake: decision.rejected_stake,
                required_stake: decision.required_stake,
                total_stake: decision.total_stake,
            };
            execution_hook
                .prepare_verified_local_continuation(
                    &crate::execution_hook::VerifiedNodeLocalContinuationV1::from_engine(
                        &continuation,
                    ),
                )
                .map_err(|reason| NodeError::Execution { reason })?;
        }
        let result = match execution_hook.on_commit_outcome_with_expected(
            context,
            expected_execution_block_hash,
            expected_execution_state_root,
        ) {
            Ok(NodeExecutionCommitOutcome::Applied(result)) => result,
            Ok(NodeExecutionCommitOutcome::Deferred) => {
                if !execution_hook.guarded_local_execution() {
                    return Err(NodeError::Execution {
                        reason: "unguarded execution returned Deferred".into(),
                    });
                }
                return Ok(false);
            }
            Err(reason)
                if execution_error_waits_for_gap_sync(reason.as_str())
                    && !self.require_execution_on_commit
                    && !guarded =>
            {
                return Ok(true);
            }
            Err(reason) => return Err(NodeError::Execution { reason }),
        };

        if result.execution_height != decision.height {
            return Err(NodeError::Execution {
                reason: format!(
                    "execution hook returned mismatched height: expected {}, got {}",
                    decision.height, result.execution_height
                ),
            });
        }
        if result.execution_block_hash.trim().is_empty() {
            return Err(NodeError::Execution {
                reason: "execution hook returned empty execution_block_hash".to_string(),
            });
        }
        if result.execution_state_root.trim().is_empty() {
            return Err(NodeError::Execution {
                reason: "execution hook returned empty execution_state_root".to_string(),
            });
        }
        if let (Some(expected_block), Some(expected_state)) =
            (expected_execution_block_hash, expected_execution_state_root)
            && (result.execution_block_hash != expected_block
                || result.execution_state_root != expected_state)
        {
            return Err(NodeError::Execution {
                reason: format!(
                    "execution hook returned peer mismatch at height {}: local_block={} peer_block={} local_state={} peer_state={}",
                    decision.height,
                    result.execution_block_hash,
                    expected_block,
                    result.execution_state_root,
                    expected_state
                ),
            });
        }

        if guarded
            && decision.height == self.last_execution_height
            && (self.last_execution_block_hash.as_deref()
                != Some(result.execution_block_hash.as_str())
                || self.last_execution_state_root.as_deref()
                    != Some(result.execution_state_root.as_str()))
        {
            return Err(NodeError::Execution {
                reason: "guarded original qualified retry changed execution binding".into(),
            });
        }
        self.last_execution_height = result.execution_height;
        self.last_execution_block_hash = Some(result.execution_block_hash);
        self.last_execution_state_root = Some(result.execution_state_root);
        self.remember_execution_binding_for_height(decision.height);
        Ok(true)
    }
}

fn execution_error_waits_for_gap_sync(reason: &str) -> bool {
    reason.starts_with(EXECUTION_MISSING_PREDECESSOR_RECORD_SIGNATURE)
}
