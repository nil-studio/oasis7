//! Independent offline recovery authorization. Ordinary Owner approval is not accepted.
use super::RecoveryFence;
use crate::{DecimalU64, approval::bytes_digest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidRecoveryActivation;
type Result<T> = core::result::Result<T, InvalidRecoveryActivation>;
fn require(valid: bool) -> Result<()> {
    if valid {
        Ok(())
    } else {
        Err(InvalidRecoveryActivation)
    }
}
fn id(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 256
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
}
fn digest(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryAuthorityKeyV1 {
    pub key_id: String,
    pub public_key: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryAuthorityPolicyV1 {
    pub api_version: u8,
    pub environment: String,
    pub authority_id: String,
    pub quorum: u8,
    pub keys: Vec<RecoveryAuthorityKeyV1>,
}
impl RecoveryAuthorityPolicyV1 {
    pub fn validate(&self) -> Result<()> {
        require(
            self.api_version == 1
                && id(&self.environment)
                && id(&self.authority_id)
                && (2..=5).contains(&self.keys.len())
                && self.quorum >= 2
                && usize::from(self.quorum) <= self.keys.len(),
        )?;
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for key in &self.keys {
            require(
                id(&key.key_id)
                    && digest(&key.public_key)
                    && ids.insert(&key.key_id)
                    && keys.insert(&key.public_key),
            )?;
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        let bytes =
            oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.recovery-policy.v1", self))
                .map_err(|_| InvalidRecoveryActivation)?;
        Ok(bytes_digest(&bytes))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryActivationManifestV1 {
    pub api_version: u8,
    pub environment: String,
    pub authority_id: String,
    pub object_id: String,
    pub operation_id: String,
    pub policy_digest: String,
    pub previous_state_digest: String,
    pub evidence_digest: String,
    pub from_epoch: DecimalU64,
    pub to_epoch: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl RecoveryActivationManifestV1 {
    pub fn validate(&self) -> Result<()> {
        require(
            self.api_version == 1
                && [
                    &self.environment,
                    &self.authority_id,
                    &self.object_id,
                    &self.operation_id,
                ]
                .iter()
                .all(|v| id(v))
                && [
                    &self.policy_digest,
                    &self.previous_state_digest,
                    &self.evidence_digest,
                ]
                .iter()
                .all(|v| digest(v))
                && self.from_epoch.0.checked_add(1) == Some(self.to_epoch.0)
                && self.issued_at.0 < self.expires_at.0
                && self.expires_at.0 - self.issued_at.0 <= 300_000,
        )
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.recovery-activation.v1", self))
            .map_err(|_| InvalidRecoveryActivation)
    }
    pub fn digest(&self) -> Result<String> {
        Ok(bytes_digest(&self.signing_bytes()?))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryAuthoritySignatureV1 {
    pub key_id: String,
    pub signature: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRecoveryActivationV1 {
    pub manifest: RecoveryActivationManifestV1,
    pub signatures: Vec<RecoveryAuthoritySignatureV1>,
}
/// Reconstructed by the independent durable authority, never decoded from a request.
pub struct RecoveryActivationCurrent<'a> {
    pub environment: &'a str,
    pub authority_id: &'a str,
    pub object_id: &'a str,
    pub state_digest: &'a str,
    pub fence: &'a RecoveryFence,
}
pub trait RecoveryActivationVerifier {
    fn verify(&mut self, public_key: &str, signature: &str, bytes: &[u8]) -> Result<()>;
}
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryActivationSignatureCheck {
    pub public_key: String,
    pub signature: String,
    pub bytes: Vec<u8>,
}
impl SignedRecoveryActivationV1 {
    /// A verified next fence is a service rejection boundary, not an account grant.
    /// The adapter must atomically retain the old frozen epoch and signed original.
    pub fn authorize(
        &self,
        policy: &RecoveryAuthorityPolicyV1,
        current: &RecoveryActivationCurrent<'_>,
        now: u64,
        verifier: &mut impl RecoveryActivationVerifier,
    ) -> Result<RecoveryFence> {
        for check in self.signature_checks(policy, current, now)? {
            verifier.verify(&check.public_key, &check.signature, &check.bytes)?;
        }
        Ok(RecoveryFence::new(self.manifest.to_epoch.0, false))
    }
    /// Only constructs exact checks; the adapter must actually verify every check.
    pub fn signature_checks(
        &self,
        policy: &RecoveryAuthorityPolicyV1,
        current: &RecoveryActivationCurrent<'_>,
        now: u64,
    ) -> Result<Vec<RecoveryActivationSignatureCheck>> {
        policy.validate()?;
        let m = &self.manifest;
        let bytes = m.signing_bytes()?;
        require(
            m.environment == current.environment
                && m.environment == policy.environment
                && m.authority_id == current.authority_id
                && m.authority_id == policy.authority_id
                && m.object_id == current.object_id
                && m.previous_state_digest == current.state_digest
                && m.policy_digest == policy.digest()?
                && m.from_epoch.0 == current.fence.epoch()
                && m.issued_at.0 <= now
                && now < m.expires_at.0
                && self.signatures.len() >= usize::from(policy.quorum)
                && self.signatures.len() <= policy.keys.len(),
        )?;
        let mut seen = BTreeSet::new();
        let mut checks = Vec::new();
        for signature in &self.signatures {
            require(
                seen.insert(&signature.key_id)
                    && signature.signature.len() == 86
                    && signature
                        .signature
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
            )?;
            let key = policy
                .keys
                .iter()
                .find(|key| key.key_id == signature.key_id)
                .ok_or(InvalidRecoveryActivation)?;
            checks.push(RecoveryActivationSignatureCheck {
                public_key: key.public_key.clone(),
                signature: signature.signature.clone(),
                bytes: bytes.clone(),
            });
        }
        Ok(checks)
    }
}
