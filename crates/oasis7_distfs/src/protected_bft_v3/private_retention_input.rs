//! Bounded confidential admission originals. DTOs grant neither read permission
//! nor execution authority. The local store uses a restricted operator-owned
//! archive. Remote release requires a separate reader with independent grants.
use super::binding::{BftSignerErrorV3, invalid};
use super::{BftSuccessorWindowArchiveRefV1, LocalBftSuccessorArchiveStoreV1};
use serde::{Deserialize, Serialize};

pub const BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1: usize =
    oasis7_proto::distributed_bft::v3::BFT_CANDIDATE_READ_MAX_PACKAGE_BYTES_V3 as usize;
const MAX_OBJECT: usize = 4 * 1024 * 1024;
pub const BFT_PRIVATE_RETENTION_INPUT_MAX_DEPENDENCY_OBJECT_BYTES_V1: usize = 4 * 1024 * 1024 + 128;
const MAX_DEPENDENCIES: usize = 4096;
const MAX_DEPENDENCY_BYTES: usize = 64 * 1024 * 1024;
const DOMAIN: &str = "oasis7.bft.private-retention.admission-input.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSourceV1 {
    pub binding_digest: String,
    pub handoff_digest: String,
    pub material_source_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionKeyV1 {
    pub ledger_origin_digest: String,
    pub height: u64,
    pub header_id: String,
}

impl BftPrivateRetentionSourceV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        for hash in [
            &self.binding_digest,
            &self.handoff_digest,
            &self.material_source_digest,
        ] {
            super::state::validate_digest(hash)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionAdmissionInputRefV1 {
    pub source: BftPrivateRetentionSourceV1,
    pub key: BftPrivateRetentionKeyV1,
    pub object: BftSuccessorWindowArchiveRefV1,
    pub admission_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BftPrivateRetentionInputObjectKindV1 {
    OriginalData,
    PublicationMetadata,
    Dependency,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionInputObjectV1 {
    pub kind: BftPrivateRetentionInputObjectKindV1,
    pub reference: BftSuccessorWindowArchiveRefV1,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes"
    )]
    pub bytes: Vec<u8>,
}
impl std::fmt::Debug for BftPrivateRetentionInputObjectV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfidentialInputObject")
            .field("kind", &self.kind)
            .field("reference", &self.reference)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionAdmissionInputV1 {
    pub schema_version: u32,
    pub source: BftPrivateRetentionSourceV1,
    pub key: BftPrivateRetentionKeyV1,
    /// Independently re-derived from exact publication metadata by the actual verifier.
    pub publication_reservation_bytes: u64,
    #[serde(deserialize_with = "deserialize_objects")]
    pub objects: Vec<BftPrivateRetentionInputObjectV1>,
}

impl BftPrivateRetentionAdmissionInputV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.source.validate()?;
        super::state::validate_digest(&self.key.ledger_origin_digest)?;
        super::state::validate_digest(&self.key.header_id)?;
        if self.key.height == 0 {
            return Err(invalid("admission input zero height"));
        }
        if self.publication_reservation_bytes == 0
            || self.publication_reservation_bytes > 512 * 1024 * 1024
            || self.schema_version != 1
            || self.objects.len() > MAX_DEPENDENCIES + 2
            || !self
                .objects
                .windows(2)
                .all(|pair| pair[0].reference.digest < pair[1].reference.digest)
        {
            return Err(invalid("admission input version/object order/count"));
        }
        let mut original = 0usize;
        let mut metadata = 0usize;
        let mut dependencies = 0usize;
        let mut dependency_bytes = 0usize;
        for object in &self.objects {
            let maximum = if object.kind == BftPrivateRetentionInputObjectKindV1::Dependency {
                BFT_PRIVATE_RETENTION_INPUT_MAX_DEPENDENCY_OBJECT_BYTES_V1
            } else {
                MAX_OBJECT
            };
            if object.bytes.is_empty() || object.bytes.len() > maximum {
                return Err(invalid("admission input object capacity"));
            }
            object.reference.validate_bytes(&object.bytes)?;
            match object.kind {
                BftPrivateRetentionInputObjectKindV1::OriginalData => original += 1,
                BftPrivateRetentionInputObjectKindV1::PublicationMetadata => metadata += 1,
                BftPrivateRetentionInputObjectKindV1::Dependency => {
                    dependencies += 1;
                    dependency_bytes = dependency_bytes
                        .checked_add(object.bytes.len())
                        .ok_or_else(|| invalid("admission dependency byte overflow"))?;
                }
            }
        }
        if original != 1
            || metadata != 1
            || dependencies > MAX_DEPENDENCIES
            || dependency_bytes > MAX_DEPENDENCY_BYTES
        {
            return Err(invalid("admission input exact original roles/capacity"));
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, BftSignerErrorV3> {
        self.canonical_with_descriptor().map(|(bytes, _)| bytes)
    }
    /// Count/hash without a full scratch buffer, then encode into exactly
    /// reserved storage. Encoded + decoded originals fit the existing two-copy
    /// transport permit; validation never allocates another full encoding.
    pub fn canonical_with_descriptor(
        &self,
    ) -> Result<(Vec<u8>, BftPrivateRetentionAdmissionInputRefV1), BftSignerErrorV3> {
        let descriptor = self.descriptor()?;
        let length = usize::try_from(descriptor.object.bytes)
            .map_err(|_| invalid("admission byte length conversion"))?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| invalid("admission input allocation"))?;
        serde_cbor::to_writer(&mut bytes, self).map_err(|_| invalid("encode admission input"))?;
        if bytes.len() != length {
            return Err(invalid("admission input encoding length changed"));
        }
        Ok((bytes, descriptor))
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, BftSignerErrorV3> {
        if bytes.is_empty() || bytes.len() > BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1 {
            return Err(invalid("admission input total byte capacity"));
        }
        let input: Self =
            serde_cbor::from_slice(bytes).map_err(|_| invalid("decode admission input"))?;
        input.validate()?;
        let mut compare = CompareEncoding {
            original: bytes,
            offset: 0,
        };
        serde_cbor::to_writer(&mut compare, &input)
            .map_err(|_| invalid("noncanonical admission input"))?;
        if compare.offset != bytes.len() {
            return Err(invalid("noncanonical admission input length"));
        }
        Ok(input)
    }
    pub fn admission_digest(&self) -> Result<String, BftSignerErrorV3> {
        self.descriptor()
            .map(|reference| reference.admission_digest)
    }
    pub fn descriptor(&self) -> Result<BftPrivateRetentionAdmissionInputRefV1, BftSignerErrorV3> {
        self.validate()?;
        let mut encoded = HashEncoding::new();
        serde_cbor::to_writer(&mut encoded, self)
            .map_err(|_| invalid("admission input total byte capacity"))?;
        Ok(BftPrivateRetentionAdmissionInputRefV1 {
            source: self.source.clone(),
            key: self.key.clone(),
            object: BftSuccessorWindowArchiveRefV1 {
                digest: encoded.raw.finalize().to_hex().to_string(),
                bytes: encoded.length as u64,
            },
            admission_digest: encoded.admission.finalize().to_hex().to_string(),
        })
    }
}
impl BftPrivateRetentionAdmissionInputRefV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.source.validate()?;
        super::state::validate_digest(&self.key.ledger_origin_digest)?;
        super::state::validate_digest(&self.key.header_id)?;
        super::state::validate_digest(&self.admission_digest)?;
        super::state::validate_digest(&self.object.digest)?;
        if self.key.height == 0
            || self.object.bytes == 0
            || self.object.bytes > BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1 as u64
        {
            return Err(invalid("admission input reference capacity/height"));
        }
        Ok(())
    }
    pub fn validate_input(
        &self,
        input: &BftPrivateRetentionAdmissionInputV1,
    ) -> Result<(), BftSignerErrorV3> {
        self.validate()?;
        if &input.descriptor()? != self {
            return Err(invalid(
                "admission input differs from exact original reference",
            ));
        }
        Ok(())
    }
}

impl LocalBftSuccessorArchiveStoreV1 {
    /// Persist the exact original pinned by the caller. Source, first charge and
    /// original object identities must agree before the store publishes a pin.
    /// Callers must obtain the reference through their own authenticated path.
    pub fn persist_original_admission_input(
        &self,
        expected: &BftPrivateRetentionAdmissionInputRefV1,
        input: &BftPrivateRetentionAdmissionInputV1,
    ) -> Result<(), BftSignerErrorV3> {
        expected.validate()?;
        let (bytes, actual) = input.canonical_with_descriptor()?;
        if &actual != expected {
            return Err(invalid(
                "admission input differs from exact original reference",
            ));
        }
        if self.persist_object(&bytes, BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1)? != expected.object
        {
            return Err(invalid("stored admission input reference changed"));
        }
        Ok(())
    }

    /// Cold-read an independently pinned reference. Loading yields material for
    /// a separate Runtime verifier and does not authorize serving it to peers.
    pub fn load_original_admission_input(
        &self,
        expected: &BftPrivateRetentionAdmissionInputRefV1,
    ) -> Result<BftPrivateRetentionAdmissionInputV1, BftSignerErrorV3> {
        expected.validate()?;
        let bytes = self.load_object(&expected.object, BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1)?;
        let input = BftPrivateRetentionAdmissionInputV1::from_canonical_bytes(&bytes)?;
        expected.validate_input(&input)?;
        Ok(input)
    }
}

fn serialize_bytes<S: serde::Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(bytes)
}
fn deserialize_bytes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<u8>, D::Error> {
    struct Bytes;
    impl<'de> serde::de::Visitor<'de> for Bytes {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded original admission object bytes")
        }
        fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
            if bytes.len() > BFT_PRIVATE_RETENTION_INPUT_MAX_DEPENDENCY_OBJECT_BYTES_V1 {
                return Err(E::custom("admission object byte capacity"));
            }
            Ok(bytes.to_vec())
        }
    }
    deserializer.deserialize_bytes(Bytes)
}
fn deserialize_objects<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BftPrivateRetentionInputObjectV1>, D::Error> {
    struct Objects;
    impl<'de> serde::de::Visitor<'de> for Objects {
        type Value = Vec<BftPrivateRetentionInputObjectV1>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded original admission objects")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_DEPENDENCIES + 2) {
                return Err(serde::de::Error::custom("admission object count"));
            }
            let mut objects = Vec::new();
            while let Some(object) = seq.next_element()? {
                if objects.len() >= MAX_DEPENDENCIES + 2 {
                    return Err(serde::de::Error::custom("admission object count"));
                }
                objects.push(object);
            }
            Ok(objects)
        }
    }
    deserializer.deserialize_seq(Objects)
}

struct HashEncoding {
    raw: blake3::Hasher,
    admission: blake3::Hasher,
    length: usize,
}
impl HashEncoding {
    fn new() -> Self {
        let mut admission = blake3::Hasher::new();
        admission.update(&(DOMAIN.len() as u64).to_be_bytes());
        admission.update(DOMAIN.as_bytes());
        Self {
            raw: blake3::Hasher::new(),
            admission,
            length: 0,
        }
    }
}
impl std::io::Write for HashEncoding {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.length = self
            .length
            .checked_add(bytes.len())
            .filter(|n| *n <= BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1)
            .ok_or_else(|| std::io::Error::other("admission encoding bound"))?;
        self.raw.update(bytes);
        self.admission.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct CompareEncoding<'a> {
    original: &'a [u8],
    offset: usize,
}
impl std::io::Write for CompareEncoding<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .offset
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("admission comparison overflow"))?;
        if self.original.get(self.offset..end) != Some(bytes) {
            return Err(std::io::Error::other("noncanonical admission encoding"));
        }
        self.offset = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "private_retention_input_tests.rs"]
mod tests;
