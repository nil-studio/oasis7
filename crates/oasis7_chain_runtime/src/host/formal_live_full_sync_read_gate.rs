//! Full-sync V3 lease consumer. This module accepts raw signed evidence from a
//! required source and reconstructs it locally; it never trusts a source's
//! “verified” flag or a lease signature in isolation.

use super::{FormalReadError, FormalReadResult};
use oasis7_client_api::world_service::WorldIdentity;
use oasis7_client_api::world_service_full_sync::{
    FormalFullSyncCutV1, FormalNodeReadGrantV1, FormalNodeReadRoleV1,
};
use oasis7_distfs::controlled_authority::formal::InitialActivationTrustV1;
use oasis7_distfs::controlled_authority::formal::full_sync_issuer::{
    FullSyncIssuerAnchorV3, FullSyncIssuerHistoryV3, FullSyncIssuerTargetV2,
    FullSyncProviderLeaseStateV3, FullSyncReadRoleV2, SignedFullSyncIssuerObservationV3,
    SignedFullSyncIssuerPolicyV2, SignedFullSyncIssuerPolicyV3, SignedFullSyncIssuerQueryV3,
    VerifiedFullSyncIssuerPolicyV2, VerifiedFullSyncIssuerPolicyV3, VerifiedFullSyncIssuerStateV3,
    full_sync_activation_trust_digest_v3,
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MAX_HISTORY_BYTES: usize = 64 * 1024 * 1024;

#[cfg(feature = "node-libp2p")]
#[path = "formal_bft_private_read_gate.rs"]
mod bft_private_read_gate;

#[path = "formal_full_sync_remote_evidence.rs"]
pub(crate) mod remote_evidence;

pub(crate) fn validate_full_sync_read_mode_gate_pair(
    mode: crate::formal_live_config::FullSyncProviderReadAuthorizationModeV1,
    has_gate: bool,
) -> Result<(), String> {
    use crate::formal_live_config::FullSyncProviderReadAuthorizationModeV1 as Mode;

    match mode {
        Mode::LegacyV1 if !has_gate => Ok(()),
        Mode::V3Required if has_gate => Ok(()),
        Mode::LegacyV1 => Err("full-sync V3 gate dependencies conflict with legacy_v1 mode".into()),
        Mode::V3Required => Err(
            "full-sync V3Required is unavailable: no production witness/history and protected-clock adapter was supplied".into(),
        ),
    }
}

/// The request is created by the provider. The evidence source must treat its
/// values as the exact intended query and return raw issuer/witness objects.
#[derive(Debug, Clone)]
pub(crate) struct FullSyncIssuerReadEvidenceRequestV3 {
    pub observation_nonce: [u8; 32],
    pub minimum_anchor: FullSyncIssuerAnchorV3,
    pub provider_public_key: String,
    pub provider_incarnation: String,
    pub target: FullSyncIssuerTargetV2,
    pub target_generation: u64,
    pub started_at: Instant,
    pub deadline: Instant,
}

/// Raw query, observation, and complete history. Every member is verified by
/// the provider before it can affect the installed state or authorize a reply.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FullSyncIssuerReadEvidenceV3 {
    pub query: SignedFullSyncIssuerQueryV3,
    pub observation: SignedFullSyncIssuerObservationV3,
    pub history: FullSyncIssuerHistoryV3,
}

pub(crate) trait FullSyncIssuerReadEvidenceSource: Send + Sync {
    /// Implementations must honor `deadline`; production must use the actual
    /// issuer/witness transport and independently protected clock profile.
    fn fetch(
        &self,
        request: &FullSyncIssuerReadEvidenceRequestV3,
    ) -> FormalReadResult<FullSyncIssuerReadEvidenceV3>;
}

/// Dependencies explicitly supplied by a host-side issuer/witness adapter.
/// The host derives the trust, provider signer, and current-process
/// incarnation; this object cannot choose those identities.
pub(crate) struct FullSyncV3ReadGateConfig {
    pub signed_v2_policy: SignedFullSyncIssuerPolicyV2,
    pub signed_v3_policy: SignedFullSyncIssuerPolicyV3,
    pub minimum_anchor: FullSyncIssuerAnchorV3,
    pub source: Arc<dyn FullSyncIssuerReadEvidenceSource>,
}

/// Untrusted candidate-read claims. Only reconstructed V3 issuer state and
/// an exact current lease can authorize this subject. The Node must separately
/// authenticate the key proof, H, current parent/set and Noise endpoints.
pub(crate) struct BftCandidateReadSubjectV1 {
    pub world: WorldIdentity,
    pub chain_id: String,
    pub authority_epoch: u64,
    pub node_public_key: String,
    pub policy_digest: String,
    pub grant_generation: u64,
}
/// Scope from an independently verified canonical BFT private-record reader.
/// This data does not qualify a cut; the Node must prove the exact retained
/// certificate and actual execution before invoking the permission gate.
pub(crate) struct BftCommittedReadScopeV1 {
    pub world: WorldIdentity,
    pub chain_id: String,
    pub authority_epoch: u64,
}
enum ReadPurpose<'a> {
    Legacy {
        grant: &'a FormalNodeReadGrantV1,
        requested_world: &'a WorldIdentity,
        requested_chain: &'a str,
        requested_authority_epoch: u64,
        subject_public_key: &'a str,
    },
    Candidate(&'a BftCandidateReadSubjectV1),
}
impl ReadPurpose<'_> {
    fn target(&self) -> FullSyncIssuerTargetV2 {
        match self {
            Self::Legacy { grant, .. } => FullSyncIssuerTargetV2 {
                node_public_key: grant.body.node_public_key.clone(),
                role: full_sync_role(&grant.body.role),
            },
            Self::Candidate(subject) => FullSyncIssuerTargetV2 {
                node_public_key: subject.node_public_key.clone(),
                role: FullSyncReadRoleV2::BftPendingCandidateV1,
            },
        }
    }
    fn scope(&self) -> (&WorldIdentity, &str, u64) {
        match self {
            Self::Legacy {
                requested_world,
                requested_chain,
                requested_authority_epoch,
                ..
            } => (requested_world, requested_chain, *requested_authority_epoch),
            Self::Candidate(subject) => {
                (&subject.world, &subject.chain_id, subject.authority_epoch)
            }
        }
    }
    fn generation(&self) -> u64 {
        match self {
            Self::Legacy { grant, .. } => grant.body.grant_epoch,
            Self::Candidate(subject) => subject.grant_generation,
        }
    }
}

struct InstalledHistory {
    state: Option<Arc<VerifiedFullSyncIssuerStateV3>>,
    poisoned: bool,
}

/// An explicit, pinned verifier for the provider's V3 full-sync read path.
/// Production configuration currently has no source/clock adapter and fails
/// closed before listener startup; tests inject signed evidence through the
/// same route-facing gate.
pub(crate) struct FullSyncV3ReadGate {
    trust: InitialActivationTrustV1,
    v2_policy: VerifiedFullSyncIssuerPolicyV2,
    v3_policy: VerifiedFullSyncIssuerPolicyV3,
    minimum_anchor: FullSyncIssuerAnchorV3,
    provider_public_key: String,
    provider_incarnation: String,
    source: Arc<dyn FullSyncIssuerReadEvidenceSource>,
    installed: Mutex<InstalledHistory>,
}

impl FullSyncV3ReadGate {
    pub(crate) fn new(
        trust: InitialActivationTrustV1,
        signed_v2_policy: SignedFullSyncIssuerPolicyV2,
        signed_v3_policy: SignedFullSyncIssuerPolicyV3,
        minimum_anchor: FullSyncIssuerAnchorV3,
        provider_public_key: String,
        provider_incarnation: String,
        source: Arc<dyn FullSyncIssuerReadEvidenceSource>,
    ) -> Result<Self, String> {
        let v2_policy = VerifiedFullSyncIssuerPolicyV2::verify(&signed_v2_policy, &trust)
            .map_err(|error| format!("verify full-sync V2 policy: {error}"))?;
        let v3_policy =
            VerifiedFullSyncIssuerPolicyV3::verify(&signed_v3_policy, &trust, &v2_policy)
                .map_err(|error| format!("verify full-sync V3 policy: {error}"))?;
        minimum_anchor
            .validate()
            .map_err(|error| format!("validate external full-sync V3 minimum: {error}"))?;
        if minimum_anchor.scope_digest != v3_policy.scope_digest()
            || minimum_anchor.policy_digest != v3_policy.policy_digest()
            || minimum_anchor.signer_generation != v3_policy.body().signer_generation
        {
            return Err("external full-sync V3 minimum differs from pinned policy".into());
        }
        validate_hex_identity(&provider_public_key, "provider public key")?;
        validate_hex_identity(&provider_incarnation, "provider incarnation")?;
        if !v3_policy
            .body()
            .provider_public_keys
            .contains(&provider_public_key)
        {
            return Err("local provider key is not allowed by full-sync V3 policy".into());
        }
        Ok(Self {
            trust,
            v2_policy,
            v3_policy,
            minimum_anchor,
            provider_public_key,
            provider_incarnation,
            source,
            installed: Mutex::new(InstalledHistory {
                state: None,
                poisoned: false,
            }),
        })
    }

    /// Checks current authorization and optionally emits a response under the
    /// same mutex used to install and compare reconstructed history. The first
    /// call happens before reading private full-sync material; the second call
    /// rechecks the current qualified World cut and signs the bounded reply.
    pub(crate) fn authorize_with_response<T>(
        &self,
        grant: &FormalNodeReadGrantV1,
        requested: &FormalFullSyncCutV1,
        subject_public_key: &str,
        make_response: impl FnOnce() -> FormalReadResult<T>,
    ) -> FormalReadResult<T> {
        self.authorize_target_with_response(
            ReadPurpose::Legacy {
                grant,
                requested_world: &requested.world,
                requested_chain: &requested.chain_id,
                requested_authority_epoch: requested.authority_epoch,
                subject_public_key,
            },
            |_| make_response(),
        )
    }

    /// Pending inputs require their own explicitly issued V3 target. Legacy
    /// grants are never converted to this role. Invoke while holding the
    /// Node's exact-parent/serving guard through the immutable chunk release.
    pub(crate) fn authorize_candidate_with_response<T>(
        &self,
        subject: &BftCandidateReadSubjectV1,
        make_response: impl FnOnce(Instant) -> FormalReadResult<T>,
    ) -> FormalReadResult<T> {
        self.authorize_target_with_response(ReadPurpose::Candidate(subject), make_response)
    }

    /// Canonical private BFT history uses the existing full-state grant and
    /// its current V3 target/lease, never the pending-candidate purpose.
    pub(crate) fn authorize_bft_committed_with_response<T>(
        &self,
        grant: &FormalNodeReadGrantV1,
        scope: &BftCommittedReadScopeV1,
        subject_public_key: &str,
        make_response: impl FnOnce(Instant) -> FormalReadResult<T>,
    ) -> FormalReadResult<T> {
        self.authorize_target_with_response(
            ReadPurpose::Legacy {
                grant,
                requested_world: &scope.world,
                requested_chain: &scope.chain_id,
                requested_authority_epoch: scope.authority_epoch,
                subject_public_key,
            },
            make_response,
        )
    }

    fn authorize_target_with_response<T>(
        &self,
        purpose: ReadPurpose<'_>,
        make_response: impl FnOnce(Instant) -> FormalReadResult<T>,
    ) -> FormalReadResult<T> {
        self.validate_purpose_identity(&purpose)?;
        let started_at = Instant::now();
        let deadline = started_at
            .checked_add(Duration::from_millis(
                self.v3_policy.body().max_authorization_staleness_ms,
            ))
            .ok_or_else(|| {
                FormalReadError::Internal("full-sync V3 read deadline overflow".into())
            })?;
        let observation_nonce = fresh_nonce().map_err(FormalReadError::Internal)?;
        let target = purpose.target();
        let request = FullSyncIssuerReadEvidenceRequestV3 {
            observation_nonce,
            minimum_anchor: self.minimum_anchor.clone(),
            provider_public_key: self.provider_public_key.clone(),
            provider_incarnation: self.provider_incarnation.clone(),
            target,
            target_generation: purpose.generation(),
            started_at,
            deadline,
        };
        let evidence = self.source.fetch(&request)?;
        if Instant::now() > request.deadline {
            return Err(FormalReadError::Internal(
                "full-sync V3 evidence source exceeded authorization deadline".into(),
            ));
        }
        evidence.query.verify(&self.v3_policy).map_err(|error| {
            FormalReadError::Integrity(format!("verify fresh full-sync V3 query: {error}"))
        })?;
        if evidence.query.body.observation_nonce != hex::encode(request.observation_nonce)
            || evidence.query.body.minimum_anchor != request.minimum_anchor
            || evidence.observation.body.query != evidence.query
            || request.provider_public_key != self.provider_public_key
            || request.provider_incarnation != self.provider_incarnation
            || request.target != purpose.target()
            || Instant::now() < request.started_at
        {
            return Err(FormalReadError::Integrity(
                "full-sync V3 evidence does not bind the fresh query nonce/minimum".into(),
            ));
        }
        evidence
            .observation
            .verify(&self.v3_policy)
            .map_err(|error| {
                FormalReadError::Integrity(format!(
                    "verify full-sync V3 witness observation: {error}"
                ))
            })?;
        let encoded_history = serde_cbor::to_vec(&evidence.history).map_err(|error| {
            FormalReadError::Internal(format!(
                "encode full-sync V3 complete history bound: {error}"
            ))
        })?;
        if encoded_history.len() > MAX_HISTORY_BYTES {
            return Err(FormalReadError::Integrity(
                "full-sync V3 complete history exceeds provider read bound".into(),
            ));
        }
        let candidate = VerifiedFullSyncIssuerStateV3::from_witness_observation(
            self.v2_policy.clone(),
            self.v3_policy.clone(),
            evidence.history,
            &evidence.observation,
        )
        .map_err(|error| {
            FormalReadError::Integrity(format!(
                "replay complete full-sync V3 issuer history: {error}"
            ))
        })?;

        let mut installed = self.installed.lock().map_err(|_| {
            FormalReadError::Internal("full-sync V3 provider read gate mutex poisoned".into())
        })?;
        if installed.poisoned {
            return Err(FormalReadError::Integrity(
                "full-sync V3 provider read gate is poisoned".into(),
            ));
        }
        let current_state = installed.state.clone();
        let selected = match current_state.as_ref() {
            None => {
                let candidate = Arc::new(candidate);
                installed.state = Some(Arc::clone(&candidate));
                candidate
            }
            Some(current) => {
                match compare_and_install(current, candidate, &mut installed.poisoned)? {
                    Some(next) => {
                        installed.state = Some(Arc::clone(&next));
                        next
                    }
                    None => Arc::clone(current),
                }
            }
        };
        let lease = self.authorize_current_lease(
            &selected,
            &purpose,
            &evidence.observation,
            request.started_at,
        )?;
        let target_state = selected.target_state(&purpose.target()).ok_or_else(|| {
            FormalReadError::Integrity("authorized V3 read target disappeared".into())
        })?;
        let expires = target_state
            .expires_unix_ms
            .ok_or_else(|| FormalReadError::Integrity("authorized target expiry absent".into()))?
            .min(lease.lease.body.expires_unix_ms);
        let strict_time_budget = expires
            .checked_sub(evidence.observation.body.protected_time.latest_unix_ms)
            .and_then(|remaining| remaining.checked_sub(1))
            .ok_or_else(|| {
                FormalReadError::GrantRejected("V3 read release interval exhausted".into())
            })?
            .min(self.v3_policy.body().max_authorization_staleness_ms);
        let release_deadline = request
            .started_at
            .checked_add(Duration::from_millis(strict_time_budget))
            .filter(|deadline| Instant::now() < *deadline)
            .ok_or_else(|| {
                FormalReadError::GrantRejected("V3 read release deadline exhausted".into())
            })?;
        // The callback constructs immutable bytes and installs this narrowing
        // deadline on the transport. It must not send before the final check.
        let reply = make_response(release_deadline)?;
        self.check_time_bounds(
            &purpose,
            &selected,
            &lease,
            &evidence.observation,
            request.started_at,
        )?;
        Ok(reply)
    }

    /// Reject an unauthenticated grant or foreign scope before asking a remote
    /// issuer to reconstruct history. Time and current generation are still
    /// checked against fresh signed evidence after this static admission.
    fn validate_purpose_identity(&self, purpose: &ReadPurpose<'_>) -> FormalReadResult<()> {
        let policy = self.v3_policy.body();
        let expected_world = WorldIdentity {
            world_id: policy.world.world_id.clone(),
            genesis_digest: policy.world.genesis_digest.clone(),
        };
        let (world, chain, epoch) = purpose.scope();
        if world != &expected_world
            || chain != policy.world.chain_id
            || epoch != policy.authority_epoch
        {
            return Err(FormalReadError::RequestRejected(
                "V3 read request is outside the pinned World".into(),
            ));
        }
        if !policy.allowed_roles.contains(&purpose.target().role) {
            return Err(FormalReadError::GrantRejected(
                "V3 issuer policy does not allow the requested read purpose".into(),
            ));
        }
        match purpose {
            ReadPurpose::Legacy {
                grant,
                subject_public_key,
                ..
            } => grant
                .verify_identity_and_signature(
                    &self.trust.issuer_public_key,
                    &expected_world,
                    &policy.world.chain_id,
                    subject_public_key,
                    &grant.body.role,
                    policy.authority_epoch,
                )
                .map_err(|error| {
                    FormalReadError::GrantRejected(format!("verify read grant identity: {error}"))
                }),
            ReadPurpose::Candidate(subject) => {
                if subject.policy_digest != self.v3_policy.policy_digest()
                    || subject.grant_generation == 0
                {
                    return Err(FormalReadError::GrantRejected(
                        "candidate read policy or grant generation mismatch".into(),
                    ));
                }
                validate_hex_identity(&subject.node_public_key, "candidate reader key")
                    .map_err(FormalReadError::GrantRejected)
            }
        }
    }

    pub(crate) fn validate_service_binding(
        &self,
        service_trust: &InitialActivationTrustV1,
        service_world: &WorldIdentity,
        service_provider_key: &str,
    ) -> Result<(), String> {
        let configured_trust_digest = full_sync_activation_trust_digest_v3(&self.trust)
            .map_err(|error| format!("digest full-sync gate activation trust: {error}"))?;
        let service_trust_digest = full_sync_activation_trust_digest_v3(service_trust)
            .map_err(|error| format!("digest formal service activation trust: {error}"))?;
        let policy_world = &self.v3_policy.body().world;
        let policy_world_identity = WorldIdentity {
            world_id: policy_world.world_id.clone(),
            genesis_digest: policy_world.genesis_digest.clone(),
        };
        if configured_trust_digest != service_trust_digest
            || service_world != &policy_world_identity
            || service_provider_key != self.provider_public_key
        {
            return Err(
                "full-sync V3 read gate does not match service trust, World, and provider signer"
                    .into(),
            );
        }
        Ok(())
    }

    fn authorize_current_lease(
        &self,
        state: &VerifiedFullSyncIssuerStateV3,
        purpose: &ReadPurpose<'_>,
        observation: &SignedFullSyncIssuerObservationV3,
        started_at: Instant,
    ) -> FormalReadResult<FullSyncProviderLeaseStateV3> {
        if state.anchor().revision < self.minimum_anchor.revision
            || state.policy().policy_digest() != self.v3_policy.policy_digest()
        {
            return Err(FormalReadError::Integrity(
                "full-sync V3 state is below the externally pinned minimum/policy".into(),
            ));
        }
        let policy_world = &self.v3_policy.body().world;
        let expected_world = WorldIdentity {
            world_id: policy_world.world_id.clone(),
            genesis_digest: policy_world.genesis_digest.clone(),
        };
        let target = purpose.target();
        let (world, chain_id, epoch) = purpose.scope();
        if world != &expected_world
            || chain_id != policy_world.chain_id
            || epoch != self.v3_policy.body().authority_epoch
        {
            return Err(FormalReadError::RequestRejected(
                "V3 read request is outside the pinned World".into(),
            ));
        }
        if !self.v3_policy.body().allowed_roles.contains(&target.role) {
            return Err(FormalReadError::GrantRejected(
                "V3 issuer policy does not allow the requested read purpose".into(),
            ));
        }
        let target_state = state.target_state(&target).ok_or_else(|| {
            FormalReadError::GrantRejected(
                "V3 read target is absent from complete issuer history".into(),
            )
        })?;
        if !target_state.active {
            return Err(FormalReadError::GrantRejected(
                "full-sync V3 target is revoked or inactive".into(),
            ));
        }
        let (target_not_before, target_expires) = match (
            target_state.not_before_unix_ms,
            target_state.expires_unix_ms,
        ) {
            (Some(not_before), Some(expires)) => (not_before, expires),
            _ => {
                return Err(FormalReadError::Integrity(
                    "V3 active read target lacks a committed interval".into(),
                ));
            }
        };
        match purpose {
            ReadPurpose::Legacy {
                grant,
                subject_public_key,
                ..
            } => {
                if *subject_public_key != grant.body.node_public_key
                    || grant.body.world != expected_world
                    || grant.body.chain_id != policy_world.chain_id
                    || grant.body.authority_epoch != self.v3_policy.body().authority_epoch
                    || target_state.generation != grant.body.grant_epoch
                    || target_not_before != grant.body.not_before_unix_ms
                    || target_expires != grant.body.expires_unix_ms
                {
                    return Err(FormalReadError::GrantRejected(
                        "legacy grant differs from current V3 target identity or interval".into(),
                    ));
                }
                grant
                    .verify(
                        &self.trust.issuer_public_key,
                        &expected_world,
                        &policy_world.chain_id,
                        subject_public_key,
                        &grant.body.role,
                        self.v3_policy.body().authority_epoch,
                        observation.body.protected_time.earliest_unix_ms,
                    )
                    .map_err(|error| {
                        FormalReadError::GrantRejected(format!(
                            "verify legacy grant at protected earliest: {error}"
                        ))
                    })?;
            }
            ReadPurpose::Candidate(subject) => {
                if subject.authority_epoch != self.v3_policy.body().authority_epoch
                    || subject.policy_digest != self.v3_policy.policy_digest()
                    || subject.grant_generation == 0
                    || subject.grant_generation != target_state.generation
                {
                    return Err(FormalReadError::GrantRejected(
                        "candidate read policy, authority or grant generation mismatch".into(),
                    ));
                }
            }
        }

        let mut selected_lease = None;
        for (key, pair) in state.leases() {
            if key.provider_public_key != self.provider_public_key
                || key.provider_incarnation != self.provider_incarnation
                || key.lease_id != pair.lease.body.lease_id
            {
                continue;
            }
            let request = &pair.request.body;
            let lease = &pair.lease.body;
            if request.provider_public_key != self.provider_public_key
                || request.provider_incarnation != self.provider_incarnation
                || request.target != target
                || request.target_generation != target_state.generation
                || lease.provider_public_key != request.provider_public_key
                || lease.provider_incarnation != request.provider_incarnation
                || lease.lease_id != request.lease_id
                || lease.request_id != request.request_id
                || lease.request_nonce != request.request_nonce
                || lease.target != request.target
                || lease.target_generation != request.target_generation
                || lease.expected_head != request.expected_head
                || lease.signer_generation != self.v3_policy.body().signer_generation
                || lease.policy_digest != self.v3_policy.policy_digest()
                || lease.scope_digest != self.v3_policy.scope_digest()
                || lease.clock_profile_digest != self.v3_policy.body().clock_profile_digest
                || lease.not_before_unix_ms < target_not_before
                || lease.expires_unix_ms > target_expires
            {
                continue;
            }
            pair.request.verify(&self.v3_policy).map_err(|error| {
                FormalReadError::Integrity(format!(
                    "verify committed V3 provider lease request: {error}"
                ))
            })?;
            pair.lease
                .verify_issuer_signature(&self.v3_policy)
                .map_err(|error| {
                    FormalReadError::Integrity(format!(
                        "verify committed V3 provider lease signature: {error}"
                    ))
                })?;
            if self.lease_covers_observation(
                pair,
                target_not_before,
                target_expires,
                observation,
                started_at,
            ) {
                selected_lease = Some(pair.clone());
                break;
            }
        }
        selected_lease.ok_or_else(|| {
            FormalReadError::GrantRejected("no exact committed full-sync V3 lease covers this provider, incarnation, target and protected time".into())
        })
    }

    fn lease_covers_observation(
        &self,
        pair: &FullSyncProviderLeaseStateV3,
        target_not_before: u64,
        target_expires: u64,
        observation: &SignedFullSyncIssuerObservationV3,
        started_at: Instant,
    ) -> bool {
        let time = &observation.body.protected_time;
        let elapsed = match elapsed_millis_ceil(started_at.elapsed()) {
            Some(value) if value <= self.v3_policy.body().max_authorization_staleness_ms => value,
            _ => return false,
        };
        let latest_after_processing = match time.latest_unix_ms.checked_add(elapsed) {
            Some(value) => value,
            None => return false,
        };
        time.earliest_unix_ms >= target_not_before
            && time.earliest_unix_ms >= pair.lease.body.not_before_unix_ms
            && latest_after_processing < target_expires
            && latest_after_processing < pair.lease.body.expires_unix_ms
    }

    fn check_time_bounds(
        &self,
        purpose: &ReadPurpose<'_>,
        state: &VerifiedFullSyncIssuerStateV3,
        pair: &FullSyncProviderLeaseStateV3,
        observation: &SignedFullSyncIssuerObservationV3,
        started_at: Instant,
    ) -> FormalReadResult<()> {
        let target = purpose.target();
        let target_state = state.target_state(&target).ok_or_else(|| {
            FormalReadError::Integrity(
                "full-sync V3 target disappeared from installed history".into(),
            )
        })?;
        if !self.lease_covers_observation(
            pair,
            target_state
                .not_before_unix_ms
                .ok_or_else(|| FormalReadError::Integrity("V3 target not-before absent".into()))?,
            target_state
                .expires_unix_ms
                .ok_or_else(|| FormalReadError::Integrity("V3 target expiry absent".into()))?,
            observation,
            started_at,
        ) {
            return Err(FormalReadError::GrantRejected(
                "full-sync V3 protected-time interval no longer covers response release".into(),
            ));
        }
        Ok(())
    }
}

fn compare_and_install(
    current: &Arc<VerifiedFullSyncIssuerStateV3>,
    candidate: VerifiedFullSyncIssuerStateV3,
    poisoned: &mut bool,
) -> FormalReadResult<Option<Arc<VerifiedFullSyncIssuerStateV3>>> {
    let installed = current.anchor();
    let observed = candidate.anchor();
    if observed.revision < installed.revision {
        return Err(FormalReadError::Integrity(
            "full-sync V3 witness history is older than installed state".into(),
        ));
    }
    if observed.revision == installed.revision {
        if observed != installed {
            *poisoned = true;
            return Err(FormalReadError::Integrity(
                "full-sync V3 same-revision anchor conflict; provider gate poisoned".into(),
            ));
        }
        return Ok(None);
    }
    if !candidate
        .entries()
        .iter()
        .any(|entry| entry.record.next_anchor == *installed)
    {
        *poisoned = true;
        return Err(FormalReadError::Integrity("full-sync V3 newer history does not contain the exact installed anchor; provider gate poisoned".into()));
    }
    Ok(Some(Arc::new(candidate)))
}

fn full_sync_role(role: &FormalNodeReadRoleV1) -> FullSyncReadRoleV2 {
    match role {
        FormalNodeReadRoleV1::StateSync => FullSyncReadRoleV2::StateSync,
        FormalNodeReadRoleV1::FullReadOnly => FullSyncReadRoleV2::FullReadOnly,
    }
}

fn validate_hex_identity(value: &str, field: &str) -> Result<(), String> {
    let decoded = hex::decode(value).map_err(|error| format!("{field}: {error}"))?;
    if decoded.len() != 32 || decoded.iter().all(|byte| *byte == 0) {
        return Err(format!("{field} must be a nonzero 32-byte hex identity"));
    }
    Ok(())
}

fn fresh_nonce() -> Result<[u8; 32], String> {
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce)
        .map_err(|error| format!("generate full-sync query nonce: {error}"))?;
    if nonce.iter().all(|byte| *byte == 0) {
        return Err("full-sync query nonce source returned zero".into());
    }
    Ok(nonce)
}

pub(crate) fn fresh_provider_incarnation() -> Result<String, String> {
    fresh_nonce()
        .map(hex::encode)
        .map_err(|error| format!("generate full-sync provider incarnation: {error}"))
}

fn elapsed_millis_ceil(duration: Duration) -> Option<u64> {
    let millis = duration.as_millis();
    let rounded = millis.checked_add(u128::from(
        !duration.subsec_nanos().is_multiple_of(1_000_000),
    ))?;
    u64::try_from(rounded).ok()
}
