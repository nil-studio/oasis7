use std::collections::HashSet;

use serde::Serialize;

use super::ed25519_signer_policy::{
    normalize_ed25519_public_key_allowlist, normalize_ed25519_public_key_hex,
};
use super::error::WorldError;
use super::signature::ED25519_SIGNATURE_V1_PREFIX;
use oasis7_proto::distributed_dht::MembershipDirectorySnapshot;

use super::util::to_canonical_cbor;
use super::{
    MembershipDirectorySigner, MembershipDirectorySignerKeyring, MembershipKeyRevocationAnnounce,
    MembershipRevocationSyncPolicy, MembershipSnapshotRestorePolicy,
};

fn extract_ed25519_signer_public_key(signature: &str) -> Result<Option<String>, WorldError> {
    if !signature.starts_with(ED25519_SIGNATURE_V1_PREFIX) {
        return Ok(None);
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
    let normalized_public_key =
        normalize_ed25519_public_key_hex(public_key_hex, "membership signature signer public key")?;
    Ok(Some(normalized_public_key))
}

#[derive(Serialize)]
struct MembershipDirectorySigningPayload<'a> {
    world_id: &'a str,
    requester_id: &'a str,
    requested_at_ms: i64,
    reason: &'a Option<String>,
    validators: &'a [String],
    quorum_threshold: usize,
    signature_key_id: &'a Option<String>,
}

pub(super) fn snapshot_signing_bytes(
    snapshot: &MembershipDirectorySnapshot,
) -> Result<Vec<u8>, WorldError> {
    let payload = MembershipDirectorySigningPayload {
        world_id: &snapshot.world_id,
        requester_id: &snapshot.requester_id,
        requested_at_ms: snapshot.requested_at_ms,
        reason: &snapshot.reason,
        validators: &snapshot.validators,
        quorum_threshold: snapshot.quorum_threshold,
        signature_key_id: &snapshot.signature_key_id,
    };
    to_canonical_cbor(&payload)
}

#[derive(Serialize)]
struct MembershipRevocationSigningPayload<'a> {
    world_id: &'a str,
    requester_id: &'a str,
    requested_at_ms: i64,
    key_id: &'a str,
    reason: &'a Option<String>,
    signature_key_id: &'a Option<String>,
}

pub(super) fn revocation_signing_bytes(
    announce: &MembershipKeyRevocationAnnounce,
) -> Result<Vec<u8>, WorldError> {
    let payload = MembershipRevocationSigningPayload {
        world_id: &announce.world_id,
        requester_id: &announce.requester_id,
        requested_at_ms: announce.requested_at_ms,
        key_id: &announce.key_id,
        reason: &announce.reason,
        signature_key_id: &announce.signature_key_id,
    };
    to_canonical_cbor(&payload)
}

pub(super) fn normalized_key_id(raw: &str) -> Result<String, WorldError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(WorldError::DistributedValidationFailed {
            reason: "membership signature key_id cannot be empty".to_string(),
        });
    }
    Ok(normalized.to_string())
}

pub(super) fn normalized_world_id(raw: &str) -> Result<String, WorldError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(WorldError::DistributedValidationFailed {
            reason: "membership world_id cannot be empty".to_string(),
        });
    }
    if normalized.contains('/') || normalized.contains('\\') || normalized.contains("..") {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!("membership world_id contains invalid path segment: {normalized}"),
        });
    }
    Ok(normalized.to_string())
}

pub(super) fn validate_key_revocation(
    world_id: &str,
    announce: &MembershipKeyRevocationAnnounce,
    signer: Option<&MembershipDirectorySigner>,
    keyring: Option<&MembershipDirectorySignerKeyring>,
    policy: &MembershipRevocationSyncPolicy,
    accepted_signature_signer_public_keys: Option<&HashSet<String>>,
) -> Result<(), WorldError> {
    if announce.world_id != world_id {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership revocation world mismatch: expected={world_id}, got={}",
                announce.world_id
            ),
        });
    }

    let _ = normalized_key_id(&announce.key_id)?;

    if !policy.trusted_requesters.is_empty()
        && !policy
            .trusted_requesters
            .iter()
            .any(|requester| requester == &announce.requester_id)
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership revocation requester {} is not trusted",
                announce.requester_id
            ),
        });
    }

    if !policy.authorized_requesters.is_empty()
        && !policy
            .authorized_requesters
            .iter()
            .any(|requester| requester == &announce.requester_id)
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership revocation requester {} is not authorized",
                announce.requester_id
            ),
        });
    }

    let has_signature = announce.signature.is_some();
    if !has_signature && announce.signature_key_id.is_some() {
        return Err(WorldError::DistributedValidationFailed {
            reason: "membership revocation contains signature_key_id without signature".to_string(),
        });
    }

    if policy.require_signature && !has_signature {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership revocation missing signature for requester {}",
                announce.requester_id
            ),
        });
    }

    if policy.require_signature_key_id
        && has_signature
        && announce.signature_key_id.as_deref().is_none()
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership revocation missing signature_key_id for requester {}",
                announce.requester_id
            ),
        });
    }

    if !policy.revoked_signature_key_ids.is_empty()
        && let Some(signature_key_id) = announce.signature_key_id.as_deref()
        && policy
            .revoked_signature_key_ids
            .iter()
            .any(|key_id| key_id == signature_key_id)
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership revocation signature_key_id {} is revoked",
                signature_key_id
            ),
        });
    }

    if !policy.accepted_signature_key_ids.is_empty() {
        let Some(signature_key_id) = announce.signature_key_id.as_deref() else {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership revocation signature_key_id is required for requester {}",
                    announce.requester_id
                ),
            });
        };
        if !policy
            .accepted_signature_key_ids
            .iter()
            .any(|key_id| key_id == signature_key_id)
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership revocation signature_key_id {} is not accepted",
                    signature_key_id
                ),
            });
        }
    }

    if let Some(accepted_signature_signer_public_keys) = accepted_signature_signer_public_keys {
        let Some(signature) = announce.signature.as_deref() else {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership revocation missing signature for requester {}",
                    announce.requester_id
                ),
            });
        };
        let Some(signature_signer_public_key) = extract_ed25519_signer_public_key(signature)?
        else {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership revocation signature signer public key is required for requester {}",
                    announce.requester_id
                ),
            });
        };
        if !accepted_signature_signer_public_keys.contains(&signature_signer_public_key) {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership revocation signature signer public key {} is not accepted",
                    signature_signer_public_key
                ),
            });
        }
    }

    if has_signature {
        if let Some(keyring) = keyring {
            keyring.verify_revocation(announce)?;
        } else if let Some(signer) = signer {
            signer.verify_revocation(announce)?;
        } else if policy.require_signature
            || policy.require_signature_key_id
            || !policy.accepted_signature_key_ids.is_empty()
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: "membership revocation verification requires signer or keyring".to_string(),
            });
        }
    }

    Ok(())
}

pub(super) fn validate_membership_snapshot(
    world_id: &str,
    snapshot: &MembershipDirectorySnapshot,
    signer: Option<&MembershipDirectorySigner>,
    keyring: Option<&MembershipDirectorySignerKeyring>,
    policy: &MembershipSnapshotRestorePolicy,
    accepted_signature_signer_public_keys: Option<&HashSet<String>>,
) -> Result<(), WorldError> {
    if snapshot.world_id != world_id {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership snapshot world mismatch: expected={world_id}, got={}",
                snapshot.world_id
            ),
        });
    }

    if !snapshot
        .validators
        .iter()
        .any(|validator| validator == &snapshot.requester_id)
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership snapshot requester {} is not in validator set",
                snapshot.requester_id
            ),
        });
    }

    if !policy.trusted_requesters.is_empty()
        && !policy
            .trusted_requesters
            .iter()
            .any(|requester| requester == &snapshot.requester_id)
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership snapshot requester {} is not trusted",
                snapshot.requester_id
            ),
        });
    }

    let requires_authentication = policy.require_signature
        || policy.require_signature_key_id
        || !policy.accepted_signature_key_ids.is_empty()
        || accepted_signature_signer_public_keys.is_some();
    let has_signature = snapshot.signature.is_some();
    if !has_signature && snapshot.signature_key_id.is_some() {
        return Err(WorldError::DistributedValidationFailed {
            reason: "membership snapshot contains signature_key_id without signature".to_string(),
        });
    }

    if requires_authentication && !has_signature {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership snapshot missing signature for requester {}",
                snapshot.requester_id
            ),
        });
    }

    if policy.require_signature_key_id
        && has_signature
        && snapshot.signature_key_id.as_deref().is_none()
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership snapshot missing signature_key_id for requester {}",
                snapshot.requester_id
            ),
        });
    }

    if !policy.revoked_signature_key_ids.is_empty()
        && let Some(signature_key_id) = snapshot.signature_key_id.as_deref()
        && policy
            .revoked_signature_key_ids
            .iter()
            .any(|key_id| key_id == signature_key_id)
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "membership snapshot signature_key_id {} is revoked",
                signature_key_id
            ),
        });
    }

    if !policy.accepted_signature_key_ids.is_empty() {
        let Some(signature_key_id) = snapshot.signature_key_id.as_deref() else {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership snapshot signature_key_id is required for requester {}",
                    snapshot.requester_id
                ),
            });
        };
        if !policy
            .accepted_signature_key_ids
            .iter()
            .any(|key_id| key_id == signature_key_id)
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership snapshot signature_key_id {} is not accepted",
                    signature_key_id
                ),
            });
        }
    }

    if let Some(accepted_signature_signer_public_keys) = accepted_signature_signer_public_keys {
        let Some(signature) = snapshot.signature.as_deref() else {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership snapshot missing signature for requester {}",
                    snapshot.requester_id
                ),
            });
        };
        let Some(signature_signer_public_key) = extract_ed25519_signer_public_key(signature)?
        else {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership snapshot signature signer public key is required for requester {}",
                    snapshot.requester_id
                ),
            });
        };
        if !accepted_signature_signer_public_keys.contains(&signature_signer_public_key) {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "membership snapshot signature signer public key {} is not accepted",
                    signature_signer_public_key
                ),
            });
        }
    }

    if has_signature {
        if let Some(keyring) = keyring {
            keyring.verify_snapshot(snapshot)?;
        } else if let Some(signer) = signer {
            signer.verify_snapshot(snapshot)?;
        } else if requires_authentication {
            return Err(WorldError::DistributedValidationFailed {
                reason: "membership snapshot verification requires signer or keyring".to_string(),
            });
        }
    }

    Ok(())
}

pub(super) fn validate_membership_snapshot_restore_policy(
    policy: &MembershipSnapshotRestorePolicy,
) -> Result<Option<HashSet<String>>, WorldError> {
    normalize_ed25519_public_key_allowlist(
        &policy.accepted_signature_signer_public_keys,
        "membership snapshot policy accepted_signature_signer_public_keys",
        "membership snapshot policy accepted_signature_signer_public_keys",
    )
}

pub(super) fn validate_membership_revocation_sync_policy(
    policy: &MembershipRevocationSyncPolicy,
) -> Result<Option<HashSet<String>>, WorldError> {
    normalize_ed25519_public_key_allowlist(
        &policy.accepted_signature_signer_public_keys,
        "membership revocation policy accepted_signature_signer_public_keys",
        "membership revocation policy accepted_signature_signer_public_keys",
    )
}
