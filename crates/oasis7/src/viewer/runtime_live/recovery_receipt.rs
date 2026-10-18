use super::*;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimeAuthoritativeRecoveryGeneration {
    pub(super) schema_version: u32,
    pub(super) ack: AuthoritativeRecoveryAck<u64>,
    pub(super) authoritative_batches: VecDeque<RuntimeAuthoritativeBatchRecord>,
    pub(super) next_authoritative_batch_id: u64,
    pub(super) authoritative_challenges: VecDeque<RuntimeAuthoritativeChallengeRecord>,
    pub(super) next_authoritative_challenge_id: u64,
    pub(super) stable_checkpoints: VecDeque<RuntimeStableCheckpoint>,
    pub(super) reorg_epoch: u64,
    #[serde(default)]
    pub(super) session_policy: RuntimeSessionPolicy,
    #[serde(default)]
    pub(super) session_revoke_metadata: Vec<RuntimePersistedSessionRevokeMetadata>,
    #[serde(default)]
    pub(super) rollback_readiness: BTreeMap<String, RuntimeRollbackReadinessRecord>,
    #[serde(default)]
    pub(super) consumed_strict_audit_nonces: BTreeSet<String>,
    #[serde(default)]
    pub(super) runtime_action_players: BTreeMap<u64, String>,
    #[serde(default)]
    pub(super) consumed_rollback_operator_nonces: BTreeSet<String>,
    #[serde(default)]
    pub(super) session_side_effects: RuntimePersistedSessionSideEffects,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimePersistedSessionSideEffects {
    pub(super) agent_player_bindings: BTreeMap<String, String>,
    pub(super) player_agent_bindings: BTreeMap<String, String>,
    pub(super) agent_public_key_bindings: BTreeMap<String, String>,
    pub(super) player_auth_last_nonce: BTreeMap<String, u64>,
    pub(super) player_chat_intent_acks: Vec<RuntimePersistedChatIntentAck>,
    #[serde(default)]
    pub(super) primary_intents: BTreeMap<String, super::control_plane::RuntimePrimaryIntent>,
    pub(super) pending_virtual_events: VecDeque<WorldEvent>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimePersistedChatIntentAck {
    pub(super) player_id: String,
    pub(super) agent_id: String,
    pub(super) intent_seq: u64,
    pub(super) record: super::control_plane::RuntimeChatIntentAckRecord,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimePersistedSessionRevokeMetadata {
    pub(super) player_id: String,
    pub(super) session_pubkey: String,
    pub(super) metadata: RuntimeSessionRevokeMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimeRollbackReadinessRecord {
    pub(super) canonical_intent_digest: String,
    pub(super) affected_census_digest: String,
    #[serde(default)]
    pub(super) rollback_ticket: String,
    #[serde(default)]
    pub(super) receipt_id: String,
    #[serde(default)]
    pub(super) recovery_snapshot_hash: String,
    #[serde(default)]
    pub(super) reorg_epoch: u64,
    #[serde(default)]
    pub(super) candidate_state_root: String,
    #[serde(default)]
    pub(super) strict_audit_evidence_digest: String,
    #[serde(default)]
    pub(super) strict_audit_observed_at_ms: u64,
    #[serde(default)]
    pub(super) strict_audit_evidence: Option<crate::viewer::protocol::RollbackStrictAuditEvidence>,
    pub(super) ready: bool,
    pub(super) blockers: Vec<String>,
    pub(super) evaluated_at_tick: u64,
}

pub(super) fn current_unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

pub(super) fn rollback_runtime_error(
    err: crate::runtime::WorldError,
    batch_id: String,
) -> AuthoritativeRecoveryError {
    let debug = format!("{err:?}");
    let code = match &err {
        crate::runtime::WorldError::JournalMismatch => "rollback_journal_mismatch",
        crate::runtime::WorldError::RollbackReplayJournalInvalid { .. } => {
            "rollback_replay_journal_invalid"
        }
        crate::runtime::WorldError::RollbackReplayTargetInvalid { .. } => {
            "rollback_replay_target_invalid"
        }
        crate::runtime::WorldError::RollbackTargetStateRootMismatch { .. } => {
            "rollback_target_state_root_mismatch"
        }
        crate::runtime::WorldError::RollbackReconciliationFailed { .. } => {
            "rollback_reconciliation_drift"
        }
        crate::runtime::WorldError::RollbackAttributionResolutionConflict { .. } => {
            "rollback_attribution_resolution_conflict"
        }
        crate::runtime::WorldError::DistributedValidationFailed { reason }
            if reason.contains("reconciliation drift") =>
        {
            "rollback_reconciliation_drift"
        }
        crate::runtime::WorldError::DistributedValidationFailed { .. } => {
            "rollback_authorization_invalid"
        }
        _ => "internal_prepare_failed",
    };
    recovery_error(code, debug, Some(batch_id), None, None)
}

impl ViewerRuntimeLiveServer {
    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn get_authenticated_rollback_receipt(
        &mut self,
        request: crate::viewer::protocol::RollbackReceiptAccessRequest,
    ) -> Result<AuthoritativeRecoveryAck<u64>, AuthoritativeRecoveryError> {
        self.authoritative_recovery_dir().ok_or_else(|| {
            recovery_error(
                "rollback_durable_sink_required",
                "complete receipt access requires a durable authoritative recovery sink",
                None,
                None,
                None,
            )
        })?;
        let payload = request.canonical_signing_payload().map_err(|err| {
            recovery_error(
                "rollback_receipt_authorization_invalid",
                err.to_string(),
                None,
                None,
                None,
            )
        })?;
        self.verify_compensation_operator(&request.authorization, payload.as_slice())
            .map_err(|error| {
                if error.code == "rollback_operator_authorization_replayed"
                    || error.code == "rollback_operator_authorization_expired"
                {
                    error
                } else {
                    recovery_error(
                        "rollback_receipt_authorization_invalid",
                        "active governance signature required for complete rollback receipt",
                        None,
                        None,
                        None,
                    )
                }
            })?;
        let previous_nonces = self.consumed_rollback_operator_nonces.clone();
        self.consumed_rollback_operator_nonces
            .insert(request.authorization.nonce);
        let ack = match self.get_rollback_receipt(request.authorization_nonce) {
            Ok(ack) => ack,
            Err(error) => {
                self.consumed_rollback_operator_nonces = previous_nonces;
                return Err(error);
            }
        };
        if let Err(error) = self.persist_current_recovery_generation(&ack) {
            self.consumed_rollback_operator_nonces = previous_nonces;
            return Err(error);
        }
        Ok(ack)
    }

    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn persist_current_recovery_generation(
        &mut self,
        ack: &AuthoritativeRecoveryAck<u64>,
    ) -> Result<(), AuthoritativeRecoveryError> {
        if self.authoritative_recovery_dir().is_none() {
            return Ok(());
        }
        let metadata = serde_json::to_vec(&RuntimeAuthoritativeRecoveryGeneration {
            schema_version: 2,
            ack: ack.clone(),
            authoritative_batches: self.authoritative_batches.clone(),
            next_authoritative_batch_id: self.next_authoritative_batch_id,
            authoritative_challenges: self.authoritative_challenges.clone(),
            next_authoritative_challenge_id: self.next_authoritative_challenge_id,
            stable_checkpoints: self.stable_checkpoints.clone(),
            reorg_epoch: self.reorg_epoch,
            session_policy: self.session_policy.clone(),
            session_revoke_metadata: self
                .session_revoke_metadata
                .iter()
                .map(|((player_id, session_pubkey), metadata)| {
                    RuntimePersistedSessionRevokeMetadata {
                        player_id: player_id.clone(),
                        session_pubkey: session_pubkey.clone(),
                        metadata: metadata.clone(),
                    }
                })
                .collect(),
            rollback_readiness: self.rollback_readiness.clone(),
            consumed_strict_audit_nonces: self.consumed_strict_audit_nonces.clone(),
            runtime_action_players: self.runtime_action_players.clone(),
            consumed_rollback_operator_nonces: self.consumed_rollback_operator_nonces.clone(),
            session_side_effects: self.persisted_session_side_effects(),
        })
        .map_err(|err| {
            recovery_error(
                "recovery_generation_encode_failed",
                format!("{err:?}"),
                ack.stable_batch_id.clone(),
                ack.player_id.clone(),
                ack.session_pubkey.clone(),
            )
        })?;
        let candidate_world = self.world.clone();
        match self.commit_authoritative_recovery_envelope(&candidate_world, &metadata) {
            Ok(crate::runtime::AuthoritativeRecoveryCommitStatus::Committed(_)) => Ok(()),
            Ok(crate::runtime::AuthoritativeRecoveryCommitStatus::NotCommitted {
                current_generation_id,
            }) => Err(recovery_error(
                "recovery_persistence_commit_failed",
                format!(
                    "Viewer recovery generation was not committed; current_generation_id={current_generation_id:?}"
                ),
                ack.stable_batch_id.clone(),
                ack.player_id.clone(),
                ack.session_pubkey.clone(),
            )),
            Err(crate::runtime::AuthoritativeRecoveryCommitError::StatusUnknown { reason }) => {
                Err(recovery_error(
                    "recovery_persistence_status_unknown",
                    reason,
                    ack.stable_batch_id.clone(),
                    ack.player_id.clone(),
                    ack.session_pubkey.clone(),
                ))
            }
        }
    }

    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn get_rollback_receipt(
        &self,
        authorization_nonce: String,
    ) -> Result<AuthoritativeRecoveryAck<u64>, AuthoritativeRecoveryError> {
        let outcome = self
            .world
            .rollback_nonce_outcome(&authorization_nonce)
            .ok_or_else(|| {
                recovery_error(
                    "rollback_receipt_not_found",
                    format!("no persisted rollback receipt for nonce {authorization_nonce}"),
                    None,
                    None,
                    None,
                )
            })?;
        let target_batch_id = outcome.target_batch_id.clone();
        let receipt = outcome.receipt.as_ref().ok_or_else(|| {
            recovery_error(
                "rollback_receipt_incomplete",
                "rollback outcome has no immutable receipt projection",
                Some(target_batch_id.clone()),
                None,
                None,
            )
        })?;
        let now_ms = current_unix_time_ms();
        let current_state_root = self
            .world
            .execution_state_identity()
            .map(|identity| identity.state_root_hash)
            .ok();
        let readiness = self
            .rollback_readiness
            .get(&authorization_nonce)
            .filter(|record| {
                record.canonical_intent_digest == outcome.canonical_intent_hash
                    && record.affected_census_digest == outcome.affected_census_digest
                    && record.rollback_ticket == outcome.rollback_ticket
                    && record.receipt_id == receipt.receipt_id
                    && record.recovery_snapshot_hash == receipt.snapshot_hash
                    && record.reorg_epoch == outcome.committed_reorg_epoch
                    && record.candidate_state_root == outcome.target_state_root
                    && !record.strict_audit_evidence_digest.is_empty()
                    && current_state_root.as_deref() == Some(outcome.target_state_root.as_str())
                    && self.reorg_epoch == outcome.committed_reorg_epoch
                    && self.world.first_tick_consensus_drift().is_none()
                    && self.world.verify_tick_consensus_chain().is_ok()
                    && outcome.dispositions.iter().all(|entry| {
                        entry.compensation.as_ref().is_none_or(|case| {
                            matches!(
                                case.state,
                                crate::runtime::RollbackCompensationState::Completed
                                    | crate::runtime::RollbackCompensationState::Rejected
                            )
                        })
                    })
                    && record.strict_audit_evidence.as_ref().is_some_and(|audit| {
                        super::recovery_audit::verify_strict_audit_evidence(
                            &self.world,
                            audit,
                            now_ms,
                        )
                        .is_ok()
                    })
            });
        #[cfg(test)]
        if self.recovery_fault_injection
            == Some(super::recovery::RuntimeRecoveryFaultInjection::AckEncode)
        {
            return Err(recovery_error(
                "rollback_ack_encode_failed",
                "injected rollback acknowledgment encoding failure",
                None,
                None,
                None,
            ));
        }
        Ok(AuthoritativeRecoveryAck {
            status: AuthoritativeRecoveryStatus::RolledBack,
            reorg_epoch: outcome.committed_reorg_epoch,
            snapshot_height: receipt.snapshot_height,
            snapshot_hash: receipt.snapshot_hash.clone(),
            log_cursor: receipt.log_cursor,
            stable_batch_id: Some(target_batch_id.clone()),
            player_id: None,
            agent_id: None,
            session_pubkey: None,
            replaced_by_pubkey: None,
            session_epoch: None,
            binding_epoch: None,
            message: Some("persisted rollback receipt".to_string()),
            revoke_reason: None,
            revoked_by: None,
            rollback_receipt: Some(AuthoritativeRollbackReceipt {
                receipt_id: receipt.receipt_id.clone(),
                authorization_nonce: authorization_nonce.clone(),
                rollback_ticket: outcome.rollback_ticket.clone(),
                canonical_intent_digest: outcome.canonical_intent_hash.clone(),
                rollback_checkpoint: Some(crate::viewer::protocol::RollbackCheckpointRef {
                    batch_id: receipt.rollback_checkpoint_batch_id.clone(),
                    snapshot_hash: receipt.rollback_checkpoint_snapshot_hash.clone(),
                    snapshot_journal_len: receipt.rollback_checkpoint_journal_len,
                }),
                replay_target: Some(crate::viewer::protocol::RollbackReplayTarget {
                    batch_id: receipt.replay_target_batch_id.clone(),
                    target_journal_len: receipt.replay_target_journal_len,
                    expected_target_state_root: receipt.replay_target_state_root.clone(),
                    journal_commitment: receipt.replay_target_journal_commitment.clone(),
                }),
                journal_commitment: outcome.target_journal_commitment.clone(),
                target_state_root: outcome.target_state_root.clone(),
                affected_event_census: outcome
                    .dispositions
                    .iter()
                    .map(|entry| crate::viewer::protocol::RollbackSourceEventRef {
                        source_batch_id: entry.source_batch_id.clone(),
                        source_event_id: entry.source_event_id,
                    })
                    .collect(),
                target_batch_id,
                invalidated_batch_ids: outcome.invalidated_batch_ids.clone(),
                prior_reorg_epoch: outcome.prior_reorg_epoch,
                committed_reorg_epoch: outcome.committed_reorg_epoch,
                replay_from_snapshot_height: receipt.snapshot_height,
                replay_from_log_cursor: receipt.log_cursor,
                snapshot_hash: receipt.snapshot_hash.clone(),
                snapshot_reload_required: true,
                player_dispositions: viewer_dispositions(
                    outcome.rollback_ticket.as_str(),
                    authorization_nonce.as_str(),
                    outcome.dispositions.as_slice(),
                ),
                ready_for_all_clear: readiness.is_some_and(|record| record.ready),
                readiness_blockers: readiness
                    .map(|record| record.blockers.clone())
                    .unwrap_or_else(|| {
                        if receipt.readiness_blockers.is_empty() {
                            vec!["readiness_evaluation_required".to_string()]
                        } else {
                            receipt.readiness_blockers.clone()
                        }
                    }),
            }),
            acknowledged_at_tick: receipt.acknowledged_at_tick,
        })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn reevaluate_rollback_readiness(
        &mut self,
        authorization_nonce: String,
        audit_evidence: Option<crate::viewer::protocol::RollbackStrictAuditEvidence>,
    ) -> Result<AuthoritativeRecoveryAck<u64>, AuthoritativeRecoveryError> {
        self.authoritative_recovery_dir().ok_or_else(|| {
            recovery_error(
                "rollback_durable_sink_required",
                "readiness reevaluation requires a durable authoritative recovery sink",
                None,
                None,
                None,
            )
        })?;
        let outcome = self
            .world
            .rollback_nonce_outcome(&authorization_nonce)
            .ok_or_else(|| {
                recovery_error(
                    "rollback_receipt_not_found",
                    format!("no persisted rollback receipt for nonce {authorization_nonce}"),
                    None,
                    None,
                    None,
                )
            })?;
        let receipt = outcome.receipt.as_ref().ok_or_else(|| {
            recovery_error(
                "rollback_receipt_incomplete",
                "rollback outcome has no immutable receipt projection",
                Some(outcome.target_batch_id.clone()),
                None,
                None,
            )
        })?;
        if audit_evidence
            .as_ref()
            .is_some_and(|audit| self.consumed_strict_audit_nonces.contains(&audit.nonce))
        {
            return Err(recovery_error(
                "strict_audit_evidence_replay",
                "strict audit evidence nonce has already been consumed",
                Some(outcome.target_batch_id.clone()),
                None,
                None,
            ));
        }
        let affected_events = outcome
            .dispositions
            .iter()
            .map(|entry| crate::runtime::RollbackSourceEventIdentity {
                source_batch_id: entry.source_batch_id.clone(),
                source_event_id: entry.source_event_id,
            })
            .collect::<Vec<_>>();
        let current_state_root = self
            .world
            .execution_state_identity()
            .map(|identity| identity.state_root_hash)
            .map_err(|err| {
                recovery_error(
                    "rollback_candidate_state_root_unavailable",
                    format!("{err:?}"),
                    Some(outcome.target_batch_id.clone()),
                    None,
                    None,
                )
            })?;
        if let Some(audit) = audit_evidence.as_ref() {
            let valid = super::recovery_audit::verify_strict_audit_evidence(
                &self.world,
                audit,
                current_unix_time_ms(),
            )
            .is_ok()
                && audit.rollback_ticket == outcome.rollback_ticket
                && audit.receipt_id == receipt.receipt_id
                && audit.canonical_intent_digest == outcome.canonical_intent_hash
                && audit.recovery_snapshot_hash == receipt.snapshot_hash
                && audit.reorg_epoch == outcome.committed_reorg_epoch
                && audit.candidate_state_root == current_state_root
                && audit.candidate_state_root == outcome.target_state_root;
            if !valid {
                return Err(recovery_error(
                    "strict_audit_evidence_invalid",
                    "supplied strict audit evidence failed signature, artifact, registry, or receipt binding validation",
                    Some(outcome.target_batch_id.clone()),
                    None,
                    None,
                ));
            }
        }
        let evidence = crate::runtime::RollbackReadinessEvidence {
            target_root_matches: current_state_root == outcome.target_state_root,
            epoch_matches: self.reorg_epoch == outcome.committed_reorg_epoch,
            drift_free: self.world.first_tick_consensus_drift().is_none(),
            consensus_chain_valid: self.world.verify_tick_consensus_chain().is_ok(),
            receipt_retrievable: true,
        };
        let mut blockers = match self.world.rollback_readiness(
            authorization_nonce.as_str(),
            affected_events.as_slice(),
            &evidence,
        ) {
            crate::runtime::RollbackReadiness::Ready => Vec::new(),
            crate::runtime::RollbackReadiness::Blocked { reasons } => reasons,
        };
        if outcome.dispositions.iter().any(|entry| {
            entry.compensation.as_ref().is_some_and(|case| {
                !matches!(
                    case.state,
                    crate::runtime::RollbackCompensationState::Completed
                        | crate::runtime::RollbackCompensationState::Rejected
                )
            })
        }) {
            blockers.push("compensation_cases_unresolved".to_string());
        }
        if outcome.dispositions.iter().any(|entry| {
            entry.action_id.is_some() && entry.player_id.is_none() && !entry.system_authored
        }) {
            blockers.push("player_attribution_incomplete".to_string());
        }
        let valid_audit = audit_evidence.is_some();
        if !valid_audit {
            blockers.push("fresh_strict_audit_evidence_required".to_string());
        }
        let valid_audit_nonce = audit_evidence
            .as_ref()
            .filter(|_| valid_audit)
            .map(|audit| audit.nonce.clone());
        blockers.sort();
        blockers.dedup();
        let record = RuntimeRollbackReadinessRecord {
            canonical_intent_digest: outcome.canonical_intent_hash.clone(),
            affected_census_digest: outcome.affected_census_digest.clone(),
            rollback_ticket: outcome.rollback_ticket.clone(),
            receipt_id: receipt.receipt_id.clone(),
            recovery_snapshot_hash: receipt.snapshot_hash.clone(),
            reorg_epoch: outcome.committed_reorg_epoch,
            candidate_state_root: current_state_root,
            strict_audit_evidence_digest: audit_evidence
                .as_ref()
                .filter(|_| valid_audit)
                .map(|audit| audit.evidence_digest.clone())
                .unwrap_or_default(),
            strict_audit_observed_at_ms: audit_evidence
                .as_ref()
                .filter(|_| valid_audit)
                .map_or(0, |audit| audit.issued_at_ms),
            strict_audit_evidence: audit_evidence.filter(|_| valid_audit),
            ready: blockers.is_empty(),
            blockers: blockers.clone(),
            evaluated_at_tick: self.world.state().time,
        };
        let mut candidate_readiness = self.rollback_readiness.clone();
        candidate_readiness.insert(authorization_nonce.clone(), record);
        let mut candidate_consumed_strict_audit_nonces = self.consumed_strict_audit_nonces.clone();
        if let Some(nonce) = valid_audit_nonce {
            candidate_consumed_strict_audit_nonces.insert(nonce);
        }
        let mut ack = self.get_rollback_receipt(authorization_nonce.clone())?;
        if let Some(public_receipt) = ack.rollback_receipt.as_mut() {
            public_receipt.ready_for_all_clear = blockers.is_empty();
            public_receipt.readiness_blockers = blockers;
        }
        let metadata = serde_json::to_vec(&RuntimeAuthoritativeRecoveryGeneration {
            schema_version: 2,
            ack: ack.clone(),
            authoritative_batches: self.authoritative_batches.clone(),
            next_authoritative_batch_id: self.next_authoritative_batch_id,
            authoritative_challenges: self.authoritative_challenges.clone(),
            next_authoritative_challenge_id: self.next_authoritative_challenge_id,
            stable_checkpoints: self.stable_checkpoints.clone(),
            reorg_epoch: self.reorg_epoch,
            session_policy: self.session_policy.clone(),
            session_revoke_metadata: self
                .session_revoke_metadata
                .iter()
                .map(|((player_id, session_pubkey), metadata)| {
                    RuntimePersistedSessionRevokeMetadata {
                        player_id: player_id.clone(),
                        session_pubkey: session_pubkey.clone(),
                        metadata: metadata.clone(),
                    }
                })
                .collect(),
            rollback_readiness: candidate_readiness.clone(),
            consumed_strict_audit_nonces: candidate_consumed_strict_audit_nonces.clone(),
            runtime_action_players: self.runtime_action_players.clone(),
            consumed_rollback_operator_nonces: self.consumed_rollback_operator_nonces.clone(),
            session_side_effects: self.persisted_session_side_effects(),
        })
        .map_err(|err| {
            recovery_error(
                "rollback_ack_encode_failed",
                format!("{err:?}"),
                Some(outcome.target_batch_id.clone()),
                None,
                None,
            )
        })?;
        let candidate_world = self.world.clone();
        match self.commit_authoritative_recovery_envelope(&candidate_world, &metadata) {
            Ok(crate::runtime::AuthoritativeRecoveryCommitStatus::Committed(_)) => {
                self.rollback_readiness = candidate_readiness;
                self.consumed_strict_audit_nonces = candidate_consumed_strict_audit_nonces;
                Ok(ack)
            }
            Ok(crate::runtime::AuthoritativeRecoveryCommitStatus::NotCommitted {
                current_generation_id,
            }) => Err(recovery_error(
                "rollback_persistence_commit_failed",
                format!(
                    "readiness generation was not committed; current_generation_id={current_generation_id:?}"
                ),
                Some(outcome.target_batch_id),
                None,
                None,
            )),
            Err(crate::runtime::AuthoritativeRecoveryCommitError::StatusUnknown { reason }) => {
                Err(recovery_error(
                    "rollback_persistence_status_unknown",
                    reason,
                    Some(outcome.target_batch_id),
                    None,
                    None,
                ))
            }
        }
    }
}

impl ViewerRuntimeLiveServer {
    fn session_revoke_metadata(
        &self,
        player_id: &str,
        session_pubkey: &str,
    ) -> Option<&RuntimeSessionRevokeMetadata> {
        self.session_revoke_metadata
            .get(&session_revoke_metadata_key(player_id, session_pubkey))
    }

    pub(super) fn recovery_error_from_session_policy(
        &self,
        message: String,
        player_id: String,
        session_pubkey: Option<String>,
    ) -> AuthoritativeRecoveryError {
        let code = map_session_policy_error_code(message.as_str());
        let (revoke_reason, revoked_by) = if code == "session_revoked" {
            session_pubkey
                .as_deref()
                .and_then(|pubkey| self.session_revoke_metadata(player_id.as_str(), pubkey))
                .map(|metadata| (metadata.revoke_reason.clone(), metadata.revoked_by.clone()))
                .unwrap_or((None, None))
        } else {
            (None, None)
        };
        recovery_error_with_revoke_metadata(
            code,
            message,
            None,
            Some(player_id),
            session_pubkey,
            revoke_reason,
            revoked_by,
        )
    }
}

pub(super) fn recovery_error(
    code: impl Into<String>,
    message: impl Into<String>,
    batch_id: Option<String>,
    player_id: Option<String>,
    session_pubkey: Option<String>,
) -> AuthoritativeRecoveryError {
    recovery_error_with_revoke_metadata(
        code,
        message,
        batch_id,
        player_id,
        session_pubkey,
        None,
        None,
    )
}

fn recovery_error_with_revoke_metadata(
    code: impl Into<String>,
    message: impl Into<String>,
    batch_id: Option<String>,
    player_id: Option<String>,
    session_pubkey: Option<String>,
    revoke_reason: Option<String>,
    revoked_by: Option<String>,
) -> AuthoritativeRecoveryError {
    AuthoritativeRecoveryError {
        code: code.into(),
        message: message.into(),
        batch_id,
        player_id,
        session_pubkey,
        revoke_reason,
        revoked_by,
    }
}

pub(super) fn viewer_dispositions(
    rollback_ticket: &str,
    authorization_nonce: &str,
    dispositions: &[crate::runtime::RollbackEventDisposition],
) -> Vec<crate::viewer::protocol::PlayerRollbackDisposition> {
    dispositions
        .iter()
        .map(|entry| {
            let stable_source = format!("{}:{}", entry.source_batch_id, entry.source_event_id);
            let disposition_id = hex::encode(Sha256::digest(format!(
                "oasis7:rollback-disposition:v1\0{stable_source}"
            )));
            let compensation_case_id = entry.compensation.as_ref().map(|case| {
                hex::encode(Sha256::digest(format!(
                    "oasis7:compensation-case:v1\0{rollback_ticket}:{authorization_nonce}:{disposition_id}:{}:{}",
                    case.owner_id, case.ticket_id
                )))
            });
            let compensation = entry.compensation.as_ref().zip(compensation_case_id.as_ref()).map(
                |(case, case_id)| crate::viewer::protocol::PlayerCompensationStatus {
                    case_id: case_id.clone(),
                    responsible_party: "player_support".to_string(),
                    ticket_reference: format!("rollback-case-{}", &case_id[..16]),
                    state: match case.state {
                        crate::runtime::RollbackCompensationState::PendingAuthorization => crate::viewer::protocol::PlayerCompensationState::PendingAuthorization,
                        crate::runtime::RollbackCompensationState::Authorized => crate::viewer::protocol::PlayerCompensationState::Authorized,
                        crate::runtime::RollbackCompensationState::InProgress => crate::viewer::protocol::PlayerCompensationState::InProgress,
                        crate::runtime::RollbackCompensationState::Completed => crate::viewer::protocol::PlayerCompensationState::Completed,
                        crate::runtime::RollbackCompensationState::Rejected => crate::viewer::protocol::PlayerCompensationState::Rejected,
                    },
                },
            );
            crate::viewer::protocol::PlayerRollbackDisposition {
                disposition_id,
                source_batch_id: entry.source_batch_id.clone(),
                source_event_id: entry.source_event_id,
                player_id: entry.player_id.clone(),
                action_id: entry.action_id.clone(),
                system_authored: entry.system_authored,
                disposition: match entry.status {
                    crate::runtime::RollbackDispositionStatus::PreservedAtTarget => {
                        crate::viewer::protocol::PlayerActionDisposition::PreservedAtTarget
                    }
                    crate::runtime::RollbackDispositionStatus::Replayed => {
                        crate::viewer::protocol::PlayerActionDisposition::Replayed
                    }
                    crate::runtime::RollbackDispositionStatus::RejectedFork => {
                        crate::viewer::protocol::PlayerActionDisposition::RejectedFork
                    }
                    crate::runtime::RollbackDispositionStatus::CompensationRequired => {
                        crate::viewer::protocol::PlayerActionDisposition::CompensationRequired
                    }
                },
                compensation_case_id,
                compensation,
            }
        })
        .collect()
}
