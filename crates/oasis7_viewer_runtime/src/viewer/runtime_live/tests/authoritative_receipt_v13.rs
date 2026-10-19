use super::*;
use crate::viewer::runtime_live::recovery_receipt::RuntimeAuthoritativeRecoveryGeneration;

#[test]
fn governed_player_attribution_preserves_non_rejected_census_semantics_across_restart() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-status-preserving-attribution-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let rollback_nonce = "nonce-status-preserving-attribution";
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    server.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize");
    let _fork = commit_single_authoritative_batch(&mut server);
    let _second_fork = commit_single_authoritative_batch(&mut server);
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "status-preserving-attribution",
        rollback_nonce,
        &on_call,
        &governance,
    );
    server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "status-preserving-attribution".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");

    let journal = server.world.journal().clone();
    let mut snapshot = server.world.snapshot();
    let outcome = snapshot
        .rollback_nonce_outcomes
        .get_mut(rollback_nonce)
        .expect("rollback outcome");
    let mut unknown = outcome
        .dispositions
        .iter_mut()
        .filter(|entry| entry.action_id.is_some() && entry.player_id.is_none())
        .take(2)
        .collect::<Vec<_>>();
    assert_eq!(unknown.len(), 2, "fixture needs two unknown action rows");
    unknown[0].status = crate::runtime::RollbackDispositionStatus::Replayed;
    unknown[1].status = crate::runtime::RollbackDispositionStatus::PreservedAtTarget;
    let sources = unknown
        .iter()
        .map(|entry| {
            (
                entry.source_batch_id.clone(),
                entry.source_event_id,
                entry.action_id.clone(),
                entry.status,
            )
        })
        .collect::<Vec<_>>();
    server.world = crate::runtime_host::world::from_snapshot(snapshot, journal)
        .expect("restore census fixture");

    let now_ms = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    for (index, (batch_id, event_id, _, _)) in sources.iter().enumerate() {
        let mut request = crate::viewer::protocol::RollbackAttributionResolutionRequest {
            authorization_nonce: rollback_nonce.to_string(),
            source_batch_id: batch_id.clone(),
            source_event_id: *event_id,
            resolution: crate::viewer::protocol::RollbackAttributionResolution::Player {
                player_id: format!("player-status-{index}"),
            },
            authorization: crate::viewer::protocol::RollbackOperatorAuthorization {
                authority_id: "governance-bob".to_string(),
                issued_at_ms: now_ms,
                expires_at_ms: now_ms.saturating_add(60_000),
                nonce: format!("operator-status-player-{index}"),
                signature_scheme: "ed25519".to_string(),
                signature_hex: String::new(),
            },
        };
        request.authorization.signature_hex = hex::encode(
            governance
                .sign(&request.canonical_signing_payload().expect("payload"))
                .to_bytes(),
        );
        server
            .handle_authoritative_recovery(
                AuthoritativeRecoveryCommand::ResolveRollbackAttribution { request },
            )
            .expect("governed player attribution preserves census status");
    }

    let outcome = server
        .world
        .rollback_nonce_outcome(rollback_nonce)
        .expect("resolved outcome");
    for (index, (batch_id, event_id, action_id, status)) in sources.iter().enumerate() {
        let resolved = outcome
            .dispositions
            .iter()
            .find(|entry| entry.source_batch_id == *batch_id && entry.source_event_id == *event_id)
            .expect("resolved census row");
        assert_eq!(resolved.status, *status);
        assert_eq!(resolved.action_id, *action_id);
        assert_eq!(
            resolved.player_id.as_deref(),
            Some(format!("player-status-{index}").as_str())
        );
        assert!(resolved.compensation.is_none());
        assert!(!resolved.system_authored);
    }
    let persisted =
        crate::runtime_host::world::load_authoritative_recovery_generation(&recovery_dir)
            .expect("load generation")
            .expect("persisted generation");
    let generation: RuntimeAuthoritativeRecoveryGeneration =
        serde_json::from_slice(&persisted.recovery_metadata).expect("decode generation");
    let persisted_outcome = persisted
        .world
        .rollback_nonce_outcome(rollback_nonce)
        .expect("persisted outcome");
    assert!(
        !generation
            .ack
            .rollback_receipt
            .as_ref()
            .expect("persisted receipt")
            .readiness_blockers
            .iter()
            .any(|blocker| blocker == "player_attribution_incomplete")
    );
    for (batch_id, event_id, _, status) in sources.iter() {
        let resolved = persisted_outcome
            .dispositions
            .iter()
            .find(|entry| entry.source_batch_id == *batch_id && entry.source_event_id == *event_id)
            .expect("persisted census row");
        assert_eq!(resolved.status, *status);
        assert!(resolved.compensation.is_none());
    }

    let mut restarted =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("restart server");
    restarted.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    restarted.world = persisted.world;
    restarted.rollback_readiness = generation.rollback_readiness;
    restarted.consumed_rollback_operator_nonces = generation.consumed_rollback_operator_nonces;
    for (index, (batch_id, event_id, _, status)) in sources.iter().enumerate() {
        let mut retry = crate::viewer::protocol::RollbackAttributionResolutionRequest {
            authorization_nonce: rollback_nonce.to_string(),
            source_batch_id: batch_id.clone(),
            source_event_id: *event_id,
            resolution: crate::viewer::protocol::RollbackAttributionResolution::Player {
                player_id: format!("player-status-{index}"),
            },
            authorization: crate::viewer::protocol::RollbackOperatorAuthorization {
                authority_id: "governance-bob".to_string(),
                issued_at_ms: now_ms,
                expires_at_ms: now_ms.saturating_add(60_000),
                nonce: format!("operator-idempotent-player-retry-{index}"),
                signature_scheme: "ed25519".to_string(),
                signature_hex: String::new(),
            },
        };
        retry.authorization.signature_hex = hex::encode(
            governance
                .sign(&retry.canonical_signing_payload().expect("payload"))
                .to_bytes(),
        );
        let world_before = restarted.world.snapshot();
        let nonces_before = restarted.consumed_rollback_operator_nonces.clone();
        let readiness_before = restarted.rollback_readiness.clone();
        let generation_before =
            crate::runtime_host::world::load_authoritative_recovery_generation(&recovery_dir)
                .expect("load generation before retry")
                .expect("generation before retry")
                .recovery_metadata;
        restarted
            .handle_authoritative_recovery(
                AuthoritativeRecoveryCommand::ResolveRollbackAttribution { request: retry },
            )
            .expect("exact terminal player retry is idempotent");
        assert_eq!(restarted.world.snapshot(), world_before);
        assert_eq!(restarted.consumed_rollback_operator_nonces, nonces_before);
        assert_eq!(restarted.rollback_readiness, readiness_before);
        assert_eq!(
            crate::runtime_host::world::load_authoritative_recovery_generation(&recovery_dir)
                .expect("load generation after retry")
                .expect("generation after retry")
                .recovery_metadata,
            generation_before,
            "{status:?} retry must not rewrite the recovery generation"
        );
    }
    let (batch_id, event_id, _, _) = &sources[0];
    let mut conflicting = crate::viewer::protocol::RollbackAttributionResolutionRequest {
        authorization_nonce: rollback_nonce.to_string(),
        source_batch_id: batch_id.clone(),
        source_event_id: *event_id,
        resolution: crate::viewer::protocol::RollbackAttributionResolution::SystemAuthored,
        authorization: crate::viewer::protocol::RollbackOperatorAuthorization {
            authority_id: "governance-bob".to_string(),
            issued_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(60_000),
            nonce: "operator-conflicting-system-after-player".to_string(),
            signature_scheme: "ed25519".to_string(),
            signature_hex: String::new(),
        },
    };
    conflicting.authorization.signature_hex = hex::encode(
        governance
            .sign(&conflicting.canonical_signing_payload().expect("payload"))
            .to_bytes(),
    );
    let world_before = restarted.world.snapshot();
    let nonces_before = restarted.consumed_rollback_operator_nonces.clone();
    let error = restarted
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ResolveRollbackAttribution {
            request: conflicting,
        })
        .expect_err("player attribution is terminal");
    assert_eq!(error.code, "rollback_attribution_resolution_conflict");
    assert_eq!(restarted.world.snapshot(), world_before);
    assert_eq!(restarted.consumed_rollback_operator_nonces, nonces_before);
    let _ = std::fs::remove_dir_all(recovery_dir);
}
