//! Domain-separated detached file signatures. The signed bytes are a canonical
//! envelope, never arbitrary caller bytes. Verification must rebuild this envelope.
use serde::{Deserialize, Serialize};

use crate::error::SignerError;
use crate::identity::{parse_sha256_hex, sha256_hex};
use crate::protocol::{MAX_ROLLBACK_PAYLOAD_BYTES, SignContext, validate_context};

pub const PURPOSE: &str = "file_ed25519_v1";
const DOMAIN: &[u8] = b"oasis7:file-signature:v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEnvelope {
    pub schema_version: String,
    pub purpose: String,
    pub context: SignContext,
    pub file_sha256: String,
    pub file_size_bytes: u64,
}

/// Build the exact signed bytes from file contents. No file path is signed.
pub fn signing_payload(context: &SignContext, file: &[u8]) -> Result<Vec<u8>, SignerError> {
    signing_payload_from_digest(context, &sha256_hex(file), file.len() as u64)
}

/// Build the same envelope for a streamed file. The caller/verifier must compute
/// the digest and size from the actual file; the worker authorizes the envelope.
pub fn signing_payload_from_digest(
    context: &SignContext,
    file_sha256: &str,
    file_size_bytes: u64,
) -> Result<Vec<u8>, SignerError> {
    validate_context(context)?;
    parse_sha256_hex(file_sha256)?;
    let envelope = FileEnvelope {
        schema_version: "oasis7.file_signature.v1".to_owned(),
        purpose: PURPOSE.to_owned(),
        context: context.clone(),
        file_sha256: file_sha256.to_owned(),
        file_size_bytes,
    };
    let mut payload = DOMAIN.to_vec();
    payload.extend(serde_json::to_vec(&envelope).map_err(|_| SignerError::CryptoOrBindingInvalid)?);
    if payload.len() > MAX_ROLLBACK_PAYLOAD_BYTES {
        return Err(SignerError::CryptoOrBindingInvalid);
    }
    Ok(payload)
}

pub fn validate_signing_payload(
    payload: &[u8],
    context: &SignContext,
) -> Result<FileEnvelope, SignerError> {
    let body = payload
        .strip_prefix(DOMAIN)
        .ok_or(SignerError::CryptoOrBindingInvalid)?;
    let envelope: FileEnvelope =
        serde_json::from_slice(body).map_err(|_| SignerError::CryptoOrBindingInvalid)?;
    if envelope.schema_version != "oasis7.file_signature.v1"
        || envelope.purpose != PURPOSE
        || &envelope.context != context
        || signing_payload_from_digest(context, &envelope.file_sha256, envelope.file_size_bytes)?
            != payload
    {
        return Err(SignerError::CryptoOrBindingInvalid);
    }
    Ok(envelope)
}

/// Verify the domain-separated envelope against an independently expected public key.
pub fn verify_file_signature(
    context: &SignContext,
    file: &[u8],
    public_key: &[u8; 32],
    signature: &[u8; 64],
) -> Result<(), SignerError> {
    verify_file_digest_signature(
        context,
        &sha256_hex(file),
        file.len() as u64,
        public_key,
        signature,
    )
}

pub fn verify_file_digest_signature(
    context: &SignContext,
    digest: &str,
    size: u64,
    public_key: &[u8; 32],
    signature: &[u8; 64],
) -> Result<(), SignerError> {
    let payload = signing_payload_from_digest(context, digest, size)?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(public_key)
        .map_err(|_| SignerError::CryptoOrBindingInvalid)?;
    key.verify_strict(&payload, &ed25519_dalek::Signature::from_bytes(signature))
        .map_err(|_| SignerError::CryptoOrBindingInvalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RollbackProtocolContext;

    fn context() -> SignContext {
        SignContext {
            deployment_id: "deployment-01".into(),
            network_id: "network-01".into(),
            task_uid: "task-01".into(),
            source_head_oid: "aa".repeat(20),
            protocol_context: RollbackProtocolContext {
                authority_id: "authority-01".into(),
                rollback_ticket: "file-01".into(),
                receipt_id: "receipt-01".into(),
                nonce: "nonce-01".into(),
            },
        }
    }
    #[test]
    fn verifies_envelope_with_independently_expected_key() {
        use ed25519_dalek::Signer;
        let signing = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let c = context();
        let sig = signing
            .sign(&signing_payload(&c, b"file").unwrap())
            .to_bytes();
        assert!(
            verify_file_signature(&c, b"file", &signing.verifying_key().to_bytes(), &sig).is_ok()
        );
        assert!(
            verify_file_signature(&c, b"changed", &signing.verifying_key().to_bytes(), &sig)
                .is_err()
        );
        let wrong = ed25519_dalek::SigningKey::from_bytes(&[8u8; 32]);
        assert!(
            verify_file_signature(&c, b"file", &wrong.verifying_key().to_bytes(), &sig).is_err()
        );
    }
    #[test]
    fn canonical_file_payload_binds_digest_size_and_complete_context() {
        let context = context();
        let payload = signing_payload(&context, b"hello").unwrap();
        let envelope = validate_signing_payload(&payload, &context).unwrap();
        assert_eq!(envelope.file_size_bytes, 5);
        assert_eq!(envelope.file_sha256, sha256_hex(b"hello"));
        assert_eq!(
            payload,
            signing_payload_from_digest(&context, &sha256_hex(b"hello"), 5).unwrap()
        );
        let mut changed = context.clone();
        changed.task_uid = "other-task".into();
        assert!(validate_signing_payload(&payload, &changed).is_err());
        assert_ne!(payload, signing_payload(&context, b"world").unwrap());
        assert_ne!(
            payload,
            signing_payload_from_digest(&context, &sha256_hex(b"hello"), 6).unwrap()
        );
    }
    #[test]
    fn reject_noncanonical_unknown_purpose_and_cross_protocol_payloads() {
        let context = context();
        let payload = signing_payload(&context, b"").unwrap();
        assert!(
            crate::rollback::validate_canonical_payload(&payload, &context.protocol_context)
                .is_err()
        );
        assert!(validate_signing_payload(b"raw data", &context).is_err());
        let mut envelope = validate_signing_payload(&payload, &context).unwrap();
        envelope.purpose = "rollback_strict_audit_v1".into();
        let mut wrong = DOMAIN.to_vec();
        wrong.extend(serde_json::to_vec(&envelope).unwrap());
        assert!(validate_signing_payload(&wrong, &context).is_err());
        let mut spaced = payload;
        spaced.push(b' ');
        assert!(validate_signing_payload(&spaced, &context).is_err());
        assert!(signing_payload_from_digest(&context, &"AA".repeat(32), 1).is_err());
    }
}
