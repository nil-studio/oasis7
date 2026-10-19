use super::*;
use crate::viewer::runtime_live::recovery_receipt::RuntimeAuthoritativeRecoveryGeneration;

use crate::governance_audit_report as canonical_governance_audit_report;
#[path = "authoritative_receipt_v14_manifest.rs"]
mod manifest_binding_tests;

fn canonical_clean_governance_audit_report_bytes(audited_manifest_digest: String) -> Vec<u8> {
    use canonical_governance_audit_report::{
        GovernanceRegistryAuditReport, RollbackAuthorityAuditRow, audit_row,
    };

    serde_json::to_vec(&GovernanceRegistryAuditReport {
        world_dir: "/operator/world".to_string(),
        audited_manifest_digest: Some(audited_manifest_digest),
        finality: audit_row("governance.finality.v1", 2, 3, 2, Some(true)),
        controllers: vec![audit_row("governance.controller.v1", 2, 3, 2, Some(true))],
        rollback_authorities: vec![
            RollbackAuthorityAuditRow {
                slot_id: "ops.rollback.on_call.v1".to_string(),
                role: "on_call".to_string(),
                configured: true,
                active: true,
                threshold: 1,
                manifest_match: Some(true),
                status: "ready".to_string(),
            },
            RollbackAuthorityAuditRow {
                slot_id: "governance.rollback.v1".to_string(),
                role: "governance".to_string(),
                configured: true,
                active: true,
                threshold: 1,
                manifest_match: Some(true),
                status: "ready".to_string(),
            },
        ],
        rollback_blockers: Vec::new(),
        overall_single_failure_tolerance_pass: true,
        manifest_match_pass: Some(true),
        overall_status: "ready_for_ops_drill".to_string(),
    })
    .expect("canonical governance audit report bytes")
}

fn signed_strict_audit_evidence(
    server: &ViewerRuntimeLiveServer,
    receipt: &crate::viewer::protocol::AuthoritativeRollbackReceipt,
    governance: &ed25519_dalek::SigningKey,
    nonce: &str,
    issued_at_ms: u64,
    expires_at_ms: u64,
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
                "threshold": 1
            }))
            .collect::<Vec<_>>()
    }))
    .expect("canonical test manifest");
    let audit_report_bytes = canonical_clean_governance_audit_report_bytes(
        crate::viewer::strict_audit_manifest_digest(&manifest_bytes),
    );
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
            issued_at_ms,
            expires_at_ms,
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
fn readiness_reevaluation_stays_blocked_without_fresh_strict_audit_evidence() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-readiness-recovery-{}-{}",
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
        "readiness-transition",
        "nonce-readiness-transition",
        &on_call,
        &governance,
    );
    let (blocked, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "readiness-transition".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let blocked_receipt = blocked.rollback_receipt.expect("receipt");
    assert!(!blocked_receipt.ready_for_all_clear);

    let (reevaluated, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-readiness-transition".to_string(),
            audit_evidence: None,
        })
        .expect("reevaluate readiness");
    let receipt = reevaluated.rollback_receipt.expect("reevaluated receipt");
    assert!(!receipt.ready_for_all_clear);
    assert!(
        receipt
            .readiness_blockers
            .iter()
            .any(|blocker| blocker == "fresh_strict_audit_evidence_required")
    );

    let metadata = RuntimeWorld::load_authoritative_recovery_metadata(&recovery_dir)
        .expect("load metadata")
        .expect("committed metadata");
    let generation: RuntimeAuthoritativeRecoveryGeneration =
        serde_json::from_slice(&metadata).expect("decode generation");
    let evidence = generation
        .rollback_readiness
        .get("nonce-readiness-transition")
        .expect("persisted readiness evidence");
    assert!(!evidence.ready);
    assert!(!evidence.canonical_intent_digest.is_empty());
    assert!(!evidence.affected_census_digest.is_empty());

    let now_ms = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    let audit_evidence = signed_strict_audit_evidence(
        &server,
        &blocked_receipt,
        &governance,
        "audit-readiness-transition",
        now_ms.saturating_sub(1_000),
        now_ms.saturating_add(60_000),
    );
    let mut mismatched_audit = audit_evidence.clone();
    mismatched_audit.candidate_state_root = "not-the-current-candidate-root".to_string();
    let root_mismatch = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-readiness-transition".to_string(),
            audit_evidence: Some(mismatched_audit),
        })
        .expect_err("audit root mismatch is rejected before readiness mutation");
    assert_eq!(root_mismatch.code, "strict_audit_evidence_invalid");
    let mut expired_audit = audit_evidence.clone();
    expired_audit.nonce = "audit-expired".to_string();
    expired_audit.issued_at_ms = now_ms.saturating_sub(120_000);
    expired_audit.expires_at_ms = now_ms.saturating_sub(60_000);
    expired_audit.signature_hex = hex::encode(
        governance
            .sign(
                &expired_audit
                    .canonical_signing_payload()
                    .expect("expired audit payload"),
            )
            .to_bytes(),
    );
    let expired = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-readiness-transition".to_string(),
            audit_evidence: Some(expired_audit),
        })
        .expect_err("expired audit is rejected before readiness mutation");
    assert_eq!(expired.code, "strict_audit_evidence_invalid");
    manifest_binding_tests::assert_manifest_identity_mismatches_stay_blocked(
        &mut server,
        &audit_evidence,
        &governance,
    );
    manifest_binding_tests::assert_missing_and_malformed_digest_stay_blocked(
        &mut server,
        &audit_evidence,
        &governance,
    );
    let (ready, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-readiness-transition".to_string(),
            audit_evidence: Some(audit_evidence.clone()),
        })
        .expect("reevaluate with fresh strict audit evidence");
    assert!(
        ready
            .rollback_receipt
            .expect("ready receipt")
            .ready_for_all_clear
    );
    server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-readiness-transition".to_string(),
            audit_evidence: None,
        })
        .expect("later blocked reevaluation must not forget consumed audit nonces");
    let replay = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-readiness-transition".to_string(),
            audit_evidence: Some(audit_evidence.clone()),
        })
        .expect_err("consumed audit nonce cannot be replayed");
    assert_eq!(replay.code, "strict_audit_evidence_replay");
    let committed_epoch = server.reorg_epoch;
    server.reorg_epoch = committed_epoch.saturating_add(1);
    let stale_epoch_receipt = server
        .get_rollback_receipt("nonce-readiness-transition".to_string())
        .expect("receipt remains retrievable after epoch drift")
        .rollback_receipt
        .expect("receipt");
    assert!(
        !stale_epoch_receipt.ready_for_all_clear,
        "retrieval must revalidate the current reorg epoch"
    );
    server.reorg_epoch = committed_epoch;

    let metadata = RuntimeWorld::load_authoritative_recovery_metadata(&recovery_dir)
        .expect("reload metadata")
        .expect("committed metadata");
    let restarted: RuntimeAuthoritativeRecoveryGeneration =
        serde_json::from_slice(&metadata).expect("decode restarted generation");
    let restarted_readiness = restarted
        .rollback_readiness
        .get("nonce-readiness-transition")
        .expect("restart preserves readiness evidence");
    assert!(!restarted_readiness.ready);
    assert!(restarted_readiness.strict_audit_evidence_digest.is_empty());
    assert_eq!(
        restarted_readiness.candidate_state_root, blocked_receipt.target_state_root,
        "persisted all-clear evidence records the observed candidate root"
    );
    let restored =
        crate::runtime_host::world::load_authoritative_recovery_generation(&recovery_dir)
            .expect("load generation")
            .expect("persisted generation");
    let generation: RuntimeAuthoritativeRecoveryGeneration =
        serde_json::from_slice(&restored.recovery_metadata).expect("decode generation");
    let mut restarted_server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("restarted server");
    restarted_server.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    restarted_server.world = restored.world;
    restarted_server.rollback_readiness = generation.rollback_readiness;
    restarted_server.consumed_strict_audit_nonces = generation.consumed_strict_audit_nonces;
    manifest_binding_tests::assert_rogue_manifest_rejected_before_restart_readiness_mutation(
        &restarted_server,
        &audit_evidence,
        &governance,
    );
    let replay = restarted_server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-readiness-transition".to_string(),
            audit_evidence: Some(audit_evidence),
        })
        .expect_err("consumed audit nonce survives readiness overwrite and restart");
    assert_eq!(replay.code, "strict_audit_evidence_replay");
    let _ = std::fs::remove_dir_all(recovery_dir);
}
#[test]
fn forged_unsigned_strict_audit_evidence_cannot_authorize_all_clear() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-forged-audit-{}-{}",
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
        "forged-audit",
        "nonce-forged-audit",
        &on_call,
        &governance,
    );
    let (rolled_back, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "forged-audit".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let receipt = rolled_back.rollback_receipt.expect("receipt");
    let now_ms = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    let mut forged = signed_strict_audit_evidence(
        &server,
        &receipt,
        &governance,
        "audit-forged",
        now_ms.saturating_sub(1_000),
        now_ms.saturating_add(60_000),
    );
    forged.signature_hex = "00".repeat(64);
    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-forged-audit".to_string(),
            audit_evidence: Some(forged),
        })
        .expect_err("forged evidence is rejected before readiness mutation");
    assert_eq!(error.code, "strict_audit_evidence_invalid");
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn receipt_retrieval_expires_stale_readiness_and_rechecks_current_evidence() {
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
        "stale-readiness",
        "nonce-stale-readiness",
        &on_call,
        &governance,
    );
    let (rolled_back, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "stale-readiness".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let receipt = rolled_back.rollback_receipt.expect("receipt");
    let now_ms = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    let stale_evidence = signed_strict_audit_evidence(
        &server,
        &receipt,
        &governance,
        "audit-stale-retrieval",
        now_ms.saturating_sub(120_000),
        now_ms.saturating_sub(60_000),
    );
    server.rollback_readiness.insert(
        "nonce-stale-readiness".to_string(),
        crate::viewer::runtime_live::recovery_receipt::RuntimeRollbackReadinessRecord {
            canonical_intent_digest: server
                .world
                .rollback_nonce_outcome("nonce-stale-readiness")
                .expect("outcome")
                .canonical_intent_hash
                .clone(),
            affected_census_digest: server
                .world
                .rollback_nonce_outcome("nonce-stale-readiness")
                .expect("outcome")
                .affected_census_digest
                .clone(),
            rollback_ticket: "ROLLBACK-2313".to_string(),
            receipt_id: server
                .world
                .rollback_receipt_projection("nonce-stale-readiness")
                .expect("receipt")
                .receipt_id
                .clone(),
            recovery_snapshot_hash: server
                .world
                .rollback_receipt_projection("nonce-stale-readiness")
                .expect("receipt")
                .snapshot_hash
                .clone(),
            reorg_epoch: server.reorg_epoch,
            candidate_state_root: server
                .world
                .execution_state_identity()
                .expect("root")
                .state_root_hash,
            strict_audit_evidence_digest: stale_evidence.evidence_digest.clone(),
            strict_audit_observed_at_ms: stale_evidence.issued_at_ms,
            strict_audit_evidence: Some(stale_evidence),
            ready: true,
            blockers: Vec::new(),
            evaluated_at_tick: server.world.state().time,
        },
    );
    let ack = server
        .get_rollback_receipt("nonce-stale-readiness".to_string())
        .expect("receipt retrieval");
    assert!(!ack.rollback_receipt.expect("receipt").ready_for_all_clear);
}

#[test]
fn durable_receipt_does_not_fabricate_player_compensation_for_system_events() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-system-attribution-{}-{}",
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
        "compensation-status",
        "nonce-compensation-status",
        &on_call,
        &governance,
    );
    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "compensation-status".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let receipt = ack.rollback_receipt.expect("receipt");
    let dispositions = &receipt.player_dispositions;
    assert!(
        dispositions.iter().all(|entry| {
            entry.player_id.is_none() && entry.compensation.is_none() && entry.action_id.is_some()
        }),
        "unattributed action events remain explicit remediation evidence without fabricated player compensation"
    );
    assert!(
        receipt
            .readiness_blockers
            .iter()
            .any(|blocker| blocker == "player_attribution_incomplete")
    );

    let missing_entries = dispositions
        .iter()
        .filter(|entry| entry.action_id.is_some() && entry.player_id.is_none())
        .cloned()
        .collect::<Vec<_>>();
    let missing = missing_entries
        .first()
        .expect("unattributed action census entry")
        .clone();
    let now_ms = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    let signed_resolution = |entry: &crate::viewer::protocol::PlayerRollbackDisposition,
                             index: usize| {
        let mut request = crate::viewer::protocol::RollbackAttributionResolutionRequest {
            authorization_nonce: "nonce-compensation-status".to_string(),
            source_batch_id: entry.source_batch_id.clone(),
            source_event_id: entry.source_event_id,
            resolution: crate::viewer::protocol::RollbackAttributionResolution::SystemAuthored,
            authorization: crate::viewer::protocol::RollbackOperatorAuthorization {
                authority_id: "governance-bob".to_string(),
                issued_at_ms: now_ms,
                expires_at_ms: now_ms.saturating_add(60_000),
                nonce: format!("operator-system-attribution-{index}"),
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

    let mut tampered = signed_resolution(&missing, 0);
    tampered.source_event_id = tampered.source_event_id.saturating_add(1);
    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ResolveRollbackAttribution {
            request: tampered,
        })
        .expect_err("signature must bind the immutable source event identity");
    assert_eq!(error.code, "rollback_compensation_authorization_invalid");

    let mut resolved_receipt = None;
    for (index, entry) in missing_entries.iter().enumerate() {
        let (resolved, _) = server
            .handle_authoritative_recovery(
                AuthoritativeRecoveryCommand::ResolveRollbackAttribution {
                    request: signed_resolution(entry, index),
                },
            )
            .expect("governed system-authored resolution");
        resolved_receipt = Some(resolved.rollback_receipt.expect("resolved receipt"));
    }
    let resolved_receipt = resolved_receipt.expect("at least one resolution receipt");
    assert!(
        !resolved_receipt
            .readiness_blockers
            .iter()
            .any(|blocker| blocker == "player_attribution_incomplete"),
        "persisted non-player classification clears only the attribution blocker"
    );
    let classified = resolved_receipt
        .player_dispositions
        .iter()
        .find(|entry| {
            entry.source_batch_id == missing.source_batch_id
                && entry.source_event_id == missing.source_event_id
        })
        .expect("classified census entry");
    assert_eq!(classified.action_id, missing.action_id);
    assert!(classified.system_authored);
    assert!(classified.player_id.is_none());
    assert!(classified.compensation.is_none());

    let restored_generation =
        crate::runtime_host::world::load_authoritative_recovery_generation(&recovery_dir)
            .expect("load generation")
            .expect("committed recovery generation");
    let generation: RuntimeAuthoritativeRecoveryGeneration =
        serde_json::from_slice(&restored_generation.recovery_metadata)
            .expect("decode recovery generation");
    let restored_receipt = generation
        .ack
        .rollback_receipt
        .expect("restored rollback receipt");
    let restored = restored_receipt
        .player_dispositions
        .iter()
        .find(|entry| {
            entry.source_batch_id == missing.source_batch_id
                && entry.source_event_id == missing.source_event_id
        })
        .expect("restored classified census entry");
    assert_eq!(restored.action_id, missing.action_id);
    assert!(restored.system_authored);
    assert!(restored.player_id.is_none());
    assert!(restored.compensation.is_none());
    let mut restarted =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("restart server");
    restarted.world = restored_generation.world;
    restarted.consumed_rollback_operator_nonces = generation.consumed_rollback_operator_nonces;

    let mut conflicting_player = crate::viewer::protocol::RollbackAttributionResolutionRequest {
        authorization_nonce: "nonce-compensation-status".to_string(),
        source_batch_id: missing.source_batch_id.clone(),
        source_event_id: missing.source_event_id,
        resolution: crate::viewer::protocol::RollbackAttributionResolution::Player {
            player_id: "player-late-claim".to_string(),
        },
        authorization: crate::viewer::protocol::RollbackOperatorAuthorization {
            authority_id: "governance-bob".to_string(),
            issued_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(60_000),
            nonce: "operator-conflicting-player-attribution".to_string(),
            signature_scheme: "ed25519".to_string(),
            signature_hex: String::new(),
        },
    };
    conflicting_player.authorization.signature_hex = hex::encode(
        governance
            .sign(
                &conflicting_player
                    .canonical_signing_payload()
                    .expect("payload"),
            )
            .to_bytes(),
    );
    let world_before_conflict = restarted.world.snapshot();
    let nonces_before_conflict = restarted.consumed_rollback_operator_nonces.clone();
    let error = restarted
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ResolveRollbackAttribution {
            request: conflicting_player,
        })
        .expect_err("persisted system-authored attribution is terminal");
    assert_eq!(error.code, "rollback_attribution_resolution_conflict");
    assert_eq!(restarted.world.snapshot(), world_before_conflict);
    assert_eq!(
        restarted.consumed_rollback_operator_nonces, nonces_before_conflict,
        "rejected conflicting resolution must not consume its operator nonce"
    );
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn attribution_resolution_protocol_accepts_governed_system_authored_classification() {
    let decoded = serde_json::from_value::<
        crate::viewer::protocol::RollbackAttributionResolutionRequest,
    >(serde_json::json!({
        "authorization_nonce": "rollback-nonce",
        "source_batch_id": "batch-system",
        "source_event_id": 9,
        "resolution": { "kind": "system_authored" },
        "authorization": {
            "authority_id": "governance-bob",
            "issued_at_ms": 10,
            "expires_at_ms": 20,
            "nonce": "operator-system-resolution",
            "signature_scheme": "ed25519",
            "signature_hex": "00"
        }
    }));
    assert!(
        decoded.is_ok(),
        "system-authored resolution is canonical protocol shape"
    );
}

#[test]
fn compensation_projection_exposes_safe_owner_ticket_and_state() {
    let dispositions = crate::viewer::runtime_live::recovery_receipt::viewer_dispositions(
        "ROLLBACK-SAFE-COMPENSATION",
        "nonce-safe-compensation",
        &[crate::runtime::RollbackEventDisposition {
            source_batch_id: "batch-player-action".to_string(),
            source_event_id: 77,
            status: crate::runtime::RollbackDispositionStatus::CompensationRequired,
            compensation: Some(crate::runtime::RollbackCompensationCaseRef {
                owner_id: "player-support-queue".to_string(),
                ticket_id: "public-case-77".to_string(),
                state: crate::runtime::RollbackCompensationState::InProgress,
            }),
            player_id: Some("player-77".to_string()),
            action_id: Some("action-77".to_string()),
            system_authored: false,
        }],
    );
    let disposition = dispositions.into_iter().next().expect("disposition");
    let compensation = disposition.compensation.expect("safe compensation status");
    assert_eq!(
        compensation.case_id,
        disposition.compensation_case_id.unwrap()
    );
    assert_eq!(compensation.responsible_party, "player_support");
    assert!(compensation.ticket_reference.starts_with("rollback-case-"));
    assert_eq!(
        serde_json::to_value(compensation.state).expect("serialize compensation state"),
        serde_json::json!("in_progress")
    );
}

#[test]
fn compensation_projection_redacts_arbitrary_restored_owner_and_ticket_values() {
    let dispositions = crate::viewer::runtime_live::recovery_receipt::viewer_dispositions(
        "rollback-public-projection",
        "nonce-public-projection",
        &[crate::runtime::RollbackEventDisposition {
            source_batch_id: "batch-public-projection".to_string(),
            source_event_id: 88,
            status: crate::runtime::RollbackDispositionStatus::CompensationRequired,
            compensation: Some(crate::runtime::RollbackCompensationCaseRef {
                owner_id: "secret-owner@example.internal".to_string(),
                ticket_id: "INCIDENT-SECRET-12345".to_string(),
                state: crate::runtime::RollbackCompensationState::InProgress,
            }),
            player_id: Some("player-88".to_string()),
            action_id: Some("action-88".to_string()),
            system_authored: false,
        }],
    );
    let compensation = dispositions[0]
        .compensation
        .as_ref()
        .expect("public compensation");
    let encoded = serde_json::to_string(compensation).expect("encode public compensation");
    assert_eq!(compensation.responsible_party, "player_support");
    assert!(compensation.ticket_reference.starts_with("rollback-case-"));
    assert!(!encoded.contains("secret-owner"));
    assert!(!encoded.contains("INCIDENT-SECRET"));
}

#[test]
fn missing_attribution_receipt_preserves_remediable_source_and_action_identity() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize");
    let _fork = commit_single_authoritative_batch(&mut server);
    let expected = server
        .authoritative_batches
        .iter()
        .skip(1)
        .flat_map(|batch| batch.events.iter().map(move |event| (batch, event)))
        .find_map(|(batch, event)| {
            let action_id = match event.runtime_event.as_ref()?.caused_by.as_ref()? {
                crate::runtime::CausedBy::Action(action_id) => *action_id,
                _ => return None,
            };
            Some((batch.batch_id.clone(), event.id, action_id.to_string()))
        })
        .expect("fork action event");
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "missing-attribution-evidence",
        "nonce-missing-attribution-evidence",
        &on_call,
        &governance,
    );
    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "missing-attribution-evidence".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let receipt = ack.rollback_receipt.expect("receipt");
    assert!(receipt.player_dispositions.iter().any(|entry| {
        entry.source_batch_id == expected.0
            && entry.source_event_id == expected.1
            && entry.action_id.as_deref() == Some(expected.2.as_str())
            && entry.player_id.is_none()
    }));
}

#[test]
fn authorized_rejected_compensation_transition_survives_restart() {
    let recovery_root = std::env::temp_dir().join(format!(
        "oasis7-compensation-restart-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let recovery_dir = recovery_root.join("runtime-live-authoritative-recovery");
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    server.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    let first = commit_single_authoritative_batch(&mut server);
    server
        .advance_authoritative_batch_finality(first.final_height)
        .expect("finalize");
    let _fork = commit_single_authoritative_batch(&mut server);
    for action_id in server
        .authoritative_batches
        .iter()
        .skip(1)
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
            .insert(action_id, "player-transition".to_string());
    }
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "durable-compensation",
        "nonce-durable-compensation",
        &on_call,
        &governance,
    );
    let (rollback, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "durable-compensation".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let disposition = rollback
        .rollback_receipt
        .expect("receipt")
        .player_dispositions
        .into_iter()
        .find(|entry| {
            entry.disposition
                == crate::viewer::protocol::PlayerActionDisposition::CompensationRequired
        })
        .expect("compensation case");
    let mut request = crate::viewer::protocol::RollbackCompensationTransitionRequest {
        authorization_nonce: "nonce-durable-compensation".to_string(),
        source_batch_id: disposition.source_batch_id,
        source_event_id: disposition.source_event_id,
        next_state: crate::viewer::protocol::PlayerCompensationState::Rejected,
        authorization: crate::viewer::protocol::RollbackOperatorAuthorization {
            authority_id: "governance-bob".to_string(),
            issued_at_ms: crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms(),
            expires_at_ms: crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
                + 60_000,
            nonce: "operator-compensation-reject-1".to_string(),
            signature_scheme: "ed25519".to_string(),
            signature_hex: String::new(),
        },
    };
    let payload = request.canonical_signing_payload().expect("payload");
    request.authorization.signature_hex = hex::encode(governance.sign(&payload).to_bytes());
    let mut expired = request.clone();
    expired.authorization.nonce = "operator-compensation-expired".to_string();
    expired.authorization.issued_at_ms = 1;
    expired.authorization.expires_at_ms = 2;
    let payload = expired
        .canonical_signing_payload()
        .expect("expired payload");
    expired.authorization.signature_hex = hex::encode(governance.sign(&payload).to_bytes());
    assert_eq!(
        server
            .handle_authoritative_recovery(
                AuthoritativeRecoveryCommand::TransitionRollbackCompensation { request: expired },
            )
            .expect_err("expired authorization")
            .code,
        "rollback_operator_authorization_expired"
    );
    let mut future = request.clone();
    let now = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    future.authorization.nonce = "operator-compensation-future".to_string();
    future.authorization.issued_at_ms = now + 60_000;
    future.authorization.expires_at_ms = now + 120_000;
    let payload = future.canonical_signing_payload().expect("future payload");
    future.authorization.signature_hex = hex::encode(governance.sign(&payload).to_bytes());
    assert_eq!(
        server
            .handle_authoritative_recovery(
                AuthoritativeRecoveryCommand::TransitionRollbackCompensation { request: future },
            )
            .expect_err("future authorization")
            .code,
        "rollback_operator_authorization_expired"
    );
    let replay = request.clone();
    let world_before_receipt_failure = server.world.clone();
    let nonces_before_receipt_failure = server.consumed_rollback_operator_nonces.clone();
    server.set_recovery_fault_injection(Some(
        crate::viewer::runtime_live::recovery::RuntimeRecoveryFaultInjection::AckEncode,
    ));
    let receipt_error = server
        .handle_authoritative_recovery(
            AuthoritativeRecoveryCommand::TransitionRollbackCompensation {
                request: request.clone(),
            },
        )
        .expect_err("receipt construction failure must abort compensation mutation");
    assert_eq!(receipt_error.code, "rollback_ack_encode_failed");
    assert_eq!(
        server.world.snapshot(),
        world_before_receipt_failure.snapshot()
    );
    assert_eq!(
        server.consumed_rollback_operator_nonces, nonces_before_receipt_failure,
        "fallible receipt construction must not consume the operator nonce"
    );
    server.set_recovery_fault_injection(None);
    let (transitioned, _) = server
        .handle_authoritative_recovery(
            AuthoritativeRecoveryCommand::TransitionRollbackCompensation { request },
        )
        .expect("authorized transition");
    let transitioned_receipt = transitioned
        .rollback_receipt
        .as_ref()
        .expect("transition receipt")
        .clone();
    let now_ms = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    let audit_evidence = signed_strict_audit_evidence(
        &server,
        &transitioned_receipt,
        &governance,
        "audit-rejected-terminal",
        now_ms.saturating_sub(1_000),
        now_ms.saturating_add(60_000),
    );
    let reevaluated = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-durable-compensation".to_string(),
            audit_evidence: Some(audit_evidence),
        })
        .expect("reevaluate rejected terminal compensation")
        .0;
    assert!(
        reevaluated
            .rollback_receipt
            .expect("reevaluated receipt")
            .ready_for_all_clear
    );
    assert!(
        server
            .get_rollback_receipt("nonce-durable-compensation".to_string())
            .expect("retrieve rejected terminal compensation")
            .rollback_receipt
            .expect("retrieved receipt")
            .ready_for_all_clear,
        "GetRollbackReceipt must preserve Reevaluate readiness for rejected terminal cases"
    );
    let restored =
        crate::runtime_host::world::load_authoritative_recovery_generation(&recovery_dir)
            .expect("load generation")
            .expect("durable generation");
    let restored_generation: RuntimeAuthoritativeRecoveryGeneration =
        serde_json::from_slice(&restored.recovery_metadata).expect("decode generation");
    let mut restarted =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("restart server");
    restarted.world = restored.world.clone();
    restarted.consumed_rollback_operator_nonces =
        restored_generation.consumed_rollback_operator_nonces;
    let replay_error = restarted
        .handle_authoritative_recovery(
            AuthoritativeRecoveryCommand::TransitionRollbackCompensation { request: replay },
        )
        .expect_err("operator authorization nonce must be one-time");
    assert_eq!(
        replay_error.code,
        "rollback_operator_authorization_replayed"
    );
    assert!(
        transitioned
            .rollback_receipt
            .expect("transition receipt")
            .player_dispositions
            .iter()
            .any(|entry| entry.compensation.as_ref().is_some_and(|case| {
                case.state == crate::viewer::protocol::PlayerCompensationState::Rejected
            }))
    );
    assert!(
        restored
            .world
            .rollback_nonce_outcome("nonce-durable-compensation")
            .expect("restored outcome")
            .dispositions
            .iter()
            .any(|entry| entry.compensation.as_ref().is_some_and(|case| {
                case.state == crate::runtime::RollbackCompensationState::Rejected
            }))
    );
    let _ = std::fs::remove_dir_all(recovery_root);
}

#[test]
fn rejected_attributed_actions_create_accountable_public_compensation_cases() {
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
        .skip(1)
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
            .insert(action_id, "player-private-42".to_string());
    }
    let (on_call, governance) = configure_rollback_authorities(&mut server);
    let approval = signed_rollback_envelope(
        &server,
        &first.batch_id,
        "accountable-compensation",
        "nonce-accountable-compensation",
        &on_call,
        &governance,
    );
    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "accountable-compensation".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let dispositions = ack.rollback_receipt.expect("receipt").player_dispositions;
    assert!(
        !dispositions.is_empty(),
        "attributed fork actions are censused"
    );
    for disposition in dispositions {
        assert_eq!(
            disposition.disposition,
            crate::viewer::protocol::PlayerActionDisposition::CompensationRequired
        );
        let compensation = disposition.compensation.expect("accountable case");
        assert!(!compensation.responsible_party.trim().is_empty());
        assert!(!compensation.ticket_reference.trim().is_empty());
        assert!(!compensation.responsible_party.contains("private"));
        assert!(
            !compensation
                .ticket_reference
                .contains("nonce-accountable-compensation")
        );
        assert!(!compensation.ticket_reference.contains("player-private-42"));
    }
}

#[test]
fn missing_action_attribution_is_explicit_fail_closed_census_evidence() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-viewer-attribution-atomicity-{}-{}",
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
        "missing-attribution",
        "nonce-missing-attribution",
        &on_call,
        &governance,
    );
    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::Rollback {
            request: AuthoritativeRollbackRequest {
                target_batch_id: Some(first.batch_id),
                reason: "missing-attribution".to_string(),
                requested_by: None,
                approval: Some(approval),
            },
        })
        .expect("rollback");
    let receipt = ack.rollback_receipt.expect("receipt");
    assert!(!receipt.ready_for_all_clear);
    assert!(
        receipt
            .readiness_blockers
            .iter()
            .any(|blocker| blocker == "player_attribution_incomplete"),
        "missing attribution must be explicit persisted blocker evidence"
    );
    let missing = receipt
        .player_dispositions
        .iter()
        .find(|entry| entry.action_id.is_some() && entry.player_id.is_none())
        .expect("missing attribution disposition");
    let now_ms = crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms();
    let mut resolution = crate::viewer::protocol::RollbackAttributionResolutionRequest {
        authorization_nonce: "nonce-missing-attribution".to_string(),
        source_batch_id: missing.source_batch_id.clone(),
        source_event_id: missing.source_event_id,
        resolution: crate::viewer::protocol::RollbackAttributionResolution::SystemAuthored,
        authorization: crate::viewer::protocol::RollbackOperatorAuthorization {
            authority_id: "governance-bob".to_string(),
            issued_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(60_000),
            nonce: "operator-attribution-receipt-failure".to_string(),
            signature_scheme: "ed25519".to_string(),
            signature_hex: String::new(),
        },
    };
    resolution.authorization.signature_hex = hex::encode(
        governance
            .sign(&resolution.canonical_signing_payload().expect("payload"))
            .to_bytes(),
    );
    let world_before_receipt_failure = server.world.clone();
    let nonces_before_receipt_failure = server.consumed_rollback_operator_nonces.clone();
    server.set_recovery_fault_injection(Some(
        crate::viewer::runtime_live::recovery::RuntimeRecoveryFaultInjection::AckEncode,
    ));
    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ResolveRollbackAttribution {
            request: resolution,
        })
        .expect_err("receipt construction failure must abort attribution mutation");
    assert_eq!(error.code, "rollback_ack_encode_failed");
    assert_eq!(
        server.world.snapshot(),
        world_before_receipt_failure.snapshot()
    );
    assert_eq!(
        server.consumed_rollback_operator_nonces, nonces_before_receipt_failure,
        "fallible receipt construction must not consume the operator nonce"
    );
    let _ = std::fs::remove_dir_all(recovery_dir);
}
