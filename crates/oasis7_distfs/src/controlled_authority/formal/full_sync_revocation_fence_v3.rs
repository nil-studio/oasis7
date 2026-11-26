//! Unwired revocation proof envelope. The signing backend, not this DTO,
//! must guarantee protected per-instance cutoff, isolation, or a fully stopped
//! read gate. A signed assertion is not an OS/process observation by itself.
use super::*;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

const FENCE_SCHEMA: u32 = 1;
const FENCE_DOMAIN: &[u8] = b"oasis7.formal.full-sync-revocation.fence.v1\0";
const MAX_FENCE_BYTES: usize = 16 * 1024;

/// A protected authority must attest one *actual* enforcement fact, not an
/// observed manager flag, a DNS/list change, a local file, or an ACK from an
/// unrelated incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FullSyncFenceKindV1 {
    /// Exact old instance/lease was fenced before any further byte release.
    InstanceCutoff,
    /// Exact old process and all copies cannot serve that original lease.
    InstanceIsolated,
    /// Signed expiry has elapsed AND the exact old gate has stopped release.
    ExpiredGateStopped,
    /// The target gate has been irreversibly fenced at revocation revision.
    TargetGateStopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncRevocationFenceBodyV1 {
    pub schema_version: u32,
    pub revocation_record_digest: String,
    pub revocation_anchor: FullSyncIssuerAnchorV3,
    pub target: FullSyncIssuerTargetV2,
    pub revoked_target_generation: u64,
    pub original_lease: Option<FullSyncIssuerTargetLeaseV3>,
    pub kind: FullSyncFenceKindV1,
    pub protected_floor_revision: u64,
    pub protected_earliest_unix_ms: u64,
}

/// The exact signed bytes are verified with the externally pinned V3 witness
/// public key. The production witness MUST issue this only after protected
/// cutoff, irrevocable isolation, or signed expiry plus actual gate stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncRevocationFenceV1 {
    pub body: FullSyncRevocationFenceBodyV1,
    pub signature_hex: String,
}

impl SignedFullSyncRevocationFenceV1 {
    fn preimage(body: &FullSyncRevocationFenceBodyV1) -> Result<Vec<u8>, FormalError> {
        let bytes = serde_json::to_vec(body)
            .map_err(|error| FormalError::Invalid(error.to_string()))?;
        if bytes.len() > MAX_FENCE_BYTES {
            return Err(FormalError::Invalid("revocation fence exceeds bound".into()));
        }
        let mut value = Vec::with_capacity(FENCE_DOMAIN.len() + bytes.len());
        value.extend_from_slice(FENCE_DOMAIN);
        value.extend_from_slice(&bytes);
        Ok(value)
    }

    pub fn verify_signature(&self, protected_witness_public_key: &str) -> Result<(), FormalError> {
        let raw_key = hex::decode(protected_witness_public_key)
            .map_err(|_| FormalError::Invalid("revocation witness key encoding".into()))?;
        let key: [u8; 32] = raw_key.try_into()
            .map_err(|_| FormalError::Invalid("revocation witness key width".into()))?;
        if self.signature_hex.len() != 128
            || !self.signature_hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(FormalError::Invalid("revocation fence signature shape".into()));
        }
        let sig_bytes = hex::decode(&self.signature_hex)
            .map_err(|_| FormalError::Invalid("revocation fence signature encoding".into()))?;
        let signature = Signature::from_slice(&sig_bytes)
            .map_err(|_| FormalError::Invalid("revocation fence signature width".into()))?;
        let verifying = VerifyingKey::from_bytes(&key)
            .map_err(|_| FormalError::Invalid("revocation witness public key".into()))?;
        verifying.verify_strict(&Self::preimage(&self.body)?, &signature)
            .map_err(|_| FormalError::Rollback("revocation fence witness signature mismatch".into()))
    }

    /// Verify immutable original-revoke binding before accepting the receipt.
    /// A new same-name incarnation/other lease cannot close an old row. The
    /// current signed state head is always supplied by the caller, not a file.
    pub fn verify_for_inventory(
        &self,
        inventory: &FullSyncRevocationInventoryV1,
        current_revision: u64,
        witness_key: &str,
    ) -> Result<(), FormalError> {
        self.verify_signature(witness_key)?;
        let b = &self.body;
        if b.schema_version != FENCE_SCHEMA
            || b.revocation_record_digest != inventory.revocation_record_digest
            || b.revocation_anchor != inventory.revocation_anchor
            || b.target != inventory.target
            || b.revoked_target_generation != inventory.revoked_target_generation
            || b.protected_floor_revision < inventory.revocation_anchor.revision
            || b.protected_floor_revision > current_revision
            || b.revoked_target_generation == 0
        {
            return Err(FormalError::Rollback("fence does not bind the original revocation".into()));
        }
        match (&b.kind, &b.original_lease) {
            (FullSyncFenceKindV1::TargetGateStopped, None) => Ok(()),
            (FullSyncFenceKindV1::TargetGateStopped, Some(_))
            | (_, None) => Err(FormalError::Conflict("fence target/lease purpose mismatch".into())),
            (_, Some(lease_id)) => {
                let original = inventory.leases.get(lease_id)
                    .ok_or_else(|| FormalError::Conflict("unrecognized original lease fence".into()))?;
                if b.kind == FullSyncFenceKindV1::ExpiredGateStopped
                    && b.protected_earliest_unix_ms < original.expires_unix_ms
                {
                    return Err(FormalError::Conflict("original lease expiry unproven".into()));
                }
                Ok(())
            }
        }
    }

    #[cfg(test)]
    fn test_sign(
        body: FullSyncRevocationFenceBodyV1,
        signer: &ed25519_dalek::SigningKey,
    ) -> Self {
        use ed25519_dalek::Signer;
        Self {
            signature_hex: hex::encode(signer.sign(&Self::preimage(&body).unwrap()).to_bytes()),
            body,
        }
    }
}

impl FullSyncRevocationInventoryV1 {
    /// A protected proof is collected in this same signed-history-bound
    /// inventory; local status and unsigned ACKs cannot mark it effective.
    /// Cold-open always reconstructs the original signed issuer history.
    pub fn record_protected_fence(
        &mut self,
        state: &VerifiedFullSyncIssuerStateV3,
        receipt: SignedFullSyncRevocationFenceV1,
    ) -> Result<(), FormalError> {
        self.verify_inventory(state)?;
        receipt.verify_for_inventory(
            self,
            state.anchor().revision,
            &state.policy().body().witness_public_key,
        )?;
        if let Some(original) = receipt.body.original_lease.clone() {
            if let Some(existing) = self.original_fences.get(&original) {
                if existing == &receipt {
                    return Ok(());
                }
                return Err(FormalError::Conflict(
                    "original lease fence already committed with different bytes".into(),
                ));
            }
            self.original_fences.insert(original, receipt);
        } else if let Some(existing) = &self.target_gate_fence {
            if existing != &receipt {
                return Err(FormalError::Conflict(
                    "target gate fence already committed with different bytes".into(),
                ));
            }
        } else {
            self.target_gate_fence = Some(receipt);
        }
        Ok(())
    }

    /// Independently validate every retained protected proof and the full
    /// original lease inventory on *each* cold open and query. A cached
    /// phase/ACK alone is not evidence and cannot make a release effective.
    pub fn summary_verified(
        &self,
        state: &VerifiedFullSyncIssuerStateV3,
    ) -> Result<FullSyncRevocationSummaryV1, FormalError> {
        self.verify_inventory(state)?;
        Ok(FullSyncRevocationSummaryV1 {
            original_lease_count: self.leases.len(),
            pending_lease_count: self.leases.len().saturating_sub(self.original_fences.len()),
            signed_expiry_observed: self.phase == FullSyncRevocationPhaseV1::SignedExpiryObserved,
            effective: self.target_gate_fence.is_some()
                && self.original_fences.len() == self.leases.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    fn hash(b: u8) -> String { hex::encode([b; 32]) }
    fn anchor() -> FullSyncIssuerAnchorV3 {
        FullSyncIssuerAnchorV3 {
            schema_version: 3, revision: 9, scope_digest: hash(1),
            log_digest: hash(2), state_digest: hash(3),
            policy_digest: hash(4), signer_generation: 2,
        }
    }
    fn inventory() -> FullSyncRevocationInventoryV1 {
        let a = anchor();
        let target = FullSyncIssuerTargetV2 {
            node_public_key: hash(7), role: FullSyncReadRoleV2::StateSync,
        };
        let key = FullSyncIssuerTargetLeaseV3 {
            provider_public_key: hash(8), provider_incarnation: hash(9), lease_id: hash(10),
        };
        FullSyncRevocationInventoryV1 {
            schema_version: 1, revocation_anchor: a, revocation_record_digest: hash(11),
            target, revoked_target_generation: 2,
            leases: [(
                key.clone(),
                FullSyncRevocationLeaseV1 {
                    lease: key, target_generation: 1, expires_unix_ms: 5000, decision_revision: 8,
                }
            )].into(),
            phase: FullSyncRevocationPhaseV1::EnforcementPending,
            observed_signed_expiry: None,
            original_fences: Default::default(), target_gate_fence: None,
        }
    }
    #[test]
    fn exact_revocation_fence_signature_identity_and_expiry_are_required() {
        let inv = inventory();
        let k = SigningKey::from_bytes(&[42u8; 32]);
        let witness = hex::encode(k.verifying_key().to_bytes());
        let lease = inv.leases.keys().next().unwrap().clone();
        let make = |kind, original_lease, time| FullSyncRevocationFenceBodyV1 {
            schema_version: 1, revocation_record_digest: inv.revocation_record_digest.clone(),
            revocation_anchor: inv.revocation_anchor.clone(), target: inv.target.clone(),
            revoked_target_generation: 2, original_lease, kind,
            protected_floor_revision: 9, protected_earliest_unix_ms: time,
        };
        let right = SignedFullSyncRevocationFenceV1::test_sign(
            make(FullSyncFenceKindV1::InstanceCutoff, Some(lease.clone()), 100), &k,
        );
        right.verify_for_inventory(&inv, 9, &witness).unwrap();
        assert!(right.verify_for_inventory(&inv, 8, &witness).is_err());
        assert!(right.verify_for_inventory(&inv, 9, &hash(80)).is_err());
        let mut tampered = right.clone();
        tampered.body.protected_floor_revision = 11;
        assert!(tampered.verify_for_inventory(&inv, 11, &witness).is_err());
        let wrong_lease = FullSyncIssuerTargetLeaseV3 { lease_id: hash(12), ..lease.clone() };
        let wrong = SignedFullSyncRevocationFenceV1::test_sign(
            make(FullSyncFenceKindV1::InstanceCutoff, Some(wrong_lease), 100), &k,
        );
        assert!(wrong.verify_for_inventory(&inv, 9, &witness).is_err());
        let early = SignedFullSyncRevocationFenceV1::test_sign(
            make(FullSyncFenceKindV1::ExpiredGateStopped, Some(lease), 4999), &k,
        );
        assert!(early.verify_for_inventory(&inv, 9, &witness).is_err());
        let gate = SignedFullSyncRevocationFenceV1::test_sign(
            make(FullSyncFenceKindV1::TargetGateStopped, None, 100), &k,
        );
        gate.verify_for_inventory(&inv, 9, &witness).unwrap();
    }
}
