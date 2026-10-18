use super::*;
use crate::viewer::runtime_live::recovery_receipt::RuntimeAuthoritativeRecoveryGeneration;

fn signed_receipt_access_request(
    rollback_nonce: &str,
    authority_id: &str,
    operator_nonce: &str,
    signer: &ed25519_dalek::SigningKey,
) -> crate::viewer::protocol::RollbackReceiptAccessRequest {
    let now_ms = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    let mut request = crate::viewer::protocol::RollbackReceiptAccessRequest {
        authorization_nonce: rollback_nonce.to_string(),
        authorization: crate::viewer::protocol::RollbackOperatorAuthorization {
            authority_id: authority_id.to_string(),
            issued_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(60_000),
            nonce: operator_nonce.to_string(),
            signature_scheme: "ed25519".to_string(),
            signature_hex: String::new(),
        },
    };
    request.authorization.signature_hex = hex::encode(
        signer
            .sign(
                &request
                    .canonical_signing_payload()
                    .expect("receipt payload"),
            )
            .to_bytes(),
    );
    request
}

#[test]
fn persisted_player_attribution_rejects_later_system_authored_resolution_atomically() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-terminal-player-attribution-{}-{}",
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
        "terminal-player-attribution",
        "nonce-terminal-player-attribution",
        &on_call,
        &governance,
    );
    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "terminal-player-attribution".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let missing = ack
        .rollback_receipt
        .expect("receipt")
        .player_dispositions
        .into_iter()
        .find(|entry| entry.action_id.is_some() && entry.player_id.is_none())
        .expect("unattributed action");
    let now_ms = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    let sign = |resolution, nonce: &str| {
        let mut request = crate::viewer::protocol::RollbackAttributionResolutionRequest {
            authorization_nonce: "nonce-terminal-player-attribution".to_string(),
            source_batch_id: missing.source_batch_id.clone(),
            source_event_id: missing.source_event_id,
            resolution,
            authorization: crate::viewer::protocol::RollbackOperatorAuthorization {
                authority_id: "governance-bob".to_string(),
                issued_at_ms: now_ms,
                expires_at_ms: now_ms.saturating_add(60_000),
                nonce: nonce.to_string(),
                signature_scheme: "ed25519".to_string(),
                signature_hex: String::new(),
            },
        };
        request.authorization.signature_hex = hex::encode(
            governance
                .sign(&request.canonical_signing_payload().expect("payload"))
                .to_bytes(),
        );
        request
    };
    server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ResolveRollbackAttribution {
            request: sign(
                crate::viewer::protocol::RollbackAttributionResolution::Player {
                    player_id: "player-confirmed".to_string(),
                },
                "operator-terminal-player",
            ),
        })
        .expect("player attribution persists");
    let restored =
        crate::runtime_host::world::load_authoritative_recovery_generation(&recovery_dir)
            .expect("load generation")
            .expect("persisted generation");
    let generation: RuntimeAuthoritativeRecoveryGeneration =
        serde_json::from_slice(&restored.recovery_metadata).expect("decode generation");
    let persisted_outcome = restored
        .world
        .rollback_nonce_outcome("nonce-terminal-player-attribution")
        .expect("persisted outcome");
    let persisted = persisted_outcome
        .dispositions
        .iter()
        .find(|entry| {
            entry.source_batch_id == missing.source_batch_id
                && entry.source_event_id == missing.source_event_id
        })
        .expect("persisted player attribution");
    assert_eq!(persisted.player_id.as_deref(), Some("player-confirmed"));
    assert!(!persisted.system_authored);
    let mut restarted =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("restart server");
    restarted.world = restored.world;
    restarted.consumed_rollback_operator_nonces = generation.consumed_rollback_operator_nonces;

    let conflicting = sign(
        crate::viewer::protocol::RollbackAttributionResolution::SystemAuthored,
        "operator-conflicting-system-attribution",
    );
    let world_before_conflict = restarted.world.snapshot();
    let nonces_before_conflict = restarted.consumed_rollback_operator_nonces.clone();
    let error = restarted
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ResolveRollbackAttribution {
            request: conflicting,
        })
        .expect_err("persisted player attribution is terminal");
    assert_eq!(error.code, "rollback_attribution_resolution_conflict");
    assert_eq!(restarted.world.snapshot(), world_before_conflict);
    assert_eq!(
        restarted.consumed_rollback_operator_nonces,
        nonces_before_conflict
    );
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn complete_receipt_requires_governance_auth_and_replay_stays_rejected_after_restart() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-receipt-access-{}-{}",
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
    let rollback_nonce = "nonce-receipt-access";
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "receipt-access",
        rollback_nonce,
        &on_call,
        &governance,
    );
    server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "receipt-access".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");

    let player_signer = ed25519_dalek::SigningKey::from_bytes(&[99; 32]);
    let unauthorized = signed_receipt_access_request(
        rollback_nonce,
        "governance-bob",
        "receipt-player-enumeration",
        &player_signer,
    );
    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::GetRollbackReceipt {
            request: unauthorized,
        })
        .expect_err("player signature cannot enumerate complete rollback receipt");
    assert_eq!(error.code, "rollback_receipt_authorization_invalid");

    let authorized = signed_receipt_access_request(
        rollback_nonce,
        "governance-bob",
        "receipt-governance-read-1",
        &governance,
    );
    let receipt = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::GetRollbackReceipt {
            request: authorized.clone(),
        })
        .expect("governance receipt read")
        .0
        .rollback_receipt
        .expect("complete receipt");
    assert_eq!(receipt.authorization_nonce, rollback_nonce);

    let restored =
        crate::runtime_host::world::load_authoritative_recovery_generation(&recovery_dir)
            .expect("load generation")
            .expect("persisted generation");
    let generation: RuntimeAuthoritativeRecoveryGeneration =
        serde_json::from_slice(&restored.recovery_metadata).expect("decode generation");
    let mut restarted =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("restarted server");
    restarted.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    restarted.world = restored.world;
    restarted.consumed_rollback_operator_nonces = generation.consumed_rollback_operator_nonces;
    let replay = restarted
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::GetRollbackReceipt {
            request: authorized,
        })
        .expect_err("receipt access nonce remains consumed after restart");
    assert_eq!(replay.code, "rollback_operator_authorization_replayed");
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn complete_receipt_fails_closed_without_durable_recovery_sink() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize");
    let _fork = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let rollback_nonce = "nonce-receipt-sinkless";
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "receipt-sinkless",
        rollback_nonce,
        &on_call,
        &governance,
    );
    server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "receipt-sinkless".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let request = signed_receipt_access_request(
        rollback_nonce,
        "governance-bob",
        "receipt-sinkless-read",
        &governance,
    );
    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::GetRollbackReceipt { request })
        .expect_err("complete receipt requires durable nonce persistence");
    assert_eq!(error.code, "rollback_durable_sink_required");
    assert!(
        !server
            .consumed_rollback_operator_nonces
            .contains("receipt-sinkless-read")
    );
}
