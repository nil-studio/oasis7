use super::*;
use crate::replication_checkpoint_lineage::verify_checkpoint_lineage_for_descriptor;
use crate::replication_state_reconcile::ReplicationCommitPayload;
use oasis7_proto::distributed::WorldHeadAnnounce;

pub(super) enum FreshObserverCheckpointBootstrap {
    Installed,
    PreflightUnavailable,
    RetryPending,
    HighCheckpointPending,
    LowHeadConfirmationPending,
    NotInstalled,
}

impl FreshObserverCheckpointBootstrap {
    pub(super) fn should_defer_height_one(&self) -> bool {
        matches!(
            self,
            Self::HighCheckpointPending | Self::LowHeadConfirmationPending
        )
    }
}

impl PosNodeEngine {
    // Mirrors release_default.execution_checkpoint_keep. Probe the advertised head first, then
    // the aligned retained-window boundaries, including the latest completed boundary.
    const HIGH_REPLICATION_CHECKPOINT_LOOKBACK_WINDOWS: u64 = 8;

    pub(super) fn retain_high_peer_checkpoint_retry_authority(&mut self) -> bool {
        let pending = self.checkpoint_bootstrap_enabled
            && self.committed_height == 0
            && self.replication_persisted_height == 0
            && self.last_execution_height == 0
            && self
                .peer_heads
                .values()
                .any(|head| head.height >= REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL);
        if pending {
            self.fresh_observer_checkpoint_bootstrap_retry_pending = true;
        }
        pending
    }

    pub(super) fn should_defer_fresh_observer_checkpoint_retry(&self) -> bool {
        let high_head_retry_pending = self.fresh_observer_checkpoint_bootstrap_retry_pending;
        self.checkpoint_bootstrap_enabled
            && self.committed_height == 0
            && self.replication_persisted_height == 0
            && self.last_execution_height == 0
            && (self
                .fresh_observer_checkpoint_low_head_confirmation
                .is_some()
                || high_head_retry_pending)
    }

    /// Keep a fresh observer at height zero while connected peers advertise a
    /// high, unresolved head that the world-head lookup may not expose yet.
    ///
    /// The world-head lookup is a bounded DHT/connected-provider probe and may
    /// transiently return a stale height-one candidate even though the
    /// validated peer-head cache already contains a current connected
    /// validator head.  Replaying that candidate would bypass checkpoint
    /// closure and can trigger an execution-peer mismatch without a height-zero
    /// rollback.  This gate is intentionally conservative: it is only
    /// used for a fresh observer with an unresolved high peer head, and callers
    /// still require a verified checkpoint receipt before advancing.
    pub(super) fn hold_fresh_observer_for_high_peer_heads(
        &mut self,
        advertised_network_height: u64,
    ) -> bool {
        if !self.checkpoint_bootstrap_enabled
            || self.committed_height != 0
            || self.replication_persisted_height != 0
            || self.last_execution_height != 0
            || advertised_network_height < REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL
        {
            return false;
        }
        let hold = self
            .peer_heads
            .values()
            .any(|head| head.height >= REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL);
        if hold {
            self.fresh_observer_checkpoint_bootstrap_retry_pending = true;
        }
        hold
    }

    /// Transport connectivity is not checkpoint authority. Keep this test
    /// seam available for regression probes without shipping a quorum hold.
    #[cfg(test)]
    pub(super) fn hold_fresh_observer_for_connected_validator_quorum(
        &mut self,
        _endpoint: &ReplicationNetworkEndpoint,
        _advertised_height: u64,
    ) -> bool {
        false
    }

    pub(super) fn cached_high_checkpoint_head(&self, world_id: &str) -> Option<WorldHeadAnnounce> {
        self.peer_heads
            .iter()
            .filter_map(|(node_id, head)| {
                let validator_id = self.validator_id_for_peer_head(node_id)?;
                if self.quarantined_validators.contains(&validator_id)
                    || head.height < REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL
                    || head.block_hash.is_empty()
                {
                    return None;
                }
                let execution_block_hash = head.execution_block_hash.as_ref()?;
                let state_root = head.execution_state_root.as_ref()?;
                if execution_block_hash.is_empty() || state_root.is_empty() {
                    return None;
                }
                Some(WorldHeadAnnounce {
                    world_id: world_id.to_string(),
                    height: head.height,
                    block_hash: head.block_hash.clone(),
                    state_root: state_root.clone(),
                    timestamp_ms: head.committed_at_ms,
                    signature: String::new(),
                })
            })
            .max_by(|left, right| {
                left.height
                    .cmp(&right.height)
                    .then_with(|| left.block_hash.cmp(&right.block_hash))
                    .then_with(|| left.state_root.cmp(&right.state_root))
            })
    }

    pub(super) fn try_bootstrap_fresh_observer_from_advertised_checkpoint(
        &mut self,
        endpoint: &ReplicationNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        replication_runtime: &mut ReplicationRuntime,
        execution_hook: &mut Option<&mut dyn NodeExecutionHook>,
        progress_callback: &mut Option<
            &mut dyn FnMut(NodeConsensusSnapshot) -> Result<(), NodeError>,
        >,
    ) -> Result<FreshObserverCheckpointBootstrap, NodeError> {
        if execution_hook.is_none()
            || !self.checkpoint_bootstrap_enabled
            || self.committed_height != 0
            || self.replication_persisted_height != 0
            || self.last_execution_height != 0
        {
            return Ok(FreshObserverCheckpointBootstrap::NotInstalled);
        }
        let Some(advertised_head) = endpoint.lookup_world_head(world_id)? else {
            self.fresh_observer_checkpoint_preflight_unavailable = true;
            self.fresh_observer_checkpoint_low_head_confirmation = None;
            return Ok(FreshObserverCheckpointBootstrap::PreflightUnavailable);
        };
        self.fresh_observer_checkpoint_preflight_unavailable = false;
        if advertised_head.height < REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL {
            // A stale world-head response must not revoke a high connected
            // peer-head observation.  The DHT/peer world-head route can still
            // report height one while a fresh observer has already observed
            // a retained high head from a validator.  Establish retry
            // authority before low-head confirmation so a later height-one
            // candidate cannot enter incremental execution without a verified
            // checkpoint closure.
            if let Some(cached_head) = self.cached_high_checkpoint_head(world_id) {
                self.fresh_observer_checkpoint_low_head_confirmation = None;
                self.fresh_observer_checkpoint_bootstrap_retry_pending = false;
                let mut retry_pending = false;
                for checkpoint_candidate in
                    Self::high_replication_checkpoint_candidates(cached_head.height, 0)
                {
                    let expected_candidate_head =
                        (checkpoint_candidate == cached_head.height).then_some(&cached_head);
                    match self.try_sync_high_replication_checkpoint_boundary(
                        endpoint,
                        node_id,
                        world_id,
                        replication_runtime,
                        checkpoint_candidate,
                        0,
                        expected_candidate_head,
                        execution_hook,
                        progress_callback,
                    ) {
                        Ok(true) => {
                            return Ok(FreshObserverCheckpointBootstrap::Installed);
                        }
                        Ok(false) => {}
                        Err(err) if Self::high_replication_checkpoint_probe_can_continue(&err) => {
                            retry_pending = true;
                            self.fresh_observer_checkpoint_bootstrap_retry_pending = true;
                        }
                        Err(err) => return Err(err),
                    }
                }
                self.fresh_observer_checkpoint_bootstrap_retry_pending = true;
                return Ok(if retry_pending {
                    FreshObserverCheckpointBootstrap::RetryPending
                } else {
                    FreshObserverCheckpointBootstrap::HighCheckpointPending
                });
            }
            // Preserve the existing fail-closed hold for high heads that are
            // not trustworthy checkpoint candidates (for example, an unknown
            // or incomplete peer binding).  They must not be allowed to age
            // into low-head confirmation and height-one replay.
            if self
                .peer_heads
                .values()
                .any(|head| head.height >= REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL)
            {
                self.fresh_observer_checkpoint_low_head_confirmation = None;
                self.fresh_observer_checkpoint_bootstrap_retry_pending = true;
                return Ok(FreshObserverCheckpointBootstrap::HighCheckpointPending);
            }
            // An already observed network height can require an immediate low-height
            // checkpoint to recover missing history. Only a completely unestablished
            // fresh observer needs to wait one poll for a stale peer head to advance.
            if self.network_committed_height != 0 {
                self.fresh_observer_checkpoint_low_head_confirmation = None;
                return Ok(if self.fresh_observer_checkpoint_bootstrap_retry_pending {
                    FreshObserverCheckpointBootstrap::HighCheckpointPending
                } else {
                    FreshObserverCheckpointBootstrap::NotInstalled
                });
            }
            let identity = (
                advertised_head.height,
                advertised_head.block_hash.clone(),
                advertised_head.state_root.clone(),
            );
            if self
                .fresh_observer_checkpoint_low_head_confirmation
                .as_ref()
                == Some(&identity)
            {
                self.fresh_observer_checkpoint_low_head_confirmation = None;
                return Ok(FreshObserverCheckpointBootstrap::NotInstalled);
            }
            self.fresh_observer_checkpoint_low_head_confirmation = Some(identity);
            return Ok(FreshObserverCheckpointBootstrap::LowHeadConfirmationPending);
        }
        self.fresh_observer_checkpoint_bootstrap_retry_pending = false;
        self.fresh_observer_checkpoint_low_head_confirmation = None;
        // The advertised live head is not necessarily itself a retained
        // checkpoint boundary (for example, head 52079 with the newest
        // retained checkpoint at 52032). Probe the head first, then the
        // bounded aligned candidates below it; each candidate must still
        // return a descriptor whose height/hash/state binding matches that
        // candidate before installation can proceed.
        let mut retry_pending = false;
        for checkpoint_candidate in
            Self::high_replication_checkpoint_candidates(advertised_head.height, 0)
        {
            let expected_candidate_head = Some(&advertised_head);
            match self.try_sync_high_replication_checkpoint_boundary(
                endpoint,
                node_id,
                world_id,
                replication_runtime,
                checkpoint_candidate,
                0,
                expected_candidate_head,
                execution_hook,
                progress_callback,
            ) {
                Ok(true) => return Ok(FreshObserverCheckpointBootstrap::Installed),
                // A high advertised head is evidence that replaying the
                // height-one tail is unsafe until this observer has either
                // installed a verified retained checkpoint or observed that
                // head change. A non-boundary head is expected to return no
                // descriptor; continue to the newest aligned candidate.
                Ok(false) => {}
                Err(err) if Self::high_replication_checkpoint_probe_can_continue(&err) => {
                    retry_pending = true;
                    self.fresh_observer_checkpoint_bootstrap_retry_pending = true;
                }
                Err(err) => return Err(err),
            }
        }
        self.fresh_observer_checkpoint_bootstrap_retry_pending = true;
        Ok(if retry_pending {
            FreshObserverCheckpointBootstrap::RetryPending
        } else {
            FreshObserverCheckpointBootstrap::HighCheckpointPending
        })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable fresh-observer recovery seam preserves execution mismatch context and progress channels"
    )]
    pub(super) fn try_fresh_observer_checkpoint_fallback_after_execution_mismatch(
        &mut self,
        endpoint: &ReplicationNetworkEndpoint,
        node_id: &str,
        world_id: &str,
        replication_runtime: &mut ReplicationRuntime,
        next_height: u64,
        execution_hook: &mut Option<&mut dyn NodeExecutionHook>,
        progress_callback: &mut Option<
            &mut dyn FnMut(NodeConsensusSnapshot) -> Result<(), NodeError>,
        >,
        err: &NodeError,
    ) -> Result<bool, NodeError> {
        // A fresh observer may have a valid successor commit in transport while its
        // clean local execution disagrees with the peer binding.  If height-zero
        // rollback is unavailable, only the authenticated retained-checkpoint path
        // can recover; unrelated execution failures must stay fail-closed.
        let fresh_observer = self.checkpoint_bootstrap_enabled
            && self.committed_height == 0
            && self.replication_persisted_height == 0
            && self.last_execution_height == 0
            && next_height == 1;
        if !fresh_observer || !err.is_fresh_observer_checkpoint_fallback_eligible() {
            return Ok(false);
        }

        let cached_head = self.cached_high_checkpoint_head(world_id);
        // FetchHead is an unsigned, bounded discovery hint only.  It may fill
        // the candidate window when the authenticated peer-head cache is
        // absent, but it never supplies checkpoint authority by itself.
        let fetch_head = if cached_head.is_none() {
            endpoint.lookup_world_head(world_id)?
        } else {
            None
        };
        let Some(candidate_head) = cached_head.as_ref().or(fetch_head.as_ref()) else {
            return Ok(false);
        };
        if candidate_head.height < REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL
            || candidate_head.block_hash.trim().is_empty()
            || candidate_head.state_root.trim().is_empty()
        {
            return Ok(false);
        }
        let now_ms = crate::runtime_util::now_unix_ms();
        let mut retry_pending = false;
        for checkpoint_candidate in
            Self::high_replication_checkpoint_candidates(candidate_head.height, next_height)
        {
            let expected_candidate_head = Some(candidate_head);
            match self.try_sync_high_replication_checkpoint_boundary(
                endpoint,
                node_id,
                world_id,
                replication_runtime,
                checkpoint_candidate,
                next_height,
                expected_candidate_head,
                execution_hook,
                progress_callback,
            ) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(probe_err)
                    if Self::high_replication_checkpoint_probe_can_continue(&probe_err) =>
                {
                    retry_pending = true;
                    self.fresh_observer_checkpoint_bootstrap_retry_pending = true;
                    if self.record_high_replication_checkpoint_probe_failure(
                        next_height,
                        checkpoint_candidate,
                        now_ms,
                        &probe_err,
                    ) {
                        return Ok(false);
                    }
                }
                Err(probe_err) => return Err(probe_err),
            }
        }
        self.fresh_observer_checkpoint_bootstrap_retry_pending = true;
        if retry_pending {
            self.last_replication_gap_sync_repair_attempt_summary = Some(
                "fresh observer execution mismatch retained checkpoint retry pending".to_string(),
            );
        }
        Ok(false)
    }

    pub(super) fn validate_world_head_checkpoint_payload(
        world_id: &str,
        payload: &ReplicationCommitPayload,
        expected_head: &WorldHeadAnnounce,
    ) -> Result<(), NodeError> {
        let payload_state_root = payload.execution_state_root.as_deref().unwrap_or_default();
        if expected_head.world_id != world_id
            || payload.world_id != world_id
            || expected_head.height != payload.height
            || expected_head.block_hash != payload.block_hash
            || expected_head.state_root != payload_state_root
        {
            return Err(NodeError::Replication {
                reason: format!(
                    "world head checkpoint mismatch: world_id={} expected_height={} payload_height={} expected_block_hash={} payload_block_hash={} expected_state_root={} payload_state_root={}",
                    world_id,
                    expected_head.height,
                    payload.height,
                    expected_head.block_hash,
                    payload.block_hash,
                    expected_head.state_root,
                    payload_state_root
                ),
            });
        }
        Ok(())
    }

    pub(super) fn validate_high_checkpoint_lineage_candidate(
        &self,
        world_id: &str,
        payload: &ReplicationCommitPayload,
        checkpoint_descriptor: &NodeExecutionCheckpointDescriptor,
        expected_checkpoint_head: Option<&WorldHeadAnnounce>,
        fresh_execution_bootstrap: bool,
        unsigned_exact_head: bool,
    ) -> bool {
        let Some(execution_block_hash) = payload.execution_block_hash.as_ref() else {
            return false;
        };
        let Some(execution_state_root) = payload.execution_state_root.as_ref() else {
            return false;
        };
        if checkpoint_descriptor.height != payload.height
            || checkpoint_descriptor.execution_block_hash != *execution_block_hash
            || checkpoint_descriptor.execution_state_root != *execution_state_root
        {
            return false;
        }
        let expected_head_matches_candidate = expected_checkpoint_head
            .is_some_and(|expected_head| expected_head.height == payload.height);
        if !(unsigned_exact_head || (fresh_execution_bootstrap && !expected_head_matches_candidate))
        {
            return true;
        }
        let Some(envelope) = payload.lineage_envelope.as_ref() else {
            return true;
        };
        let expected_lineage_head = if unsigned_exact_head {
            let Some(expected_head) = expected_checkpoint_head else {
                return false;
            };
            if envelope.world_id != expected_head.world_id
                || envelope.head.height != expected_head.height
                || envelope.head.block_hash != expected_head.block_hash
                || envelope.head.state_root != expected_head.state_root
            {
                return false;
            }
            envelope.head.clone()
        } else if let Some(discovery_head) = expected_checkpoint_head {
            // FetchHead can constrain the signed envelope to the discovered
            // public H identity, but the envelope votes remain the authority
            // for H and C.  Do not accept a candidate merely because FetchHead
            // reported its height/hash/state.
            if envelope.head.height != discovery_head.height
                || envelope.head.block_hash != discovery_head.block_hash
                || envelope.head.state_root != discovery_head.state_root
            {
                return false;
            }
            envelope.head.clone()
        } else {
            let Some(expected_lineage_head) = self.authenticated_lineage_head_for(&envelope.head)
            else {
                return false;
            };
            expected_lineage_head
        };
        verify_checkpoint_lineage_for_descriptor(
            envelope,
            world_id,
            payload,
            checkpoint_descriptor,
            &expected_lineage_head,
            &self.validators,
            &self.validator_signers,
            self.validator_set_hash.as_str(),
            self.total_stake,
            self.required_stake,
            &self.quarantined_validators,
        )
        .is_ok()
    }

    pub(super) fn authenticated_checkpoint_writer_has_supermajority_stake(
        &self,
        message: &replication::GossipReplicationMessage,
    ) -> bool {
        // The current replication/peer-head payloads do not carry a signed
        // predecessor chain or a finality proof that could bind a retained
        // checkpoint C below an authenticated live head H.  A matching writer
        // key is therefore not sufficient lineage evidence: the writer could
        // have signed a conflicting fork at C.  Keep the lower-candidate path
        // fail-closed whenever any authenticated high peer head is present;
        // the unsigned FetchHead fallback remains eligible only after the
        // checkpoint writer itself passes the validator stake gate below.
        if self.peer_heads.values().any(|head| {
            head.height >= REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL
                && head.public_key_hex.is_some()
                && head.signature_hex.is_some()
        }) {
            return false;
        }
        if self
            .latest_validated_peer_commit
            .as_ref()
            .is_some_and(|commit| {
                commit.height >= REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL
                    && commit.public_key_hex.is_some()
                    && commit.signature_hex.is_some()
                    && self
                        .validator_id_for_peer_head(commit.node_id.as_str())
                        .map(|validator_id| !self.quarantined_validators.contains(&validator_id))
                        .unwrap_or(false)
            })
        {
            return false;
        }
        let Some((validator_id, _public_key_hex)) = self
            .validator_signers
            .iter()
            .find(|(_, public_key_hex)| *public_key_hex == &message.record.writer_id)
        else {
            return false;
        };
        let Some(stake) = self.validators.get(validator_id).copied() else {
            return false;
        };
        if stake < self.required_stake {
            return false;
        }
        true
    }

    pub(super) fn high_replication_checkpoint_candidates(
        advertised_network_height: u64,
        blocked_height: u64,
    ) -> Vec<u64> {
        let mut candidates = Vec::new();
        let mut push_candidate = |height: u64| {
            if height > blocked_height && height > 0 && !candidates.contains(&height) {
                candidates.push(height);
            }
        };
        push_candidate(advertised_network_height);
        for interval in [64_u64, 32_u64] {
            let aligned = advertised_network_height - (advertised_network_height % interval);
            for lookback in 0..=Self::HIGH_REPLICATION_CHECKPOINT_LOOKBACK_WINDOWS {
                push_candidate(aligned.saturating_sub(interval.saturating_mul(lookback)));
            }
        }
        candidates
    }

    pub(super) fn high_replication_checkpoint_probe_can_continue(err: &NodeError) -> bool {
        let fetch_commit_route_error =
            crate::network_bridge::replication_network_error_is_protocol_unavailable(
                err,
                REPLICATION_FETCH_COMMIT_PROTOCOL,
            ) || crate::network_bridge::replication_network_error_is_timeout_protocol(
                err,
                REPLICATION_FETCH_COMMIT_PROTOCOL,
            );
        let checkpoint_blob_missing = match err {
            NodeError::ReplicationNetwork { .. } => Self::typed_checkpoint_blob_not_found(err),
            NodeError::Replication { reason } => {
                crate::network_error_legacy::checkpoint_blob_missing_reason(reason).is_some()
            }
            _ => false,
        };
        fetch_commit_route_error || checkpoint_blob_missing
            || crate::node_engine_replication_provider_route::replication_gap_sync_fetch_blob_rate_limited(err)
    }

    fn typed_checkpoint_blob_not_found(err: &NodeError) -> bool {
        let facts = crate::network_error_classification::replication_network_failure(err);
        facts.cause == oasis7_proto::network_failure::FailureCause::NotFound
            && facts.matches_protocol(crate::replication::REPLICATION_FETCH_BLOB_PROTOCOL)
    }

    pub(super) fn high_replication_checkpoint_closure_missing_reason(
        err: &NodeError,
    ) -> Option<String> {
        match err {
            NodeError::ReplicationNetwork { .. } if Self::typed_checkpoint_blob_not_found(err) => {
                Some(format!(
                    "execution checkpoint blob not found hash=unknown; original_error={err}"
                ))
            }
            NodeError::Replication { reason } => {
                crate::network_error_legacy::checkpoint_blob_missing_reason(reason)
            }
            _ => None,
        }
    }

    pub(super) fn replication_gap_sync_fetch_blob_rate_limited_in_cooldown(
        &self,
        next_height: u64,
        now_ms: i64,
    ) -> bool {
        crate::node_engine_replication_provider_route::replication_gap_sync_fetch_blob_rate_limited_in_cooldown(
            self.last_replication_gap_sync_blocked_height,
            self.last_replication_gap_sync_blocked_reason.as_deref(),
            self.last_replication_gap_sync_blocked_at_ms,
            next_height,
            now_ms,
        )
    }

    pub(super) fn record_high_replication_checkpoint_probe_failure(
        &mut self,
        next_height: u64,
        checkpoint_candidate: u64,
        now_ms: i64,
        err: &NodeError,
    ) -> bool {
        if crate::node_engine_replication_provider_route::replication_gap_sync_fetch_blob_rate_limited(err)
        {
            self.last_replication_gap_sync_blocked_height = Some(next_height);
            self.last_replication_gap_sync_blocked_at_ms = Some(now_ms);
            self.last_replication_gap_sync_blocked_reason = Some(format!(
                "replication checkpoint state sync rate limited at height {next_height}: {err}"
            ));
            return true;
        }
        self.last_replication_gap_sync_repair_attempt_height = Some(checkpoint_candidate);
        self.last_replication_gap_sync_repair_attempt_summary = Some(format!(
            "checkpoint_candidate={checkpoint_candidate} transient_error={err}"
        ));
        self.last_replication_gap_sync_repair_attempt_route_snapshot = None;
        false
    }

    pub(super) fn publish_execution_checkpoint_descriptor_providers(
        endpoint: &ReplicationNetworkEndpoint,
        world_id: &str,
        replication_runtime: &ReplicationRuntime,
        descriptor: &NodeExecutionCheckpointDescriptor,
    ) -> Result<(), NodeError> {
        let closure_refs = std::iter::once((
            descriptor.manifest_ref.as_str(),
            descriptor.manifest_size_bytes,
        ))
        .chain(
            descriptor
                .blobs
                .iter()
                .map(|blob_ref| (blob_ref.content_hash.as_str(), blob_ref.size_bytes)),
        )
        .collect::<Vec<_>>();
        for (content_hash, expected_size_bytes) in &closure_refs {
            let Some(bytes) = replication_runtime.load_blob_by_hash(content_hash)? else {
                return Err(NodeError::Replication {
                    reason: format!(
                        "checkpoint provider publication closure incomplete hash={content_hash}"
                    ),
                });
            };
            if bytes.len() as u64 != *expected_size_bytes {
                return Err(NodeError::Replication {
                    reason: format!(
                        "execution checkpoint provider publish local blob size mismatch hash={} expected={} actual={}",
                        content_hash,
                        expected_size_bytes,
                        bytes.len()
                    ),
                });
            }
            let actual_hash = oasis7_distfs::blake3_hex(bytes.as_slice());
            if actual_hash != *content_hash {
                return Err(NodeError::Replication {
                    reason: format!(
                        "execution checkpoint provider publish local blob hash mismatch expected={} actual={}",
                        content_hash, actual_hash
                    ),
                });
            }
        }
        for (content_hash, _) in closure_refs {
            endpoint.publish_local_content_provider_best_effort(world_id, content_hash);
        }
        Ok(())
    }
}
