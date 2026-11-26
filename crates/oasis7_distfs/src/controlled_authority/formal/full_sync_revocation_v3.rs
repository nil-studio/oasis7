//! Unwired V3 revocation inventory and irreversible mode-floor primitives.
//! Local snapshots never establish protected fencing, verified cutoff or trusted time.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const SCHEMA_V1: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncRevocationLeaseV1 {
    pub lease: FullSyncIssuerTargetLeaseV3,
    pub target_generation: u64,
    pub expires_unix_ms: u64,
    pub decision_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FullSyncRevocationPhaseV1 {
    Committed,
    EnforcementPending,
    SignedExpiryObserved,
}

/// This candidate inventory is reconstructed from the full *verified* issuer
/// history. It is a cache/checklist, not a protected revocation receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncRevocationInventoryV1 {
    pub schema_version: u32,
    pub revocation_anchor: FullSyncIssuerAnchorV3,
    pub revocation_record_digest: String,
    pub target: FullSyncIssuerTargetV2,
    pub revoked_target_generation: u64,
    pub leases: BTreeMap<FullSyncIssuerTargetLeaseV3, FullSyncRevocationLeaseV1>,
    pub phase: FullSyncRevocationPhaseV1,
    pub observed_signed_expiry: Option<SignedFullSyncIssuerObservationV3>,
    pub original_fences: BTreeMap<FullSyncIssuerTargetLeaseV3, SignedFullSyncRevocationFenceV1>,
    pub target_gate_fence: Option<SignedFullSyncRevocationFenceV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullSyncRevocationSummaryV1 {
    pub original_lease_count: usize,
    pub pending_lease_count: usize,
    pub signed_expiry_observed: bool,
    pub effective: bool,
}

impl FullSyncRevocationInventoryV1 {
    /// Select an actual committed Revoke and enumerate every pre-revocation
    /// original lease by exact provider key, incarnation AND lease ID.
    /// Refuse a target subsequently reissued at another generation.
    pub fn from_verified_history(
        state: &VerifiedFullSyncIssuerStateV3,
        revoke_record_digest: &str,
    ) -> Result<Self, FormalError> {
        let entry = state
            .entries()
            .iter()
            .find(|item| item.record.digest().ok().as_deref() == Some(revoke_record_digest))
            .ok_or_else(|| FormalError::Conflict("V3 revocation record absent".into()))?;
        let FullSyncIssuerEventV3::Management(command) = &entry.record.event else {
            return Err(FormalError::Conflict(
                "V3 revocation must be a management event".into(),
            ));
        };
        if !matches!(&command.body.operation, FullSyncIssuerOperationV3::Revoke) {
            return Err(FormalError::Conflict(
                "V3 revocation command is not Revoke".into(),
            ));
        }
        let target = command.body.target.clone();
        // Later commands for this target supersede the exact lease generation.
        let superseded = state.entries().iter().any(|later| {
            later.record.next_anchor.revision > entry.record.next_anchor.revision
                && matches!(
                    &later.record.event,
                    FullSyncIssuerEventV3::Management(next) if next.body.target == target
                )
        });
        if superseded {
            return Err(FormalError::Conflict(
                "V3 revocation superseded by later target transition".into(),
            ));
        }
        let target_state = state
            .target_state(&target)
            .ok_or_else(|| FormalError::Conflict("V3 revocation target absent".into()))?;
        if target_state.active
            || target_state.generation == 0
            || entry.record.next_anchor.revision > state.anchor().revision
        {
            return Err(FormalError::Conflict(
                "V3 revocation target reissued or history incomplete".into(),
            ));
        }
        let leases = state
            .leases()
            .iter()
            .filter(|(_, value)| {
                value.lease.body.target == target
                    && value.lease.body.target_generation < target_state.generation
                    && value.lease.body.decision_revision < entry.record.next_anchor.revision
            })
            .map(|(key, value)| {
                (
                    key.clone(),
                    FullSyncRevocationLeaseV1 {
                        lease: key.clone(),
                        target_generation: value.lease.body.target_generation,
                        expires_unix_ms: value.lease.body.expires_unix_ms,
                        decision_revision: value.lease.body.decision_revision,
                    },
                )
            })
            .collect();
        Ok(Self {
            schema_version: SCHEMA_V1,
            revocation_anchor: entry.record.next_anchor.clone(),
            revocation_record_digest: revoke_record_digest.to_owned(),
            target,
            revoked_target_generation: target_state.generation,
            leases,
            phase: FullSyncRevocationPhaseV1::Committed,
            observed_signed_expiry: None,
            original_fences: BTreeMap::new(),
            target_gate_fence: None,
        })
    }

    /// A cold reader may never trust a replayed local status file instead of
    /// comparing every original row with signed, independently pinned history.
    pub fn verify_inventory(
        &self,
        state: &VerifiedFullSyncIssuerStateV3,
    ) -> Result<(), FormalError> {
        let original = Self::from_verified_history(state, &self.revocation_record_digest)?;
        if self.schema_version != SCHEMA_V1
            || self.revocation_anchor != original.revocation_anchor
            || self.target != original.target
            || self.revoked_target_generation != original.revoked_target_generation
            || self.leases != original.leases
        {
            return Err(FormalError::Rollback(
                "V3 cached revocation inventory differs from witness".into(),
            ));
        }
        if self.original_fences.len() > self.leases.len() {
            return Err(FormalError::Rollback(
                "original fence inventory exceeded signed leases".into(),
            ));
        }
        for (original, fence) in &self.original_fences {
            if fence.body.original_lease.as_ref() != Some(original) {
                return Err(FormalError::Rollback(
                    "original fence map key changed".into(),
                ));
            }
            fence.verify_for_inventory(
                self,
                state.anchor().revision,
                &state.policy().body().witness_public_key,
            )?;
        }
        if let Some(gate) = &self.target_gate_fence {
            if gate.body.original_lease.is_some() {
                return Err(FormalError::Rollback(
                    "target gate witness has an original lease".into(),
                ));
            }
            gate.verify_for_inventory(
                self,
                state.anchor().revision,
                &state.policy().body().witness_public_key,
            )?;
        }
        if let Some(observation) = &self.observed_signed_expiry {
            self.validate_expiry_observation(state, observation)?;
        } else if self.phase == FullSyncRevocationPhaseV1::SignedExpiryObserved {
            return Err(FormalError::Rollback(
                "V3 expiry phase lacks its signed witness".into(),
            ));
        }
        Ok(())
    }

    pub fn begin_enforcement(&mut self) {
        if self.phase == FullSyncRevocationPhaseV1::Committed {
            self.phase = FullSyncRevocationPhaseV1::EnforcementPending;
        }
    }

    fn validate_expiry_observation(
        &self,
        state: &VerifiedFullSyncIssuerStateV3,
        observation: &SignedFullSyncIssuerObservationV3,
    ) -> Result<(), FormalError> {
        observation.verify(state.policy())?;
        let observed = &observation.body.current_anchor;
        if observed.scope_digest != self.revocation_anchor.scope_digest
            || observed.policy_digest != self.revocation_anchor.policy_digest
            || observed.revision < self.revocation_anchor.revision
            || (observed.revision == self.revocation_anchor.revision
                && observed != &self.revocation_anchor)
            || !all_leases_expired(
                &self.leases,
                observation.body.protected_time.earliest_unix_ms,
            )
        {
            return Err(FormalError::Rollback(
                "V3 signed time cannot prove every original lease expired".into(),
            ));
        }
        Ok(())
    }

    /// A signed expiry is a bounded *candidate*, never an ACK, enforced
    /// isolation, irreversible mode switch or effective revocation.
    pub fn observe_signed_expiry(
        &mut self,
        state: &VerifiedFullSyncIssuerStateV3,
        observation: SignedFullSyncIssuerObservationV3,
    ) -> Result<(), FormalError> {
        self.verify_inventory(state)?;
        self.validate_expiry_observation(state, &observation)?;
        self.observed_signed_expiry = Some(observation);
        self.phase = FullSyncRevocationPhaseV1::SignedExpiryObserved;
        Ok(())
    }

    pub fn summary(&self) -> FullSyncRevocationSummaryV1 {
        FullSyncRevocationSummaryV1 {
            original_lease_count: self.leases.len(),
            pending_lease_count: self.leases.len(),
            signed_expiry_observed: self.phase == FullSyncRevocationPhaseV1::SignedExpiryObserved,
            effective: false,
        }
    }
}

fn all_leases_expired(
    leases: &BTreeMap<FullSyncIssuerTargetLeaseV3, FullSyncRevocationLeaseV1>,
    protected_earliest_unix_ms: u64,
) -> bool {
    leases
        .values()
        .all(|entry| entry.expires_unix_ms <= protected_earliest_unix_ms)
}

/// Irreversible post-migration policy floor. The caller must protect this
/// minimum independently; an ordinary disk cache is not anti-rollback proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncV3RequiredFloorV1 {
    pub schema_version: u32,
    pub migration_anchor: FullSyncIssuerAnchorV3,
    pub migration_record_digest: String,
}

impl FullSyncV3RequiredFloorV1 {
    pub fn from_verified_history(
        state: &VerifiedFullSyncIssuerStateV3,
    ) -> Result<Self, FormalError> {
        let first = state
            .entries()
            .first()
            .ok_or_else(|| FormalError::Rollback("V3 migration record missing".into()))?;
        if !matches!(&first.record.event, FullSyncIssuerEventV3::Migration(_)) {
            return Err(FormalError::Rollback(
                "V3 witness history does not begin with migration".into(),
            ));
        }
        Ok(Self {
            schema_version: SCHEMA_V1,
            migration_anchor: first.record.next_anchor.clone(),
            migration_record_digest: first.record.digest()?,
        })
    }

    pub fn require_v3(&self, state: &VerifiedFullSyncIssuerStateV3) -> Result<(), FormalError> {
        let actual = Self::from_verified_history(state)?;
        if self.schema_version != SCHEMA_V1
            || actual != *self
            || state.anchor().scope_digest != self.migration_anchor.scope_digest
            || state.anchor().policy_digest != self.migration_anchor.policy_digest
            || state.anchor().revision < self.migration_anchor.revision
        {
            return Err(FormalError::Rollback(
                "V3-required mode cannot downgrade or change original migration".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(incarnation: &str, id: &str, expires: u64) -> FullSyncRevocationLeaseV1 {
        let key = FullSyncIssuerTargetLeaseV3 {
            provider_public_key: "ab".repeat(32),
            provider_incarnation: incarnation.into(),
            lease_id: id.into(),
        };
        FullSyncRevocationLeaseV1 {
            lease: key,
            target_generation: 1,
            expires_unix_ms: expires,
            decision_revision: 3,
        }
    }

    #[test]
    fn original_lease_identity_includes_incarnation_and_lease() {
        let first = lease(&"01".repeat(32), &"03".repeat(32), 1_200);
        let other_instance = lease(&"02".repeat(32), &"03".repeat(32), 1_200);
        let other_lease = lease(&"01".repeat(32), &"04".repeat(32), 1_200);
        let mut all = BTreeMap::new();
        for item in [&first, &other_instance, &other_lease] {
            all.insert(item.lease.clone(), item.clone());
        }
        assert_eq!(all.len(), 3);
        assert!(!all_leases_expired(&all, 1_199));
        assert!(all_leases_expired(&all, 1_200));
    }

    #[test]
    fn original_expiry_boundary_requires_full_lease_set() {
        let first = lease(&"01".repeat(32), &"03".repeat(32), 999);
        let second = lease(&"01".repeat(32), &"04".repeat(32), 1_000);
        let mut all = BTreeMap::new();
        all.insert(first.lease.clone(), first);
        all.insert(second.lease.clone(), second);
        assert!(!all_leases_expired(&all, 999));
        assert!(all_leases_expired(&all, 1_000));
    }
}
