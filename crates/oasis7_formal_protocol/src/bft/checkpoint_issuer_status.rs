//! Authentication of a complete renewal issuer status snapshot.
//!
//! Current use additionally requires a protected status floor and trusted time.
use super::checkpoint_time_observation::{
    AuthenticatedBftCheckpointTimeObservationV1, BftCheckpointTimeContinuityV1,
    BftCheckpointUtcCandidateV1,
};
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
            || body.issued_at_unix_ms < start
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

    /// Compare two exact root-authenticated snapshots. The caller must obtain
    /// the previous snapshot from its actual protected floor for current use.
    /// This comparison alone establishes neither freshness nor persistence.
    pub fn check_predecessor(&self, previous: &Self) -> Result<(), String> {
        let old = &previous.body;
        let new = &self.body;
        if new.origin_digest != old.origin_digest
            || new.policy_digest != old.policy_digest
            || new.root_authorization_generation != old.root_authorization_generation
        {
            return Err("checkpoint issuer status predecessor identity mismatch".into());
        }
        if new.status_sequence < old.status_sequence
            || (new.status_sequence == old.status_sequence && self.digest != previous.digest)
        {
            return Err("checkpoint issuer status sequence rollback or conflict".into());
        }
        if old.policy_revoked && !new.policy_revoked {
            return Err("checkpoint issuer status cannot revive revoked policy".into());
        }
        if new.issuer_keys.len() != old.issuer_keys.len()
            || new
                .issuer_keys
                .iter()
                .zip(&old.issuer_keys)
                .any(|(next, prior)| {
                    next.key_id != prior.key_id
                        || next.issuer_generation != prior.issuer_generation
                        || prior
                            .revoked_at_unix_ms
                            .is_some_and(|revoked| next.revoked_at_unix_ms != Some(revoked))
                })
        {
            return Err("checkpoint issuer status cannot revise a revoked key".into());
        }
        Ok(())
    }

    /// Check freshness and revocation at the original signed observation.
    /// This is a historical diagnostic: a reply may have been delayed, and
    /// no protected elapsed continuity or status floor is established here.
    pub fn check_signed_observation_window(
        &self,
        policy: &BftCheckpointTrustPolicyV1,
        observation: &AuthenticatedBftCheckpointTimeObservationV1,
    ) -> Result<(), String> {
        let time = observation.body();
        self.check_interval(
            policy,
            observation,
            time.utc_earliest_unix_ms,
            time.utc_latest_unix_ms,
        )
    }

    /// Recheck the full elapsed-expanded interval, rather than treating an
    /// authenticated source observation as a fresh status at the action boundary.
    /// The supplied continuity is only an arithmetic input here: the caller
    /// must obtain it and the status floor from the actual protection domain.
    pub fn check_candidate_window(
        &self,
        policy: &BftCheckpointTrustPolicyV1,
        observation: &AuthenticatedBftCheckpointTimeObservationV1,
        continuity: Option<&BftCheckpointTimeContinuityV1>,
    ) -> Result<BftCheckpointUtcCandidateV1, String> {
        let candidate = observation.candidate_interval(continuity)?;
        self.check_interval(
            policy,
            observation,
            candidate.earliest_unix_ms(),
            candidate.latest_unix_ms(),
        )?;
        Ok(candidate)
    }

    fn check_interval(
        &self,
        policy: &BftCheckpointTrustPolicyV1,
        observation: &AuthenticatedBftCheckpointTimeObservationV1,
        earliest: u64,
        latest: u64,
    ) -> Result<(), String> {
        let time = observation.body();
        if policy.digest()? != self.body.policy_digest
            || time.policy_digest != self.body.policy_digest
            || time.origin_digest != self.body.origin_digest
        {
            return Err("checkpoint status and time observation policy mismatch".into());
        }
        if self.body.policy_revoked {
            return Err("checkpoint issuer status policy revoked".into());
        }
        if earliest < self.body.not_before_unix_ms || latest >= self.body.expires_at_unix_ms {
            return Err("checkpoint signed observation outside issuer status window".into());
        }
        if latest
            .checked_sub(self.body.issued_at_unix_ms)
            .is_none_or(|age| age > policy.max_status_age_ms)
        {
            return Err("checkpoint issuer status exceeds maximum age".into());
        }
        if earliest < policy.security_policy.security_not_before_unix_ms
            || latest >= policy.security_policy.absolute_safe_until_unix_ms
        {
            return Err("checkpoint observation outside independent security window".into());
        }
        // Potentially eligible keys are not actual verified proof signatures.
        let eligible = policy
            .issuer_keys
            .iter()
            .zip(&self.body.issuer_keys)
            .filter(|(key, status)| {
                status.revoked_at_unix_ms.is_none()
                    && key.not_before_unix_ms <= earliest
                    && latest < key.expires_at_unix_ms
            })
            .count() as u64;
        if eligible < policy.threshold {
            return Err("checkpoint issuer status has insufficient eligible keys".into());
        }
        Ok(())
    }

    pub fn body(&self) -> &BftCheckpointIssuerStatusBodyV1 {
        &self.body
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}
