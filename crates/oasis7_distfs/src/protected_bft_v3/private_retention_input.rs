//! Bounded confidential admission originals. DTOs grant neither read permission
//! nor execution authority; only the encrypted, independently authorized reader
//! may transport these bytes. Plain signer RPCs carry references alone.
use super::binding::{BftSignerErrorV3, invalid};
use super::private_retention::{
    BftPrivateRetentionFactV1, BftPrivateRetentionKeyV1, BftPrivateRetentionSourceV1,
};
use super::successor_window::BftSuccessorWindowArchiveRefV1;
use serde::{Deserialize, Serialize};
use std::time::Instant;

pub const BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1: usize =
    oasis7_proto::distributed_bft::v3::BFT_CANDIDATE_READ_MAX_PACKAGE_BYTES_V3 as usize;
const MAX_OBJECT: usize = 4 * 1024 * 1024;
pub const BFT_PRIVATE_RETENTION_INPUT_MAX_DEPENDENCY_OBJECT_BYTES_V1: usize = 4 * 1024 * 1024 + 128;
const MAX_DEPENDENCIES: usize = 4096;
const MAX_DEPENDENCY_BYTES: usize = 64 * 1024 * 1024;
const DOMAIN: &str = "oasis7.bft.private-retention.admission-input.v1";

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
            {
                #[cfg(test)]
                let _object_hash_scope =
                    super::private_retention_memory::hash_probe::ObjectHashScope::enter();
                object.reference.validate_bytes(&object.bytes)?;
            }
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
    pub fn fact(&self) -> Result<BftPrivateRetentionFactV1, BftSignerErrorV3> {
        self.validate()?;
        let reference = |kind| {
            self.objects
                .iter()
                .find(|object| object.kind == kind)
                .map(|object| object.reference.digest.clone())
                .ok_or_else(|| invalid("admission original role missing"))
        };
        Ok(BftPrivateRetentionFactV1::Some {
            original_data_ref: reference(BftPrivateRetentionInputObjectKindV1::OriginalData)?,
            metadata_ref: reference(BftPrivateRetentionInputObjectKindV1::PublicationMetadata)?,
            admission_digest: self.admission_digest()?,
            input_ref: Some(self.descriptor()?.object),
        })
    }
}
impl BftPrivateRetentionAdmissionInputRefV1 {
    pub fn from_entry(
        source: &BftPrivateRetentionSourceV1,
        entry: &super::private_retention::BftPrivateRetentionEntryV1,
    ) -> Result<Self, BftSignerErrorV3> {
        let BftPrivateRetentionFactV1::Some {
            admission_digest,
            input_ref: Some(object),
            ..
        } = &entry.fact
        else {
            return Err(invalid(
                "original retention entry has no atomic input reference",
            ));
        };
        let reference = Self {
            source: source.clone(),
            key: entry.key.clone(),
            object: object.clone(),
            admission_digest: admission_digest.clone(),
        };
        reference.validate()?;
        Ok(reference)
    }
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

/// Implementations must authorize every bounded/chunk release using fresh
/// read grants and the real Noise/QUIC identity; an attestor role is no grant.
/// A returned DTO is material only: the Runtime verifier still checks exact
/// canonical Data/metadata, actual World, source ownership and closed dependencies.
pub trait BftPrivateRetentionAdmissionInputReaderV1: Send + Sync {
    fn read_original(
        &self,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
        deadline: Instant,
    ) -> Result<BftPrivateRetentionAdmissionInputReadV1, BftSignerErrorV3>;
}
/// Owns the receiving transport's real buffer permit until validation, atomic
/// persistence and exact readback finish. This is a resource lifetime, never an
/// authorization capability. There is deliberately no consuming DTO accessor.
pub struct BftPrivateRetentionAdmissionInputReadV1 {
    input: std::sync::Arc<BftPrivateRetentionAdmissionInputV1>,
    _hold: Box<dyn Send + Sync>,
}
impl BftPrivateRetentionAdmissionInputReadV1 {
    pub fn new(
        input: std::sync::Arc<BftPrivateRetentionAdmissionInputV1>,
        hold: Box<dyn Send + Sync>,
    ) -> Self {
        Self { input, _hold: hold }
    }
    pub fn input(&self) -> &BftPrivateRetentionAdmissionInputV1 {
        &self.input
    }
    /// Transfer this existing two-buffer permit from decoded+encoded receive
    /// work to encoded+durable-readback work. No decoded DTO or permit escapes.
    /// Shared decoded aliases must first finish; copying their payload is not a
    /// substitute for ownership transfer under the original peak reservation.
    pub fn into_canonical_archive_read(
        self,
        expected: &BftPrivateRetentionAdmissionInputRefV1,
    ) -> Result<BftPrivateRetentionEncodedArchiveReadV1, BftSignerErrorV3> {
        let Self { input, _hold } = self;
        let input = std::sync::Arc::try_unwrap(input)
            .map_err(|_| invalid("archive input decoded ownership is still shared"))?;
        expected.validate_input(&input)?;
        let bytes = input.canonical_bytes()?;
        expected.object.validate_bytes(&bytes)?;
        drop(input);
        Ok(BftPrivateRetentionEncodedArchiveReadV1 { bytes, _hold })
    }
    pub(crate) fn original_for_atomic_storage(
        &self,
    ) -> std::sync::Arc<BftPrivateRetentionAdmissionInputV1> {
        self.input.clone()
    }
}
impl std::fmt::Debug for BftPrivateRetentionAdmissionInputReadV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BudgetedAdmissionInputRead")
            .field("source", &self.input.source)
            .field("key", &self.input.key)
            .finish_non_exhaustive()
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
        #[cfg(test)]
        super::private_retention_memory::hash_probe::canonical_hasher_updated(bytes.len());
        self.admission.update(bytes);
        #[cfg(test)]
        super::private_retention_memory::hash_probe::canonical_hasher_updated(bytes.len());
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

/// Encoded original bytes plus the very same real receiving peak permit.
/// Persistence may hold these bytes and one readback buffer; decoded input has
/// already been released. This is resource ownership, not authorization.
pub struct BftPrivateRetentionEncodedArchiveReadV1 {
    bytes: Vec<u8>,
    _hold: Box<dyn Send + Sync>,
}
impl BftPrivateRetentionEncodedArchiveReadV1 {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
