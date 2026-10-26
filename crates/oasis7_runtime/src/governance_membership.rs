//! Read-only membership projection. The caller must verify the commit proof and
//! resolve its state_root_ref to the supplied state root before invoking this adapter.
//! A CAS snapshot reference is not itself a world state hash.
use super::{GovernanceFinalitySignerRegistry, GovernanceValidatorAdmissionRecord, World};
use crate::service_admission::CommitRef;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldGovernanceMembershipView {
    pub commit: CommitRef,
    pub state_root_hash: String,
    pub governance_epoch: u64,
    /// Verified history profile supplied by the proof consumer, never node configuration.
    pub profile: String,
    pub candidates: BTreeMap<String, GovernanceValidatorAdmissionRecord>,
    pub selected_records: BTreeMap<String, GovernanceValidatorAdmissionRecord>,
    pub effective_registry: Option<GovernanceFinalitySignerRegistry>,
    pub effective_registry_digest: String,
}

/// A read-only future-epoch projection derived from the same proof-bound World
/// used by [`WorldGovernanceMembershipView::read`]. This is not an install ACK,
/// does not mutate the World, and does not independently authenticate the
/// caller-supplied commit proof or profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldGovernanceFinalityRegistryProjectionV1 {
    pub source_commit: CommitRef,
    pub source_state_root_hash: String,
    pub source_profile: String,
    pub source_governance_epoch: u64,
    pub target_governance_epoch: u64,
    pub effective_registry: Option<GovernanceFinalitySignerRegistry>,
    /// Same digest domain as the existing current-registry read API.
    pub effective_registry_digest: String,
    /// Binds this projection to its exact source and target provenance.
    pub provenance_digest: String,
}

struct ValidatedMembershipSource {
    state_root_hash: String,
    governance_epoch: u64,
    candidates: BTreeMap<String, GovernanceValidatorAdmissionRecord>,
    selected_records: BTreeMap<String, GovernanceValidatorAdmissionRecord>,
}

impl WorldGovernanceMembershipView {
    /// Derive the state's semantic root from a supplied world. This does not
    /// authenticate the world; proof consumers must first pin verified snapshot
    /// bytes. Cognition base_world_hash is a domain-bound digest, not this root.
    pub fn world_state_root_hash(world: &World) -> Result<String, String> {
        world
            .current_state_root_hash()
            .map_err(|error| format!("{error:?}"))
    }
    /// Project one immutable, proof-verified world. Resolver errors propagate;
    /// an absent registry remains absent and never falls back to node configuration.
    pub fn read(
        world: &World,
        commit: &CommitRef,
        verified_state_root_hash: &str,
        verified_profile: &str,
    ) -> Result<Self, String> {
        let source =
            Self::validate_source(world, commit, verified_state_root_hash, verified_profile)?;
        let effective_registry = world
            .resolve_governance_effective_finality_signer_registry()
            .map_err(|error| format!("{error:?}"))?;
        let effective_registry_digest = oasis7_client_api::world_service::request_digest(
            "governance-membership-registry-v1",
            &effective_registry,
        )?;
        Ok(Self {
            commit: commit.clone(),
            state_root_hash: source.state_root_hash,
            governance_epoch: source.governance_epoch,
            profile: verified_profile.into(),
            candidates: source.candidates,
            selected_records: source.selected_records,
            effective_registry,
            effective_registry_digest,
        })
    }

    /// Resolve a future governance epoch without changing the authenticated
    /// source World or its current effective registry. Callers remain
    /// responsible for authenticating the commit proof and state-root reference
    /// represented by the supplied arguments.
    pub fn read_for_future_epoch(
        world: &World,
        commit: &CommitRef,
        verified_state_root_hash: &str,
        verified_profile: &str,
        target_epoch: u64,
    ) -> Result<WorldGovernanceFinalityRegistryProjectionV1, String> {
        let source =
            Self::validate_source(world, commit, verified_state_root_hash, verified_profile)?;
        if target_epoch <= source.governance_epoch {
            return Err("future membership target epoch must exceed source epoch".into());
        }
        let effective_registry = world
            .resolve_governance_effective_finality_signer_registry_from_admissions_at_epoch(
                &source.candidates,
                target_epoch,
            )
            .map_err(|error| format!("{error:?}"))?;
        let effective_registry_digest = oasis7_client_api::world_service::request_digest(
            "governance-membership-registry-v1",
            &effective_registry,
        )?;
        let source_profile = verified_profile.to_string();
        let provenance_digest = oasis7_client_api::world_service::request_digest(
            "governance-membership-future-registry-projection-v1",
            &(
                commit,
                &source.state_root_hash,
                &source_profile,
                source.governance_epoch,
                target_epoch,
                &effective_registry,
            ),
        )?;
        Ok(WorldGovernanceFinalityRegistryProjectionV1 {
            source_commit: commit.clone(),
            source_state_root_hash: source.state_root_hash,
            source_profile,
            source_governance_epoch: source.governance_epoch,
            target_governance_epoch: target_epoch,
            effective_registry,
            effective_registry_digest,
            provenance_digest,
        })
    }

    fn validate_source(
        world: &World,
        commit: &CommitRef,
        verified_state_root_hash: &str,
        verified_profile: &str,
    ) -> Result<ValidatedMembershipSource, String> {
        commit.validate().map_err(|error| format!("{error:?}"))?;
        if verified_profile.trim().is_empty() {
            return Err("membership history profile is missing".into());
        }
        let state_root_hash = Self::world_state_root_hash(world)?;
        if state_root_hash != verified_state_root_hash {
            return Err("membership committed world state root mismatch".into());
        }
        let binding = world
            .current_cognition_runtime_binding()
            .map_err(|e| format!("{e:?}"))?;
        let finality_ref = oasis7_client_api::world_service::request_digest(
            "finality",
            &(
                binding.finality_epoch,
                &binding.finality_status,
                &binding.finality_block_hash,
            ),
        )?;
        if binding.world_id != commit.world.world_id
            || binding.world_id != commit.binding.provider_world_id
            || binding.branch_id != commit.binding.branch_id
            || binding.runtime_manifest_hash.to_string() != commit.binding.governing_manifest_ref
            || binding.reorg_epoch != commit.binding.reorg_generation
            || finality_ref != commit.binding.finality_ref
            || world.capability_revocation_state().epoch != commit.binding.permission_generation
            || world
                .capability_revocation_state()
                .authority_records
                .values()
                .map(|record| record.governance_epoch)
                .max()
                .unwrap_or(0)
                != commit.binding.authority_generation
        {
            return Err("membership committed world identity/binding mismatch".into());
        }
        let candidates = world.governance_validator_admissions().clone();
        let selected_records = World::governance_effective_validator_admission_records(&candidates)
            .into_iter()
            .map(|(node_id, record)| (node_id, record.clone()))
            .collect();
        Ok(ValidatedMembershipSource {
            state_root_hash,
            governance_epoch: world.current_governance_epoch(),
            candidates,
            selected_records,
        })
    }
}

/// Startup configuration projection is diagnostic evidence only. The current
/// runtime has no consensus component acknowledgement or safe live install API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMembershipApplicationView {
    pub configured_role: String,
    pub startup_registry_digest: Option<String>,
    pub startup_registry_source: Option<String>,
    pub supports_safe_runtime_update: bool,
    /// None means no application evidence; it does not mean an empty validator set.
    pub applied_consensus_membership: Option<CommitRef>,
}
impl RuntimeMembershipApplicationView {
    pub fn startup_projection(
        configured_role: impl Into<String>,
        startup_registry_digest: Option<String>,
        startup_registry_source: Option<String>,
    ) -> Self {
        Self {
            configured_role: configured_role.into(),
            startup_registry_digest,
            startup_registry_source,
            supports_safe_runtime_update: false,
            applied_consensus_membership: None,
        }
    }
}

#[cfg(test)]
#[path = "governance_membership_tests.rs"]
mod tests;
