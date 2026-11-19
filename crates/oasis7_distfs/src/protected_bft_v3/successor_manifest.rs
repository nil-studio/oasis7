//! Canonical successor archive metadata. It carries no checkpoint authority.
use super::binding::{BftSignerErrorV3, invalid};
use super::crypto::{decode_bounded, digest_hex, encode_bounded};
use super::{
    BftSuccessorWindowArchiveRefV1, BftSuccessorWindowSourceV1, LocalBftSuccessorArchiveStoreV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1: usize = 1024 * 1024;
pub const BFT_SUCCESSOR_WINDOW_MAX_SEGMENTS_V1: u64 = 64;
pub const BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1: u64 = 4 * 1024 * 1024 * 1024;
pub const BFT_SUCCESSOR_WINDOW_MAX_REFS_V1: usize = 4096;
pub const BFT_SUCCESSOR_WINDOW_ARCHIVE_DOMAIN_V1: &str =
    "oasis7.bft.successor-window.v1.archive-manifest";

fn hash(value: &str) -> Result<(), BftSignerErrorV3> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "successor window requires a lowercase 32-byte hash",
        ));
    }
    Ok(())
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowArchiveUsageV1 {
    pub segments: u64,
    pub objects: u64,
    pub total_bytes: u64,
}
impl BftSuccessorWindowArchiveUsageV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        if self.segments > BFT_SUCCESSOR_WINDOW_MAX_SEGMENTS_V1
            || self.total_bytes > BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1
            || self.objects > self.total_bytes
        {
            return Err(invalid("successor cumulative archive capacity"));
        }
        Ok(())
    }
    pub fn checked_add(self, delta: Self) -> Result<Self, BftSignerErrorV3> {
        self.validate()?;
        delta.validate()?;
        let next = Self {
            segments: self
                .segments
                .checked_add(delta.segments)
                .ok_or_else(|| invalid("successor segment overflow"))?,
            objects: self
                .objects
                .checked_add(delta.objects)
                .ok_or_else(|| invalid("successor object overflow"))?,
            total_bytes: self
                .total_bytes
                .checked_add(delta.total_bytes)
                .ok_or_else(|| invalid("successor byte overflow"))?,
        };
        next.validate()?;
        Ok(next)
    }
    pub fn validate_after(&self, previous: &Self) -> Result<(), BftSignerErrorV3> {
        self.validate()?;
        previous.validate()?;
        if self.segments <= previous.segments
            || self.objects < previous.objects
            || self.total_bytes < previous.total_bytes
        {
            return Err(invalid("successor archive usage regression"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BftSuccessorWindowArchiveKindV1 {
    Public,
    Execution,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowArchiveEntryV1 {
    pub identity_digest: String,
    pub object: BftSuccessorWindowArchiveRefV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BftSuccessorWindowPublicIndexV1 {
    WholeStateSignedResultsV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowArchiveManifestV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_index: Option<BftSuccessorWindowPublicIndexV1>,
    pub schema_version: u32,
    pub source: BftSuccessorWindowSourceV1,
    pub kind: BftSuccessorWindowArchiveKindV1,
    pub segment_sequence: u64,
    pub predecessor_root: Option<String>,
    pub first_height: u64,
    pub last_height: u64,
    #[serde(deserialize_with = "bounded_entries")]
    pub entries: Vec<BftSuccessorWindowArchiveEntryV1>,
    pub usage: BftSuccessorWindowArchiveUsageV1,
}
fn bounded_entries<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BftSuccessorWindowArchiveEntryV1>, D::Error> {
    use serde::de::{Error, IgnoredAny, SeqAccess, Visitor};

    struct Entries;
    impl<'de> Visitor<'de> for Entries {
        type Value = Vec<BftSuccessorWindowArchiveEntryV1>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a bounded successor archive entry sequence")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence
                .size_hint()
                .is_some_and(|size| size > BFT_SUCCESSOR_WINDOW_MAX_REFS_V1)
            {
                return Err(A::Error::custom(
                    "successor archive entry count exceeds bound",
                ));
            }
            let mut entries = Vec::new();
            while entries.len() < BFT_SUCCESSOR_WINDOW_MAX_REFS_V1 {
                let Some(entry) = sequence.next_element()? else {
                    return Ok(entries);
                };
                entries.push(entry);
            }
            if sequence.next_element::<IgnoredAny>()?.is_some() {
                return Err(A::Error::custom(
                    "successor archive entry count exceeds bound",
                ));
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_seq(Entries)
}

impl BftSuccessorWindowArchiveManifestV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.source.validate().map_err(invalid)?;
        self.usage.validate()?;
        if self.public_index.is_some() && self.kind != BftSuccessorWindowArchiveKindV1::Public {
            return Err(invalid("successor public index on non-public manifest"));
        }
        if self.schema_version != 1
            || self.segment_sequence == 0
            || self.segment_sequence > self.usage.segments
            || self.first_height <= self.source.handoff_position
            || self.last_height < self.first_height
            || self
                .last_height
                .checked_sub(self.first_height)
                .is_none_or(|n| n >= 64)
            || self.entries.is_empty()
            || self.entries.len() > BFT_SUCCESSOR_WINDOW_MAX_REFS_V1
            || (self.segment_sequence == 1) != self.predecessor_root.is_none()
        {
            return Err(invalid("successor archive manifest version/range/count"));
        }
        if let Some(root) = &self.predecessor_root {
            hash(root)?;
        }
        let mut previous = None;
        let mut bytes = 0_u64;
        let mut objects = BTreeMap::new();
        for entry in &self.entries {
            hash(&entry.identity_digest)?;
            entry.object.validate()?;
            if previous.is_some_and(|id: &str| id >= entry.identity_digest.as_str()) {
                return Err(invalid(
                    "successor archive identities must be unique and sorted",
                ));
            }
            previous = Some(entry.identity_digest.as_str());
            if let Some(length) = objects.insert(&entry.object.digest, entry.object.bytes) {
                if length != entry.object.bytes {
                    return Err(invalid("successor archive digest has conflicting lengths"));
                }
            } else {
                bytes = bytes
                    .checked_add(entry.object.bytes)
                    .ok_or_else(|| invalid("successor manifest bytes overflow"))?;
            }
        }
        if bytes > self.usage.total_bytes
            || u64::try_from(objects.len()).map_err(|_| invalid("successor manifest count"))?
                > self.usage.objects
        {
            return Err(invalid("successor manifest exceeds cumulative usage"));
        }
        encode_bounded(self, BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1).map(|_| ())
    }
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        self.validate()?;
        digest_hex(BFT_SUCCESSOR_WINDOW_ARCHIVE_DOMAIN_V1, self)
    }
    pub fn encode_canonical(&self) -> Result<Vec<u8>, BftSignerErrorV3> {
        self.validate()?;
        encode_bounded(self, BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1)
    }
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BftSignerErrorV3> {
        let value: Self = decode_bounded(bytes, BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1)?;
        value.validate()?;
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowCutV1 {
    pub height: u64,
    pub header_id: String,
    pub certificate_digest: String,
    pub execution_commitment_digest: String,
    pub snapshot_ref: BftSuccessorWindowArchiveRefV1,
    pub journal_ref: BftSuccessorWindowArchiveRefV1,
    pub semantic_state_root: String,
    pub post_manifest_hash: String,
    pub business_result_root: String,
    pub driver_record_digest: String,
    pub driver_install_digest: String,
}
impl BftSuccessorWindowCutV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        if self.height == 0 || self.height.checked_add(1).is_none() {
            return Err(invalid("successor cut height"));
        }
        for value in [
            &self.header_id,
            &self.certificate_digest,
            &self.execution_commitment_digest,
            &self.semantic_state_root,
            &self.post_manifest_hash,
            &self.business_result_root,
            &self.driver_record_digest,
            &self.driver_install_digest,
        ] {
            hash(value)?;
        }
        self.snapshot_ref.validate()?;
        self.journal_ref.validate()?;
        Ok(())
    }
}

impl LocalBftSuccessorArchiveStoreV1 {
    /// Persists bounded canonical metadata and returns its raw CAS reference.
    pub fn persist_manifest(
        &self,
        manifest: &BftSuccessorWindowArchiveManifestV1,
    ) -> Result<BftSuccessorWindowArchiveRefV1, BftSignerErrorV3> {
        self.persist_object(
            &manifest.encode_canonical()?,
            BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1,
        )
    }

    /// Loads canonical metadata at the expected domain-separated root.
    /// Callers must still bind the returned material to their checkpoint.
    pub fn load_manifest(
        &self,
        reference: &BftSuccessorWindowArchiveRefV1,
        expected_root: &str,
    ) -> Result<BftSuccessorWindowArchiveManifestV1, BftSignerErrorV3> {
        hash(expected_root)?;
        let bytes = self.load_object(reference, BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1)?;
        let manifest = BftSuccessorWindowArchiveManifestV1::decode_canonical(&bytes)?;
        if manifest.digest()? != expected_root {
            return Err(invalid("successor archive manifest root differs"));
        }
        Ok(manifest)
    }
}
