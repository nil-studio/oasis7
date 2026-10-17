//! Profile 2's durable append-only business result transition. World/result DTOs
//! stay opaque here; runtime and consumers verify their exact semantic alignment.
use super::business_index::{
    BusinessIndexDeltaV1, BusinessIndexNodeReader, BusinessIndexNodeWriteV1, BusinessIndexRootV1,
};
use super::{crypto::invalid, *};
use crate::controlled_authority::replicated_protocol::{ArtifactRole, ClosedRecord};
use serde::{Deserialize, Serialize};
pub const FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1: &str = "oasis7.formal.business-result-envelope.v1";
pub const FORMAL_BUSINESS_ENVELOPE_LIMIT: usize = 64 * 1024 * 1024;
const RESULT_BYTES_LIMIT: usize = 8 * 1024 * 1024;
const DESCRIPTOR_BYTES_LIMIT: usize = 64 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBusinessResultEnvelopeV1 {
    pub schema_version: u32,
    pub domain: String,
    #[serde(serialize_with = "serialize_bytes", deserialize_with = "decode_result")]
    pub execution_result_cbor: Vec<u8>,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "decode_descriptor"
    )]
    pub tree_descriptor_cbor: Vec<u8>,
    pub delta: BusinessIndexDeltaV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_history_delta: Option<BusinessIndexDeltaV1>,
}
fn serialize_bytes<S: serde::Serializer>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(value)
}
fn decode_result<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Vec<u8>, D::Error> {
    decode_bounded_bytes::<D, RESULT_BYTES_LIMIT>(decoder)
}
fn decode_descriptor<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Vec<u8>, D::Error> {
    decode_bounded_bytes::<D, DESCRIPTOR_BYTES_LIMIT>(decoder)
}
fn decode_bounded_bytes<'de, D: serde::Deserializer<'de>, const LIMIT: usize>(
    decoder: D,
) -> Result<Vec<u8>, D::Error> {
    struct Visitor<const LIMIT: usize>;
    impl<'de, const LIMIT: usize> serde::de::Visitor<'de> for Visitor<LIMIT> {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded opaque canonical CBOR bytes")
        }
        fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Vec<u8>, E> {
            if bytes.len() > LIMIT {
                return Err(E::custom("opaque CBOR byte bound"));
            }
            Ok(bytes.to_vec())
        }
        fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<Vec<u8>, E> {
            if bytes.len() > LIMIT {
                return Err(E::custom("opaque CBOR byte bound"));
            }
            Ok(bytes)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Vec<u8>, A::Error> {
            if sequence.size_hint().is_some_and(|len| len > LIMIT) {
                return Err(serde::de::Error::custom("opaque CBOR byte bound"));
            }
            let mut bytes = Vec::new();
            while let Some(byte) = sequence.next_element()? {
                if bytes.len() == LIMIT {
                    return Err(serde::de::Error::custom("opaque CBOR byte bound"));
                }
                bytes.push(byte);
            }
            Ok(bytes)
        }
    }
    decoder.deserialize_byte_buf(Visitor::<LIMIT>)
}
impl FormalBusinessResultEnvelopeV1 {
    pub fn encode(&self) -> Result<Vec<u8>, FormalError> {
        self.validate_shape()?;
        let bytes = serde_cbor::to_vec(self).map_err(|error| invalid(error.to_string()))?;
        if bytes.len() > FORMAL_BUSINESS_ENVELOPE_LIMIT {
            return Err(invalid("formal business envelope byte bound"));
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, FormalError> {
        if bytes.len() > FORMAL_BUSINESS_ENVELOPE_LIMIT {
            return Err(invalid("formal business envelope byte bound"));
        }
        let envelope: Self =
            serde_cbor::from_slice(bytes).map_err(|error| invalid(error.to_string()))?;
        envelope.validate_shape()?;
        Ok(envelope)
    }
    fn validate_shape(&self) -> Result<(), FormalError> {
        if self.schema_version != 1
            || self.domain != FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1
            || self.execution_result_cbor.is_empty()
            || self.execution_result_cbor.len() > RESULT_BYTES_LIMIT
            || self.tree_descriptor_cbor.is_empty()
            || self.tree_descriptor_cbor.len() > DESCRIPTOR_BYTES_LIMIT
        {
            return Err(invalid(
                "formal business envelope schema/domain/opaque bytes",
            ));
        }
        Ok(())
    }
}
/// Material integrity only: this token grants neither execution nor qualification.
/// Endpoint prepare additionally verifies writer/candidate/authority permissions.
pub struct VerifiedFormalBusinessMaterialV1 {
    envelope: FormalBusinessResultEnvelopeV1,
}
impl VerifiedFormalBusinessMaterialV1 {
    pub fn envelope(&self) -> &FormalBusinessResultEnvelopeV1 {
        &self.envelope
    }
    pub fn retained_node_writes(&self) -> &[BusinessIndexNodeWriteV1] {
        self.envelope
            .retained_history_delta
            .as_ref()
            .map_or(&[], |delta| delta.node_writes.as_slice())
    }
    pub fn node_writes(&self) -> &[BusinessIndexNodeWriteV1] {
        &self.envelope.delta.node_writes
    }
}

/// Request-local evidence that a candidate's exact closed record and optional
/// Frozen closure were checked. The references make the evidence unusable for
/// an equal-valued clone or for replacement material.
pub(super) struct VerifiedCandidateMaterialClosureV1<'a> {
    candidate: &'a endpoint::FormalCandidateMaterialV1,
    record: &'a CanonicalRecordV1,
    material: &'a ClosedRecord,
    frozen_body: Option<&'a FormalFrozenBodyV1>,
    frozen_material: Option<&'a ClosedRecord>,
}

impl<'a> VerifiedCandidateMaterialClosureV1<'a> {
    pub(super) fn verify(
        candidate: &'a endpoint::FormalCandidateMaterialV1,
    ) -> Result<Self, FormalError> {
        let record = &candidate.candidate.record;
        let material = &candidate.material;
        let frozen_body = candidate
            .frozen
            .as_ref()
            .map(|certificate| &certificate.reservation.body);
        let frozen_material = candidate.frozen_material.as_ref();
        verify_record_material(record, material)?;
        match (frozen_body, frozen_material) {
            (Some(body), Some(frozen)) => verify_frozen_material(body, frozen)?,
            (None, None) if record.kind == FormalRecordKindV1::InitialActivation => {}
            _ => {
                return Err(invalid("formal candidate complete original frozen closure"));
            }
        }
        Ok(Self {
            candidate,
            record,
            material,
            frozen_body,
            frozen_material,
        })
    }

    pub(super) fn matches_candidate(
        &self,
        candidate: &endpoint::FormalCandidateMaterialV1,
    ) -> bool {
        let material = &candidate.material;
        std::ptr::eq(self.candidate, candidate)
            && std::ptr::eq(self.record, &candidate.candidate.record)
            && std::ptr::eq(self.material, material)
            && same_optional_ref(
                self.frozen_body,
                candidate
                    .frozen
                    .as_ref()
                    .map(|certificate| &certificate.reservation.body),
            )
            && same_optional_ref(self.frozen_material, candidate.frozen_material.as_ref())
    }
}

fn same_optional_ref<T>(left: Option<&T>, right: Option<&T>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => std::ptr::eq(left, right),
        (None, None) => true,
        _ => false,
    }
}

pub fn verify_formal_business_material(
    authority: &VerifiedAuthorityV1,
    record: &CanonicalRecordV1,
    material: &ClosedRecord,
    parent: Option<&endpoint::VerifiedFormalBusinessParentV1>,
    frozen_material: Option<&ClosedRecord>,
    reader: &impl BusinessIndexNodeReader,
) -> Result<Option<VerifiedFormalBusinessMaterialV1>, FormalError> {
    match authority.definition().profile_version {
        1 => return Ok(None),
        2 => {}
        _ => return Err(invalid("formal business profile contract")),
    }
    verify_record_material(record, material)?;
    verify_formal_business_material_after_closure(record, material, parent, frozen_material, reader)
}

pub(super) fn verify_formal_business_material_with_closure(
    authority: &VerifiedAuthorityV1,
    candidate: &endpoint::FormalCandidateMaterialV1,
    parent: Option<&endpoint::VerifiedFormalBusinessParentV1>,
    reader: &impl BusinessIndexNodeReader,
    closure: &VerifiedCandidateMaterialClosureV1<'_>,
) -> Result<Option<VerifiedFormalBusinessMaterialV1>, FormalError> {
    if !closure.matches_candidate(candidate) {
        return Err(invalid("formal candidate closure token identity mismatch"));
    }
    let record = &candidate.candidate.record;
    let material = &candidate.material;
    let frozen_material = candidate.frozen_material.as_ref();
    match authority.definition().profile_version {
        1 => return Ok(None),
        2 => {}
        _ => return Err(invalid("formal business profile contract")),
    }
    verify_formal_business_material_after_closure(record, material, parent, frozen_material, reader)
}

fn verify_formal_business_material_after_closure(
    record: &CanonicalRecordV1,
    material: &ClosedRecord,
    parent: Option<&endpoint::VerifiedFormalBusinessParentV1>,
    frozen_material: Option<&ClosedRecord>,
    reader: &impl BusinessIndexNodeReader,
) -> Result<Option<VerifiedFormalBusinessMaterialV1>, FormalError> {
    let root = material
        .roots
        .get(&ArtifactRole::Result)
        .ok_or_else(|| invalid("profile2 business envelope Result artifact"))?;
    let object = material
        .objects
        .iter()
        .find(|object| &object.content_hash == root)
        .ok_or_else(|| invalid("profile2 missing Result envelope bytes"))?;
    let envelope = FormalBusinessResultEnvelopeV1::decode(&object.bytes)?;
    verify_business_metadata_contract(&envelope.delta, reader)?;
    if record.kind == FormalRecordKindV1::Execution
        && frozen_material
            .map(|material| material.snapshot_bytes())
            .transpose()
            .map_err(|e| invalid(e.to_string()))?
            .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
            .as_ref()
            != Some(&record.before_state_root)
    {
        return Err(invalid(
            "retention exact signed Frozen predecessor Snapshot root",
        ));
    }
    retention::verify_retained_transition(
        record.kind == FormalRecordKindV1::InitialActivation,
        material,
        frozen_material,
        envelope.retained_history_delta.as_ref(),
        reader,
    )?;
    let parent_result_root =
        history_validation::verify_parent_history_cut(record, &envelope, parent)?;
    let empty;
    let base = match record.kind {
        FormalRecordKindV1::InitialActivation => {
            empty = BusinessIndexRootV1::empty()
                .map_err(invalid)?
                .canonical_result_root()
                .map_err(invalid)?;
            &empty
        }
        FormalRecordKindV1::Execution => parent_result_root
            .ok_or_else(|| invalid("profile2 business delta missing verified parent root"))?,
    };
    envelope.delta.verify(base, reader).map_err(invalid)?;
    if envelope.delta.new_root != record.result_root {
        return Err(invalid(
            "profile2 canonical result_root must equal business delta newRoot",
        ));
    }
    Ok(Some(VerifiedFormalBusinessMaterialV1 { envelope }))
}

/// Revalidate an already qualified historical material transition. The child
/// canonical record commits its parent ID, so the embedded exact parent body
/// supplies the base root without granting any new parent qualification.
pub(crate) fn verify_committed_business_material(
    record: &CanonicalRecordV1,
    material: &ClosedRecord,
    frozen_material: Option<&ClosedRecord>,
    reader: &impl BusinessIndexNodeReader,
) -> Result<(), FormalError> {
    verify_record_material(record, material)?;
    let root = material
        .roots
        .get(&ArtifactRole::Result)
        .ok_or_else(|| invalid("historical Result envelope root"))?;
    let object = material
        .objects
        .iter()
        .find(|object| &object.content_hash == root)
        .ok_or_else(|| invalid("historical Result envelope bytes"))?;
    let envelope = FormalBusinessResultEnvelopeV1::decode(&object.bytes)?;
    let mut cuts = envelope
        .delta
        .updates
        .iter()
        .filter(|leaf| leaf.kind == business_index::BusinessIndexLeafKindV1::HistoricalCut);
    let base = match record.kind {
        FormalRecordKindV1::InitialActivation => {
            if cuts.next().is_some() {
                return Err(invalid("initial historical material has parent cut"));
            }
            BusinessIndexRootV1::empty()
                .map_err(invalid)?
                .canonical_result_root()
                .map_err(invalid)?
        }
        FormalRecordKindV1::Execution => {
            let position = record
                .position
                .checked_sub(1)
                .ok_or_else(|| invalid("historical parent position"))?;
            let leaf = cuts
                .next()
                .ok_or_else(|| invalid("historical material missing parent cut"))?;
            if cuts.next().is_some() {
                return Err(invalid("historical material multiple parent cuts"));
            }
            let cut = FormalHistoricalCutV1::decode_leaf(leaf, &record.world, position)?;
            if cut.parent_record.record_id()? != record.parent_record_id
                || cut.parent_record.activation_ref != record.activation_ref
                || cut.parent_record.after_state_root != record.before_state_root
                || frozen_material
                    .map(|material| material.snapshot_bytes())
                    .transpose()
                    .map_err(|e| invalid(e.to_string()))?
                    .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
                    .as_ref()
                    != Some(&record.before_state_root)
            {
                return Err(invalid("historical material hash-bound exact parent"));
            }
            cut.parent_record.result_root
        }
    };
    verify_business_metadata_contract(&envelope.delta, reader)?;
    retention::verify_retained_transition(
        record.kind == FormalRecordKindV1::InitialActivation,
        material,
        frozen_material,
        envelope.retained_history_delta.as_ref(),
        reader,
    )?;
    envelope.delta.verify(&base, reader).map_err(invalid)?;
    if envelope.delta.new_root != record.result_root {
        return Err(invalid("historical business transition signed new root"));
    }
    Ok(())
}

#[path = "business_history_validation.rs"]
mod history_validation;

#[path = "business_retention.rs"]
mod retention;

fn verify_business_metadata_contract(
    delta: &BusinessIndexDeltaV1,
    reader: &impl BusinessIndexNodeReader,
) -> Result<(), FormalError> {
    use business_index::{BusinessIndexLeafKindV1, metadata_key_digest};
    let key = metadata_key_digest();
    let mut count = 0usize;
    for leaf in &delta.updates {
        if leaf.kind == BusinessIndexLeafKindV1::CommitMetadata {
            count += 1;
            if leaf.key_digest != key {
                return Err(invalid("business result metadata fixed key"));
            }
        } else if leaf.key_digest == key {
            return Err(invalid("business result immutable leaf uses metadata key"));
        }
    }
    if count != 1 {
        return Err(invalid(
            "business result delta requires exactly one metadata update",
        ));
    }
    if delta.base.leaf_count > 0
        && delta
            .base
            .get(&key, reader)
            .map_err(invalid)?
            .is_none_or(|leaf| leaf.kind != BusinessIndexLeafKindV1::CommitMetadata)
    {
        return Err(invalid(
            "business result predecessor metadata missing/wrong kind",
        ));
    }
    Ok(())
}
#[cfg(test)]
#[path = "business_metadata_tests.rs"]
mod metadata_tests;
