//! Governed v2 requests derive every Runtime authorization field from one signed intent.
use super::*;
use oasis7_proto::viewer::RollbackAuthorizationEnvelopeV2;
fn runtime_envelope(
    approval: &RollbackAuthorizationEnvelopeV2,
) -> Result<RollbackAuthorizationEnvelope, WorldError> {
    let intent = &approval.intent;
    if intent.schema_version != 2 {
        return Err(WorldError::DistributedValidationFailed {
            reason: "rollback_v2 requires intent schema_version 2".into(),
        });
    }
    let envelope = RollbackAuthorizationEnvelope {
        intent: crate::RollbackIntent {
            schema_version: intent.schema_version,
            rollback_ticket: intent.rollback_ticket.clone(),
            snapshot_hash: intent.rollback_checkpoint.snapshot_hash.clone(),
            snapshot_journal_len: intent.rollback_checkpoint.snapshot_journal_len,
            target_journal_len: intent.replay_target.target_journal_len,
            target_journal_commitment: Some(intent.replay_target.journal_commitment.clone()),
            expected_target_state_root: intent.replay_target.expected_target_state_root.clone(),
            target_batch_id: Some(intent.replay_target.batch_id.clone()),
            reason: intent.reason.clone(),
            issued_at_ms: intent.issued_at_ms,
            expires_at_ms: intent.expires_at_ms,
            nonce: intent.nonce.clone(),
        },
        signatures: approval
            .signatures
            .iter()
            .map(|signature| crate::RollbackApprovalSignature {
                authority_id: signature.authority_id.clone(),
                role: match signature.role {
                    oasis7_proto::viewer::RollbackAuthorityRole::OnCall => {
                        RollbackAuthorityRole::OnCall
                    }
                    oasis7_proto::viewer::RollbackAuthorityRole::Governance => {
                        RollbackAuthorityRole::Governance
                    }
                },
                signature_scheme: signature.signature_scheme.clone(),
                signature_hex: signature.signature_hex.clone(),
            })
            .collect(),
    };
    Ok(envelope)
}
impl World {
    /// Authenticate an exact committed retry using canonical bytes recomputed by Runtime.
    pub fn is_authenticated_committed_rollback_v2_retry(
        &self,
        approval: &RollbackAuthorizationEnvelopeV2,
        now_ms: u64,
    ) -> Result<bool, WorldError> {
        let envelope = runtime_envelope(approval)?;
        let payload = approval
            .intent
            .canonical_signing_payload()
            .map_err(|e| WorldError::Serde(e.to_string()))?;
        self.authenticate_committed_rollback_retry(&envelope, &payload, now_ms)
    }
    /// Apply one complete governed request. Reason, target and signed payload
    /// cannot be selected independently from the authorization intent.
    pub fn apply_governed_rollback_v2(
        &mut self,
        snapshot: Snapshot,
        journal: Journal,
        retained_target: Snapshot,
        approval: &RollbackAuthorizationEnvelopeV2,
        now_ms: u64,
    ) -> Result<(), WorldError> {
        // The host checks its reorg epoch and retained checkpoint identity.
        // Runtime independently enforces the execution limits signed by authorities.
        let suffix = journal.events.get(snapshot.journal_len..).ok_or_else(|| {
            WorldError::DistributedValidationFailed {
                reason: "rollback checkpoint journal position exceeds replay target".into(),
            }
        })?;
        let replay_bytes =
            serde_json::to_vec(suffix).map_err(|error| WorldError::Serde(error.to_string()))?;
        if suffix.len() > approval.intent.max_replay_events
            || replay_bytes.len() > approval.intent.max_replay_bytes
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: "resolved replay suffix exceeds signed execution bounds".into(),
            });
        }
        let checkpoint_hash = blake3::hash(&serde_json::to_vec(&snapshot)?)
            .to_hex()
            .to_string();
        let target_hash = blake3::hash(&serde_json::to_vec(&retained_target)?)
            .to_hex()
            .to_string();
        let intent = &approval.intent;
        if intent.rollback_checkpoint.snapshot_hash != checkpoint_hash
            || intent.rollback_checkpoint.snapshot_journal_len != snapshot.journal_len
            || intent.replay_target.expected_target_state_root != target_hash
            || intent.replay_target.target_journal_len != journal.len()
            || retained_target.journal_len != journal.len()
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: "signed rollback checkpoint or replay target does not match retained state"
                    .into(),
            });
        }
        // Signed v2 roots identify complete retained snapshots with BLAKE3. The
        // legacy executor instead binds SHA256 snapshot bytes and execution state.
        // Recompute both from these same authenticated inputs; never rewrite the
        // canonical payload that authorities signed.
        let replayed = Self::from_snapshot(snapshot.clone(), journal.clone())?;
        let target = Self::from_snapshot(retained_target, journal.clone())?;
        let execution_root = replayed.current_state_root_hash()?;
        if execution_root != target.current_state_root_hash()? {
            return Err(WorldError::DistributedValidationFailed {
                reason: "signed replay target state differs from replayed journal".into(),
            });
        }
        let mut envelope = runtime_envelope(approval)?;
        envelope.intent.snapshot_hash = crate::util::hash_json(&snapshot)?;
        envelope.intent.expected_target_state_root = execution_root;
        let payload = approval
            .intent
            .canonical_signing_payload()
            .map_err(|e| WorldError::Serde(e.to_string()))?;
        self.rollback_to_snapshot_with_reconciliation_v2(
            snapshot,
            journal,
            approval.intent.reason.clone(),
            Some(approval.intent.replay_target.batch_id.as_str()),
            envelope,
            &payload,
            now_ms,
        )
    }
}
