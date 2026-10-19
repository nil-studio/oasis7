use super::*;
use crate::viewer::runtime_live::recovery_receipt::RuntimeAuthoritativeRecoveryGeneration;

#[test]
fn rollback_receipt_freezes_full_signed_identity_and_evidence() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize");
    let _fork = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "full-receipt",
        "nonce-full-receipt",
        &on_call,
        &governance,
    );
    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "full-receipt".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let receipt =
        serde_json::to_value(ack.rollback_receipt.expect("receipt")).expect("receipt json");
    for field in [
        "canonical_intent_digest",
        "rollback_checkpoint",
        "replay_target",
        "journal_commitment",
        "target_state_root",
    ] {
        assert!(
            receipt.get(field).is_some(),
            "immutable receipt is missing {field}"
        );
    }
}

#[test]
fn readiness_metadata_status_unknown_installs_write_fence() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-readiness-unknown-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("server");
    server.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize");
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "readiness-unknown",
        "nonce-readiness-unknown",
        &on_call,
        &governance,
    );
    server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "readiness-unknown".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let readiness_before = server.rollback_readiness.clone();
    let root = recovery_dir.join(".distfs-state/sidecar-generations");
    std::fs::write(root.join(".test-fail-before-index-commit"), b"fail").expect("failpoint");
    std::fs::write(root.join("index.json"), b"not-json").expect("corrupt index");

    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-readiness-unknown".to_string(),
            audit_evidence: None,
        })
        .expect_err("status unknown");

    assert_eq!(error.code, "rollback_persistence_status_unknown");
    assert!(
        server.authoritative_recovery_write_fence.is_some(),
        "indeterminate readiness metadata commit must fence writes"
    );
    assert_eq!(
        server.rollback_readiness, readiness_before,
        "unknown result retains pre-mutation projection"
    );
    server
        .resolve_authoritative_recovery_write_fence()
        .expect("resolve attempt");
    assert!(
        server.authoritative_recovery_write_fence.is_some(),
        "unknown readback must remain fenced"
    );
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn rollback_player_action_census_deduplicates_multi_event_actions() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize");
    let _fork = commit_single_authoritative_batch(&mut server);
    for action_id in server
        .authoritative_batches
        .iter()
        .flat_map(|batch| batch.events.iter())
        .filter_map(|event| event.runtime_event.as_ref())
        .filter_map(|event| match event.caused_by.as_ref() {
            Some(crate::runtime::CausedBy::Action(action_id)) => Some(*action_id),
            _ => None,
        })
        .collect::<Vec<_>>()
    {
        server
            .runtime_action_players
            .insert(action_id, "player-census-a".to_string());
    }
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "action-census",
        "nonce-action-census",
        &on_call,
        &governance,
    );
    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "action-census".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let dispositions = ack.rollback_receipt.expect("receipt").player_dispositions;
    let action_ids = dispositions
        .iter()
        .map(|entry| {
            entry
                .action_id
                .clone()
                .expect("every player disposition has action identity")
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        dispositions
            .iter()
            .all(|entry| entry.player_id.as_deref() == Some("player-census-a")),
        "every player disposition must preserve exact player attribution"
    );
    assert!(
        action_ids
            .iter()
            .all(|action_id| !action_id.starts_with("unknown:"))
    );
    assert_eq!(
        action_ids.len(),
        dispositions.len(),
        "one action must have exactly one disposition even when it emitted multiple events"
    );
}

#[test]
fn rollback_all_clear_is_blocked_without_explicit_verification_evidence() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize");
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "all-clear-evidence",
        "nonce-all-clear-evidence",
        &on_call,
        &governance,
    );
    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "all-clear-evidence".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let receipt = ack.rollback_receipt.expect("receipt");
    assert!(
        !receipt.ready_for_all_clear,
        "all-clear must stay blocked until explicit root, epoch, drift, consensus, receipt, and census evidence is committed"
    );
    assert!(!receipt.readiness_blockers.is_empty());
}

#[test]
fn recovery_generation_round_trip_preserves_revoked_session_policy_and_metadata() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-session-recovery-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    server.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    server
        .session_policy
        .register_session("player-restart", "revoked-key")
        .expect("register session");
    server
        .session_policy
        .rotate_session("player-restart", "revoked-key", "replacement-key")
        .expect("rotate session");
    server.record_session_revoke_metadata(
        "player-restart",
        "revoked-key",
        RuntimeSessionRevokeMetadata {
            revoke_reason: Some("rotation".to_string()),
            revoked_by: Some("security-ops".to_string()),
        },
    );

    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize");
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "persist-session-policy",
        "nonce-persist-session-policy",
        &on_call,
        &governance,
    );
    server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "persist-session-policy".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("persist recovery generation");

    let metadata = RuntimeWorld::load_authoritative_recovery_metadata(&recovery_dir)
        .expect("load metadata")
        .expect("committed metadata");
    let mut generation: RuntimeAuthoritativeRecoveryGeneration =
        serde_json::from_slice(&metadata).expect("decode generation");
    assert!(
        generation
            .session_policy
            .register_session("player-restart", "revoked-key")
            .is_err(),
        "revoked key must remain rejected after generation restore"
    );
    assert!(generation.session_revoke_metadata.iter().any(|entry| {
        entry.player_id == "player-restart"
            && entry.session_pubkey == "revoked-key"
            && entry.metadata.revoke_reason.as_deref() == Some("rotation")
            && entry.metadata.revoked_by.as_deref() == Some("security-ops")
    }));

    let legacy_value = serde_json::json!({
        "schema_version": generation.schema_version,
        "ack": generation.ack,
        "authoritative_batches": generation.authoritative_batches,
        "next_authoritative_batch_id": generation.next_authoritative_batch_id,
        "authoritative_challenges": generation.authoritative_challenges,
        "next_authoritative_challenge_id": generation.next_authoritative_challenge_id,
        "stable_checkpoints": generation.stable_checkpoints,
        "reorg_epoch": generation.reorg_epoch
    });
    let migrated: RuntimeAuthoritativeRecoveryGeneration =
        serde_json::from_value(legacy_value).expect("older generation uses migration defaults");
    assert!(migrated.session_revoke_metadata.is_empty());
    let _ = std::fs::remove_dir_all(recovery_dir);
}
