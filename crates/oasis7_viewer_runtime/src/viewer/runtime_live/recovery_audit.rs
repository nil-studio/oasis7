use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;

use super::*;

pub(super) const STRICT_AUDIT_FUTURE_SKEW_MS: u64 = 30_000;

pub(super) fn strict_audit_evidence_digest(report: &[u8], manifest: &[u8]) -> String {
    crate::viewer::strict_audit_artifact_digest(report, manifest)
}

#[derive(Deserialize)]
struct StrictAuditManifestEntry {
    slot_id: String,
    #[serde(default)]
    signer_id: Option<String>,
    #[serde(default, rename = "node_id")]
    _node_id: Option<String>,
    scheme: String,
    public_key_hex: String,
    #[serde(default)]
    threshold: Option<u16>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StrictAuditManifest {
    Entries(Vec<StrictAuditManifestEntry>),
    Document {
        entries: Vec<StrictAuditManifestEntry>,
    },
}

fn verify_manifest_matches_active_registry(
    world: &RuntimeWorld,
    manifest_bytes: &[u8],
) -> Result<(), String> {
    let entries = match serde_json::from_slice::<StrictAuditManifest>(manifest_bytes)
        .map_err(|_| "strict audit manifest bytes are not a valid public manifest".to_string())?
    {
        StrictAuditManifest::Entries(entries) | StrictAuditManifest::Document { entries } => {
            entries
        }
    };
    let expected = [
        (
            "ops.rollback.on_call.v1",
            crate::runtime::RollbackAuthorityRole::OnCall,
        ),
        (
            "governance.rollback.v1",
            crate::runtime::RollbackAuthorityRole::Governance,
        ),
    ];
    let rollback_entries = entries
        .iter()
        .filter(|entry| {
            entry.slot_id.starts_with("ops.rollback.")
                || entry.slot_id.starts_with("governance.rollback.")
        })
        .collect::<Vec<_>>();
    if rollback_entries.len() != expected.len() {
        return Err(
            "strict audit manifest must exactly match rollback authority slots".to_string(),
        );
    }
    let registry = &world.snapshot().rollback_authority_registry;
    if registry.len() != expected.len() {
        return Err("strict audit manifest does not match active rollback registry".to_string());
    }
    for (slot_id, role) in expected {
        let matching = rollback_entries
            .iter()
            .filter(|entry| entry.slot_id == slot_id)
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err(
                "strict audit manifest must contain each rollback authority slot exactly once"
                    .to_string(),
            );
        }
        let entry = *matching[0];
        let record = registry
            .get(entry.signer_id.as_deref().unwrap_or_default())
            .filter(|record| record.active && record.role == role)
            .ok_or_else(|| {
                "strict audit manifest signer is not active in rollback registry".to_string()
            })?;
        if entry.scheme != "ed25519"
            || entry.threshold != Some(1)
            || !record
                .public_key_hex
                .eq_ignore_ascii_case(entry.public_key_hex.trim())
        {
            return Err(
                "strict audit manifest does not match active rollback registry".to_string(),
            );
        }
    }
    Ok(())
}

pub(super) fn verify_strict_audit_evidence(
    world: &RuntimeWorld,
    audit: &crate::viewer::protocol::RollbackStrictAuditEvidence,
    now_ms: u64,
) -> Result<(), String> {
    if audit.schema_version != 1
        || audit.signature_scheme != "ed25519"
        || audit.authority_id.trim().is_empty()
        || audit.nonce.trim().is_empty()
        || audit.audit_report_bytes.is_empty()
        || audit.manifest_bytes.is_empty()
        || !audit.strict_registry_audit_passed
        || !audit.strict_manifest_audit_passed
        || audit.issued_at_ms > now_ms.saturating_add(STRICT_AUDIT_FUTURE_SKEW_MS)
        || audit.expires_at_ms < now_ms
        || audit.expires_at_ms <= audit.issued_at_ms
    {
        return Err("strict audit evidence envelope is incomplete or expired".to_string());
    }
    if strict_audit_evidence_digest(&audit.audit_report_bytes, &audit.manifest_bytes)
        != audit.evidence_digest
    {
        return Err("strict audit artifact digest mismatch".to_string());
    }
    let report: serde_json::Value = serde_json::from_slice(&audit.audit_report_bytes)
        .map_err(|_| "strict audit report bytes are not valid JSON".to_string())?;
    let audited_manifest_digest = report
        .get("audited_manifest_digest")
        .and_then(serde_json::Value::as_str)
        .filter(|digest| digest.len() == 64 && hex::decode(digest).is_ok())
        .ok_or_else(|| "strict audit report manifest digest is missing or malformed".to_string())?;
    if crate::viewer::strict_audit_manifest_digest(&audit.manifest_bytes) != audited_manifest_digest
    {
        return Err("strict audit report and attached manifest digest mismatch".to_string());
    }
    verify_manifest_matches_active_registry(world, &audit.manifest_bytes)?;
    if !matches!(
        report
            .get("overall_status")
            .and_then(serde_json::Value::as_str),
        Some("pass" | "ready_for_ops_drill")
    ) || report
        .get("overall_single_failure_tolerance_pass")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
        || report
            .get("manifest_match_pass")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || report
            .get("rollback_blockers")
            .and_then(serde_json::Value::as_array)
            .is_none_or(|blockers| !blockers.is_empty())
    {
        return Err("strict audit report does not prove a blocker-free pass".to_string());
    }
    let snapshot = world.snapshot();
    let authority = snapshot
        .rollback_authority_registry
        .records()
        .find(|record| {
            record.authority_id == audit.authority_id
                && record.active
                && record.role == crate::runtime::RollbackAuthorityRole::Governance
        })
        .ok_or_else(|| "strict audit signer is not the active governance authority".to_string())?;
    let public_key: [u8; 32] = hex::decode(authority.public_key_hex.trim())
        .map_err(|_| "invalid strict audit authority public key".to_string())?
        .try_into()
        .map_err(|_| "invalid strict audit authority public key length".to_string())?;
    let signature: [u8; 64] = hex::decode(audit.signature_hex.trim())
        .map_err(|_| "invalid strict audit signature".to_string())?
        .try_into()
        .map_err(|_| "invalid strict audit signature length".to_string())?;
    let payload = audit
        .canonical_signing_payload()
        .map_err(|err| format!("strict audit canonical payload failed: {err}"))?;
    VerifyingKey::from_bytes(&public_key)
        .map_err(|_| "invalid strict audit authority public key".to_string())?
        .verify(&payload, &Signature::from_bytes(&signature))
        .map_err(|_| "strict audit signature verification failed".to_string())
}
