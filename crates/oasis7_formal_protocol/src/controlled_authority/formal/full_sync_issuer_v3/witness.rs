//! Witness-facing V3 observations, time assertions, and mode-fenced history.
use super::super::{
    FormalError, FullSyncIssuerAnchorV2, FullSyncIssuerCommittedEntryV2,
    SignedFullSyncIssuerObservationV2, SignedFullSyncIssuerQueryV2, VerifiedFullSyncIssuerPolicyV2,
    canonical, encode, invalid, verify,
};
use super::types::VerifiedFullSyncIssuerPolicyV3;
use super::types::{
    FullSyncIssuerAnchorV3, FullSyncIssuerCommittedEntryV3, FullSyncIssuerProtectedTimeIntervalV3,
    FullSyncIssuerRecordV3, MAX_ENTRY_BYTES_V3, OBSERVATION_DOMAIN_V3, QUERY_DOMAIN_V3,
    SignedFullSyncIssuerMigrationV3, SignedFullSyncIssuerReceiptV3,
};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

pub(super) const RESUME_QUERY_DOMAIN_V3: &str =
    "oasis7.formal.full-sync-issuer.migration-resume-query.v3";
pub(super) const RESUME_OBSERVATION_DOMAIN_V3: &str =
    "oasis7.formal.full-sync-issuer.migration-resume-observation.v3";

/// The active witness schema and head are signed in a migration-resume reply.
/// V2 and V3 heads are never interchangeable even when their revision matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FullSyncIssuerWitnessModeHeadV3 {
    V2(FullSyncIssuerAnchorV2),
    V3(FullSyncIssuerAnchorV3),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerQueryBodyV3 {
    pub schema_version: u32,
    pub lease_signer_public_key: String,
    pub policy_digest: String,
    pub scope_digest: String,
    pub observation_nonce: String,
    pub minimum_anchor: FullSyncIssuerAnchorV3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerQueryV3 {
    pub body: FullSyncIssuerQueryBodyV3,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerQueryV3 {
    pub fn sign(
        policy: &VerifiedFullSyncIssuerPolicyV3,
        minimum_anchor: FullSyncIssuerAnchorV3,
        nonce: [u8; 32],
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        if hex::encode(key.verifying_key().as_bytes()) != policy.body().lease_signer_public_key {
            return Err(invalid("full-sync V3 query signer mismatch"));
        }
        minimum_anchor.validate()?;
        if minimum_anchor.scope_digest != policy.scope_digest() {
            return Err(invalid("full-sync V3 query minimum scope"));
        }
        let body = FullSyncIssuerQueryBodyV3 {
            schema_version: 3,
            lease_signer_public_key: policy.body().lease_signer_public_key.clone(),
            policy_digest: policy.policy_digest().into(),
            scope_digest: policy.scope_digest().into(),
            observation_nonce: hex::encode(nonce),
            minimum_anchor,
        };
        validate_query_body_v3(&body)?;
        let signature_hex = hex::encode(
            key.sign(&encode(QUERY_DOMAIN_V3, &body, MAX_ENTRY_BYTES_V3)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify(&self, policy: &VerifiedFullSyncIssuerPolicyV3) -> Result<(), FormalError> {
        validate_query_body_v3(&self.body)?;
        if self.body.lease_signer_public_key != policy.body().lease_signer_public_key
            || self.body.policy_digest != policy.policy_digest()
            || self.body.scope_digest != policy.scope_digest()
            || self.body.minimum_anchor.scope_digest != policy.scope_digest()
        {
            return Err(invalid("full-sync V3 query policy/scope binding"));
        }
        verify(
            QUERY_DOMAIN_V3,
            &self.body,
            &self.signature_hex,
            &policy.body().lease_signer_public_key,
        )
    }
}

fn validate_query_body_v3(body: &FullSyncIssuerQueryBodyV3) -> Result<(), FormalError> {
    body.minimum_anchor.validate()?;
    for value in [
        &body.lease_signer_public_key,
        &body.policy_digest,
        &body.scope_digest,
        &body.observation_nonce,
    ] {
        canonical::<32>(value)?;
    }
    if body.schema_version != 3 || body.observation_nonce.bytes().all(|byte| byte == b'0') {
        return Err(invalid("full-sync V3 query version/nonce"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerObservationBodyV3 {
    pub schema_version: u32,
    pub query: SignedFullSyncIssuerQueryV3,
    pub current_anchor: FullSyncIssuerAnchorV3,
    pub protected_time: FullSyncIssuerProtectedTimeIntervalV3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerObservationV3 {
    pub body: FullSyncIssuerObservationBodyV3,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerObservationV3 {
    pub fn sign(
        body: FullSyncIssuerObservationBodyV3,
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        let signature_hex = hex::encode(
            key.sign(&encode(OBSERVATION_DOMAIN_V3, &body, MAX_ENTRY_BYTES_V3)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify(&self, policy: &VerifiedFullSyncIssuerPolicyV3) -> Result<(), FormalError> {
        let body = &self.body;
        body.query.verify(policy)?;
        body.current_anchor.validate()?;
        body.protected_time.validate(policy)?;
        if body.schema_version != 3
            || body.current_anchor.scope_digest != policy.scope_digest()
            || !anchor_v3_at_least(&body.current_anchor, &body.query.body.minimum_anchor)
        {
            return Err(FormalError::Rollback(
                "full-sync V3 witness observation floor".into(),
            ));
        }
        verify(
            OBSERVATION_DOMAIN_V3,
            body,
            &self.signature_hex,
            &policy.body().witness_public_key,
        )
    }
}

fn anchor_v3_at_least(current: &FullSyncIssuerAnchorV3, minimum: &FullSyncIssuerAnchorV3) -> bool {
    current.scope_digest == minimum.scope_digest
        && current.revision >= minimum.revision
        && (current.revision != minimum.revision || current == minimum)
}

impl FullSyncIssuerProtectedTimeIntervalV3 {
    pub fn validate(&self, policy: &VerifiedFullSyncIssuerPolicyV3) -> Result<(), FormalError> {
        if self.earliest_unix_ms > self.latest_unix_ms
            || self.latest_unix_ms.saturating_sub(self.earliest_unix_ms)
                > policy.body().max_clock_uncertainty_ms
            || self.clock_profile_digest != policy.body().clock_profile_digest
        {
            return Err(invalid("full-sync V3 protected time interval/profile"));
        }
        canonical::<32>(&self.clock_profile_digest).map(|_| ())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerMigrationResumeQueryBodyV3 {
    pub schema_version: u32,
    pub policy_digest: String,
    pub scope_digest: String,
    pub original_v2_query: SignedFullSyncIssuerQueryV2,
    pub exact_migration: SignedFullSyncIssuerMigrationV3,
    /// Fresh transport nonce; it is not the nonce in `original_v2_query`.
    pub resume_nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerMigrationResumeQueryV3 {
    pub body: FullSyncIssuerMigrationResumeQueryBodyV3,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerMigrationResumeQueryV3 {
    pub fn sign(
        policy: &VerifiedFullSyncIssuerPolicyV3,
        original_v2_query: SignedFullSyncIssuerQueryV2,
        exact_migration: SignedFullSyncIssuerMigrationV3,
        resume_nonce: [u8; 32],
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        if hex::encode(key.verifying_key().as_bytes()) != policy.body().lease_signer_public_key {
            return Err(invalid("full-sync V3 resume-query signer mismatch"));
        }
        let body = FullSyncIssuerMigrationResumeQueryBodyV3 {
            schema_version: 3,
            policy_digest: policy.policy_digest().into(),
            scope_digest: policy.scope_digest().into(),
            original_v2_query,
            exact_migration,
            resume_nonce: hex::encode(resume_nonce),
        };
        validate_resume_query_body_v3(&body)?;
        let signature_hex = hex::encode(
            key.sign(&encode(RESUME_QUERY_DOMAIN_V3, &body, MAX_ENTRY_BYTES_V3)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify(
        &self,
        old_policy: &VerifiedFullSyncIssuerPolicyV2,
        new_policy: &VerifiedFullSyncIssuerPolicyV3,
    ) -> Result<(), FormalError> {
        validate_resume_query_body_v3(&self.body)?;
        self.body.original_v2_query.verify(old_policy)?;
        self.body.exact_migration.verify(old_policy, new_policy)?;
        if self.body.policy_digest != new_policy.policy_digest()
            || self.body.scope_digest != new_policy.scope_digest()
            || self.body.original_v2_query.body.minimum_anchor
                != self.body.exact_migration.body.expected_v2_head
            || self.body.exact_migration.body.new_policy != *new_policy.signed()
            || self.body.resume_nonce == self.body.original_v2_query.body.observation_nonce
        {
            return Err(invalid("full-sync V3 migration-resume binding"));
        }
        verify(
            RESUME_QUERY_DOMAIN_V3,
            &self.body,
            &self.signature_hex,
            &new_policy.body().lease_signer_public_key,
        )
    }
}

fn validate_resume_query_body_v3(
    body: &FullSyncIssuerMigrationResumeQueryBodyV3,
) -> Result<(), FormalError> {
    for value in [&body.policy_digest, &body.scope_digest, &body.resume_nonce] {
        canonical::<32>(value)?;
    }
    if body.schema_version != 3 || body.resume_nonce.bytes().all(|byte| byte == b'0') {
        return Err(invalid("full-sync V3 migration-resume query identity"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerMigrationResumeObservationBodyV3 {
    pub schema_version: u32,
    pub query: SignedFullSyncIssuerMigrationResumeQueryV3,
    pub current_mode_head: FullSyncIssuerWitnessModeHeadV3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerMigrationResumeObservationV3 {
    pub body: FullSyncIssuerMigrationResumeObservationBodyV3,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerMigrationResumeObservationV3 {
    pub fn sign(
        body: FullSyncIssuerMigrationResumeObservationBodyV3,
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        let signature_hex = hex::encode(
            key.sign(&encode(
                RESUME_OBSERVATION_DOMAIN_V3,
                &body,
                MAX_ENTRY_BYTES_V3,
            )?)
            .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify(
        &self,
        old_policy: &VerifiedFullSyncIssuerPolicyV2,
        new_policy: &VerifiedFullSyncIssuerPolicyV3,
    ) -> Result<(), FormalError> {
        self.body.query.verify(old_policy, new_policy)?;
        match &self.body.current_mode_head {
            FullSyncIssuerWitnessModeHeadV3::V2(anchor) => {
                anchor.validate()?;
                if anchor.scope_digest != old_policy.scope_digest() {
                    return Err(invalid("full-sync resume V2 head scope"));
                }
            }
            FullSyncIssuerWitnessModeHeadV3::V3(anchor) => {
                anchor.validate()?;
                if anchor.scope_digest != new_policy.scope_digest() {
                    return Err(invalid("full-sync resume V3 head scope"));
                }
            }
        }
        if self.body.schema_version != 3 {
            return Err(invalid("full-sync V3 resume observation version"));
        }
        verify(
            RESUME_OBSERVATION_DOMAIN_V3,
            &self.body,
            &self.signature_hex,
            &new_policy.body().witness_public_key,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerHistoryV3 {
    pub v2_entries: Vec<FullSyncIssuerCommittedEntryV2>,
    /// Includes the unique migration event as its first V3 entry.
    pub v3_entries: Vec<FullSyncIssuerCommittedEntryV3>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerMigrationResumeBundleV3 {
    pub observation: SignedFullSyncIssuerMigrationResumeObservationV3,
    pub history: FullSyncIssuerHistoryV3,
    /// Present only when the exact requested migration is committed.
    pub original_migration_receipt: Option<SignedFullSyncIssuerReceiptV3>,
}

/// A V3 witness shares a single mode fence with every inherited V2 method.
/// Implementations must reject V2 observe/history/commit once migration wins.
/// Returned histories are full genesis-to-observed-head proofs, bounded by the
/// inherited 1,024-entry and 64 MiB transport requirements. Repeating the exact
/// observation and exact record for a CAS is idempotent and returns the original
/// receipt if committed; otherwise it may only attempt that same CAS. A reused
/// nonce or request identity with different bytes is a conflict. This permits
/// recovery from an uncertain response without re-signing or rebasing a lease.
pub trait FullSyncIssuerWitnessV3: super::super::FullSyncIssuerWitnessV2 {
    fn commit_migration_v3(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV2,
        record: &FullSyncIssuerRecordV3,
    ) -> Result<SignedFullSyncIssuerReceiptV3, FormalError>;

    fn observe_v3(
        &mut self,
        query: &SignedFullSyncIssuerQueryV3,
    ) -> Result<SignedFullSyncIssuerObservationV3, FormalError>;

    fn history_v3(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV3,
    ) -> Result<FullSyncIssuerHistoryV3, FormalError>;

    fn commit_v3(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV3,
        record: &FullSyncIssuerRecordV3,
    ) -> Result<SignedFullSyncIssuerReceiptV3, FormalError>;

    fn resume_migration_v3(
        &mut self,
        query: &SignedFullSyncIssuerMigrationResumeQueryV3,
    ) -> Result<FullSyncIssuerMigrationResumeBundleV3, FormalError>;
}
