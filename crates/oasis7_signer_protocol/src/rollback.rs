use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::SignerError;
use crate::protocol::RollbackProtocolContext;

const SIGNING_DOMAIN: &[u8] = b"oasis7:rollback-strict-audit-evidence:v1\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRollbackPayload {
    pub authority_id: String,
    pub rollback_ticket: String,
    pub receipt_id: String,
    pub canonical_intent_digest: String,
    pub recovery_snapshot_hash: String,
    pub reorg_epoch: u64,
    pub candidate_state_root: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnsignedRollbackPayload {
    pub(crate) schema_version: u32,
    pub(crate) authority_id: String,
    pub(crate) rollback_ticket: String,
    pub(crate) receipt_id: String,
    pub(crate) canonical_intent_digest: String,
    pub(crate) recovery_snapshot_hash: String,
    pub(crate) reorg_epoch: u64,
    pub(crate) candidate_state_root: String,
    pub(crate) strict_registry_audit_passed: bool,
    pub(crate) strict_manifest_audit_passed: bool,
    pub(crate) audit_report_bytes: Vec<u8>,
    pub(crate) manifest_bytes: Vec<u8>,
    pub(crate) evidence_digest: String,
    pub(crate) issued_at_ms: u64,
    pub(crate) expires_at_ms: u64,
    pub(crate) nonce: String,
    pub(crate) signature_scheme: String,
}

pub fn validate_canonical_payload(
    payload: &[u8],
    protocol_context: &RollbackProtocolContext,
) -> Result<ValidatedRollbackPayload, SignerError> {
    let unsigned_bytes = payload
        .strip_prefix(SIGNING_DOMAIN)
        .ok_or(SignerError::CryptoOrBindingInvalid)?;
    let unsigned: UnsignedRollbackPayload =
        serde_json::from_slice(unsigned_bytes).map_err(|_| SignerError::CryptoOrBindingInvalid)?;
    let canonical = canonical_signing_payload(&unsigned)?;
    if canonical != payload
        || unsigned.schema_version != 1
        || !unsigned.strict_registry_audit_passed
        || !unsigned.strict_manifest_audit_passed
        || unsigned.signature_scheme != "ed25519"
        || unsigned.issued_at_ms >= unsigned.expires_at_ms
        || unsigned.evidence_digest
            != audit_artifact_digest(&unsigned.audit_report_bytes, &unsigned.manifest_bytes)
        || unsigned.authority_id != protocol_context.authority_id
        || unsigned.rollback_ticket != protocol_context.rollback_ticket
        || unsigned.receipt_id != protocol_context.receipt_id
        || unsigned.nonce != protocol_context.nonce
    {
        return Err(SignerError::CryptoOrBindingInvalid);
    }

    Ok(ValidatedRollbackPayload {
        authority_id: unsigned.authority_id,
        rollback_ticket: unsigned.rollback_ticket,
        receipt_id: unsigned.receipt_id,
        canonical_intent_digest: unsigned.canonical_intent_digest,
        recovery_snapshot_hash: unsigned.recovery_snapshot_hash,
        reorg_epoch: unsigned.reorg_epoch,
        candidate_state_root: unsigned.candidate_state_root,
        issued_at_ms: unsigned.issued_at_ms,
        expires_at_ms: unsigned.expires_at_ms,
        nonce: unsigned.nonce,
    })
}

pub(crate) fn canonical_signing_payload(
    unsigned: &UnsignedRollbackPayload,
) -> Result<Vec<u8>, SignerError> {
    let serialized =
        serde_json::to_vec(unsigned).map_err(|_| SignerError::CryptoOrBindingInvalid)?;
    let mut payload = Vec::with_capacity(SIGNING_DOMAIN.len() + serialized.len());
    payload.extend_from_slice(SIGNING_DOMAIN);
    payload.extend_from_slice(&serialized);
    Ok(payload)
}

fn audit_artifact_digest(report: &[u8], manifest: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"oasis7:rollback-strict-audit-artifacts:v1\0");
    hasher.update((report.len() as u64).to_be_bytes());
    hasher.update(report);
    hasher.update((manifest.len() as u64).to_be_bytes());
    hasher.update(manifest);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Vec<u8>, RollbackProtocolContext) {
        let protocol_context = RollbackProtocolContext {
            authority_id: "authority-01".to_owned(),
            rollback_ticket: "ticket-01".to_owned(),
            receipt_id: "receipt-01".to_owned(),
            nonce: "nonce-01".to_owned(),
        };
        let evidence = UnsignedRollbackPayload {
            schema_version: 1,
            authority_id: protocol_context.authority_id.clone(),
            rollback_ticket: protocol_context.rollback_ticket.clone(),
            receipt_id: protocol_context.receipt_id.clone(),
            canonical_intent_digest: "11".repeat(32),
            recovery_snapshot_hash: "22".repeat(32),
            reorg_epoch: 9,
            candidate_state_root: "33".repeat(32),
            strict_registry_audit_passed: true,
            strict_manifest_audit_passed: true,
            audit_report_bytes: b"report".to_vec(),
            manifest_bytes: b"manifest".to_vec(),
            evidence_digest: audit_artifact_digest(b"report", b"manifest"),
            issued_at_ms: 1_000,
            expires_at_ms: 2_000,
            nonce: protocol_context.nonce.clone(),
            signature_scheme: "ed25519".to_owned(),
        };
        (
            canonical_signing_payload(&evidence).expect("canonical rollback payload"),
            protocol_context,
        )
    }

    #[test]
    fn rollback_validator_preserves_the_exact_canonical_signing_bytes() {
        let (payload, context) = fixture();
        let expected_unsigned = br#"{"schema_version":1,"authority_id":"authority-01","rollback_ticket":"ticket-01","receipt_id":"receipt-01","canonical_intent_digest":"1111111111111111111111111111111111111111111111111111111111111111","recovery_snapshot_hash":"2222222222222222222222222222222222222222222222222222222222222222","reorg_epoch":9,"candidate_state_root":"3333333333333333333333333333333333333333333333333333333333333333","strict_registry_audit_passed":true,"strict_manifest_audit_passed":true,"audit_report_bytes":[114,101,112,111,114,116],"manifest_bytes":[109,97,110,105,102,101,115,116],"evidence_digest":"0d4a1f3a18bcfeb47ee9364852adafeddf7f5aeb3d27849d7950c1dc482ec338","issued_at_ms":1000,"expires_at_ms":2000,"nonce":"nonce-01","signature_scheme":"ed25519"}"#;
        assert_eq!(payload, [SIGNING_DOMAIN, expected_unsigned].concat());
        let validated = validate_canonical_payload(&payload, &context).expect("valid payload");
        assert_eq!(validated.authority_id, context.authority_id);
        assert_eq!(validated.rollback_ticket, context.rollback_ticket);
        assert_eq!(validated.receipt_id, context.receipt_id);
        assert_eq!(validated.nonce, context.nonce);
        assert!(payload.starts_with(SIGNING_DOMAIN));

        let mut changed = payload;
        changed.push(b' ');
        assert_eq!(
            validate_canonical_payload(&changed, &context)
                .unwrap_err()
                .code(),
            "CRYPTO_OR_BINDING_INVALID"
        );
    }

    #[test]
    fn rollback_validator_rejects_context_mismatch_and_unsigned_audit_flags() {
        let (payload, mut context) = fixture();
        context.nonce = "nonce-other".to_owned();
        assert!(validate_canonical_payload(&payload, &context).is_err());

        let (payload, context) = fixture();
        let unsigned_bytes = payload
            .strip_prefix(SIGNING_DOMAIN)
            .expect("signing domain");
        let mut unsigned: UnsignedRollbackPayload =
            serde_json::from_slice(unsigned_bytes).expect("unsigned payload");
        unsigned.strict_registry_audit_passed = false;
        let mut changed = SIGNING_DOMAIN.to_vec();
        changed.extend(serde_json::to_vec(&unsigned).expect("serialize changed payload"));
        assert!(validate_canonical_payload(&changed, &context).is_err());
    }
}
