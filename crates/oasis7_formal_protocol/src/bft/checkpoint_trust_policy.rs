//! Bounded renewal configuration preparation, not an authorization capability.
//!
//! A syntactically valid policy is never its own independent deployment pin.
//! Current status, protected time, material closure and installation are separate
//! requirements; this module cannot authorize renewal, serving or signing.
#[path = "checkpoint_trust_bounds.rs"]
mod bounds;
use super::formal_handoff::{canonical_bytes, decode_canonical, domain_digest, validate_hash};
use crate::controlled_authority::formal::{FormalHeadRefV1, FormalWorldIdentityV1};
pub(super) use bounds::issuer_status_bounds;
pub use bounds::preflight as preflight_checkpoint_config;
use oasis7_proto::distributed_bft::v2::BftScopeV2;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, SeqAccess, Visitor},
};
use std::fmt;

pub const BFT_CHECKPOINT_POLICY_MAX_BYTES_V1: usize = 64 * 1024;
pub const BFT_CHECKPOINT_CONFIG_MAX_BYTES_V1: usize = 2 * 1024 * 1024;
const PREFIX: &str = "oasis7.bft.checkpoint-renewal.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCheckpointOriginV1 {
    pub formal_world: FormalWorldIdentityV1,
    pub bft_scope: BftScopeV2,
    pub original_c0_checkpoint_digest: String,
    pub initial_activation_digest: String,
    pub handoff_head: FormalHeadRefV1,
    pub handoff_target_digest: String,
    pub handoff_activation_digest: String,
    pub handoff_raw_snapshot_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCheckpointSecurityPolicyV1 {
    pub schema_version: u32,
    pub profile: String,
    pub origin_digest: String,
    pub old_key_constraints_digest: String,
    pub stake_constraints_digest: String,
    pub security_not_before_unix_ms: u64,
    pub absolute_safe_until_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCheckpointClockProfileV1 {
    pub schema_version: u32,
    pub profile: String,
    pub time_source_id: String,
    pub time_key_id: String,
    pub time_public_key: String,
    pub time_key_not_before_unix_ms: u64,
    pub time_key_expires_at_unix_ms: u64,
    pub max_clock_uncertainty_ms: u64,
    pub max_observation_elapsed_ms: u64,
    pub elapsed_profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCheckpointIssuerKeyV1 {
    pub issuer_id: String,
    pub key_id: String,
    pub public_key: String,
    pub issuer_generation: u64,
    pub not_before_unix_ms: u64,
    pub expires_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCheckpointTrustPolicyV1 {
    pub schema_version: u32,
    pub profile: String,
    pub profile_version: u32,
    pub wire_version: u16,
    pub origin: BftCheckpointOriginV1,
    pub policy_id: String,
    pub policy_version: u64,
    pub authorization_ref: String,
    pub renewal_root_public_key: String,
    pub root_authorization_generation: u64,
    pub root_authorized_at_unix_ms: u64,
    pub root_not_before_unix_ms: u64,
    pub root_expires_at_unix_ms: u64,
    pub max_root_revocation_staleness_ms: u64,
    pub policy_not_before_unix_ms: u64,
    pub policy_expires_at_unix_ms: u64,
    pub purpose: String,
    #[serde(deserialize_with = "bounded_issuers")]
    pub issuer_keys: Vec<BftCheckpointIssuerKeyV1>,
    pub threshold: u64,
    pub security_policy: BftCheckpointSecurityPolicyV1,
    pub security_assumptions_digest: String,
    pub max_checkpoint_validity_ms: u64,
    pub max_status_age_ms: u64,
    pub minimum_ledger_id: String,
    pub max_head_observation_age_ms: u64,
    pub clock_profile: BftCheckpointClockProfileV1,
    pub clock_profile_digest: String,
    pub max_issuer_keys: u64,
    pub max_status_bytes: u64,
    pub max_time_evidence_bytes: u64,
    pub max_update_proof_bytes: u64,
    pub max_safety_evidence_bytes: u64,
    pub max_update_decisions: u64,
    pub max_decision_archive_bytes: u64,
}

fn bounded_issuers<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<BftCheckpointIssuerKeyV1>, D::Error> {
    struct Issuers;
    impl<'de> Visitor<'de> for Issuers {
        type Value = Vec<BftCheckpointIssuerKeyV1>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most 256 issuer keys")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > 256) {
                return Err(de::Error::custom("issuer count exceeds bound"));
            }
            let mut keys = Vec::new();
            while let Some(key) = seq.next_element()? {
                if keys.len() == 256 {
                    return Err(de::Error::custom("issuer count exceeds bound"));
                }
                keys.push(key);
            }
            Ok(keys)
        }
    }
    d.deserialize_seq(Issuers)
}

fn nonempty(value: &str) -> Result<(), String> {
    if value.is_empty() {
        Err("renewal ID must not be empty".into())
    } else {
        Ok(())
    }
}
fn window(start: u64, end: u64) -> Result<(), String> {
    if start >= end {
        Err("empty renewal authorization window".into())
    } else {
        Ok(())
    }
}
fn public_key(value: &str) -> Result<(), String> {
    validate_hash("renewal public key", value)?;
    let bytes: [u8; 32] = hex::decode(value)
        .map_err(|_| "invalid public key")?
        .try_into()
        .map_err(|_| "invalid public key length")?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid public key")?;
    if key.is_weak() {
        return Err("weak renewal public key".into());
    }
    Ok(())
}
fn digest<T: Serialize>(suffix: &str, value: &T) -> Result<String, String> {
    Ok(domain_digest(
        &format!("{PREFIX}.{suffix}"),
        &canonical_bytes(
            value,
            BFT_CHECKPOINT_POLICY_MAX_BYTES_V1,
            "checkpoint policy component",
        )?,
    ))
}

impl BftCheckpointOriginV1 {
    pub fn digest(&self) -> Result<String, String> {
        digest("origin", self)
    }
    fn validate(&self) -> Result<(), String> {
        for id in [
            &self.formal_world.world_id,
            &self.formal_world.chain_id,
            &self.bft_scope.world_id,
            &self.bft_scope.chain_id,
        ] {
            nonempty(id)?;
        }
        // The two genesis fields retain their different protocol meanings.
        for hash in [
            &self.formal_world.genesis_digest,
            &self.bft_scope.genesis_hash,
            &self.original_c0_checkpoint_digest,
            &self.initial_activation_digest,
            &self.handoff_head.record_id,
            &self.handoff_target_digest,
            &self.handoff_activation_digest,
            &self.handoff_raw_snapshot_root,
        ] {
            validate_hash("renewal origin", hash)?;
        }
        if self.handoff_head.position == 0 || self.handoff_head.position.checked_add(1).is_none() {
            return Err("invalid renewal H position".into());
        }
        Ok(())
    }
}

impl BftCheckpointTrustPolicyV1 {
    /// Only validates bounded configuration consistency, never deployment authority.
    pub fn validate_configuration(&self) -> Result<(), String> {
        canonical_bytes(
            self,
            BFT_CHECKPOINT_POLICY_MAX_BYTES_V1,
            "checkpoint policy",
        )?;
        if self.schema_version != 1
            || self.profile != "bft"
            || self.profile_version != 1
            || self.wire_version != 3
            || self.purpose != "bft-checkpoint-renewal-v1"
        {
            return Err("unsupported renewal policy version or purpose".into());
        }
        self.origin.validate()?;
        for id in [
            &self.policy_id,
            &self.authorization_ref,
            &self.minimum_ledger_id,
        ] {
            nonempty(id)?;
        }
        public_key(&self.renewal_root_public_key)?;
        window(self.root_not_before_unix_ms, self.root_expires_at_unix_ms)?;
        window(
            self.policy_not_before_unix_ms,
            self.policy_expires_at_unix_ms,
        )?;
        if self.root_authorized_at_unix_ms > self.root_not_before_unix_ms
            || self
                .root_expires_at_unix_ms
                .checked_sub(self.root_authorized_at_unix_ms)
                .is_none_or(|duration| duration > self.max_root_revocation_staleness_ms)
        {
            return Err("root authorization exceeds revocation bound".into());
        }
        if self.max_issuer_keys == 0
            || self.max_issuer_keys > 256
            || self.threshold == 0
            || self.threshold > self.issuer_keys.len() as u64
            || self.issuer_keys.len() as u64 > self.max_issuer_keys
        {
            return Err("invalid renewal issuer threshold or capacity".into());
        }
        let mut previous: Option<&str> = None;
        let mut distinct_keys = std::collections::BTreeSet::new();
        for key in &self.issuer_keys {
            nonempty(&key.issuer_id)?;
            nonempty(&key.key_id)?;
            if previous.is_some_and(|id| id >= key.key_id.as_str()) {
                return Err("issuer keys must be sorted and unique".into());
            }
            previous = Some(&key.key_id);
            public_key(&key.public_key)?;
            if !distinct_keys.insert(&key.public_key) {
                return Err("duplicate issuer cryptographic key".into());
            }
            window(key.not_before_unix_ms, key.expires_at_unix_ms)?;
        }
        for duration in [
            self.max_root_revocation_staleness_ms,
            self.max_checkpoint_validity_ms,
            self.max_status_age_ms,
            self.max_head_observation_age_ms,
            self.clock_profile.max_observation_elapsed_ms,
        ] {
            if duration == 0 {
                return Err("renewal duration must be positive".into());
            }
        }
        for cap in [
            self.max_status_bytes,
            self.max_time_evidence_bytes,
            self.max_update_proof_bytes,
            self.max_safety_evidence_bytes,
        ] {
            if cap == 0 || cap > BFT_CHECKPOINT_CONFIG_MAX_BYTES_V1 as u64 {
                return Err("renewal material capacity exceeds bound".into());
            }
        }
        if !(1..=65_536).contains(&self.max_update_decisions)
            || !(1..=4 * 1024 * 1024 * 1024).contains(&self.max_decision_archive_bytes)
        {
            return Err("renewal decision archive exceeds bound".into());
        }
        let security = &self.security_policy;
        if security.schema_version != 1
            || security.profile != "independent-absolute-bound-v1"
            || security.origin_digest != self.origin.digest()?
        {
            return Err("unsupported or foreign renewal security policy".into());
        }
        validate_hash("old key constraints", &security.old_key_constraints_digest)?;
        validate_hash("stake constraints", &security.stake_constraints_digest)?;
        window(
            security.security_not_before_unix_ms,
            security.absolute_safe_until_unix_ms,
        )?;
        if self.security_assumptions_digest != digest("security-policy", security)? {
            return Err("renewal security policy digest mismatch".into());
        }
        let clock = &self.clock_profile;
        if clock.schema_version != 1
            || clock.profile != "bounded-utc-observation-v1"
            || clock.elapsed_profile != "protected-elapsed-upper-bound-v1"
        {
            return Err("unsupported renewal clock profile".into());
        }
        nonempty(&clock.time_source_id)?;
        nonempty(&clock.time_key_id)?;
        public_key(&clock.time_public_key)?;
        window(
            clock.time_key_not_before_unix_ms,
            clock.time_key_expires_at_unix_ms,
        )?;
        if self.clock_profile_digest != digest("clock-profile", clock)? {
            return Err("renewal clock profile digest mismatch".into());
        }
        // A root key reused as an issuer could forge root-signed revocations;
        // an issuer reused as the clock could manufacture time observations.
        // Independent fault domains still require deployment evidence, but
        // the fixed policy must not explicitly collapse these roles.
        if clock.time_public_key == self.renewal_root_public_key
            || self.issuer_keys.iter().any(|key| {
                key.public_key == self.renewal_root_public_key
                    || key.public_key == clock.time_public_key
            })
        {
            return Err("checkpoint root, issuer, and time signing keys must be distinct".into());
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_configuration()?;
        canonical_bytes(
            self,
            BFT_CHECKPOINT_POLICY_MAX_BYTES_V1,
            "checkpoint policy",
        )
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > BFT_CHECKPOINT_POLICY_MAX_BYTES_V1
            || !bytes.starts_with(&[0xd9, 0xd9, 0xf7])
        {
            return Err("invalid bounded policy framing".into());
        }
        bounds::policy_issuer_bounds(&bytes[3..])?;
        let policy: Self = decode_canonical(
            bytes,
            BFT_CHECKPOINT_POLICY_MAX_BYTES_V1,
            "checkpoint policy",
        )?;
        policy.validate_configuration()?;
        Ok(policy)
    }

    pub fn digest(&self) -> Result<String, String> {
        self.validate_configuration()?;
        digest("policy", self)
    }
}
