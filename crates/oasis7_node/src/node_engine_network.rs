use crate::network_bridge_gap_sync_observability::GapSyncFetchCommitResponse;
use crate::node_engine_gap_sync_outcome::GapSyncHeightOutcome;
use crate::replication_state_reconcile::ReplicationCommitPayload;

use super::node_engine_core::InboundSlotWindow;
use super::node_engine_network_hash::{
    execution_error_is_peer_mismatch, peer_commit_heads_conflict,
    validated_commits_share_identity_block_action,
};
use super::*;

impl PosNodeEngine {
    pub(super) fn observe_network_replication_commit(
        &mut self,
        peer_node_id: &str,
        payload: &ReplicationCommitPayloadView,
    ) {
        if payload.height == 0 {
            return;
        }
        if self
            .validator_id_for_peer_head(peer_node_id)
            .map(|validator_id| self.quarantined_validators.contains(&validator_id))
            .unwrap_or(false)
        {
            return;
        }
        self.network_committed_height = self.network_committed_height.max(payload.height);
        if self
            .peer_heads
            .get(peer_node_id)
            .map(|head| payload.height >= head.height)
            .unwrap_or(true)
        {
            self.peer_heads.insert(
                peer_node_id.to_string(),
                PeerCommittedHead {
                    height: payload.height,
                    block_hash: payload.block_hash.clone(),
                    committed_at_ms: payload.committed_at_ms,
                    observed_at_ms: crate::runtime_util::now_unix_ms(),
                    execution_block_hash: payload.execution_block_hash.clone(),
                    execution_state_root: payload.execution_state_root.clone(),
                    action_root: String::new(),
                    public_key_hex: None,
                    signature_hex: None,
                },
            );
        }
    }

    pub(super) fn observe_peer_commit_message(&mut self, commit: &GossipCommitMessage) {
        if commit.height == 0 {
            return;
        }
        let next_head = PeerCommittedHead {
            height: commit.height,
            block_hash: commit.block_hash.clone(),
            committed_at_ms: commit.committed_at_ms,
            observed_at_ms: crate::runtime_util::now_unix_ms(),
            execution_block_hash: commit.execution_block_hash.clone(),
            execution_state_root: commit.execution_state_root.clone(),
            action_root: commit.action_root.clone(),
            public_key_hex: commit.public_key_hex.clone(),
            signature_hex: commit.signature_hex.clone(),
        };
        self.observe_peer_committed_head(commit.node_id.as_str(), next_head);
        if self
            .peer_heads
            .get(commit.node_id.as_str())
            .is_some_and(|head| {
                head.height == commit.height && head.block_hash == commit.block_hash
            })
        {
            self.remember_latest_validated_peer_commit(commit);
        }
    }

    fn remember_latest_validated_peer_commit(&mut self, commit: &GossipCommitMessage) {
        let replace = match self.latest_validated_peer_commit.as_ref() {
            None => true,
            Some(previous) if commit.height > previous.height => true,
            Some(previous) if commit.height < previous.height => false,
            Some(previous) => {
                validated_commits_share_identity_block_action(previous, commit)
                    && commit.execution_block_hash.is_some()
                    && commit.execution_state_root.is_some()
                    && (previous.execution_block_hash.is_none()
                        || previous.execution_state_root.is_none())
            }
        };
        if replace {
            self.latest_validated_peer_commit = Some(commit.clone());
        }
    }

    pub(super) fn observe_peer_committed_head(
        &mut self,
        peer_node_id: &str,
        mut next_head: PeerCommittedHead,
    ) {
        if next_head.height == 0 {
            return;
        }
        let validator_id = self.validator_id_for_peer_head(peer_node_id);
        if validator_id
            .as_deref()
            .map(|id| self.quarantined_validators.contains(id))
            .unwrap_or(false)
        {
            return;
        }
        let now_ms = crate::runtime_util::now_unix_ms();
        if let Some(previous) = self.peer_heads.get(peer_node_id) {
            if next_head.height < previous.height {
                return;
            }
            if next_head.height == previous.height
                && peer_commit_heads_conflict(previous, &next_head)
            {
                if let Some(validator_id) = validator_id {
                    self.record_commit_equivocation_evidence(
                        validator_id,
                        peer_node_id.to_string(),
                        previous.clone(),
                        next_head,
                        now_ms,
                    );
                    self.peer_heads.remove(peer_node_id);
                }
                return;
            }
            if next_head.height == previous.height && next_head.block_hash == previous.block_hash {
                if next_head.execution_block_hash.is_none() {
                    next_head.execution_block_hash = previous.execution_block_hash.clone();
                }
                if next_head.execution_state_root.is_none() {
                    next_head.execution_state_root = previous.execution_state_root.clone();
                }
                if next_head.action_root.is_empty() {
                    next_head.action_root = previous.action_root.clone();
                }
            }
        }
        self.network_committed_height = self.network_committed_height.max(next_head.height);
        self.peer_heads.insert(peer_node_id.to_string(), next_head);
    }

    fn record_commit_equivocation_evidence(
        &mut self,
        validator_id: String,
        node_id: String,
        first: PeerCommittedHead,
        second: PeerCommittedHead,
        observed_at_ms: i64,
    ) {
        let key = format!("commit_equivocation:{validator_id}:{}", first.height);
        self.quarantined_validators.insert(validator_id.clone());
        if self
            .latest_validated_peer_commit
            .as_ref()
            .is_some_and(|commit| {
                commit.node_id == node_id
                    || self
                        .validator_id_for_peer_head(commit.node_id.as_str())
                        .as_deref()
                        == Some(validator_id.as_str())
            })
        {
            self.latest_validated_peer_commit = None;
        }
        self.misbehavior_evidence
            .entry(key)
            .or_insert_with(|| ConsensusMisbehaviorEvidence {
                kind: "commit_equivocation".to_string(),
                validator_id: validator_id.clone(),
                node_id,
                height: first.height,
                observed_at_ms,
                first_block_hash: first.block_hash,
                second_block_hash: second.block_hash,
                first_execution_block_hash: first.execution_block_hash,
                second_execution_block_hash: second.execution_block_hash,
                first_execution_state_root: first.execution_state_root,
                second_execution_state_root: second.execution_state_root,
                first_action_root: first.action_root,
                second_action_root: second.action_root,
                first_public_key_hex: first.public_key_hex,
                second_public_key_hex: second.public_key_hex,
                first_signature_hex: first.signature_hex,
                second_signature_hex: second.signature_hex,
                slashable_stake: self
                    .validators
                    .get(validator_id.as_str())
                    .copied()
                    .unwrap_or(0),
                total_stake: self.total_stake,
                validator_stake_root: self.validator_stake_root.clone(),
                quarantined: true,
            });
    }

    pub(super) fn sync_replication_height_once(
        &self,
        endpoint: &ReplicationNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        replication_runtime: &mut ReplicationRuntime,
        height: u64,
    ) -> Result<GapSyncHeightOutcome, NodeError> {
        self.sync_replication_height_once_with_fetch(
            endpoint,
            node_id,
            world_id,
            replication_runtime,
            height,
            |endpoint, request| endpoint.request_fetch_commit_for_gap_sync(request),
        )
    }

    pub(super) fn sync_replication_height_once_for_successor_probe(
        &self,
        endpoint: &ReplicationNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        replication_runtime: &mut ReplicationRuntime,
        height: u64,
    ) -> Result<GapSyncHeightOutcome, NodeError> {
        self.sync_replication_height_once_with_fetch(
            endpoint,
            node_id,
            world_id,
            replication_runtime,
            height,
            |endpoint, request| endpoint.request_fetch_commit_for_gap_sync_single_probe(request),
        )
    }

    pub(super) fn sync_replication_height_once_for_high_checkpoint_probe(
        &self,
        endpoint: &ReplicationNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        replication_runtime: &mut ReplicationRuntime,
        height: u64,
    ) -> Result<GapSyncHeightOutcome, NodeError> {
        self.sync_replication_height_once_with_fetch(
            endpoint,
            node_id,
            world_id,
            replication_runtime,
            height,
            |endpoint, request| endpoint.request_fetch_commit_for_high_checkpoint_probe(request),
        )
    }

    fn sync_replication_height_once_with_fetch(
        &self,
        endpoint: &ReplicationNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        replication_runtime: &mut ReplicationRuntime,
        height: u64,
        fetch_commit: impl FnOnce(
            &ReplicationNetworkEndpoint,
            &FetchCommitRequest,
        ) -> Result<GapSyncFetchCommitResponse, NodeError>,
    ) -> Result<GapSyncHeightOutcome, NodeError> {
        let request = replication_runtime.build_fetch_commit_request(world_id, height)?;
        let fetch_commit = fetch_commit(endpoint, &request)
            .map_err(|err| Self::annotate_fetch_commit_gap_sync_error(height, err))?;
        if !fetch_commit.response.found {
            return Ok(GapSyncHeightOutcome::NotFound {
                repair_summary: fetch_commit.repair_summary,
                route_snapshot: fetch_commit.route_snapshot,
            });
        }
        let repair_summary = fetch_commit.repair_summary;
        let route_snapshot = fetch_commit.route_snapshot;
        let mut message = fetch_commit
            .response
            .message
            .ok_or_else(|| NodeError::Replication {
                reason: format!(
                    "gap sync height {} commit response missing payload (found=true)",
                    height
                ),
            })?;
        if message.world_id != world_id || message.record.world_id != world_id {
            return Err(NodeError::Replication {
                reason: format!(
                    "gap sync height {} world mismatch: expected={} actual_message={} actual_record={}",
                    height, world_id, message.world_id, message.record.world_id
                ),
            });
        }

        let blob_request =
            replication_runtime.build_fetch_blob_request(message.record.content_hash.as_str())?;
        let provider_lookup = endpoint
            .lookup_provider_ids_for_content_hash(world_id, message.record.content_hash.as_str())?;
        let provider_lookup = provider_lookup.as_deref().filter(|ids| !ids.is_empty());
        let blob_response = request_fetch_blob_with_route_fallback(
            endpoint,
            world_id,
            message.record.content_hash.as_str(),
            &blob_request,
            provider_lookup,
        )
        .map_err(|err| Self::annotate_fetch_blob_gap_sync_error(height, err))?;
        if !blob_response.found {
            return Err(NodeError::Replication {
                reason: format!(
                    "gap sync height {} blob not found for hash {}",
                    height, message.record.content_hash
                ),
            });
        }
        let blob = blob_response.blob.ok_or_else(|| NodeError::Replication {
            reason: format!(
                "gap sync height {} blob payload missing for hash {}",
                height, message.record.content_hash
            ),
        })?;
        message.payload = blob;
        let payload =
            parse_replication_commit_payload(message.payload.as_slice()).ok_or_else(|| {
                NodeError::Replication {
                    reason: format!("gap sync height {} payload decode failed", height),
                }
            })?;
        // The provider response sidecar is only a transport copy of the
        // source-authored value embedded in the signed payload.  Never allow
        // a route/provider to add, remove, or substitute lineage authority.
        // This remains a consistency gate until the runtime validator has a
        // trusted consensus-produced envelope to verify.
        if fetch_commit.response.lineage_envelope.as_ref() != payload.lineage_envelope.as_ref() {
            return Err(NodeError::Replication {
                reason: format!(
                    "gap sync height {} checkpoint lineage sidecar mismatch between response and signed payload",
                    height
                ),
            });
        }
        if payload.world_id != world_id {
            return Err(NodeError::Replication {
                reason: format!(
                    "gap sync height {} payload world mismatch expected={} actual={}",
                    height, world_id, payload.world_id
                ),
            });
        }
        if payload.node_id != message.node_id {
            return Err(NodeError::Replication {
                reason: format!(
                    "gap sync height {} payload node mismatch expected={} actual={}",
                    height, message.node_id, payload.node_id
                ),
            });
        }
        if payload.height != height {
            return Err(NodeError::Replication {
                reason: format!(
                    "gap sync height {} payload mismatch actual={}",
                    height, payload.height
                ),
            });
        }
        if payload.block_hash.trim().is_empty() {
            return Err(NodeError::Replication {
                reason: format!("gap sync height {} payload block_hash is empty", height),
            });
        }
        match replication_runtime.validate_remote_message_for_apply(node_id, world_id, &message) {
            Ok(true) => {}
            Ok(false) => {
                return Ok(GapSyncHeightOutcome::NotFound {
                    repair_summary: "commit validation rejected apply".to_string(),
                    route_snapshot,
                });
            }
            Err(err) => return Err(err),
        }
        validate_consensus_action_root(payload.action_root.as_str(), payload.actions.as_slice())
            .map_err(|err| NodeError::Replication {
                reason: format!(
                    "gap sync height {} action_root validation failed: {:?}",
                    height, err
                ),
            })?;
        self.validate_peer_commit_execution_binding(
            payload.height,
            payload.execution_block_hash.as_deref(),
            payload.execution_state_root.as_deref(),
        )
        .map_err(|err| NodeError::Replication {
            reason: format!(
                "gap sync height {} execution hash validation failed: {}",
                height, err
            ),
        })?;
        endpoint.remember_validated_fetch_commit_success(
            &request,
            &FetchCommitResponse {
                found: true,
                message: Some(message.clone()),
                lineage_envelope: fetch_commit.response.lineage_envelope.clone(),
            },
        );
        Ok(GapSyncHeightOutcome::Synced {
            message,
            payload,
            repair_summary,
            route_snapshot,
        })
    }

    fn annotate_fetch_commit_gap_sync_error(height: u64, err: NodeError) -> NodeError {
        let NodeError::Replication { reason } = err else {
            return err;
        };
        if reason.contains(REPLICATION_FETCH_COMMIT_PROTOCOL) {
            return NodeError::Replication { reason };
        }
        if reason.contains("replication network availability gap")
            && (reason.contains("NetworkProtocolUnavailable")
                || reason.contains("request failed: Timeout"))
        {
            return NodeError::Replication {
                reason: format!(
                    "{reason}; gap sync height {height} fetch-commit {REPLICATION_FETCH_COMMIT_PROTOCOL}"
                ),
            };
        }
        NodeError::Replication { reason }
    }

    fn annotate_fetch_blob_gap_sync_error(height: u64, err: NodeError) -> NodeError {
        let NodeError::Replication { reason } = err else {
            return err;
        };
        NodeError::Replication {
            reason: format!("{reason}; gap sync height {height} fetch-blob"),
        }
    }

    pub(super) fn persist_synced_replication_message(
        &self,
        endpoint: &ReplicationNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        replication_runtime: &mut ReplicationRuntime,
        message: &replication::GossipReplicationMessage,
        height: u64,
    ) -> Result<(), NodeError> {
        replication_runtime.apply_remote_message(node_id, world_id, message)?;
        let persisted = replication_runtime.load_commit_message_by_height(world_id, height)?;
        if persisted
            .as_ref()
            .map(|entry| entry.record.content_hash.as_str())
            != Some(message.record.content_hash.as_str())
        {
            return Err(NodeError::Replication {
                reason: format!(
                    "synced replication height {} persisted commit hash mismatch expected={}",
                    height, message.record.content_hash
                ),
            });
        }
        endpoint.publish_local_content_provider_best_effort(
            world_id,
            message.record.content_hash.as_str(),
        );
        Ok(())
    }

    pub(super) fn execute_synced_replication_commit(
        &mut self,
        world_id: &str,
        payload: &ReplicationCommitPayload,
        mut execution_hook: Option<&mut dyn NodeExecutionHook>,
    ) -> Result<(String, i64), NodeError> {
        super::node_engine_guarded_local::ensure_ordinary_network_recovery(
            execution_hook.as_deref(),
        )?;
        if payload.execution_block_hash.is_some() != payload.execution_state_root.is_some() {
            return Err(NodeError::Replication {
                reason: format!(
                    "synced replication height {} execution binding malformed",
                    payload.height
                ),
            });
        }
        self.validate_peer_commit_execution_binding(
            payload.height,
            payload.execution_block_hash.as_deref(),
            payload.execution_state_root.as_deref(),
        )
        .map_err(|err| NodeError::Replication {
            reason: format!(
                "synced replication height {} execution hash validation failed before replay: {}",
                payload.height, err
            ),
        })?;
        validate_replicated_execution_input_actions(payload.actions.as_slice(), payload.height)
            .map_err(|err| NodeError::Replication {
                reason: format!(
                    "synced replication height {} replicated execution input validation failed: {}",
                    payload.height, err
                ),
            })?;
        let previous_execution_height = self.last_execution_height;
        let previous_execution_block_hash = self.last_execution_block_hash.clone();
        let previous_execution_state_root = self.last_execution_state_root.clone();
        let decision = PosDecision {
            height: payload.height,
            slot: payload.slot,
            epoch: payload.epoch,
            proposer_id: payload
                .proposer_id
                .clone()
                .unwrap_or_else(|| payload.node_id.clone()),
            status: PosConsensusStatus::Committed,
            block_hash: payload.block_hash.clone(),
            action_root: payload.action_root.clone(),
            committed_actions: payload.actions.clone(),
            approved_stake: self.required_stake,
            rejected_stake: 0,
            required_stake: self.required_stake,
            total_stake: self.total_stake,
        };
        let apply_result = match execution_hook {
            Some(ref mut hook) => self.apply_committed_execution_with_expected(
                payload.node_id.as_str(),
                world_id,
                payload.committed_at_ms,
                &decision,
                Some(&mut **hook),
                payload.execution_block_hash.as_deref(),
                payload.execution_state_root.as_deref(),
            ),
            None => self.apply_committed_execution_with_expected(
                payload.node_id.as_str(),
                world_id,
                payload.committed_at_ms,
                &decision,
                None,
                payload.execution_block_hash.as_deref(),
                payload.execution_state_root.as_deref(),
            ),
        };
        if let Err(err) = apply_result {
            let peer_mismatch = execution_error_is_peer_mismatch(&err);
            self.rollback_synced_execution_after_failure(
                world_id,
                payload.height,
                previous_execution_height,
                previous_execution_block_hash.as_deref(),
                previous_execution_state_root.as_deref(),
                execution_hook,
                peer_mismatch,
                &err,
            )?;
            if !peer_mismatch {
                return Err(err);
            }
            return Err(NodeError::Replication {
                reason: format!(
                    "synced replication height {} execution hash validation failed: {}",
                    payload.height, err
                ),
            });
        }
        if let Err(err) = self.validate_peer_commit_execution_binding(
            payload.height,
            payload.execution_block_hash.as_deref(),
            payload.execution_state_root.as_deref(),
        ) {
            let err = NodeError::Replication {
                reason: format!(
                    "synced replication height {} execution hash validation failed after replay: {}",
                    payload.height, err
                ),
            };
            self.rollback_synced_execution_after_failure(
                world_id,
                payload.height,
                previous_execution_height,
                previous_execution_block_hash.as_deref(),
                previous_execution_state_root.as_deref(),
                execution_hook,
                true,
                &err,
            )?;
            return Err(err);
        }
        if self.last_execution_height == payload.height
            && self.execution_binding_for_height(payload.height).is_none()
            && let (Some(execution_block_hash), Some(execution_state_root)) = (
                payload.execution_block_hash.clone(),
                payload.execution_state_root.clone(),
            )
        {
            self.remember_execution_binding(
                payload.height,
                execution_block_hash,
                execution_state_root,
            );
        }
        Ok((payload.block_hash.clone(), payload.committed_at_ms))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable replication recovery seam keeps prior execution bindings and mismatch context explicit"
    )]
    fn rollback_synced_execution_after_failure(
        &mut self,
        world_id: &str,
        payload_height: u64,
        previous_execution_height: u64,
        previous_execution_block_hash: Option<&str>,
        previous_execution_state_root: Option<&str>,
        execution_hook: Option<&mut (dyn NodeExecutionHook + '_)>,
        execution_mismatch: bool,
        err: &NodeError,
    ) -> Result<(), NodeError> {
        self.last_execution_height = previous_execution_height;
        self.last_execution_block_hash = previous_execution_block_hash.map(str::to_string);
        self.last_execution_state_root = previous_execution_state_root.map(str::to_string);
        if payload_height > 0 {
            self.execution_bindings.remove(&payload_height);
        }
        if let Some(hook) = execution_hook {
            let restored = hook.restore_to_height(world_id, previous_execution_height).map_err(
                |restore_err| NodeError::Replication {
                    reason: format!(
                        "synced replication height {} execution hash validation failed: {}; rollback to height {} failed: {}",
                        payload_height, err, previous_execution_height, restore_err
                    ),
                },
            )?;
            if !restored {
                if execution_mismatch {
                    return Err(NodeError::ExecutionMismatchRollbackUnavailable {
                        payload_height,
                        rollback_height: previous_execution_height,
                        reason: err.to_string(),
                    });
                }
                return Err(NodeError::Replication {
                    reason: format!(
                        "synced replication height {} execution hash validation failed: {}; rollback record for height {} is unavailable",
                        payload_height, err, previous_execution_height
                    ),
                });
            }
        }
        Ok(())
    }

    pub(super) fn apply_synced_replication_commit(
        &mut self,
        world_id: &str,
        payload: &ReplicationCommitPayload,
        execution_hook: Option<&mut dyn NodeExecutionHook>,
    ) -> Result<(), NodeError> {
        let (block_hash, committed_at_ms) =
            self.execute_synced_replication_commit(world_id, payload, execution_hook)?;
        self.record_synced_replication_height(payload.height, block_hash, committed_at_ms)
    }

    pub(super) fn rollback_to_replicated_commit_boundary(
        &mut self,
        height: u64,
        block_hash: String,
        committed_at_ms: i64,
        execution_block_hash: Option<String>,
        execution_state_root: Option<String>,
    ) -> Result<(), NodeError> {
        if execution_block_hash.is_some() != execution_state_root.is_some() {
            return Err(NodeError::Replication {
                reason: format!(
                    "replication boundary rollback height {} execution binding malformed",
                    height
                ),
            });
        }
        let next_height = checked_replication_successor(
            height,
            "height",
            "rolling back to replicated commit boundary",
        )?;
        self.clear_pending_action_reservation()?;
        self.committed_height = height;
        self.network_committed_height = self.network_committed_height.min(height);
        self.replication_persisted_height = self.replication_persisted_height.min(height);
        self.last_replication_gap_sync_blocked_height = None;
        self.last_replication_gap_sync_blocked_reason = None;
        self.last_replication_gap_sync_blocked_at_ms = None;
        self.last_replication_gap_sync_repair_attempt_height = None;
        self.last_replication_gap_sync_repair_attempt_summary = None;
        self.last_replication_gap_sync_repair_attempt_route_snapshot = None;
        self.last_committed_at_ms = Some(committed_at_ms);
        self.next_height = next_height;
        self.last_committed_block_hash = Some(block_hash);
        self.last_execution_height = height;
        self.last_execution_block_hash = execution_block_hash;
        self.last_execution_state_root = execution_state_root;
        self.execution_bindings.clear();
        self.remember_execution_binding_for_height(height);
        self.pending = None;
        Ok(())
    }

    pub(super) fn ingest_proposal_message(
        &mut self,
        world_id: &str,
        message: &GossipProposalMessage,
        current_slot: u64,
    ) -> Result<(), NodeError> {
        if message.version != 1 || message.world_id != world_id {
            return Ok(());
        }
        if message.node_id != message.proposer_id {
            return Ok(());
        }
        if self
            .validate_message_player_binding(
                message.proposer_id.as_str(),
                message.player_id.as_str(),
                "proposal",
            )
            .is_err()
        {
            return Ok(());
        }
        if self
            .validate_message_signer_binding(
                message.proposer_id.as_str(),
                message.public_key_hex.as_deref(),
                "proposal",
            )
            .is_err()
        {
            return Ok(());
        }
        match self.classify_inbound_slot_window(message.slot, current_slot) {
            InboundSlotWindow::Accept => {}
            InboundSlotWindow::Future => {
                self.inbound_rejected_proposal_future_slot =
                    self.inbound_rejected_proposal_future_slot.saturating_add(1);
                self.note_inbound_timing_reject(format!(
                    "reject proposal timing: future slot={} current_slot={} height={} proposer_id={}",
                    message.slot, current_slot, message.height, message.proposer_id
                ));
                return Ok(());
            }
            InboundSlotWindow::Stale => {
                self.inbound_rejected_proposal_stale_slot =
                    self.inbound_rejected_proposal_stale_slot.saturating_add(1);
                self.note_inbound_timing_reject(format!(
                    "reject proposal timing: stale slot={} current_slot={} lag={} height={} proposer_id={}",
                    message.slot,
                    current_slot,
                    self.max_past_slot_lag,
                    message.height,
                    message.proposer_id
                ));
                return Ok(());
            }
        }
        if message.height < self.next_height {
            return Ok(());
        }
        if let Some(current) = self.pending.as_ref() {
            if current.height > message.height {
                return Ok(());
            }
            if current.height == message.height && current.block_hash == message.block_hash {
                return Ok(());
            }
        }
        let proposal_action_bytes =
            match self.validate_inbound_proposal_actions(message.actions.as_slice()) {
                Ok(bytes) => bytes,
                Err(_) => return Ok(()),
            };
        if validate_replicated_execution_input_actions(message.actions.as_slice(), message.height)
            .is_err()
        {
            return Ok(());
        }
        if validate_consensus_action_root(message.action_root.as_str(), message.actions.as_slice())
            .is_err()
        {
            return Ok(());
        }

        let mut proposal = PendingProposal {
            height: message.height,
            slot: message.slot,
            epoch: message.epoch,
            opened_at_ms: message.proposed_at_ms,
            proposer_id: message.proposer_id.clone(),
            block_hash: message.block_hash.clone(),
            action_root: message.action_root.clone(),
            committed_actions: message.actions.clone(),
            attestations: BTreeMap::new(),
            approved_stake: 0,
            rejected_stake: 0,
            status: PosConsensusStatus::Pending,
        };
        self.insert_attestation(
            &mut proposal,
            &message.proposer_id,
            true,
            message.proposed_at_ms,
            message.epoch.saturating_sub(1),
            message.epoch,
            Some(format!("proposal gossiped from {}", message.node_id)),
        )?;
        let next_height = self.next_height.max(proposal.height);
        let mut next_slot = self.next_slot;
        if proposal.slot >= self.next_slot {
            next_slot = checked_consensus_successor(
                proposal.slot,
                "proposal.slot",
                "ingesting proposal message",
            )?;
        }
        if self
            .adjust_pending_proposal_reservation(proposal_action_bytes)
            .is_err()
        {
            return Ok(());
        }
        // A higher proposal implies its predecessor chain must already exist remotely,
        // so replication gap sync should backfill up to height-1 before this proposal commits.
        self.network_committed_height = self
            .network_committed_height
            .max(proposal.height.saturating_sub(1));
        self.next_height = next_height;
        self.next_slot = next_slot;
        self.set_pending_action_reservation(proposal_action_bytes);
        self.pending = Some(proposal);
        Ok(())
    }

    pub(super) fn ingest_attestation_message(
        &mut self,
        world_id: &str,
        message: &GossipAttestationMessage,
        current_slot: u64,
    ) -> Result<(), NodeError> {
        if message.version != 1 || message.world_id != world_id {
            return Ok(());
        }
        if message.node_id != message.validator_id {
            return Ok(());
        }
        if self
            .validate_message_player_binding(
                message.validator_id.as_str(),
                message.player_id.as_str(),
                "attestation",
            )
            .is_err()
        {
            return Ok(());
        }
        if self
            .validate_message_signer_binding(
                message.validator_id.as_str(),
                message.public_key_hex.as_deref(),
                "attestation",
            )
            .is_err()
        {
            return Ok(());
        }
        match self.classify_inbound_slot_window(message.slot, current_slot) {
            InboundSlotWindow::Accept => {}
            InboundSlotWindow::Future => {
                self.inbound_rejected_attestation_future_slot = self
                    .inbound_rejected_attestation_future_slot
                    .saturating_add(1);
                self.note_inbound_timing_reject(format!(
                    "reject attestation timing: future slot={} current_slot={} height={} validator_id={}",
                    message.slot, current_slot, message.height, message.validator_id
                ));
                return Ok(());
            }
            InboundSlotWindow::Stale => {
                self.inbound_rejected_attestation_stale_slot = self
                    .inbound_rejected_attestation_stale_slot
                    .saturating_add(1);
                self.note_inbound_timing_reject(format!(
                    "reject attestation timing: stale slot={} current_slot={} lag={} height={} validator_id={}",
                    message.slot,
                    current_slot,
                    self.max_past_slot_lag,
                    message.height,
                    message.validator_id
                ));
                return Ok(());
            }
        }
        let Some(mut proposal) = self.pending.clone() else {
            return Ok(());
        };
        if proposal.height != message.height || proposal.block_hash != message.block_hash {
            return Ok(());
        }
        if message.slot != proposal.slot || message.epoch != proposal.epoch {
            self.inbound_rejected_attestation_epoch_mismatch = self
                .inbound_rejected_attestation_epoch_mismatch
                .saturating_add(1);
            self.note_inbound_timing_reject(format!(
                "reject attestation timing: slot/epoch mismatch height={} validator_id={} message_slot={} message_epoch={} proposal_slot={} proposal_epoch={}",
                message.height,
                message.validator_id,
                message.slot,
                message.epoch,
                proposal.slot,
                proposal.epoch
            ));
            return Ok(());
        }
        let expected_target_epoch = self.slot_epoch(proposal.slot);
        if message.target_epoch != expected_target_epoch {
            self.inbound_rejected_attestation_epoch_mismatch = self
                .inbound_rejected_attestation_epoch_mismatch
                .saturating_add(1);
            self.note_inbound_timing_reject(format!(
                "reject attestation timing: target_epoch mismatch height={} validator_id={} expected_target_epoch={} actual_target_epoch={} slot={}",
                message.height,
                message.validator_id,
                expected_target_epoch,
                message.target_epoch,
                proposal.slot
            ));
            return Ok(());
        }

        self.insert_attestation(
            &mut proposal,
            &message.validator_id,
            message.approve,
            message.voted_at_ms,
            message.source_epoch,
            message.target_epoch,
            message.reason.clone(),
        )?;
        self.pending = Some(proposal);
        Ok(())
    }

    pub(super) fn ingest_consensus_network_messages(
        &mut self,
        endpoint: &ConsensusNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        current_slot: u64,
        replication: Option<&mut ReplicationRuntime>,
    ) -> Result<(), NodeError> {
        let mut replication = replication;
        let messages = endpoint.drain_messages()?;
        for message in messages {
            match message {
                GossipMessage::Hello(_) => {}
                GossipMessage::Commit(commit) => {
                    if commit.version != 1 || commit.world_id != world_id {
                        continue;
                    }
                    if verify_commit_message_signature(&commit, self.enforce_consensus_signature)
                        .is_err()
                    {
                        continue;
                    }
                    if self
                        .validate_peer_commit_execution_binding(
                            commit.height,
                            commit.execution_block_hash.as_deref(),
                            commit.execution_state_root.as_deref(),
                        )
                        .is_err()
                    {
                        continue;
                    }
                    if validate_consensus_action_root(
                        commit.action_root.as_str(),
                        commit.actions.as_slice(),
                    )
                    .is_err()
                    {
                        continue;
                    }
                    if validate_replicated_execution_input_actions(
                        commit.actions.as_slice(),
                        commit.height,
                    )
                    .is_err()
                    {
                        continue;
                    }
                    if self
                        .validate_message_player_binding(
                            commit.node_id.as_str(),
                            commit.player_id.as_str(),
                            "commit",
                        )
                        .is_err()
                    {
                        continue;
                    }
                    if self
                        .validate_message_signer_binding(
                            commit.node_id.as_str(),
                            commit.public_key_hex.as_deref(),
                            "commit",
                        )
                        .is_err()
                    {
                        continue;
                    }
                    self.observe_peer_commit_message(&commit);
                }
                GossipMessage::Proposal(proposal) => {
                    if proposal.version != 1 || proposal.world_id != world_id {
                        continue;
                    }
                    if verify_proposal_message_signature(
                        &proposal,
                        self.enforce_consensus_signature,
                    )
                    .is_err()
                    {
                        continue;
                    }
                    self.ingest_proposal_message(world_id, &proposal, current_slot)?;
                }
                GossipMessage::Attestation(attestation) => {
                    if attestation.version != 1 || attestation.world_id != world_id {
                        continue;
                    }
                    if verify_attestation_message_signature(
                        &attestation,
                        self.enforce_consensus_signature,
                    )
                    .is_err()
                    {
                        continue;
                    }
                    self.ingest_attestation_message(world_id, &attestation, current_slot)?;
                }
                GossipMessage::CheckpointLineageVote(vote) => {
                    self.ingest_checkpoint_lineage_vote_message(
                        node_id,
                        world_id,
                        &vote,
                        replication.as_deref_mut(),
                    )?;
                }
                GossipMessage::Replication(_) | GossipMessage::Bft { .. } => {}
            }
        }
        Ok(())
    }

    pub(super) fn ingest_peer_messages(
        &mut self,
        endpoint: &GossipEndpoint,
        node_id: &str,
        world_id: &str,
        mut replication: Option<&mut ReplicationRuntime>,
        current_slot: u64,
    ) -> Result<(), NodeError> {
        let messages = endpoint.drain_messages()?;
        for received in messages {
            let from = received.from;
            match received.message {
                GossipMessage::Hello(hello) => {
                    if hello.version != 1 || hello.world_id != world_id || hello.node_id == node_id
                    {
                        continue;
                    }
                    endpoint.remember_peer(from)?;
                }
                GossipMessage::Commit(commit) => {
                    if commit.version != 1 || commit.world_id != world_id {
                        continue;
                    }
                    if verify_commit_message_signature(&commit, self.enforce_consensus_signature)
                        .is_err()
                    {
                        continue;
                    }
                    if self
                        .validate_peer_commit_execution_binding(
                            commit.height,
                            commit.execution_block_hash.as_deref(),
                            commit.execution_state_root.as_deref(),
                        )
                        .is_err()
                    {
                        continue;
                    }
                    if validate_consensus_action_root(
                        commit.action_root.as_str(),
                        commit.actions.as_slice(),
                    )
                    .is_err()
                    {
                        continue;
                    }
                    if self
                        .validate_message_player_binding(
                            commit.node_id.as_str(),
                            commit.player_id.as_str(),
                            "commit",
                        )
                        .is_err()
                    {
                        continue;
                    }
                    if self
                        .validate_message_signer_binding(
                            commit.node_id.as_str(),
                            commit.public_key_hex.as_deref(),
                            "commit",
                        )
                        .is_err()
                    {
                        continue;
                    }
                    endpoint.remember_peer(from)?;
                    self.observe_peer_commit_message(&commit);
                }
                GossipMessage::Proposal(proposal) => {
                    if proposal.version != 1 || proposal.world_id != world_id {
                        continue;
                    }
                    if verify_proposal_message_signature(
                        &proposal,
                        self.enforce_consensus_signature,
                    )
                    .is_err()
                    {
                        continue;
                    }
                    self.ingest_proposal_message(world_id, &proposal, current_slot)?;
                    endpoint.remember_peer(from)?;
                }
                GossipMessage::Attestation(attestation) => {
                    if attestation.version != 1 || attestation.world_id != world_id {
                        continue;
                    }
                    if verify_attestation_message_signature(
                        &attestation,
                        self.enforce_consensus_signature,
                    )
                    .is_err()
                    {
                        continue;
                    }
                    self.ingest_attestation_message(world_id, &attestation, current_slot)?;
                    endpoint.remember_peer(from)?;
                }
                GossipMessage::CheckpointLineageVote(vote) => {
                    self.ingest_checkpoint_lineage_vote_message(
                        node_id,
                        world_id,
                        &vote,
                        replication.as_deref_mut(),
                    )?;
                    endpoint.remember_peer(from)?;
                }
                GossipMessage::Replication(replication_msg) => {
                    if replication_msg.version != 1
                        || replication_msg.world_id != world_id
                        || replication_msg.record.world_id != world_id
                    {
                        continue;
                    }
                    if let Some(replication_runtime) = replication.as_deref_mut()
                        && replication_runtime
                            .apply_remote_message(node_id, world_id, &replication_msg)
                            .is_ok()
                    {
                        endpoint.remember_peer(from)?;
                    }
                }
                GossipMessage::Bft { .. } => {}
            }
        }
        Ok(())
    }
}
