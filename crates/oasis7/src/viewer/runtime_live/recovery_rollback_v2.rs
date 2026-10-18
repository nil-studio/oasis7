use super::authoritative::compute_runtime_snapshot_hash;
use super::recovery_receipt::recovery_error;
use super::*;
use sha2::{Digest, Sha256};

impl ViewerRuntimeLiveServer {
    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn rollback_to_stable_checkpoint_v2(
        &mut self,
        request: AuthoritativeRollbackRequest,
        canonical_v2_payload: Vec<u8>,
    ) -> Result<AuthoritativeRecoveryAck<u64>, AuthoritativeRecoveryError> {
        self.rollback_to_stable_checkpoint_payload(request, Some(canonical_v2_payload))
    }

    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn rollback_v2_to_replay_target(
        &mut self,
        request: AuthoritativeRollbackV2Request,
    ) -> Result<AuthoritativeRecoveryAck<u64>, AuthoritativeRecoveryError> {
        let retry_approval = request.approval.clone();
        let intent = request.approval.intent;
        if intent.schema_version != 2 {
            return Err(recovery_error(
                "rollback_schema_unsupported",
                "rollback_v2 requires intent schema_version 2",
                Some(intent.replay_target.batch_id),
                None,
                None,
            ));
        }
        let requested_reason = request.reason.trim();
        let rollback_reason = if requested_reason.is_empty() {
            "authoritative_recovery_rollback"
        } else {
            requested_reason
        };
        if intent.reason != rollback_reason {
            return Err(recovery_error(
                "rollback_authorization_invalid",
                "outer rollback request reason does not match the signed intent reason",
                Some(intent.replay_target.batch_id),
                None,
                None,
            ));
        }
        let canonical_v2_payload = intent.canonical_signing_payload().map_err(|err| {
            recovery_error(
                "rollback_intent_encode_failed",
                format!("canonical rollback intent encoding failed: {err}"),
                Some(intent.replay_target.batch_id.clone()),
                None,
                None,
            )
        })?;

        if self
            .world
            .is_authenticated_committed_rollback_v2_retry(
                &retry_approval,
                super::recovery_receipt::current_unix_time_ms(),
            )
            .map_err(|err| {
                super::recovery_receipt::rollback_runtime_error(
                    err,
                    intent.replay_target.batch_id.clone(),
                )
            })?
        {
            return self.get_rollback_receipt(intent.nonce);
        }
        if intent.expected_reorg_epoch != self.reorg_epoch {
            return Err(recovery_error(
                "rollback_reorg_epoch_mismatch",
                "rollback intent expected a different reorg epoch",
                Some(intent.replay_target.batch_id),
                None,
                None,
            ));
        }
        let checkpoint_index = self
            .stable_checkpoints
            .iter()
            .position(|checkpoint| checkpoint.batch_id == intent.rollback_checkpoint.batch_id);
        let target_index = self
            .stable_checkpoints
            .iter()
            .position(|checkpoint| checkpoint.batch_id == intent.replay_target.batch_id);
        if matches!((checkpoint_index, target_index), (Some(checkpoint), Some(target)) if checkpoint >= target)
        {
            return Err(recovery_error(
                "rollback_target_precedes_checkpoint",
                "replay target must be a finalized checkpoint strictly after rollback checkpoint",
                Some(intent.replay_target.batch_id),
                None,
                None,
            ));
        }
        if self.authoritative_recovery_dir().is_none() {
            return Err(recovery_error(
                "rollback_durable_sink_required",
                "governed rollback requires a durable authoritative recovery sink",
                Some(intent.replay_target.batch_id.clone()),
                None,
                None,
            ));
        }
        let checkpoint_index = checkpoint_index.ok_or_else(|| {
            recovery_error(
                "rollback_checkpoint_not_found",
                "signed rollback checkpoint is not retained",
                Some(intent.rollback_checkpoint.batch_id.clone()),
                None,
                None,
            )
        })?;
        let target_index = target_index.ok_or_else(|| {
            recovery_error(
                "stable_checkpoint_not_found",
                "signed replay target is not retained as finalized",
                Some(intent.replay_target.batch_id.clone()),
                None,
                None,
            )
        })?;
        let rollback_checkpoint = &self.stable_checkpoints[checkpoint_index];
        let replay_target = &self.stable_checkpoints[target_index];
        let signed_checkpoint_hash = compute_runtime_snapshot_hash(&rollback_checkpoint.snapshot)
            .map_err(|err| {
            recovery_error(
                "rollback_checkpoint_mismatch",
                format!("compute rollback checkpoint hash failed: {err:?}"),
                Some(intent.rollback_checkpoint.batch_id.clone()),
                None,
                None,
            )
        })?;
        let signed_target_state_root = compute_runtime_snapshot_hash(&replay_target.snapshot)
            .map_err(|err| {
                recovery_error(
                    "rollback_authorization_invalid",
                    format!("compute replay target state root failed: {err:?}"),
                    Some(intent.replay_target.batch_id.clone()),
                    None,
                    None,
                )
            })?;
        if intent.rollback_checkpoint.snapshot_hash != signed_checkpoint_hash
            || intent.rollback_checkpoint.snapshot_journal_len
                != rollback_checkpoint.snapshot.journal_len
            || intent.replay_target.expected_target_state_root != signed_target_state_root
            || intent.replay_target.target_journal_len != replay_target.journal.len()
        {
            return Err(recovery_error(
                "rollback_authorization_invalid",
                "signed rollback checkpoint or replay target does not match retained state",
                Some(intent.replay_target.batch_id.clone()),
                None,
                None,
            ));
        }
        let runtime_snapshot_hash = hex::encode(Sha256::digest(
            serde_json::to_vec(&rollback_checkpoint.snapshot).map_err(|err| {
                recovery_error(
                    "rollback_intent_encode_failed",
                    format!("encode rollback checkpoint failed: {err}"),
                    Some(intent.rollback_checkpoint.batch_id.clone()),
                    None,
                    None,
                )
            })?,
        ));
        let runtime_target_state_root = crate::runtime::World::from_snapshot(
            rollback_checkpoint.snapshot.clone(),
            replay_target.journal.clone(),
        )
        .and_then(|world| {
            world
                .execution_state_identity()
                .map(|identity| identity.state_root_hash)
        })
        .map_err(|err| {
            super::recovery_receipt::rollback_runtime_error(
                err,
                intent.replay_target.batch_id.clone(),
            )
        })?;
        let signatures = request.approval.signatures;
        let target_batch_id = intent.replay_target.batch_id.clone();
        let legacy = RollbackIntent {
            schema_version: intent.schema_version,
            rollback_ticket: intent.rollback_ticket,
            snapshot_hash: runtime_snapshot_hash,
            snapshot_journal_len: intent.rollback_checkpoint.snapshot_journal_len,
            target_journal_len: intent.replay_target.target_journal_len,
            expected_target_state_root: runtime_target_state_root,
            target_batch_id: Some(target_batch_id.clone()),
            reason: intent.reason,
            issued_at_ms: intent.issued_at_ms,
            expires_at_ms: intent.expires_at_ms,
            nonce: intent.nonce,
            rollback_checkpoint: Some(intent.rollback_checkpoint),
            replay_target: Some(intent.replay_target),
            expected_reorg_epoch: Some(intent.expected_reorg_epoch),
            max_replay_events: Some(intent.max_replay_events),
            max_replay_bytes: Some(intent.max_replay_bytes),
        };
        self.rollback_to_stable_checkpoint_v2(
            AuthoritativeRollbackRequest {
                target_batch_id: Some(target_batch_id),
                reason: request.reason,
                requested_by: None,
                approval: Some(RollbackAuthorizationEnvelope {
                    intent: legacy,
                    signatures,
                }),
            },
            canonical_v2_payload,
        )
    }
}
