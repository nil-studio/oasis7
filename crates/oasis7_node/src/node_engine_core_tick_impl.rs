impl PosNodeEngine {
    #[cfg(test)]
    #[expect(
        clippy::too_many_arguments,
        reason = "Legacy test tick seam preserves the existing consensus harness call shape"
    )]
    pub(super) fn tick(
        &mut self,
        node_id: &str,
        world_id: &str,
        now_ms: i64,
        gossip: Option<&GossipEndpoint>,
        replication: Option<&mut ReplicationRuntime>,
        replication_network: Option<&mut ReplicationNetworkEndpoint>,
        consensus_network: Option<&mut ConsensusNetworkEndpoint>,
        queued_actions: Vec<NodeConsensusAction>,
        execution_hook: Option<&mut dyn NodeExecutionHook>,
    ) -> Result<NodeEngineTickResult, NodeError> {
        self.tick_with_progress(
            node_id,
            world_id,
            now_ms,
            gossip,
            replication,
            replication_network,
            consensus_network,
            queued_actions,
            false,
            execution_hook,
            None,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable node tick state-machine seam keeps network, execution, and progress channels explicit"
    )]
    pub(super) fn tick_with_progress(
        &mut self,
        node_id: &str,
        world_id: &str,
        now_ms: i64,
        gossip: Option<&GossipEndpoint>,
        mut replication: Option<&mut ReplicationRuntime>,
        replication_network: Option<&mut ReplicationNetworkEndpoint>,
        consensus_network: Option<&mut ConsensusNetworkEndpoint>,
        queued_actions: Vec<NodeConsensusAction>,
        incoming_actions_already_reserved: bool,
        mut execution_hook: Option<&mut dyn NodeExecutionHook>,
        mut progress_callback: Option<
            &mut dyn FnMut(NodeConsensusSnapshot) -> Result<(), NodeError>,
        >,
    ) -> Result<NodeEngineTickResult, NodeError> {
        merge_pending_consensus_actions_with_budget(
            &mut self.pending_consensus_actions,
            queued_actions,
            self.max_pending_consensus_actions,
            &self.pending_consensus_action_queue_bytes,
            self.max_pending_consensus_action_queue_bytes,
            incoming_actions_already_reserved,
        )?;

        let mut continuation = with_execution_hook(&mut execution_hook, |hook| match hook {
            Some(hook) if hook.guarded_local_execution() => hook
                .pending_local_continuation()
                .map_err(|reason| NodeError::Execution { reason }),
            _ => Ok(None),
        })?;
        if let Some(saved) = continuation.as_ref()
            && self.committed_height == saved.context.height
            && self.last_execution_height == saved.context.height
            && self.last_committed_block_hash.as_deref()
                == Some(saved.context.node_block_hash.as_str())
        {
            with_execution_hook(&mut execution_hook, |hook| {
                hook.ok_or_else(|| NodeError::Execution {
                    reason: "guarded hook unavailable".into(),
                })?
                .complete_local_continuation(&saved.context)
                .map_err(|reason| NodeError::Execution { reason })
            })?;
            continuation = None;
        }
        let decision_time = continuation
            .as_ref()
            .map_or(now_ms, |saved| saved.context.committed_at_unix_ms);
        let resuming_local = continuation.is_some();
        let observed_tick = self.observe_wall_clock_tick(now_ms)?;
        let current_slot = observed_tick.slot;
        let decision = if let Some(saved) = continuation.as_ref() {
            self.restore_guarded_local_decision(saved, node_id, world_id)?
        } else {
            if let Some(endpoint) = gossip.as_ref() {
                self.seed_reverse_gossip_path(endpoint, node_id, world_id, now_ms)?;
            }
            if let Some(endpoint) = gossip.as_ref() {
                self.ingest_peer_messages(
                    endpoint,
                    node_id,
                    world_id,
                    replication.as_deref_mut(),
                    current_slot,
                )?;
            }
            if let Some(endpoint) = consensus_network.as_ref() {
                self.ingest_consensus_network_messages(
                    endpoint,
                    node_id,
                    world_id,
                    current_slot,
                    replication.as_deref_mut(),
                )?;
            }
            if let Some(endpoint) = replication_network.as_ref() {
                let record_peer_heads_from_gap_sync =
                    gossip.is_some() || consensus_network.is_some();
                match (&mut execution_hook, &mut progress_callback) {
                    (Some(hook), Some(callback)) => {
                        self.ingest_network_replications_with_progress(
                            endpoint,
                            node_id,
                            world_id,
                            replication.as_deref_mut(),
                            Some(&mut **hook),
                            Some(&mut **callback),
                        )?;
                        self.sync_missing_replication_commits_with_progress(
                            endpoint,
                            node_id,
                            world_id,
                            replication.as_deref_mut(),
                            Some(&mut **hook),
                            Some(&mut **callback),
                            record_peer_heads_from_gap_sync,
                        )?;
                    }
                    (Some(hook), None) => {
                        self.ingest_network_replications_with_progress(
                            endpoint,
                            node_id,
                            world_id,
                            replication.as_deref_mut(),
                            Some(&mut **hook),
                            None,
                        )?;
                        self.sync_missing_replication_commits_with_progress(
                            endpoint,
                            node_id,
                            world_id,
                            replication.as_deref_mut(),
                            Some(&mut **hook),
                            None,
                            record_peer_heads_from_gap_sync,
                        )?;
                    }
                    (None, Some(callback)) => {
                        self.ingest_network_replications_with_progress(
                            endpoint,
                            node_id,
                            world_id,
                            replication.as_deref_mut(),
                            None,
                            Some(&mut **callback),
                        )?;
                        self.sync_missing_replication_commits_with_progress(
                            endpoint,
                            node_id,
                            world_id,
                            replication.as_deref_mut(),
                            None,
                            Some(&mut **callback),
                            record_peer_heads_from_gap_sync,
                        )?;
                    }
                    (None, None) => {
                        self.ingest_network_replications_with_progress(
                            endpoint,
                            node_id,
                            world_id,
                            replication.as_deref_mut(),
                            None,
                            None,
                        )?;
                        self.sync_missing_replication_commits_with_progress(
                            endpoint,
                            node_id,
                            world_id,
                            replication.as_deref_mut(),
                            None,
                            None,
                            record_peer_heads_from_gap_sync,
                        )?;
                    }
                }
            }
            self.maybe_publish_local_checkpoint_lineage_vote(
                consensus_network.as_deref(),
                gossip,
                node_id,
                world_id,
                replication.as_deref_mut(),
            )?;
            if let Some(callback) = progress_callback.as_deref_mut() {
                let observed = self.idle_pending_decision()?;
                callback(self.snapshot_from_decision(&observed))?;
            }
            self.rebroadcast_replicated_commit_head(
                consensus_network.as_deref(),
                gossip,
                node_id,
                world_id,
                now_ms,
                replication.as_deref(),
            )?;
            let hold_for_replication_probe = if let Some(endpoint) = replication_network.as_ref() {
                with_execution_hook(&mut execution_hook, |hook| {
                    self.maybe_hold_proposal_for_replication_successor_probe(
                        endpoint,
                        node_id,
                        world_id,
                        now_ms,
                        replication.as_deref_mut(),
                        hook,
                    )
                })?
            } else {
                false
            };
            let recovered_from_skipped_slots = self.align_next_slot_to_wall_clock(current_slot)?;
            let consensus_participation_safe = self.consensus_participation_safe();

            let mut decision = if self.pending.is_some() {
                self.advance_pending_attestations(now_ms)?
            } else if hold_for_replication_probe
                || !consensus_participation_safe
                || !self.allow_local_proposals
            {
                self.idle_pending_decision()?
            } else if self.next_slot <= current_slot
                && (observed_tick.tick_phase == self.proposal_tick_phase
                    || recovered_from_skipped_slots)
            {
                with_execution_hook(&mut execution_hook, |hook| {
                    self.discard_invalid_governance_actions(hook.map(|hook| &*hook));
                });
                self.propose_next_head(node_id, world_id, now_ms)?
            } else {
                self.idle_pending_decision()?
            };

            if matches!(decision.status, PosConsensusStatus::Pending) && self.pending.is_some() {
                decision = self.advance_pending_attestations(now_ms)?;
            }

            decision
        };
        let consensus_participation_safe = self.consensus_participation_safe();
        if resuming_local || !consensus_participation_safe {
            // Continue ingesting/repairing state, but do not advertise local
            // consensus votes while this node is outside the verified sync boundary.
        } else if let Some(endpoint) = consensus_network.as_ref() {
            self.broadcast_local_proposal_network(endpoint, node_id, world_id, now_ms)?;
            self.broadcast_local_attestation_network(endpoint, node_id, world_id, now_ms)?;
        } else if let Some(endpoint) = gossip.as_ref() {
            self.broadcast_local_proposal(endpoint, node_id, world_id, now_ms)?;
            self.broadcast_local_attestation(endpoint, node_id, world_id, now_ms)?;
        }

        let prev_committed_height = self.committed_height;
        if matches!(decision.status, PosConsensusStatus::Committed)
            && decision.height > prev_committed_height
            && self
                .pending
                .as_ref()
                .map(|proposal| proposal.proposer_id.as_str() != node_id)
                .unwrap_or(false)
        {
            let has_matching_remote_replication = match replication.as_deref() {
                Some(replication_runtime) => replication_runtime
                    .load_commit_message_by_height(world_id, decision.height)?
                    .and_then(|message| {
                        parse_replication_commit_payload(message.payload.as_slice())
                    })
                    .is_some_and(|payload| {
                        payload.world_id == world_id
                            && payload.height == decision.height
                            && payload.block_hash == decision.block_hash
                            && payload.action_root == decision.action_root
                            && payload.actions == decision.committed_actions
                    }),
                None => true,
            };
            if !has_matching_remote_replication {
                self.last_inbound_timing_reject_reason = Some(format!(
                    "drop remote committed height {} without matching persisted replication commit",
                    decision.height
                ));
                let held = self.idle_pending_decision()?;
                return Ok(NodeEngineTickResult {
                    consensus_snapshot: self.snapshot_from_decision(&held),
                    committed_action_batch: None,
                });
            }
        }
        let decision_time = with_execution_hook(&mut execution_hook, |hook| {
            let Some(hook) = hook else {
                return Ok(decision_time);
            };
            if hook.guarded_local_execution()
                || resuming_local
                || !matches!(decision.status, PosConsensusStatus::Committed)
                || decision.proposer_id != node_id
            {
                return Ok(decision_time);
            }
            let context = NodeExecutionCommitContext {
                world_id: world_id.into(),
                node_id: node_id.into(),
                proposer_id: decision.proposer_id.clone(),
                height: decision.height,
                slot: decision.slot,
                epoch: decision.epoch,
                node_block_hash: decision.block_hash.clone(),
                action_root: decision.action_root.clone(),
                committed_actions: decision.committed_actions.clone(),
                committed_at_unix_ms: decision_time,
            };
            hook.retained_local_commit_time(&context)
                .map(|retained| retained.unwrap_or(decision_time))
                .map_err(|reason| NodeError::Execution { reason })
        })?;
        let previous_execution_height = self.last_execution_height;
        let previous_execution_block_hash = self.last_execution_block_hash.clone();
        let previous_execution_state_root = self.last_execution_state_root.clone();
        let execution_complete = with_execution_hook(&mut execution_hook, |hook| {
            self.apply_committed_execution(node_id, world_id, decision_time, &decision, hook)
        })?;
        if !execution_complete {
            return Ok(NodeEngineTickResult {
                consensus_snapshot: self.snapshot_from_decision(&decision),
                committed_action_batch: None,
            });
        }
        let local_execution_applied = matches!(decision.status, PosConsensusStatus::Committed)
            && decision.height > previous_execution_height
            && self.last_execution_height >= decision.height;
        if let Err(err) = with_execution_hook(&mut execution_hook, |hook| {
            self.broadcast_local_replication(
                gossip,
                replication_network.as_deref(),
                node_id,
                world_id,
                decision_time,
                &decision,
                replication.as_deref_mut(),
                hook,
            )
        }) {
            self.rollback_local_committed_execution_after_failure(
                world_id,
                decision.height,
                previous_execution_height,
                previous_execution_block_hash.as_deref(),
                previous_execution_state_root.as_deref(),
                execution_hook.as_deref_mut(),
                &err,
            )?;
            return Err(err);
        }
        if matches!(decision.status, PosConsensusStatus::Committed)
            && decision.height > prev_committed_height
            && let Some(latency_ms) = self.pending.as_ref().and_then(|proposal| {
                (proposal.height == decision.height)
                    .then(|| now_ms.saturating_sub(proposal.opened_at_ms))
            })
        {
            self.record_finality_latency(latency_ms);
        }
        if let Err(err) = self.apply_decision(&decision) {
            self.rollback_local_committed_execution_after_failure(
                world_id,
                decision.height,
                previous_execution_height,
                previous_execution_block_hash.as_deref(),
                previous_execution_state_root.as_deref(),
                execution_hook.as_deref_mut(),
                &err,
            )?;
            return Err(err);
        }
        if matches!(decision.status, PosConsensusStatus::Committed)
            && decision.height > prev_committed_height
        {
            self.last_committed_at_ms = Some(decision_time);
            with_execution_hook(&mut execution_hook, |hook| {
                if let Some(hook) = hook
                    && hook.guarded_local_execution()
                {
                    let context = NodeExecutionCommitContext {
                        world_id: world_id.into(),
                        node_id: node_id.into(),
                        proposer_id: decision.proposer_id.clone(),
                        height: decision.height,
                        slot: decision.slot,
                        epoch: decision.epoch,
                        node_block_hash: decision.block_hash.clone(),
                        action_root: decision.action_root.clone(),
                        committed_actions: decision.committed_actions.clone(),
                        committed_at_unix_ms: decision_time,
                    };
                    hook.complete_local_continuation(&context)
                        .map_err(|reason| NodeError::Execution { reason })?;
                }
                Ok(())
            })?;
        }
        if let Some(endpoint) = consensus_network.as_ref() {
            self.broadcast_local_commit_network(endpoint, node_id, world_id, now_ms, &decision)?;
        }
        if let Some(endpoint) = gossip.as_ref() {
            self.broadcast_local_commit(endpoint, node_id, world_id, now_ms, &decision)?;
        }
        self.rebroadcast_replicated_commit_head(
            consensus_network.as_deref(),
            gossip,
            node_id,
            world_id,
            now_ms,
            replication.as_deref(),
        )?;
        if let Some(endpoint) = gossip.as_ref() {
            self.ingest_peer_messages(
                endpoint,
                node_id,
                world_id,
                replication.as_deref_mut(),
                current_slot,
            )?;
        }
        if let Some(endpoint) = consensus_network.as_ref() {
            self.ingest_consensus_network_messages(
                endpoint,
                node_id,
                world_id,
                current_slot,
                replication.as_deref_mut(),
            )?;
        }
        if let Some(endpoint) = replication_network.as_ref() {
            with_execution_hook(&mut execution_hook, |hook| {
                self.ingest_network_replications(endpoint, node_id, world_id, replication, hook)
            })?;
        }
        if local_execution_applied
            && (self.committed_height < decision.height
                || self.last_execution_height < decision.height)
        {
            let err = NodeError::Execution {
                reason: format!(
                    "local committed height {} execution did not remain at the committed boundary: committed_height={} last_execution_height={}",
                    decision.height, self.committed_height, self.last_execution_height
                ),
            };
            if self.committed_height < decision.height {
                self.rollback_local_committed_execution_after_failure(
                    world_id,
                    decision.height,
                    previous_execution_height,
                    previous_execution_block_hash.as_deref(),
                    previous_execution_state_root.as_deref(),
                    execution_hook,
                    &err,
                )?;
            }
            return Err(err);
        }
        let committed_action_batch = if matches!(decision.status, PosConsensusStatus::Committed)
            && !decision.committed_actions.is_empty()
            && decision.height > prev_committed_height
        {
            Some(NodeCommittedActionBatch {
                height: decision.height,
                slot: decision.slot,
                epoch: decision.epoch,
                block_hash: decision.block_hash.clone(),
                action_root: decision.action_root.clone(),
                committed_at_unix_ms: decision_time,
                actions: decision.committed_actions.clone(),
            })
        } else {
            None
        };

        let consensus_snapshot = self.snapshot_from_decision(&decision);
        if let Some(callback) = progress_callback {
            callback(consensus_snapshot.clone())?;
        }
        Ok(NodeEngineTickResult {
            consensus_snapshot,
            committed_action_batch,
        })
    }

    fn rebroadcast_replicated_commit_head(
        &mut self,
        consensus_network: Option<&ConsensusNetworkEndpoint>,
        gossip: Option<&GossipEndpoint>,
        node_id: &str,
        world_id: &str,
        now_ms: i64,
        replication: Option<&ReplicationRuntime>,
    ) -> Result<(), NodeError> {
        if let Some(endpoint) = consensus_network {
            self.broadcast_replicated_commit_head_network(
                endpoint,
                node_id,
                world_id,
                now_ms,
                replication,
            )?;
        }
        if let Some(endpoint) = gossip {
            self.broadcast_replicated_commit_head_gossip(
                endpoint,
                node_id,
                world_id,
                now_ms,
                replication,
            )?;
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable local execution rollback seam keeps prior bindings and failure context explicit"
    )]
    fn rollback_local_committed_execution_after_failure(
        &mut self,
        world_id: &str,
        decision_height: u64,
        previous_execution_height: u64,
        previous_execution_block_hash: Option<&str>,
        previous_execution_state_root: Option<&str>,
        execution_hook: Option<&mut (dyn NodeExecutionHook + '_)>,
        err: &NodeError,
    ) -> Result<(), NodeError> {
        // A guarded Applied result already means independently verified dual
        // durability. Local dissemination failure cannot revoke that history.
        // Keep the original continuation and qualified execution for retry.
        if execution_hook
            .as_ref()
            .is_some_and(|hook| hook.guarded_local_execution())
        {
            return Ok(());
        }
        if decision_height <= previous_execution_height {
            return Ok(());
        }
        self.last_execution_height = previous_execution_height;
        self.last_execution_block_hash = previous_execution_block_hash.map(str::to_string);
        self.last_execution_state_root = previous_execution_state_root.map(str::to_string);
        self.execution_bindings.remove(&decision_height);
        if let Some(hook) = execution_hook {
            let restored = hook.restore_to_height(world_id, previous_execution_height).map_err(
                |restore_err| NodeError::Execution {
                    reason: format!(
                        "local committed height {} failed after execution: {}; rollback to height {} failed: {}",
                        decision_height, err, previous_execution_height, restore_err
                    ),
                },
            )?;
            if !restored {
                return Err(NodeError::Execution {
                    reason: format!(
                        "local committed height {} failed after execution: {}; rollback record for height {} is unavailable",
                        decision_height, err, previous_execution_height
                    ),
                });
            }
        }
        Ok(())
    }
}
