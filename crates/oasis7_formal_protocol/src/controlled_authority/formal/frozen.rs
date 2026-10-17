use super::{crypto::*, *};
use crate::controlled_authority::replicated_protocol::{ArtifactRole, ClosedRecord, EndpointRole};
use serde::{Deserialize, Serialize};
const RESERVATION: &str = "oasis7.formal.frozen_reservation.sign.v1";
const RECEIPT: &str = "oasis7.formal.frozen_receipt.sign.v1";
/// Fresh normal reservation authorization, before endpoint durability receipts.
#[derive(Debug, Clone)]
pub struct VerifiedFormalReservationV1 {
    reservation: SignedFormalFrozenV1,
}
impl VerifiedFormalReservationV1 {
    pub fn reservation(&self) -> &SignedFormalFrozenV1 {
        &self.reservation
    }
    pub fn body(&self) -> &FormalFrozenBodyV1 {
        &self.reservation.body
    }
    pub fn reservation_id(&self) -> Result<String, FormalError> {
        self.body().reservation_id()
    }
}
pub fn verify_formal_reservation(
    reservation: &SignedFormalFrozenV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
) -> Result<VerifiedFormalReservationV1, FormalError> {
    let body = &reservation.body;
    body.validate()?;
    super::proof::verify_normal_parent_continuity(authority, parent, body.next_position)?;
    if parent
        .record()
        .artifact_roots
        .contains_key(&ArtifactRole::NativeExecutionEnvironment)
        && body.schema_version < 3
    {
        return Err(invalid("native parent requires native frozen schema"));
    }
    if body.world != authority.definition().world
        || body.authority_id != authority.authority_id()?
        || body.epoch != authority.definition().epoch
        || body.activation_ref != authority.activation_ref()
        || body.parent != parent.head()
        || body.pre_state_root != parent.record().after_state_root
        || body.governing_manifest_digest != parent.record().governing_manifest_digest
        || body.execution_policy_digest != parent.record().execution_policy_digest
    {
        return Err(invalid(
            "formal fresh frozen authority/committed parent/state binding",
        ));
    }
    verify(
        RESERVATION,
        body,
        &reservation.signature_hex,
        &authority.definition().writer_public_key,
    )?;
    Ok(VerifiedFormalReservationV1 {
        reservation: reservation.clone(),
    })
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFrozenBodyV1 {
    pub schema_version: u32,
    pub world: FormalWorldIdentityV1,
    pub authority_id: String,
    pub epoch: u64,
    pub activation_ref: String,
    pub parent: FormalHeadRefV1,
    pub next_position: u64,
    pub request_digest: String,
    pub payload_digest: String,
    pub runtime_manifest_digest: String,
    /// Input prestate governing Cognition manifest hash, distinct from closure manifest.
    pub governing_manifest_digest: String,
    pub execution_policy_digest: String,
    pub pre_state_root: String,
    pub frozen_material_digest: String,
    pub handoff: Option<ProfileHandoffV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFormalFrozenV1 {
    pub body: FormalFrozenBodyV1,
    pub signature_hex: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFrozenReceiptBodyV1 {
    pub authority_id: String,
    pub role: EndpointRole,
    pub instance_digest: String,
    pub reservation_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFrozenReceiptV1 {
    pub body: FormalFrozenReceiptBodyV1,
    pub signature_hex: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFrozenCertificateV1 {
    pub reservation: SignedFormalFrozenV1,
    pub primary: FormalFrozenReceiptV1,
    pub replica: FormalFrozenReceiptV1,
}
impl FormalFrozenBodyV1 {
    pub fn validate(&self) -> Result<(), FormalError> {
        self.world.validate()?;
        if !matches!(self.schema_version, 1..=3)
            || self.epoch == 0
            || self.parent.position == 0
            || self.parent.position.checked_add(1) != Some(self.next_position)
        {
            return Err(invalid("formal frozen version/epoch/parent/position"));
        }
        for digest in [
            &self.authority_id,
            &self.activation_ref,
            &self.parent.record_id,
            &self.request_digest,
            &self.payload_digest,
            &self.runtime_manifest_digest,
            &self.governing_manifest_digest,
            &self.execution_policy_digest,
            &self.pre_state_root,
            &self.frozen_material_digest,
        ] {
            hash_hex(digest)?;
        }
        if let Some(handoff) = &self.handoff {
            handoff.validate(self.next_position)?;
        }
        Ok(())
    }
    pub fn reservation_id(&self) -> Result<String, FormalError> {
        self.validate()?;
        hash("oasis7.formal.frozen_identity.v1", self)
    }
}
pub fn frozen_signing_bytes(body: &FormalFrozenBodyV1) -> Result<Vec<u8>, FormalError> {
    body.validate()?;
    bytes(RESERVATION, body)
}
pub fn frozen_receipt_signing_bytes(
    body: &FormalFrozenReceiptBodyV1,
) -> Result<Vec<u8>, FormalError> {
    bytes(RECEIPT, body)
}
pub fn frozen_material_digest(material: &ClosedRecord) -> Result<String, FormalError> {
    material.validate().map_err(|e| invalid(e.to_string()))?;
    hash("oasis7.formal.frozen_material.v1", material)
}
pub fn verify_frozen_material(
    body: &FormalFrozenBodyV1,
    material: &ClosedRecord,
) -> Result<(), FormalError> {
    verify_frozen_material_retaining_snapshot(body, material).map(|_| ())
}

/// Perform the same complete verification, retaining an already expanded
/// versioned Snapshot for a caller's operation-local typed restoration. Legacy
/// schema 1 keeps its original validation and returns no expanded allocation.
pub fn verify_frozen_material_retaining_snapshot(
    body: &FormalFrozenBodyV1,
    material: &ClosedRecord,
) -> Result<Option<Vec<u8>>, FormalError> {
    body.validate()?;
    if frozen_material_digest(material)? != body.frozen_material_digest
        || material.payload_digest != body.payload_digest
        || material.before_state_root != body.pre_state_root
        || material.after_state_root != body.pre_state_root
        || (body.schema_version == 1
            && material.roots.get(&ArtifactRole::Snapshot) != Some(&body.pre_state_root))
        || material.roots.get(&ArtifactRole::ExecutionManifest)
            != Some(&body.runtime_manifest_digest)
    {
        return Err(invalid("formal frozen full-material closure mismatch"));
    }
    if (body.schema_version == 3)
        != material
            .native_environment()
            .map_err(|e| invalid(e.to_string()))?
            .is_some()
    {
        return Err(invalid("native frozen environment root required"));
    }
    if body.schema_version >= 2 {
        let expanded = material
            .snapshot_bytes()
            .map_err(|e| invalid(e.to_string()))?;
        if blake3::hash(&expanded).to_hex().as_str() != body.pre_state_root {
            return Err(invalid(
                "formal frozen delta Snapshot expanded prestate mismatch",
            ));
        }
        return Ok(Some(expanded));
    }
    Ok(None)
}
impl FormalFrozenCertificateV1 {
    pub fn verify(
        &self,
        authority: &AuthorityDefinitionV1,
        activation_ref: &str,
    ) -> Result<String, FormalError> {
        authority.validate()?;
        let body = &self.reservation.body;
        body.validate()?;
        let authority_id = authority.authority_id()?;
        let reservation_id = body.reservation_id()?;
        if body.world != authority.world
            || body.authority_id != authority_id
            || body.epoch != authority.epoch
            || body.activation_ref != activation_ref
            || body.next_position < authority.valid_from_position
        {
            return Err(invalid("formal frozen authority/activation binding"));
        }
        verify(
            RESERVATION,
            body,
            &self.reservation.signature_hex,
            &authority.writer_public_key,
        )?;
        for (receipt, binding, role) in [
            (&self.primary, &authority.primary, EndpointRole::Primary),
            (&self.replica, &authority.replica, EndpointRole::Replica),
        ] {
            if receipt.body.authority_id != authority_id
                || receipt.body.reservation_id != reservation_id
                || receipt.body.role != role
                || receipt.body.instance_digest
                    != binding
                        .instance
                        .digest()
                        .map_err(|e| invalid(e.to_string()))?
            {
                return Err(invalid("formal frozen endpoint receipt binding"));
            }
            verify(
                RECEIPT,
                &receipt.body,
                &receipt.signature_hex,
                &binding.instance.endpoint_public_key,
            )?;
        }
        Ok(reservation_id)
    }
    pub fn matches_record(&self, record: &CanonicalRecordV1) -> Result<(), FormalError> {
        let body = &self.reservation.body;
        if (body.schema_version >= 3 && record.schema_version < 5)
            || record.frozen_reservation_id.as_deref() != Some(body.reservation_id()?.as_str())
            || record.world != body.world
            || record.original_authority_id != body.authority_id
            || record.original_epoch != body.epoch
            || record.activation_ref != body.activation_ref
            || record.position != body.next_position
            || record.parent_record_id != body.parent.record_id
            || record.request_digest != body.request_digest
            || record.payload_digest != body.payload_digest
            || record.runtime_manifest_digest != body.runtime_manifest_digest
            || record.execution_policy_digest != body.execution_policy_digest
            || record.before_state_root != body.pre_state_root
            || record.handoff != body.handoff
        {
            return Err(invalid(
                "formal record/original frozen reservation mismatch",
            ));
        }
        Ok(())
    }
}
