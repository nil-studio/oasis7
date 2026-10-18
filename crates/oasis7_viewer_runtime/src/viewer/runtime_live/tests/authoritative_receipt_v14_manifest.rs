use super::*;

pub(super) fn assert_manifest_identity_mismatches_stay_blocked(
    server: &mut ViewerRuntimeLiveServer,
    audit_evidence: &crate::viewer::protocol::RollbackStrictAuditEvidence,
    governance: &ed25519_dalek::SigningKey,
) {
    let mut mismatched = audit_evidence.clone();
    mismatched.nonce = "audit-manifest-mismatch".to_string();
    mismatched.manifest_bytes = br#"{"entries":[]}"#.to_vec();
    sign_mutated_evidence(&mut mismatched, governance);
    assert_blocked(server, mismatched, "manifest mismatch receipt");

    let mut stale = audit_evidence.clone();
    stale.nonce = "audit-stale-registry-manifest".to_string();
    stale.manifest_bytes = br#"{"entries":[]}"#.to_vec();
    let mut report: serde_json::Value =
        serde_json::from_slice(&stale.audit_report_bytes).expect("audit report");
    report["audited_manifest_digest"] = serde_json::Value::String(
        crate::viewer::strict_audit_manifest_digest(&stale.manifest_bytes),
    );
    stale.audit_report_bytes = serde_json::to_vec(&report).expect("stale report");
    sign_mutated_evidence(&mut stale, governance);
    assert_blocked(server, stale, "stale registry receipt");

    for (nonce, mutate_entries) in [
        (
            "audit-duplicate-manifest-entry",
            duplicate_first_manifest_entry as fn(&mut Vec<serde_json::Value>),
        ),
        (
            "audit-rogue-extra-manifest-entry",
            push_rogue_manifest_entry,
        ),
    ] {
        let mut evidence = audit_evidence.clone();
        evidence.nonce = nonce.to_string();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&evidence.manifest_bytes).expect("manifest document");
        mutate_entries(
            manifest["entries"]
                .as_array_mut()
                .expect("manifest entries"),
        );
        evidence.manifest_bytes = serde_json::to_vec(&manifest).expect("mutated manifest");
        let mut report: serde_json::Value =
            serde_json::from_slice(&evidence.audit_report_bytes).expect("audit report");
        report["audited_manifest_digest"] = serde_json::Value::String(
            crate::viewer::strict_audit_manifest_digest(&evidence.manifest_bytes),
        );
        evidence.audit_report_bytes = serde_json::to_vec(&report).expect("mutated report");
        sign_mutated_evidence(&mut evidence, governance);
        assert_blocked(server, evidence, "non-canonical full-set receipt");
    }
}

fn duplicate_first_manifest_entry(entries: &mut Vec<serde_json::Value>) {
    entries.push(entries.first().expect("first manifest entry").clone());
}

fn push_rogue_manifest_entry(entries: &mut Vec<serde_json::Value>) {
    entries.push(serde_json::json!({
        "slot_id": "governance.rollback.stale.v0",
        "signer_id": "retired-governance",
        "scheme": "ed25519",
        "public_key_hex": "00".repeat(32),
        "threshold": 1
    }));
}

fn push_realistic_governance_entries(entries: &mut Vec<serde_json::Value>) {
    entries.extend([
        serde_json::json!({
            "slot_id": "governance.finality.v1",
            "node_id": "validator-a",
            "scheme": "ed25519",
            "public_key_hex": "11".repeat(32),
            "stake": 70
        }),
        serde_json::json!({
            "slot_id": "msig.genesis.v1",
            "signer_id": "signer01",
            "scheme": "ed25519",
            "public_key_hex": "22".repeat(32),
            "threshold": 1
        }),
    ]);
}

pub(super) fn assert_rogue_manifest_rejected_before_restart_readiness_mutation(
    server: &ViewerRuntimeLiveServer,
    audit_evidence: &crate::viewer::protocol::RollbackStrictAuditEvidence,
    governance: &ed25519_dalek::SigningKey,
) {
    let mut full_evidence = audit_evidence.clone();
    full_evidence.nonce = "audit-restarted-full-governance-manifest".to_string();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&full_evidence.manifest_bytes).expect("manifest document");
    push_realistic_governance_entries(
        manifest["entries"]
            .as_array_mut()
            .expect("manifest entries"),
    );
    full_evidence.manifest_bytes = serde_json::to_vec(&manifest).expect("full manifest");
    let mut report: serde_json::Value =
        serde_json::from_slice(&full_evidence.audit_report_bytes).expect("audit report");
    report["audited_manifest_digest"] = serde_json::Value::String(
        crate::viewer::strict_audit_manifest_digest(&full_evidence.manifest_bytes),
    );
    full_evidence.audit_report_bytes = serde_json::to_vec(&report).expect("full report");
    sign_mutated_evidence(&mut full_evidence, governance);
    assert!(
        crate::viewer::runtime_live::recovery_audit::verify_strict_audit_evidence(
            &server.world,
            &full_evidence,
            crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms(),
        )
        .is_ok(),
        "restarted verifier must accept an importable full governance manifest"
    );

    let mut evidence = full_evidence;
    evidence.nonce = "audit-restarted-rogue-extra-manifest-entry".to_string();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&evidence.manifest_bytes).expect("full manifest document");
    push_rogue_manifest_entry(
        manifest["entries"]
            .as_array_mut()
            .expect("manifest entries"),
    );
    evidence.manifest_bytes = serde_json::to_vec(&manifest).expect("mutated manifest");
    let mut report: serde_json::Value =
        serde_json::from_slice(&evidence.audit_report_bytes).expect("audit report");
    report["audited_manifest_digest"] = serde_json::Value::String(
        crate::viewer::strict_audit_manifest_digest(&evidence.manifest_bytes),
    );
    evidence.audit_report_bytes = serde_json::to_vec(&report).expect("mutated report");
    sign_mutated_evidence(&mut evidence, governance);
    assert!(
        crate::viewer::runtime_live::recovery_audit::verify_strict_audit_evidence(
            &server.world,
            &evidence,
            crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms(),
        )
        .is_err(),
        "restarted verifier must reject rogue manifest before readiness mutation"
    );
}

fn sign_mutated_evidence(
    evidence: &mut crate::viewer::protocol::RollbackStrictAuditEvidence,
    governance: &ed25519_dalek::SigningKey,
) {
    evidence.evidence_digest = crate::viewer::strict_audit_artifact_digest(
        &evidence.audit_report_bytes,
        &evidence.manifest_bytes,
    );
    evidence.signature_hex = hex::encode(
        governance
            .sign(&evidence.canonical_signing_payload().expect("payload"))
            .to_bytes(),
    );
}

fn assert_blocked(
    server: &mut ViewerRuntimeLiveServer,
    evidence: crate::viewer::protocol::RollbackStrictAuditEvidence,
    _receipt_label: &str,
) {
    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
            authorization_nonce: "nonce-readiness-transition".to_string(),
            audit_evidence: Some(evidence),
        })
        .expect_err("manifest mismatch is rejected before readiness mutation");
    assert_eq!(error.code, "strict_audit_evidence_invalid");
}

pub(super) fn assert_missing_and_malformed_digest_stay_blocked(
    server: &mut ViewerRuntimeLiveServer,
    audit_evidence: &crate::viewer::protocol::RollbackStrictAuditEvidence,
    governance: &ed25519_dalek::SigningKey,
) {
    for (nonce, digest) in [
        ("audit-missing-manifest-digest", None),
        ("audit-malformed-manifest-digest", Some("not-a-sha256")),
    ] {
        let mut evidence = audit_evidence.clone();
        evidence.nonce = nonce.to_string();
        let mut report: serde_json::Value =
            serde_json::from_slice(&evidence.audit_report_bytes).expect("audit report");
        match digest {
            Some(digest) => {
                report["audited_manifest_digest"] = serde_json::Value::String(digest.to_string());
            }
            None => {
                report
                    .as_object_mut()
                    .expect("report object")
                    .remove("audited_manifest_digest");
            }
        }
        evidence.audit_report_bytes = serde_json::to_vec(&report).expect("mutated report");
        evidence.evidence_digest = crate::viewer::strict_audit_artifact_digest(
            &evidence.audit_report_bytes,
            &evidence.manifest_bytes,
        );
        evidence.signature_hex = hex::encode(
            governance
                .sign(
                    &evidence
                        .canonical_signing_payload()
                        .expect("mutated digest payload"),
                )
                .to_bytes(),
        );
        let error = server
            .handle_authoritative_recovery(
                AuthoritativeRecoveryCommand::ReevaluateRollbackReadiness {
                    authorization_nonce: "nonce-readiness-transition".to_string(),
                    audit_evidence: Some(evidence),
                },
            )
            .expect_err("invalid manifest digest is rejected before readiness mutation");
        assert_eq!(error.code, "strict_audit_evidence_invalid");
    }
}
