//! Original renewal evidence reader; this is not current deployment authority.
//!
//! The caller must obtain the previous status digest, elapsed continuity and
//! time floor from its independent fenced protection domain. These arguments
//! are checked for consistency but are not proven protected by this module.
use super::checkpoint_issuer_status::AuthenticatedBftCheckpointIssuerStatusV1;
use super::checkpoint_time_observation::{
    AuthenticatedBftCheckpointTimeObservationV1, BftCheckpointTimeChallengeV1,
    BftCheckpointTimeContinuityV1, BftCheckpointUtcCandidateV1,
};
use super::checkpoint_trust_policy::BftCheckpointTrustPolicyV1;

/// Portable, uninstalled renewal candidate bound to exact signed originals.
/// No signing, voting, trust-minimum update or durable state mutation occurs.
#[derive(Debug)]
pub struct PreparedBftCheckpointRenewalReadV1 {
    previous_status_digest: String,
    status_digest: String,
    time_digest: String,
    interval: BftCheckpointUtcCandidateV1,
}

impl PreparedBftCheckpointRenewalReadV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn verify(
        policy: &BftCheckpointTrustPolicyV1,
        independent_policy_digest: &str,
        previous_status_bytes: &[u8],
        independent_previous_status_digest: &str,
        current_status_bytes: &[u8],
        time_observation_bytes: &[u8],
        original_challenge: &BftCheckpointTimeChallengeV1,
        protected_continuity: Option<&BftCheckpointTimeContinuityV1>,
        original_old_key_constraints: &[u8],
        original_stake_constraints: &[u8],
    ) -> Result<Self, String> {
        // The pinned predecessor is essential: a newly supplied signed status
        // cannot replace an older revocation/floor with a convenient snapshot.
        let previous = AuthenticatedBftCheckpointIssuerStatusV1::decode(
            previous_status_bytes,
            policy,
            independent_policy_digest,
        )?;
        if previous.digest() != independent_previous_status_digest {
            return Err("checkpoint predecessor differs from independent floor".into());
        }
        let current = AuthenticatedBftCheckpointIssuerStatusV1::decode(
            current_status_bytes,
            policy,
            independent_policy_digest,
        )?;
        current.check_predecessor(&previous)?;
        policy.check_safety_constraint_originals(
            independent_policy_digest,
            original_old_key_constraints,
            original_stake_constraints,
        )?;
        let observed = AuthenticatedBftCheckpointTimeObservationV1::decode(
            time_observation_bytes,
            policy,
            independent_policy_digest,
            original_challenge,
        )?;
        let continuity =
            protected_continuity.ok_or("checkpoint protected continuity is required")?;
        if continuity.previous_observation.is_none() {
            return Err("checkpoint renewal requires protected time history".into());
        }
        let interval = current.check_candidate_window(policy, &observed, Some(continuity))?;
        Ok(Self {
            previous_status_digest: previous.digest().to_owned(),
            status_digest: current.digest().to_owned(),
            time_digest: observed.digest().to_owned(),
            interval,
        })
    }

    pub fn previous_status_digest(&self) -> &str {
        &self.previous_status_digest
    }

    pub fn status_digest(&self) -> &str {
        &self.status_digest
    }

    pub fn time_digest(&self) -> &str {
        &self.time_digest
    }

    pub fn interval(&self) -> &BftCheckpointUtcCandidateV1 {
        &self.interval
    }
}
