use super::*;

#[test]
fn runtime_authoritative_recovery_rejects_checkpoint_after_replay_target_explicitly() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    let second = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(second.final_height)
        .expect("finalize second batch");
    let first_checkpoint = server
        .stable_checkpoints
        .iter()
        .find(|checkpoint| checkpoint.batch_id == first.batch_id)
        .expect("first checkpoint");
    let second_checkpoint = server
        .stable_checkpoints
        .iter()
        .find(|checkpoint| checkpoint.batch_id == second.batch_id)
        .expect("second checkpoint");
    let request = AuthoritativeRollbackV2Request {
        reason: "reversed-c-to-t".to_string(),
        approval: RollbackAuthorizationEnvelopeV2 {
            intent: RollbackIntentV2 {
                schema_version: 2,
                rollback_ticket: "ROLLBACK-2313-REVERSED".to_string(),
                rollback_checkpoint: RollbackCheckpointRef {
                    batch_id: second.batch_id,
                    snapshot_hash: compute_runtime_snapshot_hash(&second_checkpoint.snapshot)
                        .expect("second snapshot hash"),
                    snapshot_journal_len: second_checkpoint.snapshot.journal_len,
                },
                replay_target: RollbackReplayTarget {
                    batch_id: first.batch_id,
                    target_journal_len: first_checkpoint.journal.len(),
                    expected_target_state_root: first.state_root,
                    journal_commitment: "unused-for-reversed-range".to_string(),
                },
                expected_reorg_epoch: server.reorg_epoch,
                max_replay_events: 0,
                max_replay_bytes: 0,
                reason: "reversed-c-to-t".to_string(),
                issued_at_ms: 1,
                expires_at_ms: u64::MAX,
                nonce: "nonce-reversed-c-to-t".to_string(),
            },
            signatures: Vec::new(),
        },
    };

    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RollbackV2 { request })
        .expect_err("checkpoint C after target T must fail before authorization work");
    assert_eq!(error.code, "rollback_target_precedes_checkpoint");
}

#[test]
fn runtime_authoritative_recovery_rejects_rollback_without_a_durable_sink() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    let fork = commit_single_authoritative_batch(&mut server);
    let checkpoint = server
        .stable_checkpoints
        .iter()
        .find(|checkpoint| checkpoint.batch_id == first.batch_id)
        .expect("stable rollback checkpoint");
    let request = AuthoritativeRollbackV2Request {
        reason: "durable-sink-required".to_string(),
        approval: RollbackAuthorizationEnvelopeV2 {
            intent: RollbackIntentV2 {
                schema_version: 2,
                rollback_ticket: "ROLLBACK-DURABLE-SINK".to_string(),
                rollback_checkpoint: RollbackCheckpointRef {
                    batch_id: first.batch_id,
                    snapshot_hash: compute_runtime_snapshot_hash(&checkpoint.snapshot)
                        .expect("checkpoint snapshot hash"),
                    snapshot_journal_len: checkpoint.snapshot.journal_len,
                },
                replay_target: RollbackReplayTarget {
                    batch_id: fork.batch_id,
                    target_journal_len: checkpoint.journal.len(),
                    expected_target_state_root: fork.state_root,
                    journal_commitment: "preflight-does-not-consume-evidence".to_string(),
                },
                expected_reorg_epoch: server.reorg_epoch,
                max_replay_events: usize::MAX,
                max_replay_bytes: usize::MAX,
                reason: "durable-sink-required".to_string(),
                issued_at_ms: 1,
                expires_at_ms: u64::MAX,
                nonce: "nonce-durable-sink-required".to_string(),
            },
            signatures: Vec::new(),
        },
    };

    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RollbackV2 { request })
        .expect_err("rollback must fail closed when no durable recovery sink is configured");
    assert_eq!(error.code, "rollback_durable_sink_required");
}

#[test]
fn runtime_authoritative_recovery_converges_after_post_index_commit_error() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-recovery-post-commit-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let generation_root = recovery_dir.join(".distfs-state/sidecar-generations");
    std::fs::create_dir_all(&generation_root).expect("create generation root");
    std::fs::write(
        generation_root.join(".test-fail-after-index-commit"),
        b"fail",
    )
    .expect("install post-commit failpoint");
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    server.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize first batch");
    let _fork = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "post-commit-converge",
        "nonce-post-commit-converge",
        &on_call,
        &governance,
    );

    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "post-commit-converge".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("readback of a committed generation must converge live state and acknowledgment");
    assert_eq!(server.reorg_epoch, ack.reorg_epoch);
    assert!(
        server
            .world
            .rollback_nonce_outcome("nonce-post-commit-converge")
            .is_some()
    );
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn authoritative_recovery_generation_contains_restartable_viewer_state() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-recovery-restart-state-{}-{}",
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
        .expect("finalize");
    let _fork = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "restartable",
        "nonce-restartable",
        &on_call,
        &governance,
    );
    server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "restartable".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");

    let metadata = RuntimeWorld::load_authoritative_recovery_metadata(&recovery_dir)
        .expect("load generation metadata")
        .expect("metadata");
    let value: serde_json::Value =
        serde_json::from_slice(&metadata).expect("generation metadata json");
    assert!(value.get("authoritative_batches").is_some());
    assert!(value.get("stable_checkpoints").is_some());
    assert!(value.get("next_authoritative_batch_id").is_some());
    assert!(value.get("next_authoritative_challenge_id").is_some());
    let _ = std::fs::remove_dir_all(recovery_dir);
}
