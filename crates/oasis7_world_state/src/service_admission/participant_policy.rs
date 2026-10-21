//! Quorum-authorized participant policy and externally attested migration.
//! Completeness of the identity database is an issuer/governance trust assertion,
//! not a fact inferred from an empty local index.
use super::participant::*;
use super::{SignedReadRequest, request_digest};
use oasis7_agent_api::game::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const PARTICIPANT_MIGRATION_OWNER_DOMAIN: &str = "participant_migration_owner_v1";
pub const PARTICIPANT_MIGRATION_MANIFEST_DOMAIN: &str = "participant_migration_manifest_v1";
pub const PARTICIPANT_POLICY_INSTALL_DOMAIN: &str = "participant_policy_install_v1";
pub const PARTICIPANT_POLICY_UPDATE_DOMAIN: &str = "participant_policy_update_v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantMigrationBindingV1 {
    pub registration: GameParticipantRegisterV1,
    pub agent_id: String,
    pub owner_binding: String,
    pub owner_generation: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantMigrationManifestV1 {
    pub api_rev: u32,
    pub policy_digest: String,
    pub world_id: String,
    pub genesis_digest: String,
    pub branch_id: String,
    pub reorg_epoch: u64,
    pub identity_namespace: GameIdentityNamespaceV1,
    /// Authoritative identity DB snapshot epoch/watermark/digest, including empty snapshots.
    pub source_snapshot_epoch: u64,
    pub source_watermark: String,
    pub source_snapshot_digest: String,
    pub complete: bool,
    pub previous_registry_digest: String,
    pub legacy_owner_state_digest: String,
    pub entries: Vec<SignedReadRequest<ParticipantMigrationBindingV1>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantPolicyProofV1 {
    pub manifest: SignedReadRequest<ParticipantMigrationManifestV1>,
    pub certificate: crate::governance::GovernanceFinalityCertificate,
    /// The exact historical certificate signer set additionally signs policy and manifest.
    pub signatures: BTreeMap<String, String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantPolicyAuthorizationV1 {
    pub policy: ParticipantEntryPolicyV1,
    pub proof: ParticipantPolicyProofV1,
}
pub fn policy_digest(policy: &ParticipantEntryPolicyV1) -> Result<String, String> {
    request_digest("participant-policy", policy)
}
pub fn policy_signing_payload(
    policy: &ParticipantEntryPolicyV1,
    proof: &ParticipantPolicyProofV1,
) -> serde_json::Value {
    serde_json::json!({"policy":policy,"manifest":proof.manifest,"certificate":proof.certificate})
}
pub fn migration_receipt(
    binding: &ParticipantMigrationBindingV1,
    tick: u64,
) -> Result<GameParticipantReceiptV1, String> {
    let r = &binding.registration;
    Ok(GameParticipantReceiptV1 {
        api_rev: 1,
        identity_namespace: r.identity_namespace.clone(),
        account_id: r.account_id.clone(),
        world_id: r.world_id.clone(),
        genesis_digest: r.genesis_digest.clone(),
        agent_id: binding.agent_id.clone(),
        owner_public_key: r.owner_public_key.clone(),
        owner_binding: binding.owner_binding.clone(),
        owner_generation: DecimalU64(binding.owner_generation),
        client_operation_id: r.client_operation_id.clone(),
        business_digest: business_digest(r)?,
        registered_tick: DecimalU64(tick),
        imported: true,
    })
}

/// Exact governance proposal commitment. A historical quorum cannot reuse an unrelated approval.
pub fn policy_proposal_commitment(
    policy: &ParticipantEntryPolicyV1,
    manifest: &SignedReadRequest<ParticipantMigrationManifestV1>,
) -> Result<serde_json::Value, String> {
    Ok(
        serde_json::json!({"domain":if policy.generation==1 {PARTICIPANT_POLICY_INSTALL_DOMAIN}else{PARTICIPANT_POLICY_UPDATE_DOMAIN},
        "policy_digest":policy_digest(policy)?,"manifest_digest":request_digest("participant-policy-approved-manifest",manifest)?}),
    )
}

/// Explicit operator authorization, separate from governed finality evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevLocalParticipantPolicyAuthorizationV1 {
    pub policy: ParticipantEntryPolicyV1,
    pub manifest: SignedReadRequest<ParticipantMigrationManifestV1>,
}
pub const PARTICIPANT_DEVLOCAL_POLICY_INSTALL_DOMAIN: &str =
    "participant_devlocal_policy_install_v1";
pub const PARTICIPANT_DEVLOCAL_POLICY_UPDATE_DOMAIN: &str = "participant_devlocal_policy_update_v1";
pub fn devlocal_policy_signing_domain(generation: u64) -> &'static str {
    if generation == 1 {
        PARTICIPANT_DEVLOCAL_POLICY_INSTALL_DOMAIN
    } else {
        PARTICIPANT_DEVLOCAL_POLICY_UPDATE_DOMAIN
    }
}
