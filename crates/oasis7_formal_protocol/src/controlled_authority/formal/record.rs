use super::{crypto::*, *};
use crate::controlled_authority::replicated_protocol::{ArtifactRole, ClosedRecord, EndpointRole};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FORMAL_PROFILE: &str = "controlled_single_authority";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormalRecordKindV1 {
    InitialActivation,
    Execution,
}
/// Canonical logical identity: no writer signature, endpoint receipt, recovery
/// finalizer or proof envelope is hashed into this record body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalRecordV1 {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durable_set_transition_digest: Option<String>,
    pub kind: FormalRecordKindV1,
    pub world: FormalWorldIdentityV1,
    pub position: u64,
    pub parent_record_id: String,
    pub original_authority_id: String,
    pub original_epoch: u64,
    pub activation_ref: String,
    pub request_digest: String,
    pub payload_digest: String,
    pub frozen_reservation_id: Option<String>,
    pub runtime_manifest_digest: String,
    /// Committed output governing Cognition manifest hash, distinct from closure manifest.
    pub governing_manifest_digest: String,
    pub execution_policy_digest: String,
    pub before_state_root: String,
    pub after_state_root: String,
    pub action_root: String,
    pub event_root: String,
    pub result_root: String,
    /// Commitment to the visibility-aware projection tree, not private contents.
    pub projection_root: String,
    pub artifact_roots: BTreeMap<ArtifactRole, String>,
    pub handoff: Option<ProfileHandoffV1>,
}
impl FormalWorldIdentityV1 {
    pub fn validate(&self) -> Result<(), FormalError> {
        for value in [&self.world_id, &self.chain_id] {
            if value.trim().is_empty() || value.len() > 256 {
                return Err(invalid("formal world identity bounds"));
            }
        }
        hash_hex(&self.genesis_digest)
    }
    pub fn genesis_head(&self) -> Result<FormalHeadRefV1, FormalError> {
        self.validate()?;
        Ok(FormalHeadRefV1 {
            position: 0,
            record_id: hash("oasis7.formal.genesis.v1", self)?,
        })
    }
}
impl RecoveryPolicyV1 {
    pub fn validate(&self) -> Result<(), FormalError> {
        if self.schema_version != 1 || !self.terminates_at_profile_transition {
            return Err(invalid("formal recovery policy version/termination"));
        }
        for key in [
            &self.recovery_issuer_public_key,
            &self.fencing_provider_public_key,
            &self.issuer_anchor_public_key,
        ] {
            public_key(key)?;
        }
        if self.recovery_issuer_public_key == self.issuer_anchor_public_key
            || self.recovery_issuer_public_key == self.fencing_provider_public_key
            || self.issuer_anchor_public_key == self.fencing_provider_public_key
        {
            return Err(invalid(
                "recovery issuer, fencer and unique-successor witness require distinct keys",
            ));
        }
        if self.authorized_fencing_resource_bindings.is_empty()
            || self.authorized_fencing_resource_bindings.len() > 64
        {
            return Err(invalid("recovery policy external fencing resource bounds"));
        }
        let mut previous: Option<&String> = None;
        for resource in &self.authorized_fencing_resource_bindings {
            hash_hex(resource)?;
            if previous.is_some_and(|old| old >= resource) {
                return Err(invalid("recovery fencing resources must be sorted unique"));
            }
            previous = Some(resource);
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, FormalError> {
        self.validate()?;
        hash("oasis7.formal.recovery_policy.v1", self)
    }
}
impl AuthorityDefinitionV1 {
    pub fn validate(&self) -> Result<(), FormalError> {
        self.world.validate()?;
        public_key(&self.writer_public_key)?;
        hash_hex(&self.recovery_policy_digest)?;
        if self.schema_version != 1
            || self.profile != FORMAL_PROFILE
            || !matches!(self.profile_version, 1 | 2)
            || self.epoch == 0
            || self.valid_from_position == 0
        {
            return Err(invalid("formal authority profile/version/epoch/start"));
        }
        for (binding, role) in [
            (&self.primary, EndpointRole::Primary),
            (&self.replica, EndpointRole::Replica),
        ] {
            let instance = &binding.instance;
            public_key(&instance.endpoint_public_key)?;
            instance.validate().map_err(|e| invalid(e.to_string()))?;
            if instance.world_id != self.world.world_id
                || instance.chain_id != self.world.chain_id
                || instance.genesis_digest != self.world.genesis_digest
                || instance.authority_epoch != self.epoch
                || instance.endpoint_role != role
                || binding.fault_domain_id.trim().is_empty()
                || binding.fault_domain_id.len() > 256
            {
                return Err(invalid("formal endpoint instance/world/role/epoch binding"));
            }
        }
        let p = &self.primary.instance;
        let r = &self.replica.instance;
        if p.endpoint_id == r.endpoint_id
            || p.endpoint_public_key == r.endpoint_public_key
            || p.instance_id == r.instance_id
            || p.storage_id == r.storage_id
            || self.primary.fault_domain_id == self.replica.fault_domain_id
            || self.writer_public_key == p.endpoint_public_key
            || self.writer_public_key == r.endpoint_public_key
        {
            return Err(invalid(
                "formal two-endpoint distinct identities/keys/domains",
            ));
        }
        if let Some(parent) = &self.predecessor_authority_id {
            hash_hex(parent)?;
        }
        Ok(())
    }
    pub fn authority_id(&self) -> Result<String, FormalError> {
        self.validate()?;
        hash("oasis7.formal.authority.v1", self)
    }
    pub fn verify_policy(&self, policy: &RecoveryPolicyV1) -> Result<(), FormalError> {
        self.validate()?;
        policy.validate()?;
        if self.recovery_policy_digest != policy.digest()?
            || [
                &self.writer_public_key,
                &self.primary.instance.endpoint_public_key,
                &self.replica.instance.endpoint_public_key,
            ]
            .iter()
            .any(|key| {
                [
                    &policy.recovery_issuer_public_key,
                    &policy.fencing_provider_public_key,
                    &policy.issuer_anchor_public_key,
                ]
                .contains(key)
            })
        {
            return Err(invalid(
                "recovery policy binding/independent issuer, fencer and witness",
            ));
        }
        Ok(())
    }
}
impl ProfileHandoffV1 {
    pub fn validate(&self, position: u64) -> Result<(), FormalError> {
        hash_hex(&self.next_authority_digest)?;
        if self.next_profile.trim().is_empty()
            || self.next_profile.len() > 256
            || self.next_profile_version == 0
            || position.checked_add(1) != Some(self.valid_from_position)
        {
            return Err(invalid("formal handoff target/boundary"));
        }
        Ok(())
    }
}
impl CanonicalRecordV1 {
    pub fn record_id(&self) -> Result<String, FormalError> {
        self.validate()?;
        hash("oasis7.formal.canonical_record.v1", self)
    }
    pub fn validate(&self) -> Result<(), FormalError> {
        self.world.validate()?;
        if !matches!(
            (self.schema_version, &self.durable_set_transition_digest),
            (1, None) | (2, Some(_)) | (3, None) | (4, Some(_)) | (5, None) | (6, Some(_))
        ) || (!matches!(self.schema_version, 1 | 5)
            && self.kind != FormalRecordKindV1::Execution)
            || self.position == 0
            || self.original_epoch == 0
            || self.artifact_roots.len() != if self.schema_version >= 5 { 10 } else { 9 }
            || self
                .artifact_roots
                .contains_key(&ArtifactRole::NativeExecutionEnvironment)
                != (self.schema_version >= 5)
        {
            return Err(invalid("formal record version/position/closure roots"));
        }
        if let Some(digest) = &self.durable_set_transition_digest {
            hash_hex(digest)?;
            if self.handoff.is_some() {
                return Err(invalid(
                    "durable set transition cannot combine profile handoff",
                ));
            }
        }
        for digest in [
            &self.parent_record_id,
            &self.original_authority_id,
            &self.activation_ref,
            &self.request_digest,
            &self.payload_digest,
            &self.runtime_manifest_digest,
            &self.governing_manifest_digest,
            &self.execution_policy_digest,
            &self.before_state_root,
            &self.after_state_root,
            &self.action_root,
            &self.event_root,
            &self.result_root,
            &self.projection_root,
        ] {
            hash_hex(digest)?;
        }
        for digest in self.artifact_roots.values() {
            hash_hex(digest)?;
        }
        match self.kind {
            FormalRecordKindV1::InitialActivation
                if self.position != 1
                    || self.frozen_reservation_id.is_some()
                    || self.handoff.is_some() =>
            {
                return Err(invalid("formal initial record boundary"));
            }
            FormalRecordKindV1::Execution if self.frozen_reservation_id.is_none() => {
                return Err(invalid(
                    "formal execution requires original frozen reservation",
                ));
            }
            _ => {}
        }
        if let Some(reservation) = &self.frozen_reservation_id {
            hash_hex(reservation)?;
        }
        if let Some(handoff) = &self.handoff {
            handoff.validate(self.position)?;
        }
        Ok(())
    }
}
/// Full-state endpoints/replay consumers call this closure check; light readers
/// use attested commitments without receiving private snapshot/input bytes.
pub fn verify_record_material(
    record: &CanonicalRecordV1,
    material: &ClosedRecord,
) -> Result<(), FormalError> {
    verify_record_material_retaining_snapshot(record, material).map(|_| ())
}

/// Complete original verification, retaining only a versioned Snapshot already
/// expanded by this pass. Legacy schemas preserve their original checks and
/// return None without an additional Snapshot read or allocation.
pub fn verify_record_material_retaining_snapshot(
    record: &CanonicalRecordV1,
    material: &ClosedRecord,
) -> Result<Option<Vec<u8>>, FormalError> {
    record.validate()?;
    material.validate().map_err(|e| invalid(e.to_string()))?;
    if record.payload_digest != material.payload_digest
        || record.before_state_root != material.before_state_root
        || record.after_state_root != material.after_state_root
        || record.artifact_roots != material.roots
    {
        return Err(invalid("formal record material commitment mismatch"));
    }
    if record.schema_version >= 5
        && material
            .native_environment()
            .map_err(|e| invalid(e.to_string()))?
            .is_none()
    {
        return Err(invalid("native environment root required"));
    }
    if record.schema_version < 3
        && let Some(object) = material.objects.iter().find(|object| {
            material.roots.get(&ArtifactRole::Snapshot) == Some(&object.content_hash)
        })
        && crate::snapshot_delta::SnapshotArtifactV1::decode(&object.bytes)
            .map_err(invalid)?
            .is_some()
    {
        return Err(invalid(
            "legacy formal record cannot reinterpret Snapshot delta artifacts",
        ));
    }
    if record.schema_version >= 3 {
        let expanded = material
            .snapshot_bytes()
            .map_err(|e| invalid(e.to_string()))?;
        if blake3::hash(&expanded).to_hex().as_str() != record.after_state_root {
            return Err(invalid(
                "formal delta Snapshot expanded state-root mismatch",
            ));
        }
        return Ok(Some(expanded));
    }
    Ok(None)
}
