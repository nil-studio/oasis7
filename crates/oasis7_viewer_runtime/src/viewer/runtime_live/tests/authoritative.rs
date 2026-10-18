use super::*;
use crate::viewer::protocol::{
    RollbackApprovalSignature, RollbackAuthorityRole, RollbackAuthorizationEnvelope, RollbackIntent,
};
use crate::viewer::runtime_live::authoritative::{
    RuntimeBatchChallengeState, compute_runtime_snapshot_hash, is_valid_root_hash,
};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_proto::viewer::{
    AuthoritativeRollbackV2Request, RollbackAuthorizationEnvelopeV2, RollbackCheckpointRef,
    RollbackIntentV2, RollbackReplayTarget,
};
use sha2::{Digest, Sha256};

#[path = "authoritative_receipt.rs"]
mod receipt_tests;

#[path = "authoritative_receipt_v11.rs"]
mod receipt_v11_tests;

#[path = "authoritative_receipt_v12.rs"]
mod receipt_v12_tests;

#[path = "authoritative_receipt_v13.rs"]
mod receipt_v13_tests;

#[path = "authoritative_receipt_v15_atomicity.rs"]
mod receipt_v15_atomicity_tests;

#[path = "authoritative_rollback_v2_exact_retry_tests.rs"]
mod rollback_v2_exact_retry_tests;

#[path = "authoritative_recovery_tail.rs"]
mod authoritative_recovery_tail_tests;

fn configure_rollback_authorities(
    server: &mut ViewerRuntimeLiveServer,
) -> (SigningKey, SigningKey) {
    let on_call = SigningKey::from_bytes(&[41; 32]);
    let governance = SigningKey::from_bytes(&[73; 32]);
    server
        .world
        .set_rollback_authority_registry(
            crate::runtime::RollbackAuthorityRegistry::new([
                crate::runtime::RollbackAuthorityRecord {
                    authority_id: "on-call-alice".to_string(),
                    role: crate::runtime::RollbackAuthorityRole::OnCall,
                    public_key_hex: hex::encode(on_call.verifying_key().to_bytes()),
                    active: true,
                },
                crate::runtime::RollbackAuthorityRecord {
                    authority_id: "governance-bob".to_string(),
                    role: crate::runtime::RollbackAuthorityRole::Governance,
                    public_key_hex: hex::encode(governance.verifying_key().to_bytes()),
                    active: true,
                },
            ])
            .expect("registry"),
        )
        .expect("configure rollback authorities");
    (on_call, governance)
}

fn signed_rollback_envelope(
    server: &ViewerRuntimeLiveServer,
    batch_id: &str,
    reason: &str,
    nonce: &str,
    on_call: &SigningKey,
    governance: &SigningKey,
) -> RollbackAuthorizationEnvelope {
    let checkpoint = server
        .stable_checkpoints
        .iter()
        .find(|checkpoint| checkpoint.batch_id == batch_id)
        .expect("stable checkpoint");
    let snapshot = &checkpoint.snapshot;
    let target_state_root = crate::runtime::World::from_snapshot(
        checkpoint.snapshot.clone(),
        checkpoint.journal.clone(),
    )
    .expect("reconstruct authoritative target")
    .execution_state_identity()
    .expect("target state root")
    .state_root_hash;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_millis() as u64;
    let intent = RollbackIntent {
        schema_version: 1,
        rollback_ticket: "ROLLBACK-2313".to_string(),
        snapshot_hash: hex::encode(Sha256::digest(
            serde_json::to_vec(snapshot).expect("snapshot json"),
        )),
        snapshot_journal_len: snapshot.journal_len,
        target_journal_len: checkpoint.journal.len(),
        expected_target_state_root: target_state_root,
        target_batch_id: Some(batch_id.to_string()),
        reason: reason.to_string(),
        issued_at_ms: now_ms.saturating_sub(1_000),
        expires_at_ms: now_ms.saturating_add(60_000),
        nonce: nonce.to_string(),
        rollback_checkpoint: None,
        replay_target: None,
        expected_reorg_epoch: None,
        max_replay_events: None,
        max_replay_bytes: None,
    };
    let runtime_intent = crate::runtime::RollbackIntent {
        schema_version: intent.schema_version,
        rollback_ticket: intent.rollback_ticket.clone(),
        snapshot_hash: intent.snapshot_hash.clone(),
        snapshot_journal_len: intent.snapshot_journal_len,
        target_journal_len: intent.target_journal_len,
        target_journal_commitment: None,
        expected_target_state_root: intent.expected_target_state_root.clone(),
        target_batch_id: intent.target_batch_id.clone(),
        reason: intent.reason.clone(),
        issued_at_ms: intent.issued_at_ms,
        expires_at_ms: intent.expires_at_ms,
        nonce: intent.nonce.clone(),
    };
    let payload = runtime_intent
        .canonical_signing_payload()
        .expect("signing payload");
    RollbackAuthorizationEnvelope {
        intent,
        signatures: vec![
            RollbackApprovalSignature {
                authority_id: "on-call-alice".to_string(),
                role: RollbackAuthorityRole::OnCall,
                signature_scheme: "ed25519".to_string(),
                signature_hex: hex::encode(on_call.sign(&payload).to_bytes()),
            },
            RollbackApprovalSignature {
                authority_id: "governance-bob".to_string(),
                role: RollbackAuthorityRole::Governance,
                signature_scheme: "ed25519".to_string(),
                signature_hex: hex::encode(governance.sign(&payload).to_bytes()),
            },
        ],
    }
}

fn commit_single_authoritative_batch(
    server: &mut ViewerRuntimeLiveServer,
) -> AuthoritativeBatchFinality {
    let journal_start = server.world.journal().events.len();
    server.script.enqueue(&mut server.world);
    server.world.step().expect("runtime step");

    let mut mapped_events = Vec::new();
    for runtime_event in &server.world.journal().events[journal_start..] {
        mapped_events.push(map_runtime_event(
            runtime_event,
            &server.snapshot_config,
            server.seed_model.as_ref(),
        ));
    }
    mapped_events.extend(server.pending_virtual_events.drain(..));

    server
        .register_authoritative_batch(mapped_events.as_slice())
        .expect("register authoritative batch")
}

#[test]
fn runtime_authoritative_batch_commit_records_required_roots() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");

    let batch = commit_single_authoritative_batch(&mut server);
    assert_eq!(batch.finality_state, AuthoritativeFinalityState::Pending);
    assert!(!batch.batch_id.is_empty());
    assert!(is_valid_root_hash(batch.state_root.as_str()));
    assert!(is_valid_root_hash(batch.data_root.as_str()));
    assert_eq!(server.authoritative_batches.len(), 1);
}

#[test]
fn runtime_authoritative_batch_finality_is_monotonic_and_final_only_gates_settlement() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");

    let pending = commit_single_authoritative_batch(&mut server);
    assert!(!pending.settlement_ready);
    assert!(!pending.ranking_ready);

    let confirmed_updates = server
        .advance_authoritative_batch_finality(pending.confirm_height)
        .expect("advance to confirmed");
    assert_eq!(confirmed_updates.len(), 1);
    let confirmed = &confirmed_updates[0];
    assert_eq!(
        confirmed.finality_state,
        AuthoritativeFinalityState::Confirmed
    );
    assert!(!confirmed.settlement_ready);
    assert!(!confirmed.ranking_ready);

    let final_updates = server
        .advance_authoritative_batch_finality(pending.final_height)
        .expect("advance to final");
    assert_eq!(final_updates.len(), 1);
    let final_update = &final_updates[0];
    assert_eq!(
        final_update.finality_state,
        AuthoritativeFinalityState::Final
    );
    assert!(final_update.settlement_ready);
    assert!(final_update.ranking_ready);

    let no_regression = server
        .advance_authoritative_batch_finality(pending.confirm_height)
        .expect("finality should be monotonic");
    assert!(no_regression.is_empty());
    let stored = server.authoritative_batches.back().expect("stored batch");
    assert_eq!(stored.finality_state, AuthoritativeFinalityState::Final);
}

#[test]
fn runtime_authoritative_batch_data_root_mismatch_blocks_confirmation() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");

    let pending = commit_single_authoritative_batch(&mut server);
    let tampered_root = "f".repeat(64);
    let batch = server
        .authoritative_batches
        .back_mut()
        .expect("stored batch for tamper");
    batch.data_root = tampered_root;

    let updates = server
        .advance_authoritative_batch_finality(pending.final_height.saturating_add(10))
        .expect("advance finality");
    assert!(updates.is_empty());

    let stored = server.authoritative_batches.back().expect("stored batch");
    assert_eq!(stored.finality_state, AuthoritativeFinalityState::Pending);
    let wire = stored.as_wire(&server.settlement_ranking_gate);
    assert!(!wire.settlement_ready);
    assert!(!wire.ranking_ready);
}

#[test]
fn runtime_authoritative_challenge_submit_opens_challenge_and_blocks_finality_progress() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");

    let pending = commit_single_authoritative_batch(&mut server);
    let (_, maybe_batch_update) = server
        .handle_authoritative_challenge(AuthoritativeChallengeCommand::Submit {
            request: AuthoritativeChallengeSubmitRequest {
                batch_id: pending.batch_id.clone(),
                watcher_id: "watcher-1".to_string(),
                recomputed_state_root: pending.state_root.clone(),
                recomputed_data_root: pending.data_root.clone(),
                challenge_id: Some("challenge-1".to_string()),
            },
        })
        .expect("submit challenge");
    let batch_update = maybe_batch_update.expect("batch update");
    assert!(batch_update.challenge_open);
    assert_eq!(
        batch_update.active_challenge_id.as_deref(),
        Some("challenge-1")
    );

    let updates = server
        .advance_authoritative_batch_finality(pending.final_height.saturating_add(10))
        .expect("advance while challenged");
    assert!(updates.is_empty());
    let stored = server.authoritative_batches.back().expect("stored batch");
    assert_ne!(stored.finality_state, AuthoritativeFinalityState::Final);
}

#[test]
fn runtime_authoritative_challenge_resolve_no_fraud_unblocks_finality_without_slash() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");

    let pending = commit_single_authoritative_batch(&mut server);
    let (submit_ack, _) = server
        .handle_authoritative_challenge(AuthoritativeChallengeCommand::Submit {
            request: AuthoritativeChallengeSubmitRequest {
                batch_id: pending.batch_id.clone(),
                watcher_id: "watcher-2".to_string(),
                recomputed_state_root: pending.state_root.clone(),
                recomputed_data_root: pending.data_root.clone(),
                challenge_id: None,
            },
        })
        .expect("submit challenge");
    assert_eq!(submit_ack.status, AuthoritativeChallengeStatus::Challenged);

    let (resolve_ack, maybe_batch_update) = server
        .handle_authoritative_challenge(AuthoritativeChallengeCommand::Resolve {
            request: AuthoritativeChallengeResolveRequest {
                challenge_id: submit_ack.challenge_id.clone(),
                resolved_by: Some("arbiter-1".to_string()),
            },
        })
        .expect("resolve challenge");
    assert_eq!(
        resolve_ack.status,
        AuthoritativeChallengeStatus::ResolvedNoFraud
    );
    assert!(!resolve_ack.slash_applied);
    let batch_update = maybe_batch_update.expect("batch update");
    assert!(!batch_update.challenge_open);
    assert!(!batch_update.slashed);

    let final_updates = server
        .advance_authoritative_batch_finality(pending.final_height)
        .expect("advance after resolve");
    assert!(final_updates.iter().any(|update| {
        update.batch_id == pending.batch_id
            && update.finality_state == AuthoritativeFinalityState::Final
            && !update.slashed
    }));
}

#[test]
fn runtime_authoritative_challenge_resolve_fraud_slashes_and_blocks_finality() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");

    let pending = commit_single_authoritative_batch(&mut server);
    let (submit_ack, _) = server
        .handle_authoritative_challenge(AuthoritativeChallengeCommand::Submit {
            request: AuthoritativeChallengeSubmitRequest {
                batch_id: pending.batch_id.clone(),
                watcher_id: "watcher-3".to_string(),
                recomputed_state_root: "f".repeat(64),
                recomputed_data_root: pending.data_root.clone(),
                challenge_id: None,
            },
        })
        .expect("submit challenge");

    let (resolve_ack, maybe_batch_update) = server
        .handle_authoritative_challenge(AuthoritativeChallengeCommand::Resolve {
            request: AuthoritativeChallengeResolveRequest {
                challenge_id: submit_ack.challenge_id,
                resolved_by: Some("arbiter-1".to_string()),
            },
        })
        .expect("resolve challenge");
    assert_eq!(
        resolve_ack.status,
        AuthoritativeChallengeStatus::ResolvedFraudSlashed
    );
    assert!(resolve_ack.slash_applied);
    assert_eq!(
        resolve_ack.slash_reason.as_deref(),
        Some("state_root_mismatch")
    );
    let batch_update = maybe_batch_update.expect("batch update");
    assert!(batch_update.slashed);
    assert!(!batch_update.challenge_open);

    let updates = server
        .advance_authoritative_batch_finality(pending.final_height.saturating_add(10))
        .expect("advance after slash");
    assert!(
        updates
            .iter()
            .all(|update| update.batch_id != pending.batch_id)
    );
    let stored = server.authoritative_batches.back().expect("stored batch");
    assert_eq!(
        stored.challenge_state,
        RuntimeBatchChallengeState::ResolvedFraudSlashed
    );
    assert_ne!(stored.finality_state, AuthoritativeFinalityState::Final);
}

#[test]
fn runtime_authoritative_challenge_duplicate_resolve_is_rejected() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");

    let pending = commit_single_authoritative_batch(&mut server);
    let (submit_ack, _) = server
        .handle_authoritative_challenge(AuthoritativeChallengeCommand::Submit {
            request: AuthoritativeChallengeSubmitRequest {
                batch_id: pending.batch_id,
                watcher_id: "watcher-4".to_string(),
                recomputed_state_root: pending.state_root,
                recomputed_data_root: pending.data_root,
                challenge_id: Some("challenge-dup".to_string()),
            },
        })
        .expect("submit challenge");
    let _ = server
        .handle_authoritative_challenge(AuthoritativeChallengeCommand::Resolve {
            request: AuthoritativeChallengeResolveRequest {
                challenge_id: submit_ack.challenge_id.clone(),
                resolved_by: None,
            },
        })
        .expect("first resolve");

    let err = server
        .handle_authoritative_challenge(AuthoritativeChallengeCommand::Resolve {
            request: AuthoritativeChallengeResolveRequest {
                challenge_id: submit_ack.challenge_id,
                resolved_by: None,
            },
        })
        .expect_err("duplicate resolve should reject");
    assert_eq!(err.code, "challenge_already_resolved");
}

#[test]
fn runtime_authoritative_recovery_rollback_prunes_fork_batches() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");

    let first = commit_single_authoritative_batch(&mut server);
    let updates = server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    assert!(updates.iter().any(|batch| {
        batch.batch_id == first.batch_id
            && batch.finality_state == AuthoritativeFinalityState::Final
    }));
    assert_eq!(server.stable_checkpoints.len(), 1);

    let second = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "test_reorg",
        "viewer-valid-1",
        &on_call,
        &governance,
    );
    assert_eq!(server.authoritative_batches.len(), 2);
    assert_eq!(server.authoritative_batches[1].batch_id, second.batch_id);

    let (ack, emit_snapshot_after_ack) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id.clone()),
                reason: "test_reorg".to_string(),
                requested_by: Some("ops".to_string()),
                approval: Some(approval),
            },
        })
        .expect("rollback to first stable batch");
    assert!(emit_snapshot_after_ack);
    assert_eq!(ack.status, AuthoritativeRecoveryStatus::RolledBack);
    assert_eq!(
        ack.stable_batch_id.as_deref(),
        Some(first.batch_id.as_str())
    );
    assert_eq!(server.reorg_epoch, 1);
    assert_eq!(server.authoritative_batches.len(), 1);
    assert_eq!(server.authoritative_batches[0].batch_id, first.batch_id);
}

#[test]
fn runtime_authoritative_recovery_receipt_is_immutable_after_later_progress() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-immutable-receipt-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    server.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    let _second = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let nonce = "viewer-immutable-receipt-1";
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "immutable-receipt",
        nonce,
        &on_call,
        &governance,
    );
    let (committed, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "immutable-receipt".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("commit rollback");
    let committed_receipt = committed
        .rollback_receipt
        .expect("committed rollback receipt");

    server.world.step().expect("later world progress");
    let now_ms = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    let mut request = crate::viewer::protocol::RollbackReceiptAccessRequest {
        authorization_nonce: nonce.to_string(),
        authorization: crate::viewer::protocol::RollbackOperatorAuthorization {
            authority_id: "governance-bob".to_string(),
            issued_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(60_000),
            nonce: "immutable-receipt-read-1".to_string(),
            signature_scheme: "ed25519".to_string(),
            signature_hex: String::new(),
        },
    };
    request.authorization.signature_hex = hex::encode(
        governance
            .sign(
                &request
                    .canonical_signing_payload()
                    .expect("receipt payload"),
            )
            .to_bytes(),
    );
    let (retried, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::GetRollbackReceipt { request })
        .expect("retrieve persisted receipt after progress");

    assert_eq!(
        retried.rollback_receipt.as_ref(),
        Some(&committed_receipt),
        "later world progress must not rewrite any commit-time receipt field"
    );
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn runtime_authoritative_recovery_rollback_rejects_missing_approval_before_mutation() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    let second = commit_single_authoritative_batch(&mut server);
    let state_before = server.world.state().clone();
    let journal_before = server.world.journal().clone();
    let batch_ids_before = server
        .authoritative_batches
        .iter()
        .map(|batch| batch.batch_id.clone())
        .collect::<Vec<_>>();
    let reorg_epoch_before = server.reorg_epoch;

    let err = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "missing-approval".to_string(),
                requested_by: Some("ops".to_string()),
                approval: None,
            },
        })
        .expect_err("viewer recovery must require rollback approval evidence");

    assert_eq!(err.code, "rollback_approval_required");
    assert_eq!(server.world.state(), &state_before);
    assert_eq!(server.world.journal(), &journal_before);
    assert_eq!(server.authoritative_batches.len(), batch_ids_before.len());
    assert_eq!(
        server
            .authoritative_batches
            .iter()
            .map(|batch| batch.batch_id.as_str())
            .collect::<Vec<_>>(),
        batch_ids_before
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    assert_eq!(server.reorg_epoch, reorg_epoch_before);
    assert_eq!(server.authoritative_batches[1].batch_id, second.batch_id);
}

#[test]
fn runtime_authoritative_recovery_rollback_rejects_tampered_envelope_before_mutation() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    let _second = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let mut approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "tampered-approval",
        "viewer-tampered-1",
        &on_call,
        &governance,
    );
    approval.intent.reason = "changed-after-signing".to_string();
    let state_before = server.world.state().clone();
    let journal_before = server.world.journal().clone();
    let batch_count_before = server.authoritative_batches.len();
    let reorg_epoch_before = server.reorg_epoch;

    let err = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "changed-after-signing".to_string(),
                requested_by: Some("ops".to_string()),
                approval: Some(approval),
            },
        })
        .expect_err("tampered signed intent must fail closed");

    assert_eq!(err.code, "rollback_authorization_invalid");
    assert_eq!(server.world.state(), &state_before);
    assert_eq!(server.world.journal(), &journal_before);
    assert_eq!(server.authoritative_batches.len(), batch_count_before);
    assert_eq!(server.reorg_epoch, reorg_epoch_before);
}

#[test]
fn runtime_authoritative_recovery_rejects_tampered_signed_replay_target_fields() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    let _second = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let signed = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "tampered-replay-target",
        "viewer-tampered-target-1",
        &on_call,
        &governance,
    );
    let world_before = server.world.snapshot();
    let batches_before = server.authoritative_batches.len();
    let epoch_before = server.reorg_epoch;

    for (index, mut approval) in [signed.clone(), signed].into_iter().enumerate() {
        if index == 0 {
            approval.intent.target_journal_len =
                approval.intent.target_journal_len.saturating_add(1);
        } else {
            approval.intent.expected_target_state_root = "f".repeat(64);
        }
        let err = server
            .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
                request: AuthoritativeRollbackRequest {
                    target_batch_id: Some(first.batch_id.clone()),
                    reason: "tampered-replay-target".to_string(),
                    requested_by: Some("ops".to_string()),
                    approval: Some(approval),
                },
            })
            .expect_err("signed replay target tampering must fail closed");
        assert_eq!(err.code, "rollback_authorization_invalid");
    }

    assert_eq!(server.world.snapshot(), world_before);
    assert_eq!(server.authoritative_batches.len(), batches_before);
    assert_eq!(server.reorg_epoch, epoch_before);
}

#[test]
fn runtime_authoritative_recovery_classifies_journal_mismatch_separately_from_authorization() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    let _second = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "journal-mismatch",
        "viewer-journal-mismatch-1",
        &on_call,
        &governance,
    );
    let checkpoint = server
        .stable_checkpoints
        .iter_mut()
        .find(|checkpoint| checkpoint.batch_id == first.batch_id)
        .expect("stable checkpoint");
    checkpoint.snapshot.journal_len = checkpoint.journal.len().saturating_add(1);
    let state_before = server.world.state().clone();
    let journal_before = server.world.journal().clone();
    let batches_before = server.authoritative_batches.clone();
    let reorg_epoch_before = server.reorg_epoch;

    let err = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "journal-mismatch".to_string(),
                requested_by: Some("ops".to_string()),
                approval: Some(approval),
            },
        })
        .expect_err("journal mismatch must fail before mutation");

    assert_eq!(err.code, "rollback_journal_mismatch");
    assert_eq!(server.world.state(), &state_before);
    assert_eq!(server.world.journal(), &journal_before);
    assert_eq!(server.authoritative_batches.len(), batches_before.len());
    assert_eq!(server.reorg_epoch, reorg_epoch_before);
}

#[test]
fn runtime_authoritative_recovery_requires_v2_negotiated_signed_rollback_capability() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    let _second = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "protocol-v2-required",
        "viewer-protocol-v2-1",
        &on_call,
        &governance,
    );
    let command = AuthoritativeRecoveryCommand::Rollback {
        request: AuthoritativeRollbackRequest {
            target_batch_id: Some(first.batch_id),
            reason: "protocol-v2-required".to_string(),
            requested_by: Some("ops".to_string()),
            approval: Some(approval),
        },
    };
    let world_before = server.world.snapshot();
    let batch_count_before = server.authoritative_batches.len();
    let epoch_before = server.reorg_epoch;

    let err = server
        .handle_authoritative_recovery_for_protocol(
            command,
            &crate::viewer::protocol::NegotiatedViewerProtocol::v1_without_capabilities(),
        )
        .expect_err("v1 session must not reach signed rollback execution");

    assert_eq!(err.code, "protocol_upgrade_required");
    assert_eq!(server.world.snapshot(), world_before);
    assert_eq!(server.authoritative_batches.len(), batch_count_before);
    assert_eq!(server.reorg_epoch, epoch_before);
}

#[test]
fn runtime_authoritative_recovery_prepare_faults_are_atomic_and_classified() {
    use crate::viewer::runtime_live::recovery::RuntimeRecoveryFaultInjection;

    for (fault, expected_code) in [
        (
            RuntimeRecoveryFaultInjection::ReconciliationDrift,
            "rollback_reconciliation_drift",
        ),
        (
            RuntimeRecoveryFaultInjection::CursorCompute,
            "cursor_compute_failed",
        ),
        (
            RuntimeRecoveryFaultInjection::AckEncode,
            "rollback_ack_encode_failed",
        ),
    ] {
        let mut server = ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(
            WorldScenario::Minimal,
        ))
        .expect("runtime server");
        let first = commit_single_authoritative_batch(&mut server);
        server
            .advance_authoritative_batch_finality(first.final_height)
            .expect("finalize first batch");
        let _second = commit_single_authoritative_batch(&mut server);
        let (on_call, governance) = configure_rollback_authorities(&mut server);
        let approval = signed_rollback_envelope(
            &server,
            &first.batch_id,
            "atomic-prepare-fault",
            &format!("viewer-atomic-{expected_code}"),
            &on_call,
            &governance,
        );
        let world_before = server.world.snapshot();
        let batch_ids_before = server
            .authoritative_batches
            .iter()
            .map(|batch| batch.batch_id.clone())
            .collect::<Vec<_>>();
        let checkpoint_ids_before = server
            .stable_checkpoints
            .iter()
            .map(|checkpoint| checkpoint.batch_id.clone())
            .collect::<Vec<_>>();
        let epoch_before = server.reorg_epoch;
        server.set_recovery_fault_injection(Some(fault));

        let err = server
            .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
                request: AuthoritativeRollbackRequest {
                    target_batch_id: Some(first.batch_id),
                    reason: "atomic-prepare-fault".to_string(),
                    requested_by: Some("ops".to_string()),
                    approval: Some(approval),
                },
            })
            .expect_err("injected prepare fault must fail");

        assert_eq!(err.code, expected_code);
        assert_eq!(server.world.snapshot(), world_before);
        assert_eq!(
            server
                .authoritative_batches
                .iter()
                .map(|batch| batch.batch_id.clone())
                .collect::<Vec<_>>(),
            batch_ids_before
        );
        assert_eq!(
            server
                .stable_checkpoints
                .iter()
                .map(|checkpoint| checkpoint.batch_id.clone())
                .collect::<Vec<_>>(),
            checkpoint_ids_before
        );
        assert_eq!(server.reorg_epoch, epoch_before);
    }
}

#[test]
fn runtime_authoritative_recovery_does_not_swap_or_ack_before_persistence_commit() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-recovery-pre-commit-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let generation_root = recovery_dir
        .join(".distfs-state")
        .join("sidecar-generations");
    std::fs::create_dir_all(&generation_root).expect("create generation root");
    std::fs::write(
        generation_root.join(".test-fail-before-index-commit"),
        b"fail",
    )
    .expect("install pre-commit failpoint");

    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    server.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    let _second = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "persistence-pre-commit",
        "viewer-persistence-pre-commit",
        &on_call,
        &governance,
    );
    let world_before = server.world.snapshot();
    let epoch_before = server.reorg_epoch;

    let err = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "persistence-pre-commit".to_string(),
                requested_by: Some("ops".to_string()),
                approval: Some(approval),
            },
        })
        .expect_err("persistence failure must prevent acknowledgment");

    assert_eq!(err.code, "rollback_persistence_commit_failed");
    assert_eq!(server.world.snapshot(), world_before);
    assert_eq!(server.reorg_epoch, epoch_before);
    assert_eq!(
        RuntimeWorld::load_authoritative_recovery_metadata(&recovery_dir)
            .expect("inspect committed metadata"),
        None,
        "no acknowledgment metadata may become committed before the index commit point"
    );
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn runtime_authoritative_recovery_reconnect_detects_reorg_epoch_mismatch() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");

    let first = commit_single_authoritative_batch(&mut server);
    let _ = server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    let initial_cursor = latest_runtime_event_seq(&server.world);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "force_reorg",
        "viewer-valid-2",
        &on_call,
        &governance,
    );

    let (initial_ack, emit_snapshot_after_ack) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReconnectSync {
            request: AuthoritativeReconnectSyncRequest {
                player_id: "player-a".to_string(),
                session_pubkey: None,
                last_known_log_cursor: Some(initial_cursor),
                expected_reorg_epoch: Some(0),
            },
        })
        .expect("initial reconnect sync");
    assert!(!emit_snapshot_after_ack);
    assert_eq!(
        initial_ack.status,
        AuthoritativeRecoveryStatus::CatchUpReady
    );
    assert_eq!(initial_ack.message.as_deref(), Some("delta_replay_allowed"));

    let _ = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "force_reorg".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    assert_eq!(server.reorg_epoch, 1);

    let (stale_ack, emit_snapshot_after_ack) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReconnectSync {
            request: AuthoritativeReconnectSyncRequest {
                player_id: "player-a".to_string(),
                session_pubkey: None,
                last_known_log_cursor: Some(initial_cursor),
                expected_reorg_epoch: Some(0),
            },
        })
        .expect("stale reconnect sync");
    assert!(!emit_snapshot_after_ack);
    assert_eq!(stale_ack.status, AuthoritativeRecoveryStatus::CatchUpReady);
    assert!(
        stale_ack
            .message
            .as_deref()
            .is_some_and(|message| message.contains("snapshot_reload_required"))
    );
}

#[test]
fn runtime_authoritative_reconnect_reads_binding_epoch_and_cannot_restore_revoked_binding() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(51);
    let registration = register_runtime_session(
        &mut server,
        "player-reconnect-binding",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        server
            .llm_sidecar
            .bound_agent_for_player("player-reconnect-binding"),
        Some(agent_id.as_str())
    );

    let (reconnect_ack, emit_snapshot_after_ack) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReconnectSync {
            request: AuthoritativeReconnectSyncRequest {
                player_id: "player-reconnect-binding".to_string(),
                session_pubkey: Some(public_key.clone()),
                last_known_log_cursor: None,
                expected_reorg_epoch: Some(server.reorg_epoch),
            },
        })
        .expect("bound session reconnect");
    assert!(!emit_snapshot_after_ack);
    assert_eq!(
        reconnect_ack.status,
        AuthoritativeRecoveryStatus::CatchUpReady
    );
    assert_eq!(
        reconnect_ack.player_id.as_deref(),
        Some("player-reconnect-binding")
    );
    assert_eq!(reconnect_ack.agent_id.as_deref(), Some(agent_id.as_str()));
    assert_eq!(reconnect_ack.session_epoch, registration.session_epoch);
    assert_eq!(
        reconnect_ack.message.as_deref(),
        Some("delta_replay_allowed")
    );

    let (revoke_ack, emit_snapshot_after_ack) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RevokeSession {
            request: AuthoritativeSessionRevokeRequest {
                player_id: "player-reconnect-binding".to_string(),
                session_pubkey: Some(public_key.clone()),
                revoke_reason: "reconnect-binding-test".to_string(),
                revoked_by: Some("runtime-test".to_string()),
            },
        })
        .expect("revoke session");
    assert!(!emit_snapshot_after_ack);
    assert_eq!(
        revoke_ack.status,
        AuthoritativeRecoveryStatus::SessionRevoked
    );
    assert_eq!(
        server
            .llm_sidecar
            .bound_agent_for_player("player-reconnect-binding"),
        None,
        "revocation must clear the old player/agent binding"
    );

    let revoked_reconnect = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReconnectSync {
            request: AuthoritativeReconnectSyncRequest {
                player_id: "player-reconnect-binding".to_string(),
                session_pubkey: Some(public_key),
                last_known_log_cursor: None,
                expected_reorg_epoch: Some(server.reorg_epoch),
            },
        })
        .expect_err("revoked session cannot reconnect using the old key");
    assert_eq!(revoked_reconnect.code, "session_revoked");

    let (anonymous_reconnect, emit_snapshot_after_ack) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReconnectSync {
            request: AuthoritativeReconnectSyncRequest {
                player_id: "player-reconnect-binding".to_string(),
                session_pubkey: None,
                last_known_log_cursor: None,
                expected_reorg_epoch: Some(server.reorg_epoch),
            },
        })
        .expect("uncredentialed reconnect readback");
    assert!(!emit_snapshot_after_ack);
    assert_eq!(
        anonymous_reconnect.status,
        AuthoritativeRecoveryStatus::CatchUpReady
    );
    assert!(anonymous_reconnect.agent_id.is_none());
    assert!(anonymous_reconnect.session_epoch.is_none());
    assert!(anonymous_reconnect.binding_epoch.is_none());
    assert_eq!(
        reconnect_ack.binding_epoch, registration.binding_epoch,
        "reconnect readback must carry the current binding epoch"
    );
}
