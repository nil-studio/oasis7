use sha2::{Digest, Sha256};

use oasis7_proto::viewer::RollbackStrictAuditEvidence;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackStrictAuditEvidenceInput {
    pub authority_id: String,
    pub rollback_ticket: String,
    pub receipt_id: String,
    pub canonical_intent_digest: String,
    pub recovery_snapshot_hash: String,
    pub reorg_epoch: u64,
    pub candidate_state_root: String,
    pub audit_report_bytes: Vec<u8>,
    pub manifest_bytes: Vec<u8>,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub nonce: String,
}

pub fn strict_audit_artifact_digest(report: &[u8], manifest: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"oasis7:rollback-strict-audit-artifacts:v1\0");
    hasher.update((report.len() as u64).to_be_bytes());
    hasher.update(report);
    hasher.update((manifest.len() as u64).to_be_bytes());
    hasher.update(manifest);
    hex::encode(hasher.finalize())
}

pub fn strict_audit_manifest_digest(manifest: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"oasis7:rollback-strict-audit-manifest:v1\0");
    hasher.update((manifest.len() as u64).to_be_bytes());
    hasher.update(manifest);
    hex::encode(hasher.finalize())
}

pub fn build_unsigned_strict_audit_evidence(
    input: RollbackStrictAuditEvidenceInput,
) -> RollbackStrictAuditEvidence {
    let evidence_digest =
        strict_audit_artifact_digest(&input.audit_report_bytes, &input.manifest_bytes);
    RollbackStrictAuditEvidence {
        schema_version: 1,
        authority_id: input.authority_id,
        rollback_ticket: input.rollback_ticket,
        receipt_id: input.receipt_id,
        canonical_intent_digest: input.canonical_intent_digest,
        recovery_snapshot_hash: input.recovery_snapshot_hash,
        reorg_epoch: input.reorg_epoch,
        candidate_state_root: input.candidate_state_root,
        strict_registry_audit_passed: true,
        strict_manifest_audit_passed: true,
        evidence_digest,
        audit_report_bytes: input.audit_report_bytes,
        manifest_bytes: input.manifest_bytes,
        issued_at_ms: input.issued_at_ms,
        expires_at_ms: input.expires_at_ms,
        nonce: input.nonce,
        signature_scheme: "ed25519".to_string(),
        signature_hex: String::new(),
    }
}
