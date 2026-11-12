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
    verify_formal_reservation_with(reservation, authority, parent, &mut LegacyReservationOps)
}

pub(super) trait ReservationValidationOps {
    type Preimage: AsRef<[u8]>;
    fn before_validation(
        &mut self,
        _: &SignedFormalFrozenV1,
        _: &VerifiedAuthorityV1,
        _: &VerifiedFormalCommitV1,
    ) -> Result<(), FormalError>;
    fn authority_id(&self) -> Option<&str> {
        None
    }
    fn preimage(&mut self, body: &FormalFrozenBodyV1) -> Result<Self::Preimage, FormalError>;
    fn crypto_work(&mut self, n: usize) -> Result<(), FormalError>;
    fn before_clone(&mut self, _: &SignedFormalFrozenV1) -> Result<(), FormalError>;
}
struct LegacyReservationOps;
impl ReservationValidationOps for LegacyReservationOps {
    type Preimage = Vec<u8>;
    fn before_validation(
        &mut self,
        _: &SignedFormalFrozenV1,
        _: &VerifiedAuthorityV1,
        _: &VerifiedFormalCommitV1,
    ) -> Result<(), FormalError> {
        Ok(())
    }
    fn preimage(&mut self, body: &FormalFrozenBodyV1) -> Result<Vec<u8>, FormalError> {
        bytes(RESERVATION, body)
    }
    fn crypto_work(&mut self, _: usize) -> Result<(), FormalError> {
        Ok(())
    }
    fn before_clone(&mut self, _: &SignedFormalFrozenV1) -> Result<(), FormalError> {
        Ok(())
    }
}
pub(super) fn verify_formal_reservation_with<O: ReservationValidationOps>(
    reservation: &SignedFormalFrozenV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    ops: &mut O,
) -> Result<VerifiedFormalReservationV1, FormalError> {
    let body = &reservation.body;
    ops.before_validation(reservation, authority, parent)?;
    body.validate()?;
    if (authority.definition().profile_version == 3) != (body.schema_version == 4) {
        return Err(invalid("frozen reservation authority format"));
    }
    if super::incremental::is_iws_record(parent.record().schema_version)
        != (body.schema_version == 4)
    {
        return Err(invalid("frozen reservation incremental parent format"));
    }
    super::proof::verify_normal_parent_continuity_with_id(
        authority,
        parent,
        body.next_position,
        ops.authority_id(),
    )?;
    if parent
        .record()
        .artifact_roots
        .contains_key(&ArtifactRole::NativeExecutionEnvironment)
        && body.schema_version < 3
    {
        return Err(invalid("native parent requires native frozen schema"));
    }
    if body.world != authority.definition().world
        || body.authority_id
            != match ops.authority_id() {
                Some(value) => std::borrow::Cow::Borrowed(value),
                None => std::borrow::Cow::Owned(authority.authority_id()?),
            }
            .as_ref()
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
    {
        let ops = std::cell::RefCell::new(&mut *ops);
        verify_with_preimage(
            &reservation.signature_hex,
            &authority.definition().writer_public_key,
            || ops.borrow_mut().preimage(body),
            |n| ops.borrow_mut().crypto_work(n),
        )?;
    }
    ops.before_clone(reservation)?;
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
        if !matches!(self.schema_version, 1..=4)
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
    if body.schema_version == 4 {
        return Err(invalid(
            "incremental frozen requires bounded typed material verification",
        ));
    }
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

use super::fencing::EndpointInstanceBindingV1;

pub(super) trait CertificateValidationOps {
    type Digest: AsRef<str>;
    type Preimage: AsRef<[u8]>;
    fn before_validation(
        &mut self,
        certificate: &FormalFrozenCertificateV1,
        authority: &AuthorityDefinitionV1,
    ) -> Result<(), FormalError>;
    fn authority_id(
        &mut self,
        authority: &AuthorityDefinitionV1,
    ) -> Result<Self::Digest, FormalError>;
    fn reservation_id(&mut self, body: &FormalFrozenBodyV1) -> Result<Self::Digest, FormalError>;
    fn instance_id(
        &mut self,
        instance: &EndpointInstanceBindingV1,
    ) -> Result<Self::Digest, FormalError>;
    fn reservation_preimage(
        &mut self,
        body: &FormalFrozenBodyV1,
    ) -> Result<Self::Preimage, FormalError>;
    fn receipt_preimage(
        &mut self,
        body: &FormalFrozenReceiptBodyV1,
    ) -> Result<Self::Preimage, FormalError>;
    fn crypto_work(&mut self, bytes: usize) -> Result<(), FormalError>;
}
struct LegacyCertificateOps;
impl CertificateValidationOps for LegacyCertificateOps {
    type Digest = String;
    type Preimage = Vec<u8>;
    fn before_validation(
        &mut self,
        _: &FormalFrozenCertificateV1,
        _: &AuthorityDefinitionV1,
    ) -> Result<(), FormalError> {
        Ok(())
    }
    fn authority_id(&mut self, authority: &AuthorityDefinitionV1) -> Result<String, FormalError> {
        authority.authority_id()
    }
    fn reservation_id(&mut self, body: &FormalFrozenBodyV1) -> Result<String, FormalError> {
        body.reservation_id()
    }
    fn instance_id(&mut self, instance: &EndpointInstanceBindingV1) -> Result<String, FormalError> {
        instance.digest().map_err(|e| invalid(e.to_string()))
    }
    fn reservation_preimage(&mut self, body: &FormalFrozenBodyV1) -> Result<Vec<u8>, FormalError> {
        bytes(RESERVATION, body)
    }
    fn receipt_preimage(
        &mut self,
        body: &FormalFrozenReceiptBodyV1,
    ) -> Result<Vec<u8>, FormalError> {
        bytes(RECEIPT, body)
    }
    fn crypto_work(&mut self, _: usize) -> Result<(), FormalError> {
        Ok(())
    }
}
pub(super) fn verify_frozen_certificate_with<O: CertificateValidationOps>(
    certificate: &FormalFrozenCertificateV1,
    authority: &AuthorityDefinitionV1,
    activation_ref: &str,
    ops: &mut O,
) -> Result<O::Digest, FormalError> {
    ops.before_validation(certificate, authority)?;
    authority.validate()?;
    let body = &certificate.reservation.body;
    body.validate()?;
    if (body.schema_version == 4) != (authority.profile_version == 3) {
        return Err(invalid("frozen authority incremental format mismatch"));
    }
    let authority_id = ops.authority_id(authority)?;
    let reservation_id = ops.reservation_id(body)?;
    if body.world != authority.world
        || body.authority_id != authority_id.as_ref()
        || body.epoch != authority.epoch
        || body.activation_ref != activation_ref
        || body.next_position < authority.valid_from_position
    {
        return Err(invalid("formal frozen authority/activation binding"));
    }
    {
        let ops = std::cell::RefCell::new(&mut *ops);
        verify_with_preimage(
            &certificate.reservation.signature_hex,
            &authority.writer_public_key,
            || ops.borrow_mut().reservation_preimage(body),
            |n| ops.borrow_mut().crypto_work(n),
        )?;
    }
    for (receipt, binding, role) in [
        (
            &certificate.primary,
            &authority.primary,
            EndpointRole::Primary,
        ),
        (
            &certificate.replica,
            &authority.replica,
            EndpointRole::Replica,
        ),
    ] {
        if receipt.body.authority_id != authority_id.as_ref()
            || receipt.body.reservation_id != reservation_id.as_ref()
            || receipt.body.role != role
            || receipt.body.instance_digest != ops.instance_id(&binding.instance)?.as_ref()
        {
            return Err(invalid("formal frozen endpoint receipt binding"));
        }
        let ops = std::cell::RefCell::new(&mut *ops);
        verify_with_preimage(
            &receipt.signature_hex,
            &binding.instance.endpoint_public_key,
            || ops.borrow_mut().receipt_preimage(&receipt.body),
            |n| ops.borrow_mut().crypto_work(n),
        )?;
    }
    Ok(reservation_id)
}

impl FormalFrozenCertificateV1 {
    pub fn verify(
        &self,
        authority: &AuthorityDefinitionV1,
        activation_ref: &str,
    ) -> Result<String, FormalError> {
        verify_frozen_certificate_with(self, authority, activation_ref, &mut LegacyCertificateOps)
    }
    pub fn matches_record(&self, record: &CanonicalRecordV1) -> Result<(), FormalError> {
        let body = &self.reservation.body;
        if ((body.schema_version == 4) != super::incremental::is_iws_record(record.schema_version))
            || (body.schema_version >= 3 && record.schema_version < 5)
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

#[path = "frozen_material_stream.rs"]
mod material_stream;
pub use material_stream::frozen_material_digest_bounded_v1;
