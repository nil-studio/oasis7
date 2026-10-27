use super::distributed::{
    topic_membership, topic_membership_reconcile, topic_membership_revocation,
};
use super::distributed_consensus::{
    ConsensusMembershipChange, ConsensusMembershipChangeRequest, ConsensusMembershipChangeResult,
    QuorumConsensus,
};
use super::distributed_dht::{DistributedDht, MembershipDirectorySnapshot};
use super::distributed_net::{DistributedNetwork, NetworkSubscription};
use super::error::WorldError;
use super::membership_logic;
use super::signature::ED25519_SIGNATURE_V1_PREFIX;
use super::tiered_file_log;
pub(super) use super::util::to_canonical_cbor;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hmac::{Hmac, KeyInit, Mac};
use oasis7_cas::LocalCasStore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type HmacSha256 = Hmac<Sha256>;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipDirectoryAnnounce {
    pub world_id: String,
    pub requester_id: String,
    pub requested_at_ms: i64,
    pub reason: Option<String>,
    pub validators: Vec<String>,
    pub quorum_threshold: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}
impl MembershipDirectoryAnnounce {
    pub fn from_membership_change(
        world_id: &str,
        request: &ConsensusMembershipChangeRequest,
        result: &ConsensusMembershipChangeResult,
    ) -> Self {
        Self {
            world_id: world_id.to_string(),
            requester_id: request.requester_id.clone(),
            requested_at_ms: request.requested_at_ms,
            reason: request.reason.clone(),
            validators: result.validators.clone(),
            quorum_threshold: result.quorum_threshold,
            signature_key_id: None,
            signature: None,
        }
    }

    pub fn into_snapshot(self) -> MembershipDirectorySnapshot {
        MembershipDirectorySnapshot {
            world_id: self.world_id,
            requester_id: self.requester_id,
            requested_at_ms: self.requested_at_ms,
            reason: self.reason,
            validators: self.validators,
            quorum_threshold: self.quorum_threshold,
            signature_key_id: self.signature_key_id,
            signature: self.signature,
        }
    }
}

impl From<MembershipDirectorySnapshot> for MembershipDirectoryAnnounce {
    fn from(value: MembershipDirectorySnapshot) -> Self {
        Self {
            world_id: value.world_id,
            requester_id: value.requester_id,
            requested_at_ms: value.requested_at_ms,
            reason: value.reason,
            validators: value.validators,
            quorum_threshold: value.quorum_threshold,
            signature_key_id: value.signature_key_id,
            signature: value.signature,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipKeyRevocationAnnounce {
    pub world_id: String,
    pub requester_id: String,
    pub requested_at_ms: i64,
    pub key_id: String,
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MembershipDirectorySigner {
    kind: MembershipDirectorySignerKind,
}

#[derive(Debug, Clone)]
enum MembershipDirectorySignerKind {
    HmacSha256 {
        key: Vec<u8>,
    },
    Ed25519 {
        signing_key: Box<SigningKey>,
        public_key_hex: String,
    },
}

impl MembershipDirectorySigner {
    pub fn hmac_sha256(key: impl Into<Vec<u8>>) -> Self {
        Self {
            kind: MembershipDirectorySignerKind::HmacSha256 { key: key.into() },
        }
    }

    pub fn ed25519(private_key_hex: &str, public_key_hex: &str) -> Result<Self, WorldError> {
        let private_key =
            decode_hex_array::<32>(private_key_hex, "membership ed25519 private key")?;
        let signing_key = SigningKey::from_bytes(&private_key);
        let expected_public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
        if expected_public_key_hex != public_key_hex {
            return Err(WorldError::DistributedValidationFailed {
                reason: "membership ed25519 public key does not match private key".to_string(),
            });
        }
        Ok(Self {
            kind: MembershipDirectorySignerKind::Ed25519 {
                signing_key: Box::new(signing_key),
                public_key_hex: public_key_hex.to_string(),
            },
        })
    }

    pub fn sign_snapshot(
        &self,
        snapshot: &MembershipDirectorySnapshot,
    ) -> Result<String, WorldError> {
        let payload = membership_logic::snapshot_signing_bytes(snapshot)?;
        match &self.kind {
            MembershipDirectorySignerKind::HmacSha256 { key } => {
                let mut mac =
                    HmacSha256::new_from_slice(key).map_err(|_| WorldError::SignatureKeyInvalid)?;
                mac.update(&payload);
                Ok(hex::encode(mac.finalize().into_bytes()))
            }
            MembershipDirectorySignerKind::Ed25519 {
                signing_key,
                public_key_hex,
            } => {
                let signature: Signature = signing_key.sign(payload.as_slice());
                Ok(format!(
                    "{ED25519_SIGNATURE_V1_PREFIX}{}:{}",
                    public_key_hex,
                    hex::encode(signature.to_bytes())
                ))
            }
        }
    }

    pub fn verify_snapshot(
        &self,
        snapshot: &MembershipDirectorySnapshot,
    ) -> Result<(), WorldError> {
        let Some(signature_hex) = snapshot.signature.as_deref() else {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership snapshot missing signature for requester {}",
                    snapshot.requester_id
                ),
            });
        };
        let payload = membership_logic::snapshot_signing_bytes(snapshot)?;
        match &self.kind {
            MembershipDirectorySignerKind::HmacSha256 { key } => {
                let signature = hex::decode(signature_hex).map_err(|_| {
                    WorldError::DistributedValidationFailed {
                        reason: format!(
                            "membership snapshot signature is not valid hex for requester {}",
                            snapshot.requester_id
                        ),
                    }
                })?;
                let mut mac =
                    HmacSha256::new_from_slice(key).map_err(|_| WorldError::SignatureKeyInvalid)?;
                mac.update(&payload);
                mac.verify_slice(&signature)
                    .map_err(|_| WorldError::DistributedValidationFailed {
                        reason: format!(
                            "membership snapshot signature mismatch for requester {}",
                            snapshot.requester_id
                        ),
                    })
            }
            MembershipDirectorySignerKind::Ed25519 {
                signing_key,
                public_key_hex,
            } => {
                let (signature_public_key_hex, signature_hex) =
                    parse_ed25519_signature(signature_hex).map_err(|_| {
                        WorldError::DistributedValidationFailed {
                            reason: format!(
                        "membership snapshot signature is not valid ed25519:v1 for requester {}",
                        snapshot.requester_id
                    ),
                        }
                    })?;
                if signature_public_key_hex != public_key_hex {
                    return Err(WorldError::DistributedValidationFailed {
                        reason: format!(
                            "membership snapshot signature signer public key mismatch for requester {}",
                            snapshot.requester_id
                        ),
                    });
                }
                let signature_bytes =
                    decode_hex_array::<64>(signature_hex, "membership snapshot signature")
                        .map_err(|_| WorldError::DistributedValidationFailed {
                            reason: format!(
                                "membership snapshot signature is not valid hex for requester {}",
                                snapshot.requester_id
                            ),
                        })?;
                let verifying_key = signing_key.verifying_key();
                let signature = Signature::from_bytes(&signature_bytes);
                verifying_key
                    .verify(payload.as_slice(), &signature)
                    .map_err(|_| WorldError::DistributedValidationFailed {
                        reason: format!(
                            "membership snapshot signature mismatch for requester {}",
                            snapshot.requester_id
                        ),
                    })
            }
        }
    }

    pub fn sign_revocation(
        &self,
        announce: &MembershipKeyRevocationAnnounce,
    ) -> Result<String, WorldError> {
        let payload = membership_logic::revocation_signing_bytes(announce)?;
        match &self.kind {
            MembershipDirectorySignerKind::HmacSha256 { key } => {
                let mut mac =
                    HmacSha256::new_from_slice(key).map_err(|_| WorldError::SignatureKeyInvalid)?;
                mac.update(&payload);
                Ok(hex::encode(mac.finalize().into_bytes()))
            }
            MembershipDirectorySignerKind::Ed25519 {
                signing_key,
                public_key_hex,
            } => {
                let signature: Signature = signing_key.sign(payload.as_slice());
                Ok(format!(
                    "{ED25519_SIGNATURE_V1_PREFIX}{}:{}",
                    public_key_hex,
                    hex::encode(signature.to_bytes())
                ))
            }
        }
    }

    pub fn verify_revocation(
        &self,
        announce: &MembershipKeyRevocationAnnounce,
    ) -> Result<(), WorldError> {
        let Some(signature_hex) = announce.signature.as_deref() else {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership revocation missing signature for requester {}",
                    announce.requester_id
                ),
            });
        };
        let payload = membership_logic::revocation_signing_bytes(announce)?;
        match &self.kind {
            MembershipDirectorySignerKind::HmacSha256 { key } => {
                let signature = hex::decode(signature_hex).map_err(|_| {
                    WorldError::DistributedValidationFailed {
                        reason: format!(
                            "membership revocation signature is not valid hex for requester {}",
                            announce.requester_id
                        ),
                    }
                })?;
                let mut mac =
                    HmacSha256::new_from_slice(key).map_err(|_| WorldError::SignatureKeyInvalid)?;
                mac.update(&payload);
                mac.verify_slice(&signature)
                    .map_err(|_| WorldError::DistributedValidationFailed {
                        reason: format!(
                            "membership revocation signature mismatch for requester {}",
                            announce.requester_id
                        ),
                    })
            }
            MembershipDirectorySignerKind::Ed25519 {
                signing_key,
                public_key_hex,
            } => {
                let (signature_public_key_hex, signature_hex) =
                    parse_ed25519_signature(signature_hex).map_err(|_| {
                        WorldError::DistributedValidationFailed {
                            reason: format!(
                        "membership revocation signature is not valid ed25519:v1 for requester {}",
                        announce.requester_id
                    ),
                        }
                    })?;
                if signature_public_key_hex != public_key_hex {
                    return Err(WorldError::DistributedValidationFailed {
                        reason: format!(
                            "membership revocation signature signer public key mismatch for requester {}",
                            announce.requester_id
                        ),
                    });
                }
                let signature_bytes =
                    decode_hex_array::<64>(signature_hex, "membership revocation signature")
                        .map_err(|_| WorldError::DistributedValidationFailed {
                            reason: format!(
                                "membership revocation signature is not valid hex for requester {}",
                                announce.requester_id
                            ),
                        })?;
                let verifying_key = signing_key.verifying_key();
                let signature = Signature::from_bytes(&signature_bytes);
                verifying_key
                    .verify(payload.as_slice(), &signature)
                    .map_err(|_| WorldError::DistributedValidationFailed {
                        reason: format!(
                            "membership revocation signature mismatch for requester {}",
                            announce.requester_id
                        ),
                    })
            }
        }
    }
}

fn parse_ed25519_signature(signature: &str) -> Result<(&str, &str), WorldError> {
    if !signature.starts_with(ED25519_SIGNATURE_V1_PREFIX) {
        return Err(WorldError::DistributedValidationFailed {
            reason: "membership signature must use ed25519:v1 format".to_string(),
        });
    }
    let encoded = &signature[ED25519_SIGNATURE_V1_PREFIX.len()..];
    let (public_key_hex, signature_hex) =
        encoded
            .split_once(':')
            .ok_or_else(|| WorldError::DistributedValidationFailed {
                reason: "membership signature must include signer public key and signature hex"
                    .to_string(),
            })?;
    if public_key_hex.is_empty() || signature_hex.is_empty() {
        return Err(WorldError::DistributedValidationFailed {
            reason: "membership signature must include signer public key and signature hex"
                .to_string(),
        });
    }
    let _ = VerifyingKey::from_bytes(&decode_hex_array::<32>(
        public_key_hex,
        "membership signature public key",
    )?)
    .map_err(|_| WorldError::DistributedValidationFailed {
        reason: "membership signature public key is invalid".to_string(),
    })?;
    Ok((public_key_hex, signature_hex))
}

fn decode_hex_array<const N: usize>(input: &str, field: &str) -> Result<[u8; N], WorldError> {
    let bytes = hex::decode(input).map_err(|_| WorldError::DistributedValidationFailed {
        reason: format!("{field} must be valid hex"),
    })?;
    bytes
        .try_into()
        .map_err(|_| WorldError::DistributedValidationFailed {
            reason: format!("{field} must be {N}-byte hex"),
        })
}

#[derive(Debug, Clone, Default)]
pub struct MembershipDirectorySignerKeyring {
    active_key_id: Option<String>,
    signers: BTreeMap<String, MembershipDirectorySigner>,
    revoked_key_ids: BTreeSet<String>,
}

impl MembershipDirectorySignerKeyring {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_hmac_sha256_key(
        &mut self,
        key_id: impl Into<String>,
        key: impl Into<Vec<u8>>,
    ) -> Result<(), WorldError> {
        let key_id = key_id.into();
        let key_id = membership_logic::normalized_key_id(&key_id)?;
        if self.signers.contains_key(&key_id) {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!("membership signing key already exists: {key_id}"),
            });
        }
        self.signers
            .insert(key_id, MembershipDirectorySigner::hmac_sha256(key));
        Ok(())
    }

    pub fn add_ed25519_key(
        &mut self,
        key_id: impl Into<String>,
        private_key_hex: &str,
        public_key_hex: &str,
    ) -> Result<(), WorldError> {
        let key_id = key_id.into();
        let key_id = membership_logic::normalized_key_id(&key_id)?;
        if self.signers.contains_key(&key_id) {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!("membership signing key already exists: {key_id}"),
            });
        }
        let signer = MembershipDirectorySigner::ed25519(private_key_hex, public_key_hex)?;
        self.signers.insert(key_id, signer);
        Ok(())
    }

    pub fn set_active_key(&mut self, key_id: &str) -> Result<(), WorldError> {
        let key_id = membership_logic::normalized_key_id(key_id)?;
        if !self.signers.contains_key(&key_id) {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!("membership signing key not found: {key_id}"),
            });
        }
        if self.revoked_key_ids.contains(&key_id) {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!("membership signing key is revoked: {key_id}"),
            });
        }
        self.active_key_id = Some(key_id);
        Ok(())
    }

    pub fn active_key_id(&self) -> Option<&str> {
        self.active_key_id.as_deref()
    }

    pub fn revoke_key(&mut self, key_id: &str) -> Result<bool, WorldError> {
        let key_id = membership_logic::normalized_key_id(key_id)?;
        let inserted = self.revoked_key_ids.insert(key_id.clone());
        if self.active_key_id.as_deref() == Some(key_id.as_str()) {
            self.active_key_id = None;
        }
        Ok(inserted)
    }

    pub fn is_key_revoked(&self, key_id: &str) -> bool {
        let normalized = key_id.trim();
        if normalized.is_empty() {
            return false;
        }
        self.revoked_key_ids.contains(normalized)
    }

    pub fn revoked_keys(&self) -> Vec<String> {
        self.revoked_key_ids.iter().cloned().collect()
    }

    pub fn sign_snapshot_with_active_key(
        &self,
        snapshot: &MembershipDirectorySnapshot,
    ) -> Result<(String, String), WorldError> {
        let active_key_id = self.active_key_id.as_deref().ok_or_else(|| {
            WorldError::DistributedValidationFailed {
                reason: "membership signing keyring has no active key".to_string(),
            }
        })?;
        let signature = self.sign_snapshot_with_key_id(active_key_id, snapshot)?;
        Ok((active_key_id.to_string(), signature))
    }

    pub fn sign_snapshot_with_key_id(
        &self,
        key_id: &str,
        snapshot: &MembershipDirectorySnapshot,
    ) -> Result<String, WorldError> {
        let key_id = membership_logic::normalized_key_id(key_id)?;
        if self.revoked_key_ids.contains(&key_id) {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!("membership signing key is revoked: {key_id}"),
            });
        }
        let signer =
            self.signers
                .get(&key_id)
                .ok_or_else(|| WorldError::DistributedValidationFailed {
                    reason: format!("membership signing key not found: {key_id}"),
                })?;
        let mut signable = snapshot.clone();
        signable.signature_key_id = Some(key_id);
        signable.signature = None;
        signer.sign_snapshot(&signable)
    }

    pub fn sign_revocation_with_active_key(
        &self,
        announce: &MembershipKeyRevocationAnnounce,
    ) -> Result<(String, String), WorldError> {
        let active_key_id = self.active_key_id.as_deref().ok_or_else(|| {
            WorldError::DistributedValidationFailed {
                reason: "membership signing keyring has no active key".to_string(),
            }
        })?;
        let signature = self.sign_revocation_with_key_id(active_key_id, announce)?;
        Ok((active_key_id.to_string(), signature))
    }

    pub fn sign_revocation_with_key_id(
        &self,
        key_id: &str,
        announce: &MembershipKeyRevocationAnnounce,
    ) -> Result<String, WorldError> {
        let key_id = membership_logic::normalized_key_id(key_id)?;
        if self.revoked_key_ids.contains(&key_id) {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!("membership signing key is revoked: {key_id}"),
            });
        }
        let signer =
            self.signers
                .get(&key_id)
                .ok_or_else(|| WorldError::DistributedValidationFailed {
                    reason: format!("membership signing key not found: {key_id}"),
                })?;
        let mut signable = announce.clone();
        signable.signature_key_id = Some(key_id);
        signable.signature = None;
        signer.sign_revocation(&signable)
    }

    pub fn verify_snapshot(
        &self,
        snapshot: &MembershipDirectorySnapshot,
    ) -> Result<(), WorldError> {
        let Some(signature_hex) = snapshot.signature.as_deref() else {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership snapshot missing signature for requester {}",
                    snapshot.requester_id
                ),
            });
        };
        if signature_hex.is_empty() {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership snapshot signature is empty for requester {}",
                    snapshot.requester_id
                ),
            });
        }

        if let Some(key_id) = snapshot.signature_key_id.as_deref() {
            let key_id = membership_logic::normalized_key_id(key_id)?;
            if self.revoked_key_ids.contains(&key_id) {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!("membership signature key_id is revoked: {key_id}"),
                });
            }
            let signer = self.signers.get(&key_id).ok_or_else(|| {
                WorldError::DistributedValidationFailed {
                    reason: format!("membership signature key_id is unknown: {key_id}"),
                }
            })?;
            return signer.verify_snapshot(snapshot);
        }

        let mut try_order: Vec<&MembershipDirectorySigner> = Vec::new();
        if let Some(active_key_id) = self.active_key_id.as_deref()
            && let Some(active_signer) = self.signers.get(active_key_id)
            && !self.revoked_key_ids.contains(active_key_id)
        {
            try_order.push(active_signer);
        }
        for (key_id, signer) in &self.signers {
            if self.active_key_id.as_deref() != Some(key_id.as_str())
                && !self.revoked_key_ids.contains(key_id)
            {
                try_order.push(signer);
            }
        }

        for signer in try_order {
            if signer.verify_snapshot(snapshot).is_ok() {
                return Ok(());
            }
        }

        Err(WorldError::DistributedValidationFailed {
            reason: "membership snapshot verification failed for all non-revoked keys in keyring"
                .to_string(),
        })
    }

    pub fn verify_revocation(
        &self,
        announce: &MembershipKeyRevocationAnnounce,
    ) -> Result<(), WorldError> {
        let Some(signature_hex) = announce.signature.as_deref() else {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership revocation missing signature for requester {}",
                    announce.requester_id
                ),
            });
        };
        if signature_hex.is_empty() {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership revocation signature is empty for requester {}",
                    announce.requester_id
                ),
            });
        }

        if let Some(key_id) = announce.signature_key_id.as_deref() {
            let key_id = membership_logic::normalized_key_id(key_id)?;
            if self.revoked_key_ids.contains(&key_id) {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!("membership revocation signature key_id is revoked: {key_id}"),
                });
            }
            let signer = self.signers.get(&key_id).ok_or_else(|| {
                WorldError::DistributedValidationFailed {
                    reason: format!("membership revocation signature key_id is unknown: {key_id}"),
                }
            })?;
            return signer.verify_revocation(announce);
        }

        let mut try_order: Vec<&MembershipDirectorySigner> = Vec::new();
        if let Some(active_key_id) = self.active_key_id.as_deref()
            && let Some(active_signer) = self.signers.get(active_key_id)
            && !self.revoked_key_ids.contains(active_key_id)
        {
            try_order.push(active_signer);
        }
        for (key_id, signer) in &self.signers {
            if self.active_key_id.as_deref() != Some(key_id.as_str())
                && !self.revoked_key_ids.contains(key_id)
            {
                try_order.push(signer);
            }
        }

        for signer in try_order {
            if signer.verify_revocation(announce).is_ok() {
                return Ok(());
            }
        }

        Err(WorldError::DistributedValidationFailed {
            reason: "membership revocation verification failed for all non-revoked keys in keyring"
                .to_string(),
        })
    }
}

/// Authentication constraints require a signature and an applicable verifier.
/// An unconstrained policy without a verifier retains unauthenticated diagnostic
/// compatibility; it does not establish a verified signer or BFT authority.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MembershipSnapshotRestorePolicy {
    pub trusted_requesters: Vec<String>,
    pub require_signature: bool,
    pub require_signature_key_id: bool,
    pub accepted_signature_key_ids: Vec<String>,
    pub accepted_signature_signer_public_keys: Vec<String>,
    pub revoked_signature_key_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MembershipRevocationSyncPolicy {
    pub trusted_requesters: Vec<String>,
    pub authorized_requesters: Vec<String>,
    pub require_signature: bool,
    pub require_signature_key_id: bool,
    pub accepted_signature_key_ids: Vec<String>,
    pub accepted_signature_signer_public_keys: Vec<String>,
    pub revoked_signature_key_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipSnapshotAuditOutcome {
    MissingSnapshot,
    Applied,
    Ignored,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipSnapshotAuditRecord {
    pub world_id: String,
    pub requester_id: Option<String>,
    pub requested_at_ms: Option<i64>,
    pub signature_key_id: Option<String>,
    pub outcome: MembershipSnapshotAuditOutcome,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipRestoreAuditReport {
    pub restored: Option<ConsensusMembershipChangeResult>,
    pub audit: MembershipSnapshotAuditRecord,
}

pub trait MembershipAuditStore {
    fn append(&self, record: &MembershipSnapshotAuditRecord) -> Result<(), WorldError>;
    fn list(&self, world_id: &str) -> Result<Vec<MembershipSnapshotAuditRecord>, WorldError>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryMembershipAuditStore {
    records: Arc<Mutex<Vec<MembershipSnapshotAuditRecord>>>,
}

impl InMemoryMembershipAuditStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl MembershipAuditStore for InMemoryMembershipAuditStore {
    fn append(&self, record: &MembershipSnapshotAuditRecord) -> Result<(), WorldError> {
        let mut records = self.records.lock().expect("lock membership audit records");
        records.push(record.clone());
        Ok(())
    }

    fn list(&self, world_id: &str) -> Result<Vec<MembershipSnapshotAuditRecord>, WorldError> {
        let records = self.records.lock().expect("lock membership audit records");
        Ok(records
            .iter()
            .filter(|record| record.world_id == world_id)
            .cloned()
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct FileMembershipAuditStore {
    root_dir: PathBuf,
    cas_store: LocalCasStore,
}

impl FileMembershipAuditStore {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        let root_dir = root_dir.into();
        Self {
            cas_store: LocalCasStore::new(root_dir.join("cas")),
            root_dir,
        }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    fn world_log_path(&self, world_id: &str) -> Result<PathBuf, WorldError> {
        let world_id = membership_logic::normalized_world_id(world_id)?;
        Ok(self.root_dir.join(format!("{world_id}.jsonl")))
    }

    fn world_cold_refs_path(&self, world_id: &str) -> Result<PathBuf, WorldError> {
        let world_id = membership_logic::normalized_world_id(world_id)?;
        Ok(self.root_dir.join(format!("{world_id}.cold.refs.jsonl")))
    }
}

const MEMBERSHIP_AUDIT_HOT_MAX_RECORDS: usize = 4096;
const MEMBERSHIP_AUDIT_COLD_SEGMENT_MAX_LINES: usize = 256;

impl MembershipAuditStore for FileMembershipAuditStore {
    fn append(&self, record: &MembershipSnapshotAuditRecord) -> Result<(), WorldError> {
        let path = self.world_log_path(&record.world_id)?;
        let cold_refs_path = self.world_cold_refs_path(&record.world_id)?;
        let line = serde_json::to_string(record)?;
        tiered_file_log::append_jsonl_line_with_cas_offload(
            path.as_path(),
            cold_refs_path.as_path(),
            &self.cas_store,
            MEMBERSHIP_AUDIT_HOT_MAX_RECORDS,
            MEMBERSHIP_AUDIT_COLD_SEGMENT_MAX_LINES,
            line.as_str(),
        )
    }

    fn list(&self, world_id: &str) -> Result<Vec<MembershipSnapshotAuditRecord>, WorldError> {
        let path = self.world_log_path(world_id)?;
        let cold_refs_path = self.world_cold_refs_path(world_id)?;
        let lines = tiered_file_log::collect_jsonl_lines_with_cas_refs(
            path.as_path(),
            cold_refs_path.as_path(),
            &self.cas_store,
        )?;
        let mut records = Vec::new();
        for line in lines {
            records.push(serde_json::from_str(line.as_str())?);
        }
        Ok(records)
    }
}

#[derive(Debug, Clone)]
pub struct MembershipSyncSubscription {
    pub membership_sub: NetworkSubscription,
    pub revocation_sub: NetworkSubscription,
    pub reconcile_sub: NetworkSubscription,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipSyncReport {
    pub drained: usize,
    pub applied: usize,
    pub ignored: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipRevocationSyncReport {
    pub drained: usize,
    pub applied: usize,
    pub ignored: usize,
    pub rejected: usize,
}

#[derive(Clone)]
pub struct MembershipSyncClient {
    pub(crate) network: Arc<dyn DistributedNetwork + Send + Sync>,
}
