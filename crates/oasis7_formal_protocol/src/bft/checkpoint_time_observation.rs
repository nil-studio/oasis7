//! Authentication and portable arithmetic for an independently signed UTC
//! observation. Neither a signed observation nor a candidate interval proves
//! protected elapsed continuity, a persisted floor, or current authority.
use super::checkpoint_trust_policy::{BftCheckpointTrustPolicyV1, time_observation_bounds};
use super::formal_handoff::{canonical_bytes, decode_canonical, domain_digest, validate_hash};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

const DOMAIN: &str = "oasis7.bft.checkpoint-renewal.v1.time-observation";
const CONTEXT: &str = "checkpoint time observation";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCheckpointTimeObservationBodyV1 {
    pub schema_version: u32,
    pub origin_digest: String,
    pub policy_digest: String,
    pub clock_profile_digest: String,
    pub consumer_binding_digest: String,
    pub observation_nonce: String,
    pub clock_epoch: String,
    pub time_generation: u64,
    pub utc_earliest_unix_ms: u64,
    pub utc_latest_unix_ms: u64,
    pub time_source_id: String,
    pub time_key_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BftCheckpointTimeObservationV1 {
    body: BftCheckpointTimeObservationBodyV1,
    signature_hex: String,
}

/// The independently fixed request identity, not a claim that the request
/// was durably recorded before transmission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftCheckpointTimeChallengeV1 {
    pub consumer_binding_digest: String,
    pub observation_nonce: String,
    pub clock_epoch: String,
}

/// Raw inputs to portable interval arithmetic. A production caller must obtain
/// these from its original fenced protection domain, not from a reply, a new
/// attempt, a wall clock, or an unprotected process timer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftCheckpointTimeContinuityV1 {
    pub challenge: BftCheckpointTimeChallengeV1,
    pub elapsed_upper_ms: u64,
    pub utc_lower_floor_unix_ms: u64,
    pub previous_observation: Option<BftCheckpointPreviousObservationV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftCheckpointPreviousObservationV1 {
    pub time_generation: u64,
    pub clock_epoch: String,
    pub observation_nonce: String,
    pub observation_digest: String,
}

/// An arithmetic candidate only. The lower bound is the proposed UTC floor;
/// the upper bound must never be used as a durable lower floor. Current
/// authorization and atomic persistence must be established separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BftCheckpointUtcCandidateV1 {
    earliest_unix_ms: u64,
    latest_unix_ms: u64,
}

impl BftCheckpointUtcCandidateV1 {
    pub fn earliest_unix_ms(&self) -> u64 {
        self.earliest_unix_ms
    }

    pub fn latest_unix_ms(&self) -> u64 {
        self.latest_unix_ms
    }

    /// Check the entire closed uncertainty interval against an authorization
    /// window with an exclusive expiry.
    pub fn within_window(&self, not_before_unix_ms: u64, expires_at_unix_ms: u64) -> bool {
        not_before_unix_ms <= self.earliest_unix_ms && self.latest_unix_ms < expires_at_unix_ms
    }
}

#[derive(Debug)]
pub struct AuthenticatedBftCheckpointTimeObservationV1 {
    body: BftCheckpointTimeObservationBodyV1,
    digest: String,
    max_uncertainty_ms: u64,
    max_elapsed_ms: u64,
    time_key_not_before_unix_ms: u64,
    time_key_expires_at_unix_ms: u64,
}

impl AuthenticatedBftCheckpointTimeObservationV1 {
    /// Authenticate exact original bytes under the independently pinned
    /// policy, clock key, consumer and request. This is also usable for
    /// historical diagnostics; it does not establish current readiness.
    pub fn decode(
        bytes: &[u8],
        policy: &BftCheckpointTrustPolicyV1,
        independent_policy_digest: &str,
        challenge: &BftCheckpointTimeChallengeV1,
    ) -> Result<Self, String> {
        let policy_digest = policy.digest()?;
        if policy_digest != independent_policy_digest {
            return Err("checkpoint time observation differs from independent policy pin".into());
        }
        validate_hash("time consumer binding", &challenge.consumer_binding_digest)?;
        validate_hash("time observation nonce", &challenge.observation_nonce)?;
        validate_hash("time clock epoch", &challenge.clock_epoch)?;
        let maximum = usize::try_from(policy.max_time_evidence_bytes)
            .map_err(|_| "checkpoint time observation byte bound is unsupported")?;
        time_observation_bounds(bytes, maximum)?;
        let BftCheckpointTimeObservationV1 {
            body,
            signature_hex,
        } = decode_canonical(bytes, maximum, CONTEXT)?;
        let clock = &policy.clock_profile;
        if body.schema_version != 1
            || body.origin_digest != policy.origin.digest()?
            || body.policy_digest != policy_digest
            || body.clock_profile_digest != policy.clock_profile_digest
            || body.consumer_binding_digest != challenge.consumer_binding_digest
            || body.observation_nonce != challenge.observation_nonce
            || body.clock_epoch != challenge.clock_epoch
            || body.time_source_id != clock.time_source_id
            || body.time_key_id != clock.time_key_id
        {
            return Err("checkpoint time observation identity or challenge mismatch".into());
        }
        if body
            .utc_latest_unix_ms
            .checked_sub(body.utc_earliest_unix_ms)
            .is_none_or(|width| width > clock.max_clock_uncertainty_ms)
            || body.utc_earliest_unix_ms < clock.time_key_not_before_unix_ms
            || body.utc_latest_unix_ms >= clock.time_key_expires_at_unix_ms
        {
            return Err("checkpoint time observation exceeds source interval bounds".into());
        }
        let digest = domain_digest(DOMAIN, &canonical_bytes(&body, maximum, CONTEXT)?);
        let mut key_bytes = [0; 32];
        hex::decode_to_slice(&clock.time_public_key, &mut key_bytes)
            .map_err(|_| "invalid checkpoint time key")?;
        let key =
            VerifyingKey::from_bytes(&key_bytes).map_err(|_| "invalid checkpoint time key")?;
        let mut signature = [0; 64];
        hex::decode_to_slice(&signature_hex, &mut signature)
            .map_err(|_| "invalid checkpoint time observation signature")?;
        if hex::encode(signature) != signature_hex {
            return Err("noncanonical checkpoint time observation signature".into());
        }
        let mut message = [0; 32];
        hex::decode_to_slice(&digest, &mut message)
            .map_err(|_| "invalid checkpoint time observation digest")?;
        key.verify_strict(&message, &Signature::from_bytes(&signature))
            .map_err(|_| "checkpoint time observation signature rejected")?;
        Ok(Self {
            body,
            digest,
            max_uncertainty_ms: clock.max_clock_uncertainty_ms,
            max_elapsed_ms: clock.max_observation_elapsed_ms,
            time_key_not_before_unix_ms: clock.time_key_not_before_unix_ms,
            time_key_expires_at_unix_ms: clock.time_key_expires_at_unix_ms,
        })
    }

    pub fn body(&self) -> &BftCheckpointTimeObservationBodyV1 {
        &self.body
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Calculate a candidate using the original request-start upper bound.
    /// Missing or unproved continuity must be represented by None. A caller
    /// must persist the observation and lower floor under its real fence and
    /// separately check root, policy, status, safety and installation.
    pub fn candidate_interval(
        &self,
        continuity: Option<&BftCheckpointTimeContinuityV1>,
    ) -> Result<BftCheckpointUtcCandidateV1, String> {
        let sample = continuity.ok_or("checkpoint protected elapsed continuity unavailable")?;
        if sample.challenge.consumer_binding_digest != self.body.consumer_binding_digest
            || sample.challenge.observation_nonce != self.body.observation_nonce
            || sample.challenge.clock_epoch != self.body.clock_epoch
        {
            return Err("checkpoint time continuity differs from original request".into());
        }
        if let Some(previous) = &sample.previous_observation {
            validate_hash("previous time epoch", &previous.clock_epoch)?;
            validate_hash("previous time nonce", &previous.observation_nonce)?;
            validate_hash("previous time observation", &previous.observation_digest)?;
            if self.body.time_generation < previous.time_generation
                || (self.body.time_generation == previous.time_generation
                    && self.body.clock_epoch != previous.clock_epoch)
                || (self.body.observation_nonce == previous.observation_nonce
                    && self.digest != previous.observation_digest)
            {
                return Err("checkpoint time observation conflicts with protected history".into());
            }
        }
        if sample.elapsed_upper_ms > self.max_elapsed_ms {
            return Err("checkpoint time observation elapsed bound exceeded".into());
        }
        let latest = self
            .body
            .utc_latest_unix_ms
            .checked_add(sample.elapsed_upper_ms)
            .ok_or("checkpoint UTC upper bound overflow")?;
        if latest
            .checked_sub(self.body.utc_earliest_unix_ms)
            .is_none_or(|width| width > self.max_uncertainty_ms)
            || latest < sample.utc_lower_floor_unix_ms
        {
            return Err("checkpoint UTC uncertainty or protected floor rejected".into());
        }
        let candidate = BftCheckpointUtcCandidateV1 {
            earliest_unix_ms: self
                .body
                .utc_earliest_unix_ms
                .max(sample.utc_lower_floor_unix_ms),
            latest_unix_ms: latest,
        };
        if !candidate.within_window(
            self.time_key_not_before_unix_ms,
            self.time_key_expires_at_unix_ms,
        ) {
            return Err("checkpoint time key authorization expired or unavailable".into());
        }
        Ok(candidate)
    }
}

#[cfg(test)]
#[path = "checkpoint_time_observation_tests.rs"]
mod tests;
