//! Authentication of a complete renewal issuer status snapshot.
//!
//! Current use additionally requires a protected status floor and trusted time.
use super::checkpoint_trust_policy::{BftCheckpointTrustPolicyV1, issuer_status_bounds};
use super::formal_handoff::{canonical_bytes, decode_canonical, domain_digest};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

const STATUS_DOMAIN: &str = "oasis7.bft.checkpoint-renewal.v1.issuer-status";
const STATUS_CONTEXT: &str = "checkpoint issuer status";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCheckpointIssuerRevocationV1 {
    pub key_id: String,
    pub issuer_generation: u64,
    pub revoked_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCheckpointIssuerStatusBodyV1 {
    pub schema_version: u32,
    pub origin_digest: String,
    pub policy_digest: String,
    pub root_authorization_generation: u64,
    pub status_sequence: u64,
    pub issued_at_unix_ms: u64,
    pub not_before_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub policy_revoked: bool,
    pub issuer_keys: Vec<BftCheckpointIssuerRevocationV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BftCheckpointIssuerStatusV1 {
    body: BftCheckpointIssuerStatusBodyV1,
    signature_hex: String,
}

/// Root-authenticated snapshot within its original policy's authorized interval.
/// It carries neither current authority nor a protected revocation floor.
#[derive(Debug)]
pub struct AuthenticatedBftCheckpointIssuerStatusV1 {
    body: BftCheckpointIssuerStatusBodyV1,
    digest: String,
}

impl AuthenticatedBftCheckpointIssuerStatusV1 {
    pub fn decode(
        bytes: &[u8],
        policy: &BftCheckpointTrustPolicyV1,
        independent_policy_digest: &str,
    ) -> Result<Self, String> {
        let policy_digest = policy.digest()?;
        if policy_digest != independent_policy_digest {
            return Err("checkpoint issuer status policy differs from independent pin".into());
        }
        let max_bytes = usize::try_from(policy.max_status_bytes)
            .map_err(|_| "checkpoint issuer status byte bound is unsupported")?;
        issuer_status_bounds(bytes, max_bytes, policy.issuer_keys.len())?;
        let BftCheckpointIssuerStatusV1 {
            body,
            signature_hex,
        } = decode_canonical(bytes, max_bytes, STATUS_CONTEXT)?;
        if body.schema_version != 1
            || body.origin_digest != policy.origin.digest()?
            || body.policy_digest != policy_digest
            || body.root_authorization_generation != policy.root_authorization_generation
        {
            return Err("checkpoint issuer status origin, policy, or generation mismatch".into());
        }
        let start = policy
            .root_not_before_unix_ms
            .max(policy.policy_not_before_unix_ms);
        let end = policy
            .root_expires_at_unix_ms
            .min(policy.policy_expires_at_unix_ms);
        if body.not_before_unix_ms < start
            || body.issued_at_unix_ms > body.not_before_unix_ms
            || body.not_before_unix_ms >= body.expires_at_unix_ms
            || body.expires_at_unix_ms > end
        {
            return Err("checkpoint issuer status exceeds original authorization window".into());
        }
        if body.issuer_keys.len() != policy.issuer_keys.len()
            || body
                .issuer_keys
                .iter()
                .zip(&policy.issuer_keys)
                .any(|(status, key)| {
                    status.key_id != key.key_id
                        || status.issuer_generation != key.issuer_generation
                        || status
                            .revoked_at_unix_ms
                            .is_some_and(|time| time > body.issued_at_unix_ms)
                })
        {
            return Err(
                "checkpoint issuer status does not cover the exact policy key generations".into(),
            );
        }
        let digest = domain_digest(
            STATUS_DOMAIN,
            &canonical_bytes(&body, max_bytes, STATUS_CONTEXT)?,
        );
        let mut key_bytes = [0; 32];
        hex::decode_to_slice(&policy.renewal_root_public_key, &mut key_bytes)
            .map_err(|_| "invalid checkpoint renewal root key")?;
        let key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|_| "invalid checkpoint renewal root key")?;
        let mut signature = [0; 64];
        hex::decode_to_slice(&signature_hex, &mut signature)
            .map_err(|_| "invalid checkpoint issuer status signature")?;
        if hex::encode(signature) != signature_hex {
            return Err("noncanonical checkpoint issuer status signature".into());
        }
        let mut message = [0; 32];
        hex::decode_to_slice(&digest, &mut message)
            .map_err(|_| "invalid checkpoint issuer status digest")?;
        key.verify_strict(&message, &Signature::from_bytes(&signature))
            .map_err(|_| "checkpoint issuer status root signature rejected")?;
        Ok(Self { body, digest })
    }

    pub fn body(&self) -> &BftCheckpointIssuerStatusBodyV1 {
        &self.body
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}
