use super::*;
use crate::viewer::runtime_live::recovery_receipt::RuntimeAuthoritativeRecoveryGeneration;

fn signed_strict_audit(
    server: &ViewerRuntimeLiveServer,
    receipt: &crate::viewer::protocol::AuthoritativeRollbackReceipt,
    governance: &SigningKey,
    nonce: &str,
) -> crate::viewer::protocol::RollbackStrictAuditEvidence {
    let manifest_bytes = serde_json::to_vec(&serde_json::json!({
        "entries": server
            .world
            .snapshot()
            .rollback_authority_registry
            .records()
            .map(|record| serde_json::json!({
                "slot_id": match record.role {
                    crate::runtime::RollbackAuthorityRole::OnCall => "ops.rollback.on_call.v1",
                    crate::runtime::RollbackAuthorityRole::Governance => "governance.rollback.v1",
                },
                "signer_id": record.authority_id,
                "scheme": "ed25519",
                "public_key_hex": record.public_key_hex,
                "threshold": 1,
            }))
            .collect::<Vec<_>>()
    }))
    .expect("strict audit manifest");
    let manifest_digest = crate::viewer::strict_audit_manifest_digest(&manifest_bytes);
    let audit_report_bytes = serde_json::to_vec(&serde_json::json!({
        "audited_manifest_digest": manifest_digest,
        "finality": { "status": "ready" },
        "controllers": [{ "status": "ready" }],
        "rollback_authorities": [
            {
                "slot_id": "ops.rollback.on_call.v1",
                "role": "on_call",
                "configured": true,
                "active": true,
                "threshold": 1,
                "manifest_match": true,
                "status": "ready"
            },
            {
                "slot_id": "governance.rollback.v1",
                "role": "governance",
                "configured": true,
                "active": true,
                "threshold": 1,
                "manifest_match": true,
                "status": "ready"
            }
        ],
        "rollback_blockers": [],
        "overall_single_failure_tolerance_pass": true,
        "manifest_match_pass": true,
        "overall_status": "ready_for_ops_drill"
    }))
    .expect("strict audit report");
    let now_ms = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    let mut evidence = crate::viewer::build_unsigned_strict_audit_evidence(
        crate::viewer::RollbackStrictAuditEvidenceInput {
            authority_id: "governance-bob".to_string(),
            rollback_ticket: receipt.rollback_ticket.clone(),
            receipt_id: receipt.receipt_id.clone(),
            canonical_intent_digest: receipt.canonical_intent_digest.clone(),
            recovery_snapshot_hash: receipt.snapshot_hash.clone(),
            reorg_epoch: receipt.committed_reorg_epoch,
            candidate_state_root: server
                .world
                .execution_state_identity()
                .expect("current root")
                .state_root_hash,
            audit_report_bytes,
            manifest_bytes,
            issued_at_ms: now_ms.saturating_sub(1_000),
            expires_at_ms: now_ms.saturating_add(60_000),
            nonce: nonce.to_string(),
        },
    );
    evidence.signature_hex = hex::encode(
        governance
            .sign(&evidence.canonical_signing_payload().expect("audit payload"))
            .to_bytes(),
    );
    evidence
}

#[test]
fn invalid_supplied_strict_audit_is_atomic_after_all_clear_across_restart() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-invalid-audit-atomicity-{}-{}",
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
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "invalid-audit-atomicity",
        "nonce-invalid-audit-atomicity",
        &on_call,
        &governance,
    );
    let (rolled_back, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "invalid-audit-atomicity".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let rollback_receipt = rolled_back.rollback_receipt.expect("rollback receipt");
    let valid_audit = signed_strict_audit(&server, &rollback_receipt, &governance, "audit-valid");
    let (all_clear, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-invalid-audit-atomicity".to_string(),
            audit_evidence: Some(valid_audit),
        })
        .expect("valid strict audit");
    assert!(
        all_clear
            .rollback_receipt
            .expect("all-clear receipt")
            .ready_for_all_clear
    );

    let readiness_before = server.rollback_readiness.clone();
    let consumed_before = server.consumed_strict_audit_nonces.clone();
    let generation_before =
        crate::runtime_host::world::load_authoritative_recovery_generation(&recovery_dir)
            .expect("load generation before invalid audit")
            .expect("generation before invalid audit");
    let receipt_before = serde_json::to_vec(
        &server
            .get_rollback_receipt("nonce-invalid-audit-atomicity".to_string())
            .expect("receipt before invalid audit"),
    )
    .expect("encode receipt before invalid audit");

    let mut forged = signed_strict_audit(&server, &rollback_receipt, &governance, "audit-forged");
    forged.signature_hex = "00".repeat(64);
    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-invalid-audit-atomicity".to_string(),
            audit_evidence: Some(forged),
        })
        .expect_err("invalid supplied strict audit must error atomically");
    assert_eq!(error.code, "strict_audit_evidence_invalid");
    assert_eq!(server.rollback_readiness, readiness_before);
    assert_eq!(server.consumed_strict_audit_nonces, consumed_before);

    let generation_after =
        crate::runtime_host::world::load_authoritative_recovery_generation(&recovery_dir)
            .expect("load generation after invalid audit")
            .expect("generation after invalid audit");
    assert_eq!(
        generation_after.generation_id,
        generation_before.generation_id
    );
    assert_eq!(
        generation_after.recovery_metadata,
        generation_before.recovery_metadata
    );

    let persisted: RuntimeAuthoritativeRecoveryGeneration =
        serde_json::from_slice(&generation_after.recovery_metadata).expect("decode generation");
    let mut restarted =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("restarted server");
    restarted.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    restarted.world = generation_after.world;
    restarted.reorg_epoch = persisted.reorg_epoch;
    restarted.rollback_readiness = persisted.rollback_readiness;
    restarted.consumed_strict_audit_nonces = persisted.consumed_strict_audit_nonces;
    let receipt_after_restart = serde_json::to_vec(
        &restarted
            .get_rollback_receipt("nonce-invalid-audit-atomicity".to_string())
            .expect("receipt after restart"),
    )
    .expect("encode receipt after restart");
    assert_eq!(receipt_after_restart, receipt_before);
    assert_eq!(restarted.rollback_readiness, readiness_before);
    assert_eq!(restarted.consumed_strict_audit_nonces, consumed_before);
    let _ = std::fs::remove_dir_all(recovery_dir);
}
