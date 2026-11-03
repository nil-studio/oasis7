//! Original-source retention facts. These DTOs do not grant private read or signing rights.
use super::binding::{BftSignerErrorV3, decode_key, invalid};
use super::crypto::{decode_bounded, digest_hex, encode_bounded, verify_signature};
use serde::{Deserialize, Serialize};

pub const BFT_PRIVATE_RETENTION_MAX_ENTRIES_V1: usize = 256;
pub const BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1: usize = 16;
pub const BFT_PRIVATE_RETENTION_MAX_MAP_BYTES_V1: usize = 1024 * 1024;
pub(crate) const RECEIPT_DOMAIN: &str = "oasis7.bft.private-retention.v1.head-receipt";
const MAP_DOMAIN: &str = "oasis7.bft.private-retention.v1.map";
const ROLE_DOMAIN: &str = "oasis7.bft.private-retention.v1.attestor";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSourceV1 {
    pub binding_digest: String,
    pub handoff_digest: String,
    pub material_source_digest: String,
}

/// Explicit external configuration. Deserializing a capability from an archive
/// does not establish that the verifier trusts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionAttestorV1 {
    pub schema_version: u32,
    pub provider_public_key: String,
    pub source: BftPrivateRetentionSourceV1,
    pub creation_marker_digest: String,
    pub coverage_start_height: u64,
    pub allow_authenticated_successor_origins: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionKeyV1 {
    pub ledger_origin_digest: String,
    pub height: u64,
    pub header_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum BftPrivateRetentionFactV1 {
    Some {
        original_data_ref: String,
        metadata_ref: String,
        admission_digest: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_ref: Option<super::successor_window::BftSuccessorWindowArchiveRefV1>,
    },
    ExplicitLegacyNone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BftPrivateRetentionLifecycleV1 {
    ReservedPending,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionEntryV1 {
    pub key: BftPrivateRetentionKeyV1,
    pub fact: BftPrivateRetentionFactV1,
    pub lifecycle: BftPrivateRetentionLifecycleV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionMapV1 {
    pub schema_version: u32,
    pub source: BftPrivateRetentionSourceV1,
    pub coverage_start_height: u64,
    /// Previous immutable seal, never a replacement genesis or a quota reset.
    pub previous_seal_receipt_digest: Option<String>,
    #[serde(deserialize_with = "bounded_entries")]
    pub entries: Vec<BftPrivateRetentionEntryV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionHeadV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_commitment:
        Option<super::private_retention_selection::BftPrivateRetentionSelectionCommitmentV1>,
    pub schema_version: u32,
    pub source: BftPrivateRetentionSourceV1,
    pub capability_digest: String,
    pub sequence: u64,
    pub map_digest: String,
    pub map_byte_hash: String,
    pub entry_count: u32,
    pub previous_receipt_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_material_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionReceiptBodyV1 {
    pub head: BftPrivateRetentionHeadV1,
    pub operation_digest: String,
    pub original_lease_generation: u64,
    pub before_revision: u64,
    pub before_state_digest: String,
    pub after_revision: u64,
    pub after_state_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionReceiptV1 {
    pub body: BftPrivateRetentionReceiptBodyV1,
    pub signature_hex: String,
}

impl BftPrivateRetentionSourceV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        for hash in [
            &self.binding_digest,
            &self.handoff_digest,
            &self.material_source_digest,
        ] {
            hash_value(hash)?;
        }
        Ok(())
    }
}

impl BftPrivateRetentionAttestorV1 {
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        if self.schema_version != 1 || self.coverage_start_height == 0 {
            return Err(invalid("private retention capability schema/coverage"));
        }
        self.source.validate()?;
        hash_value(&self.creation_marker_digest)?;
        decode_key(&self.provider_public_key)?;
        digest_hex(ROLE_DOMAIN, self)
    }
}

impl BftPrivateRetentionMapV1 {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, BftSignerErrorV3> {
        self.validate()?;
        encode_bounded(self, BFT_PRIVATE_RETENTION_MAX_MAP_BYTES_V1)
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, BftSignerErrorV3> {
        let map: Self = decode_bounded(bytes, BFT_PRIVATE_RETENTION_MAX_MAP_BYTES_V1)?;
        map.validate()?;
        Ok(map)
    }

    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        self.validate()?;
        digest_hex(MAP_DOMAIN, self)
    }

    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.source.validate()?;
        if self.schema_version != 1
            || self.coverage_start_height == 0
            || self.entries.len() > BFT_PRIVATE_RETENTION_MAX_ENTRIES_V1
        {
            return Err(invalid("private retention map schema/coverage/count"));
        }
        optional_hash(&self.previous_seal_receipt_digest)?;
        let mut counts = std::collections::BTreeMap::new();
        let mut previous = None;
        for entry in &self.entries {
            hash_value(&entry.key.ledger_origin_digest)?;
            hash_value(&entry.key.header_id)?;
            if entry.key.height < self.coverage_start_height
                || previous.is_some_and(|key| key >= &entry.key)
            {
                return Err(invalid("private retention map order/coverage"));
            }
            previous = Some(&entry.key);
            let count = counts
                .entry((&entry.key.ledger_origin_digest, entry.key.height))
                .or_insert(0);
            *count += 1;
            if *count > BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1 {
                return Err(invalid("private retention alternatives exceed bound"));
            }
            if let BftPrivateRetentionFactV1::Some {
                original_data_ref,
                metadata_ref,
                admission_digest,
                input_ref,
            } = &entry.fact
            {
                hash_value(original_data_ref)?;
                hash_value(metadata_ref)?;
                hash_value(admission_digest)?;
                if let Some(reference) = input_ref {
                    reference.validate()?;
                    if reference.bytes
                        > super::private_retention_input::BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1
                            as u64
                    {
                        return Err(invalid("retention input reference capacity"));
                    }
                }
            }
        }
        Ok(())
    }
}

impl BftPrivateRetentionReceiptV1 {
    /// `trusted` must come from external configuration, never from this receipt.
    /// Origin derivation, original admission and Ready publication are checked by
    /// the service/chain verifier; this only authenticates the exact map envelope.
    pub fn verify_map(
        &self,
        trusted: &BftPrivateRetentionAttestorV1,
        map: &BftPrivateRetentionMapV1,
    ) -> Result<(), BftSignerErrorV3> {
        let head = &self.body.head;
        head.validate()?;
        let bytes = map.canonical_bytes()?;
        if head.sequence == 0
            || head.source != trusted.source
            || map.source != trusted.source
            || map.coverage_start_height != trusted.coverage_start_height
            || head.capability_digest != trusted.digest()?
            || head.map_digest != map.digest()?
            || head.map_byte_hash != blake3::hash(&bytes).to_hex().as_str()
            || usize::try_from(head.entry_count).ok() != Some(map.entries.len())
            || self.body.after_revision
                != self
                    .body
                    .before_revision
                    .checked_add(1)
                    .ok_or_else(|| invalid("private retention revision overflow"))?
        {
            return Err(invalid("private retention receipt/map mismatch"));
        }
        optional_hash(&head.previous_receipt_digest)?;
        hash_value(&self.body.operation_digest)?;
        hash_value(&self.body.before_state_digest)?;
        hash_value(&self.body.after_state_digest)?;
        verify_signature(
            RECEIPT_DOMAIN,
            &self.body,
            &self.signature_hex,
            &trusted.provider_public_key,
        )
    }
}

impl BftPrivateRetentionHeadV1 {
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        self.validate()?;
        digest_hex(
            if self.schema_version == 3 {
                "oasis7.bft.private-retention.head.v3"
            } else {
                "oasis7.bft.private-retention.head.v1"
            },
            self,
        )
    }
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.source.validate()?;
        if !matches!(
            (self.schema_version, self.retained_material_bytes),
            (1, None) | (2 | 3, Some(_))
        ) || self.retained_material_bytes.is_some_and(|bytes| {
            bytes > super::successor_window::BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1
        }) || self.sequence == 0
            || self.entry_count as usize > BFT_PRIVATE_RETENTION_MAX_ENTRIES_V1
        {
            return Err(invalid("private retention head schema/sequence/count"));
        }
        for value in [
            &self.capability_digest,
            &self.map_digest,
            &self.map_byte_hash,
        ] {
            hash_value(value)?;
        }
        match (&self.selection_commitment, self.schema_version) {
            (None, 1 | 2) => {}
            (Some(choice), 3)
                if choice.count as usize <= BFT_PRIVATE_RETENTION_MAX_ENTRIES_V1
                    && choice.sequence > 0 =>
            {
                hash_value(&choice.root)?;
            }
            _ => {
                return Err(invalid(
                    "private retention selection commitment version differs",
                ));
            }
        }
        optional_hash(&self.previous_receipt_digest)
    }
}

pub(super) fn hash_value(value: &str) -> Result<(), BftSignerErrorV3> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "private retention hash must be canonical lowercase hex",
        ));
    }
    Ok(())
}

fn optional_hash(value: &Option<String>) -> Result<(), BftSignerErrorV3> {
    if let Some(value) = value {
        hash_value(value)?;
    }
    Ok(())
}

fn bounded_entries<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BftPrivateRetentionEntryV1>, D::Error> {
    struct Entries;
    impl<'de> serde::de::Visitor<'de> for Entries {
        type Value = Vec<BftPrivateRetentionEntryV1>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 256 retention entries")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            if sequence
                .size_hint()
                .is_some_and(|size| size > BFT_PRIVATE_RETENTION_MAX_ENTRIES_V1)
            {
                return Err(serde::de::Error::custom("retention entry bound"));
            }
            let mut entries = Vec::new();
            while let Some(entry) = sequence.next_element()? {
                if entries.len() == BFT_PRIVATE_RETENTION_MAX_ENTRIES_V1 {
                    return Err(serde::de::Error::custom("retention entry bound"));
                }
                entries.push(entry);
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(Entries)
}
