use super::node_engine_replication_provider_route::{
    record_replication_gap_sync_fetch_blob_rate_limit,
    replication_gap_sync_provider_blob_route_blocked,
    replication_gap_sync_provider_blob_route_blocked_in_cooldown,
};
use super::node_engine_storage_challenge::{
    StorageChallengeSampleOutcome, evaluate_storage_challenge_sample,
};
use super::*;
use crate::node_engine_gap_sync_outcome::GapSyncHeightOutcome;
use crate::node_engine_replication_checkpoint::FreshObserverCheckpointBootstrap;
use crate::replication_state_reconcile::ReplicationCommitPayload;
use oasis7_proto::distributed::WorldHeadAnnounce;

impl PosNodeEngine {
    #[expect(
        clippy::too_many_arguments,
        reason = "Stable replication publication seam keeps transport, commit, and execution channels explicit"
    )]
    pub(super) fn broadcast_local_replication(
        &mut self,
        gossip_endpoint: Option<&GossipEndpoint>,
        network_endpoint: Option<&ReplicationNetworkEndpoint>,
        node_id: &str,
        world_id: &str,
        now_ms: i64,
        decision: &PosDecision,
        replication: Option<&mut ReplicationRuntime>,
        execution_hook: Option<&mut dyn NodeExecutionHook>,
    ) -> Result<(), NodeError> {
        if !self.replicate_local_commits {
            return Ok(());
        }
        if !matches!(decision.status, PosConsensusStatus::Committed) {
            return Ok(());
        }
        if self
            .expected_proposer(decision.slot)
            .as_deref()
            .map(|proposer_id| proposer_id != node_id)
            .unwrap_or(false)
        {
            return Ok(());
        }
        let Some(replication) = replication else {
            return Ok(());
        };
        self.enforce_storage_challenge_gate(
            replication,
            network_endpoint,
            node_id,
            world_id,
            now_ms,
        )?;
        let (execution_block_hash, execution_state_root) =
            self.commit_execution_binding_for_height(decision.height)?;
        let execution_checkpoint = match (execution_hook, execution_block_hash) {
            (Some(hook), Some(_)) => hook
                .export_checkpoint_bundle(decision.height)
                .map_err(|reason| NodeError::Execution { reason })?,
            _ => None,
        };
        if let Some(message) = replication.build_local_commit_message_with_checkpoint(
            node_id,
            world_id,
            now_ms,
            decision,
            execution_block_hash,
            execution_state_root,
            execution_checkpoint,
        )? {
            replication
                .persist_local_checkpoint_message_for_lineage(node_id, world_id, &message)?;
            if let Some(endpoint) = network_endpoint {
                if let Some(payload) = parse_replication_commit_payload(message.payload.as_slice())
                    && let Some(descriptor) = payload.execution_checkpoint.as_ref()
                {
                    Self::publish_execution_checkpoint_descriptor_providers(
                        endpoint,
                        world_id,
                        replication,
                        descriptor,
                    )?;
                }
                endpoint.publish_local_content_provider_best_effort(
                    world_id,
                    message.record.content_hash.as_str(),
                );
                endpoint.publish_replication(&message)?;
            } else if let Some(endpoint) = gossip_endpoint {
                endpoint.broadcast_replication(&message)?;
            }
            self.advance_contiguous_replication_persisted_height(
                replication,
                world_id,
                decision.height,
            )?;
        }
        Ok(())
    }
    pub(super) fn enforce_storage_challenge_gate(
        &mut self,
        replication: &ReplicationRuntime,
        network_endpoint: Option<&ReplicationNetworkEndpoint>,
        node_id: &str,
        world_id: &str,
        now_ms: i64,
    ) -> Result<(), NodeError> {
        let report = replication.probe_storage_challenges(world_id, node_id, now_ms)?;
        if report.failed_checks > 0 {
            return Err(NodeError::Consensus {
                reason: format!(
                    "storage challenge gate failed: total_checks={} failed_checks={} reasons={:?}",
                    report.total_checks, report.failed_checks, report.failure_reasons
                ),
            });
        }
        let Some(endpoint) = network_endpoint else {
            return Ok(());
        };
        if self.committed_height < STORAGE_GATE_NETWORK_WARMUP_HEIGHT && self.peer_heads.is_empty()
        {
            return Ok(());
        }
        self.prune_storage_challenge_success_cache();
        if self.storage_challenge_network_probe_in_cooldown(now_ms) {
            return Ok(());
        }
        let primary_samples = replication
            .recent_replicated_content_refs(world_id, STORAGE_GATE_NETWORK_SAMPLES_PER_CHECK)?;
        if primary_samples.is_empty() {
            self.clear_storage_challenge_network_degraded();
            return Ok(());
        }
        let mut successful_matches = 0usize;
        let mut attempted_probes = 0usize;
        let mut total_samples = 0usize;
        let mut failure_reasons = Vec::new();
        let mut hard_failure = false;
        for (_, content_hash) in primary_samples.iter() {
            total_samples = total_samples.saturating_add(1);
            if self.storage_challenge_success_cache_hit(replication, content_hash.as_str())? {
                successful_matches = successful_matches.saturating_add(1);
                continue;
            }
            attempted_probes = attempted_probes.saturating_add(1);
            match evaluate_storage_challenge_sample(
                replication,
                endpoint,
                world_id,
                content_hash.as_str(),
            )? {
                StorageChallengeSampleOutcome::Matched => {
                    successful_matches = successful_matches.saturating_add(1);
                    self.mark_storage_challenge_success(content_hash.as_str());
                }
                StorageChallengeSampleOutcome::Unavailable { reason } => {
                    failure_reasons.push(reason);
                    break;
                }
                StorageChallengeSampleOutcome::HardFailure { reason } => {
                    hard_failure = true;
                    failure_reasons.push(reason);
                    break;
                }
            }
        }

        let mut required_matches = required_network_blob_matches(primary_samples.len());
        if self.committed_height < STORAGE_GATE_NETWORK_WARMUP_HEIGHT
            || (self.require_peer_execution_hashes && self.peer_heads.is_empty())
        {
            required_matches = required_matches.min(1);
        }
        if successful_matches >= required_matches {
            self.clear_storage_challenge_network_degraded();
            return Ok(());
        }

        if !hard_failure && successful_matches < required_matches {
            let fallback_samples = replication.replicated_content_refs_from_height(
                world_id,
                self.storage_challenge_fallback_height,
                STORAGE_GATE_FALLBACK_SAMPLES_PER_CHECK,
            )?;
            for (height, content_hash) in fallback_samples {
                total_samples = total_samples.saturating_add(1);
                if self.storage_challenge_success_cache_hit(replication, content_hash.as_str())? {
                    successful_matches = successful_matches.saturating_add(1);
                    if successful_matches >= required_matches {
                        self.storage_challenge_fallback_height = height.saturating_add(1);
                        self.clear_storage_challenge_network_degraded();
                        return Ok(());
                    }
                    continue;
                }
                attempted_probes = attempted_probes.saturating_add(1);
                match evaluate_storage_challenge_sample(
                    replication,
                    endpoint,
                    world_id,
                    content_hash.as_str(),
                )? {
                    StorageChallengeSampleOutcome::Matched => {
                        successful_matches = successful_matches.saturating_add(1);
                        self.mark_storage_challenge_success(content_hash.as_str());
                    }
                    StorageChallengeSampleOutcome::Unavailable { reason } => {
                        failure_reasons.push(reason);
                        break;
                    }
                    StorageChallengeSampleOutcome::HardFailure { reason } => {
                        hard_failure = true;
                        failure_reasons.push(reason);
                        break;
                    }
                }
                if successful_matches >= required_matches {
                    self.storage_challenge_fallback_height = height.saturating_add(1);
                    self.clear_storage_challenge_network_degraded();
                    return Ok(());
                }
            }
        }

        if hard_failure && successful_matches < required_matches {
            return Err(NodeError::Consensus {
                reason: format!(
                    "storage challenge gate network threshold unmet: total_samples={} attempted_probes={} required_matches={} successful_matches={} reasons={:?}",
                    total_samples,
                    attempted_probes,
                    required_matches,
                    successful_matches,
                    failure_reasons
                ),
            });
        }
        if successful_matches < required_matches {
            self.mark_storage_challenge_network_degraded(
                now_ms,
                required_matches,
                successful_matches,
                failure_reasons,
            );
        } else {
            self.clear_storage_challenge_network_degraded();
        }
        Ok(())
    }
    pub(super) fn ingest_network_replications(
        &mut self,
        endpoint: &ReplicationNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        replication: Option<&mut ReplicationRuntime>,
        execution_hook: Option<&mut dyn NodeExecutionHook>,
    ) -> Result<(), NodeError> {
        self.ingest_network_replications_with_progress(
            endpoint,
            node_id,
            world_id,
            replication,
            execution_hook,
            None,
        )
    }
    pub(super) fn ingest_network_replications_with_progress(
        &mut self,
        endpoint: &ReplicationNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        mut replication: Option<&mut ReplicationRuntime>,
        mut execution_hook: Option<&mut dyn NodeExecutionHook>,
        mut progress_callback: Option<
            &mut dyn FnMut(NodeConsensusSnapshot) -> Result<(), NodeError>,
        >,
    ) -> Result<(), NodeError> {
        let Some(replication_runtime) = replication.as_mut() else {
            return Ok(());
        };
        self.refresh_replication_persisted_height(replication_runtime, world_id)?;
        let checkpoint_bootstrap_preflight = self
            .try_bootstrap_fresh_observer_from_advertised_checkpoint(
                endpoint,
                node_id,
                world_id,
                replication_runtime,
                &mut execution_hook,
                &mut progress_callback,
            )?;
        let checkpoint_bootstrapped_before_ingest = matches!(
            checkpoint_bootstrap_preflight,
            FreshObserverCheckpointBootstrap::Installed
        );
        let checkpoint_preflight_unavailable = matches!(
            checkpoint_bootstrap_preflight,
            FreshObserverCheckpointBootstrap::PreflightUnavailable
        );
        let checkpoint_bootstrap_retry_pending = matches!(
            checkpoint_bootstrap_preflight,
            FreshObserverCheckpointBootstrap::RetryPending
        );
        let high_peer_head_retry_pending = self.retain_high_peer_checkpoint_retry_authority();
        let messages = endpoint.drain_replications()?;
        let mut rejected = Vec::new();
        let mut validated_messages = Vec::new();
        for message in messages {
            if message.node_id == node_id {
                if parse_replication_commit_payload(message.payload.as_slice())
                    .is_some_and(|payload| payload.execution_checkpoint.is_some())
                {
                    replication_runtime.persist_local_checkpoint_message_for_lineage(
                        node_id, world_id, &message,
                    )?;
                }
                continue;
            }
            let payload_view = parse_replication_commit_payload_view(message.payload.as_slice());
            match replication_runtime.validate_remote_message_for_apply(node_id, world_id, &message)
            {
                Ok(true) => {}
                Ok(false) => continue,
                Err(err) => {
                    rejected.push(format!(
                        "node_id={} world_id={} err={}",
                        message.node_id, message.world_id, err
                    ));
                    continue;
                }
            }
            if let Some(payload) = payload_view.as_ref() {
                if self
                    .validate_peer_commit_execution_binding(
                        payload.height,
                        payload.execution_block_hash.as_deref(),
                        payload.execution_state_root.as_deref(),
                    )
                    .is_err()
                {
                    rejected.push(format!(
                        "node_id={} world_id={} err=peer execution hash validation failed for height {}",
                        message.node_id, message.world_id, payload.height
                    ));
                    continue;
                }
                self.observe_network_replication_commit(message.node_id.as_str(), payload);
            }
            validated_messages.push((message, payload_view));
        }
        for (message, payload_view) in validated_messages {
            if checkpoint_bootstrapped_before_ingest
                && payload_view
                    .as_ref()
                    .map(|payload| payload.height <= self.replication_persisted_height)
                    .unwrap_or(false)
            {
                continue;
            }
            let committed_successor = checked_replication_successor(
                self.committed_height,
                "committed_height",
                "ingesting replication message",
            )?;
            let persisted_successor = checked_replication_successor(
                self.replication_persisted_height,
                "replication_persisted_height",
                "ingesting replication message",
            )?;
            let defer_fresh_height_one_for_checkpoint_bootstrap = execution_hook.is_some()
                && self.checkpoint_bootstrap_enabled
                && self.committed_height == 0
                && self.replication_persisted_height == 0
                && self.last_execution_height == 0
                && (self.network_committed_height >= REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL
                    || self.should_defer_fresh_observer_checkpoint_retry()
                    || high_peer_head_retry_pending
                    || checkpoint_preflight_unavailable
                    || checkpoint_bootstrap_retry_pending
                    || checkpoint_bootstrap_preflight.should_defer_height_one());
            let should_apply = payload_view
                .as_ref()
                .map(|payload| {
                    payload.height <= persisted_successor
                        && !(defer_fresh_height_one_for_checkpoint_bootstrap
                            && payload.height == persisted_successor)
                })
                .unwrap_or(true);
            if !should_apply {
                continue;
            }
            let mut executed_commit = None;
            if let Some(payload) = payload_view.as_ref()
                && payload.height == committed_successor
            {
                let full_payload = parse_replication_commit_payload(message.payload.as_slice())
                    .ok_or_else(|| NodeError::Replication {
                        reason: format!(
                            "replication message payload decode failed at height {}",
                            payload.height
                        ),
                    })?;
                let executed = with_execution_hook(&mut execution_hook, |hook| {
                    self.execute_synced_replication_commit(world_id, &full_payload, hook)
                })?;
                executed_commit = Some((full_payload.height, executed.0, executed.1));
            }
            let mut persisted_commit = false;
            match replication_runtime.apply_remote_message(node_id, world_id, &message) {
                Ok(()) => {
                    persisted_commit = true;
                    endpoint.publish_local_content_provider_best_effort(
                        world_id,
                        message.record.content_hash.as_str(),
                    );
                    if let Some(full_payload) =
                        parse_replication_commit_payload(message.payload.as_slice())
                        && let Some(descriptor) = full_payload.execution_checkpoint.as_ref()
                    {
                        Self::publish_execution_checkpoint_descriptor_providers(
                            endpoint,
                            world_id,
                            replication_runtime,
                            descriptor,
                        )?;
                    }
                    if let Some(payload) = payload_view {
                        self.advance_contiguous_replication_persisted_height(
                            replication_runtime,
                            world_id,
                            payload.height,
                        )?;
                    }
                }
                Err(err) => rejected.push(format!(
                    "node_id={} world_id={} err={}",
                    message.node_id, message.world_id, err
                )),
            }
            if persisted_commit && let Some((height, block_hash, committed_at_ms)) = executed_commit
            {
                self.record_synced_replication_height(height, block_hash, committed_at_ms)?;
                if let Some(callback) = progress_callback.as_deref_mut() {
                    let decision = self.idle_pending_decision()?;
                    callback(self.snapshot_from_decision(&decision))?;
                }
            }
        }
        if !rejected.is_empty() {
            let rejected_count = rejected.len();
            let sample = rejected.into_iter().take(3).collect::<Vec<_>>();
            return Err(NodeError::Replication {
                reason: format!(
                    "replication ingest rejected {rejected_count} message(s); sample={sample:?}"
                ),
            });
        }
        Ok(())
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "High-checkpoint recovery seam preserves explicit network, execution, and progress channels"
    )]
    pub(super) fn try_sync_high_replication_checkpoint_boundary(
        &mut self,
        endpoint: &ReplicationNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        replication_runtime: &mut ReplicationRuntime,
        checkpoint_height: u64,
        blocked_height: u64,
        expected_checkpoint_head: Option<&WorldHeadAnnounce>,
        execution_hook: &mut Option<&mut dyn NodeExecutionHook>,
        progress_callback: &mut Option<
            &mut dyn FnMut(NodeConsensusSnapshot) -> Result<(), NodeError>,
        >,
    ) -> Result<bool, NodeError> {
        replication_runtime.ensure_checkpoint_lineage_healthy()?;
        let fresh_execution_bootstrap = self.committed_height == 0
            && self.replication_persisted_height == 0
            && self.last_execution_height == 0;
        if (self.require_execution_on_commit && !fresh_execution_bootstrap)
            || checkpoint_height <= blocked_height
            || checkpoint_height <= self.replication_persisted_height
        {
            return Ok(false);
        }
        if self.pending_checkpoint_receipt.is_some() {
            return self.finalize_pending_checkpoint_receipt(
                endpoint,
                node_id,
                world_id,
                replication_runtime,
                checkpoint_height,
                progress_callback,
            );
        }
        let checkpoint = match self.sync_replication_height_once_for_high_checkpoint_probe(
            endpoint,
            node_id,
            world_id,
            replication_runtime,
            checkpoint_height,
        )? {
            GapSyncHeightOutcome::Synced {
                message, payload, ..
            } => (message, payload),
            GapSyncHeightOutcome::NotFound { .. } => return Ok(false),
        };
        let (message, payload) = checkpoint;
        if payload.execution_block_hash.is_none() || payload.execution_state_root.is_none() {
            return Ok(false);
        }
        let expected_head_matches_candidate = expected_checkpoint_head
            .is_some_and(|expected_head| checkpoint_height == expected_head.height);
        let unsigned_exact_head = expected_head_matches_candidate
            && expected_checkpoint_head
                .is_some_and(|expected_head| expected_head.signature.trim().is_empty());
        if unsigned_exact_head && payload.lineage_envelope.is_none() {
            return Ok(false);
        }
        if fresh_execution_bootstrap && !expected_head_matches_candidate {
            let discovered_head_lineage = self.checkpoint_bootstrap_enabled
                && self.require_execution_on_commit
                && self.peer_heads.is_empty()
                && expected_checkpoint_head
                    .is_some_and(|expected_head| checkpoint_height < expected_head.height);
            if payload.lineage_envelope.is_none()
                && (discovered_head_lineage
                    || !self.authenticated_checkpoint_writer_has_supermajority_stake(&message))
            {
                return Ok(false);
            }
        }
        if expected_head_matches_candidate {
            let Some(expected_head) = expected_checkpoint_head else {
                return Ok(false);
            };
            if Self::validate_world_head_checkpoint_payload(world_id, &payload, expected_head)
                .is_err()
            {
                return Ok(false);
            }
        }

        let Some((block_hash, committed_at_ms)) = (if let Some(checkpoint_descriptor) =
            payload.execution_checkpoint.clone()
        {
            if !self.validate_high_checkpoint_lineage_candidate(
                world_id,
                &payload,
                &checkpoint_descriptor,
                expected_checkpoint_head,
                fresh_execution_bootstrap,
                unsigned_exact_head,
            ) {
                return Ok(false);
            }
            let Some(execution_block_hash) = payload.execution_block_hash.clone() else {
                return Ok(false);
            };
            let Some(execution_state_root) = payload.execution_state_root.clone() else {
                return Ok(false);
            };
            let probe_nonce = std::env::var("OASIS7_CHECKPOINT_PROBE_NONCE").ok();
            if let Some(probe_nonce) = probe_nonce.as_deref()
                && !ReplicationRuntime::checkpoint_probe_nonce_is_valid(probe_nonce)
            {
                return Err(NodeError::Replication {
                    reason: "checkpoint verification receipt probe nonce must be at least 32 URL-safe characters".to_string(),
                });
            }
            let (checkpoint_bundle, checkpoint_fetch_observations) = self
                .fetch_execution_checkpoint_bundle(
                    endpoint,
                    world_id,
                    replication_runtime,
                    &checkpoint_descriptor,
                    probe_nonce.is_some(),
                )?;
            // The closure can be large. Normal sync has no receipt consumer,
            // so retain a second owned copy only for an explicit probe run.
            let checkpoint_receipt_bundle = probe_nonce.as_ref().map(|_| checkpoint_bundle.clone());
            with_execution_hook(execution_hook, |hook| {
                let Some(hook) = hook else {
                    return Ok(None);
                };
                let result = hook
                    .install_checkpoint_bundle(
                        NodeExecutionCheckpointInstallContext {
                            world_id: world_id.to_string(),
                            node_id: node_id.to_string(),
                            height: payload.height,
                            node_block_hash: payload.block_hash.clone(),
                            execution_block_hash: execution_block_hash.clone(),
                            execution_state_root: execution_state_root.clone(),
                            committed_at_unix_ms: payload.committed_at_ms,
                        },
                        checkpoint_bundle,
                    )
                    .map_err(|reason| NodeError::Execution { reason })?;
                if result.execution_height != payload.height
                    || result.execution_block_hash != execution_block_hash
                    || result.execution_state_root != execution_state_root
                {
                    return Err(NodeError::Execution {
                        reason: format!(
                            "execution checkpoint install returned mismatched binding at height {}",
                            payload.height
                        ),
                    });
                }
                // Installation is irreversible from the engine's perspective.
                // Record its execution boundary before optional probe-receipt
                // finalization so a receipt failure cannot cause the next
                // high-checkpoint probe to install the same closure again.
                self.last_execution_height = result.execution_height;
                self.last_execution_block_hash = Some(result.execution_block_hash.clone());
                self.last_execution_state_root = Some(result.execution_state_root.clone());
                self.remember_execution_binding_for_height(payload.height);
                if let (Some(probe_nonce), Some(bundle)) =
                    (probe_nonce.as_deref(), checkpoint_receipt_bundle.as_ref())
                {
                    self.pending_checkpoint_receipt = Some(
                        crate::node_engine_replication_pending_checkpoint_receipt::PendingCheckpointReceipt {
                        world_id: world_id.to_string(),
                        node_id: node_id.to_string(),
                        height: payload.height,
                        message: message.clone(),
                        descriptor: checkpoint_descriptor.clone(),
                        bundle: bundle.clone(),
                        fetch_observations: checkpoint_fetch_observations.clone(),
                        probe_nonce: probe_nonce.to_string(),
                        receipt_persisted: false,
                        block_hash: payload.block_hash.clone(),
                        committed_at_ms: payload.committed_at_ms,
                        },
                    );
                }
                replication_runtime.persist_checkpoint_verification_receipt(
                    world_id,
                    probe_nonce.as_deref(),
                    &checkpoint_descriptor,
                    checkpoint_receipt_bundle.as_ref(),
                    checkpoint_fetch_observations.as_slice(),
                )?;
                if let Some(pending) = self.pending_checkpoint_receipt.as_mut() {
                    pending.receipt_persisted = true;
                }
                Ok(Some((payload.block_hash.clone(), payload.committed_at_ms)))
            })?
        } else {
            if execution_hook.is_some()
                && checkpoint_height > self.last_execution_height.saturating_add(1)
            {
                return Ok(false);
            }
            Some(with_execution_hook(execution_hook, |hook| {
                self.execute_synced_replication_commit(world_id, &payload, hook)
            })?)
        }) else {
            return Ok(false);
        };
        self.persist_synced_replication_message(
            endpoint,
            node_id,
            world_id,
            replication_runtime,
            &message,
            checkpoint_height,
        )?;
        self.replication_persisted_height =
            self.replication_persisted_height.max(checkpoint_height);
        self.record_synced_replication_height(checkpoint_height, block_hash, committed_at_ms)?;
        self.pending_checkpoint_receipt = None;
        self.last_replication_gap_sync_blocked_height = None;
        self.last_replication_gap_sync_blocked_reason = None;
        self.last_replication_gap_sync_blocked_at_ms = None;
        self.last_replication_gap_sync_repair_attempt_height = None;
        self.last_replication_gap_sync_repair_attempt_summary = None;
        self.last_replication_gap_sync_repair_attempt_route_snapshot = None;
        if let Some(callback) = progress_callback.as_deref_mut() {
            let decision = self.idle_pending_decision()?;
            callback(self.snapshot_from_decision(&decision))?;
        }
        Ok(true)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Gap-sync recovery seam keeps replication, execution, progress, and peer-head tracking channels explicit"
    )]
    pub(super) fn sync_missing_replication_commits_with_progress(
        &mut self,
        endpoint: &ReplicationNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        mut replication: Option<&mut ReplicationRuntime>,
        mut execution_hook: Option<&mut dyn NodeExecutionHook>,
        mut progress_callback: Option<
            &mut dyn FnMut(NodeConsensusSnapshot) -> Result<(), NodeError>,
        >,
        record_peer_heads_from_gap_sync: bool,
    ) -> Result<(), NodeError> {
        let Some(replication_runtime) = replication.as_mut() else {
            return Ok(());
        };
        self.refresh_replication_persisted_height(replication_runtime, world_id)?;
        let starting_replication_persisted_height = self.replication_persisted_height;
        let advertised_world_head = endpoint.lookup_world_head(world_id)?;
        if advertised_world_head.is_none()
            && self.checkpoint_bootstrap_enabled
            && self.committed_height == 0
            && self.replication_persisted_height == 0
            && self.last_execution_height == 0
            && self.peer_heads.values().all(|head| head.height <= 1)
            && self.fresh_observer_checkpoint_preflight_unavailable
        {
            // Match ingest's fresh-observer deferral above. A missing peer
            // head is a preflight result, not evidence that it is safe to
            // execute the height-one tail before a checkpoint bootstrap.
            return Ok(());
        }
        let cached_high_checkpoint_height = if self.checkpoint_bootstrap_enabled
            && self.committed_height == 0
            && self.replication_persisted_height == 0
            && self.last_execution_height == 0
        {
            // A cached high peer head is a bounded discovery hint for a fresh
            // observer when the world-head route is temporarily unavailable.
            // `cached_high_checkpoint_head` only returns a complete,
            // authenticated peer binding; checkpoint payload and lineage
            // verification below remain authoritative before installation.
            self.cached_high_checkpoint_head(world_id)
                .map(|head| head.height)
                .unwrap_or(0)
        } else {
            0
        };
        let cached_head_without_world_head =
            advertised_world_head.is_none() && cached_high_checkpoint_height > 0;
        if self.should_defer_fresh_observer_checkpoint_retry() && !cached_head_without_world_head {
            return Ok(());
        }
        let advertised_network_height = self
            .network_committed_height
            .max(
                advertised_world_head
                    .as_ref()
                    .map(|head| head.height)
                    .unwrap_or(0),
            )
            .max(cached_high_checkpoint_height);
        let expected_checkpoint_head = advertised_world_head
            .as_ref()
            .filter(|head| head.height == advertised_network_height);
        if advertised_network_height <= self.replication_persisted_height {
            self.last_replication_gap_sync_blocked_height = None;
            self.last_replication_gap_sync_blocked_reason = None;
            self.last_replication_gap_sync_blocked_at_ms = None;
            return Ok(());
        }
        let network_lag =
            advertised_network_height.saturating_sub(self.replication_persisted_height);
        let next_height = checked_replication_successor(
            self.replication_persisted_height,
            "replication_persisted_height",
            "starting replication gap sync",
        )?;
        let now_ms = crate::runtime_util::now_unix_ms();
        if self.replication_gap_sync_fetch_blob_rate_limited_in_cooldown(next_height, now_ms) {
            return Ok(());
        }
        let mut missing_checkpoint_closure_reason = None;
        if network_lag >= REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL {
            for checkpoint_candidate in Self::high_replication_checkpoint_candidates(
                advertised_network_height,
                self.replication_persisted_height,
            ) {
                let expected_candidate_head = expected_checkpoint_head;
                match self.try_sync_high_replication_checkpoint_boundary(
                    endpoint,
                    node_id,
                    world_id,
                    replication_runtime,
                    checkpoint_candidate,
                    self.replication_persisted_height,
                    expected_candidate_head,
                    &mut execution_hook,
                    &mut progress_callback,
                ) {
                    Ok(true) => {
                        if self.replication_persisted_height > starting_replication_persisted_height
                        {
                            self.network_committed_height =
                                self.network_committed_height.max(advertised_network_height);
                        }
                        return Ok(());
                    }
                    Ok(false) => {}
                    Err(err) if Self::high_replication_checkpoint_probe_can_continue(&err) => {
                        if let Some(reason) =
                            Self::high_replication_checkpoint_closure_missing_reason(&err)
                        {
                            missing_checkpoint_closure_reason = Some(reason);
                        }
                        if self.record_high_replication_checkpoint_probe_failure(
                            next_height,
                            checkpoint_candidate,
                            now_ms,
                            &err,
                        ) {
                            return Ok(());
                        }
                    }
                    Err(err) => return Err(err),
                }
            }
            // When this runs from the runtime tick, consensus has already
            // observed the high peer head. Hold at height zero after failed
            // checkpoint probes so the tick can publish that authority
            // snapshot and retry preflight on the next tick. Direct repair
            // callers retain the terminal mismatch when no closure exists.
            if self.hold_fresh_observer_for_high_peer_heads(advertised_network_height)
                && (!cached_head_without_world_head || progress_callback.is_some())
            {
                return Ok(());
            }
            if let Some(reason) = missing_checkpoint_closure_reason {
                return Err(NodeError::Replication { reason });
            }
            if self.checkpoint_bootstrap_enabled
                && self.committed_height == 0
                && self.replication_persisted_height == 0
                && self.last_execution_height == 0
                && self.fresh_observer_checkpoint_preflight_unavailable
            {
                return Ok(());
            }
        }
        if Self::replication_gap_sync_local_state_blocked(
            self.last_replication_gap_sync_blocked_height,
            self.last_replication_gap_sync_blocked_reason.as_deref(),
            next_height,
        ) {
            return Ok(());
        }
        let mut next_height = next_height;
        if replication_gap_sync_provider_blob_route_blocked_in_cooldown(
            self.last_replication_gap_sync_blocked_height,
            self.last_replication_gap_sync_blocked_reason.as_deref(),
            self.last_replication_gap_sync_blocked_at_ms,
            next_height,
            now_ms,
        ) {
            return Ok(());
        }
        let gap_sync_target_height = advertised_network_height.min(
            self.replication_persisted_height
                .saturating_add(REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL),
        );
        while next_height <= gap_sync_target_height {
            let mut synced_commit: Option<(
                replication::GossipReplicationMessage,
                ReplicationCommitPayload,
            )> = None;
            let mut not_found = false;
            let mut provider_route_blocked = false;
            let mut last_error = None;
            for attempt in 1..=REPLICATION_GAP_SYNC_MAX_RETRIES_PER_HEIGHT {
                match self.sync_replication_height_once(
                    endpoint,
                    node_id,
                    world_id,
                    replication_runtime,
                    next_height,
                ) {
                    Ok(GapSyncHeightOutcome::Synced {
                        message,
                        payload,
                        repair_summary,
                        route_snapshot,
                    }) => {
                        self.record_replication_gap_sync_repair_attempt(
                            next_height,
                            repair_summary,
                            route_snapshot,
                        );
                        synced_commit = Some((message, payload));
                        break;
                    }
                    Ok(GapSyncHeightOutcome::NotFound {
                        repair_summary,
                        route_snapshot,
                    }) => {
                        not_found = true;
                        self.record_replication_gap_sync_repair_attempt(
                            next_height,
                            repair_summary,
                            route_snapshot,
                        );
                        break;
                    }
                    Err(err) if replication_request_waitable_connection_gap(&err) => {
                        if self.replication_persisted_height > starting_replication_persisted_height
                        {
                            self.network_committed_height =
                                self.network_committed_height.max(advertised_network_height);
                        }
                        return Ok(());
                    }
                    Err(err) if replication_gap_sync_provider_blob_route_blocked(&err) => {
                        provider_route_blocked = true;
                        let summary = crate::network_error_classification::persisted_network_reason(
                            format!(
                                "attempt {attempt}/{} failed: {err}",
                                REPLICATION_GAP_SYNC_MAX_RETRIES_PER_HEIGHT
                            ),
                            &err,
                        );
                        self.last_replication_gap_sync_repair_attempt_height = Some(next_height);
                        self.last_replication_gap_sync_repair_attempt_summary =
                            Some(summary.clone());
                        self.last_replication_gap_sync_repair_attempt_route_snapshot =
                            endpoint.take_last_gap_sync_fetch_commit_failure_route_snapshot();
                        last_error = Some(summary);
                        break;
                    }
                    Err(err)
                        if record_replication_gap_sync_fetch_blob_rate_limit(
                            self,
                            next_height,
                            now_ms,
                            attempt,
                            &err,
                        ) =>
                    {
                        return Ok(());
                    }
                    Err(err) => {
                        last_error = Some(format!(
                            "attempt {attempt}/{} failed: {}",
                            REPLICATION_GAP_SYNC_MAX_RETRIES_PER_HEIGHT, err
                        ));
                        self.last_replication_gap_sync_repair_attempt_height = Some(next_height);
                        self.last_replication_gap_sync_repair_attempt_summary = last_error.clone();
                        self.last_replication_gap_sync_repair_attempt_route_snapshot =
                            endpoint.take_last_gap_sync_fetch_commit_failure_route_snapshot();
                        if Self::replication_gap_sync_local_state_blocked_reason(
                            last_error.as_deref().unwrap_or(""),
                        ) {
                            break;
                        }
                    }
                }
            }
            if let Some((message, payload)) = synced_commit {
                let execution_result = with_execution_hook(&mut execution_hook, |hook| {
                    self.execute_synced_replication_commit(world_id, &payload, hook)
                });
                let (block_hash, committed_at_ms) = match execution_result {
                    Ok(result) => result,
                    Err(err) => {
                        if self.try_fresh_observer_checkpoint_fallback_after_execution_mismatch(
                            endpoint,
                            node_id,
                            world_id,
                            replication_runtime,
                            next_height,
                            &mut execution_hook,
                            &mut progress_callback,
                            &err,
                        )? {
                            return Ok(());
                        }
                        self.record_replication_gap_sync_local_state_block(
                            next_height,
                            advertised_network_height,
                            gap_sync_target_height,
                            err.to_string(),
                        );
                        return Err(err);
                    }
                };
                if let Err(err) = self.persist_synced_replication_message(
                    endpoint,
                    node_id,
                    world_id,
                    replication_runtime,
                    &message,
                    next_height,
                ) {
                    self.record_replication_gap_sync_local_state_block(
                        next_height,
                        advertised_network_height,
                        gap_sync_target_height,
                        err.to_string(),
                    );
                    return Err(err);
                }
                self.replication_persisted_height =
                    self.replication_persisted_height.max(next_height);
                if let Err(err) =
                    self.record_synced_replication_height(next_height, block_hash, committed_at_ms)
                {
                    self.record_replication_gap_sync_local_state_block(
                        next_height,
                        advertised_network_height,
                        gap_sync_target_height,
                        err.to_string(),
                    );
                    return Err(err);
                }
                if record_peer_heads_from_gap_sync {
                    self.observe_peer_committed_head(
                        payload.node_id.as_str(),
                        PeerCommittedHead {
                            height: payload.height,
                            block_hash: payload.block_hash.clone(),
                            committed_at_ms: payload.committed_at_ms,
                            observed_at_ms: crate::runtime_util::now_unix_ms(),
                            execution_block_hash: payload.execution_block_hash.clone(),
                            execution_state_root: payload.execution_state_root.clone(),
                            action_root: payload.action_root.clone(),
                            public_key_hex: None,
                            signature_hex: None,
                        },
                    );
                }
                if let Some(callback) = progress_callback.as_deref_mut() {
                    let decision = self.idle_pending_decision()?;
                    callback(self.snapshot_from_decision(&decision))?;
                }
                next_height = checked_replication_successor(
                    next_height,
                    "next_height",
                    "advancing replication gap sync cursor",
                )?;
                continue;
            }
            if provider_route_blocked {
                self.last_replication_gap_sync_blocked_height = Some(next_height);
                self.last_replication_gap_sync_blocked_at_ms = Some(now_ms);
                self.last_replication_gap_sync_blocked_reason = Some(format!(
                    "replication gap sync provider route blocked: missing commit height {next_height} while advertised_network_height={} network_committed_height={} gap_sync_target_height={} replication_persisted_height={} repair_attempt={}",
                    advertised_network_height,
                    self.network_committed_height,
                    gap_sync_target_height,
                    self.replication_persisted_height,
                    last_error
                        .as_deref()
                        .unwrap_or("provider route unavailable")
                ));
                break;
            }
            if not_found {
                for checkpoint_candidate in Self::high_replication_checkpoint_candidates(
                    advertised_network_height,
                    next_height,
                ) {
                    let expected_candidate_head = expected_checkpoint_head;
                    match self.try_sync_high_replication_checkpoint_boundary(
                        endpoint,
                        node_id,
                        world_id,
                        replication_runtime,
                        checkpoint_candidate,
                        next_height,
                        expected_candidate_head,
                        &mut execution_hook,
                        &mut progress_callback,
                    ) {
                        Ok(true) => break,
                        Ok(false) => {}
                        Err(err) if Self::high_replication_checkpoint_probe_can_continue(&err) => {
                            if self.record_high_replication_checkpoint_probe_failure(
                                next_height,
                                checkpoint_candidate,
                                now_ms,
                                &err,
                            ) {
                                return Ok(());
                            }
                        }
                        Err(err) => return Err(err),
                    }
                }
                if self.replication_persisted_height > next_height {
                    break;
                }
                self.last_replication_gap_sync_blocked_height = Some(next_height);
                self.last_replication_gap_sync_blocked_at_ms = None;
                self.last_replication_gap_sync_blocked_reason = Some(format!(
                    "replication gap sync blocked: missing commit height {next_height} while advertised_network_height={} network_committed_height={} gap_sync_target_height={} replication_persisted_height={} repair_attempt={}",
                    advertised_network_height,
                    self.network_committed_height,
                    gap_sync_target_height,
                    self.replication_persisted_height,
                    self.last_replication_gap_sync_repair_attempt_summary
                        .as_deref()
                        .unwrap_or("unavailable")
                ));
                break;
            }
            self.last_replication_gap_sync_blocked_height = Some(next_height);
            self.last_replication_gap_sync_blocked_at_ms = None;
            let last_error = last_error.unwrap_or_else(|| "unknown error".to_string());
            if Self::replication_gap_sync_local_state_blocked_reason(last_error.as_str()) {
                self.record_replication_gap_sync_local_state_block(
                    next_height,
                    advertised_network_height,
                    gap_sync_target_height,
                    last_error.clone(),
                );
            } else {
                self.last_replication_gap_sync_blocked_reason = Some(format!(
                    "replication gap sync failed at height {next_height}: {}",
                    last_error
                ));
            }
            return Err(NodeError::Replication {
                reason: format!(
                    "gap sync height {} failed after {} attempts: {}",
                    next_height, REPLICATION_GAP_SYNC_MAX_RETRIES_PER_HEIGHT, last_error
                ),
            });
        }
        if self.replication_persisted_height >= advertised_network_height {
            self.last_replication_gap_sync_blocked_height = None;
            self.last_replication_gap_sync_blocked_reason = None;
            self.last_replication_gap_sync_blocked_at_ms = None;
            self.last_replication_gap_sync_repair_attempt_height = None;
            self.last_replication_gap_sync_repair_attempt_summary = None;
            self.last_replication_gap_sync_repair_attempt_route_snapshot = None;
        } else {
            self.clear_replication_gap_sync_blocked_if_unblocked();
        }
        if self.replication_persisted_height > starting_replication_persisted_height {
            self.network_committed_height =
                self.network_committed_height.max(advertised_network_height);
        }
        Ok(())
    }

    pub(super) fn record_synced_replication_height(
        &mut self,
        height: u64,
        block_hash: String,
        committed_at_ms: i64,
    ) -> Result<(), NodeError> {
        if height <= self.committed_height {
            return Ok(());
        }
        let next_synced_height =
            checked_replication_successor(height, "height", "recording synced replication height")?;
        self.clear_pending_action_reservation()?;
        self.discard_replicated_execution_inputs_through_height(height)?;
        self.replication_persisted_height = self.replication_persisted_height.max(height);
        self.committed_height = height;
        self.network_committed_height = self.network_committed_height.max(height);
        self.last_committed_at_ms = Some(committed_at_ms);
        self.next_height = next_synced_height;
        self.last_committed_block_hash = Some(block_hash);
        self.pending = None;
        Ok(())
    }
}
