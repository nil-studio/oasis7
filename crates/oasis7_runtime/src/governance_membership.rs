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
        let effective_registry = world
            .resolve_governance_effective_finality_signer_registry()
            .map_err(|error| format!("{error:?}"))?;
        let effective_registry_digest = oasis7_client_api::world_service::request_digest(
            "governance-membership-registry-v1",
            &effective_registry,
        )?;
        Ok(Self {
            commit: commit.clone(),
            state_root_hash,
            governance_epoch: world.current_governance_epoch(),
            profile: verified_profile.into(),
            candidates,
            selected_records,
            effective_registry,
            effective_registry_digest,
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
