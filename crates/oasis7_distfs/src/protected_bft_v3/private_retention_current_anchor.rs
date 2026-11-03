//! Bounded Current authentication. This is deliberately not archive evidence.
use super::binding::{BftSignerErrorV3, invalid};
use super::crypto::{encode_bounded, verify_signature};
use super::private_retention::{
    BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1, BftPrivateRetentionAttestorV1,
    BftPrivateRetentionEntryV1, BftPrivateRetentionHeadV1, BftPrivateRetentionLifecycleV1,
    BftPrivateRetentionMapV1,
};
use super::private_retention_backend::BftPrivateRetentionLedgerV1;
use super::private_retention_seal::{BftPrivateRetentionSealBodyV1, verify_map_at_floor};
use super::successor_window::BftSuccessorWindowCutV1;
use serde::{Deserialize, Serialize};

pub(crate) const SEAL_ANCHOR_DOMAIN: &str = "oasis7.bft.private-retention.seal-anchor.v1";
const MAX_ANCHOR: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSealAnchorBodyV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carry_selection_index: Option<super::BftPrivateRetentionSelectionIndexV1>,
    pub schema_version: u32,
    pub original_head: BftPrivateRetentionHeadV1,
    pub original_receipt_digest: String,
    pub original_coverage_start_height: u64,
    pub seal_body_digest: String,
    pub cut: BftSuccessorWindowCutV1,
    #[serde(deserialize_with = "deserialize_carry")]
    pub carry_entries: Vec<BftPrivateRetentionEntryV1>,
}

/// A provider-authenticated latest cut and at most sixteen exact T+1 facts.
/// It contains no Data/metadata/dependency payload and no archive authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSealAnchorV1 {
    pub body: BftPrivateRetentionSealAnchorBodyV1,
    pub signature_hex: String,
}
impl BftPrivateRetentionSealAnchorBodyV1 {
    pub(crate) fn from_original(
        body: &BftPrivateRetentionSealBodyV1,
    ) -> Result<Self, BftSignerErrorV3> {
        body.validate_shape()?;
        let anchor = Self {
            schema_version: body.schema_version,
            carry_selection_index: body.carry_selection_index()?,
            original_head: body.original.receipt.body.head.clone(),
            original_receipt_digest: super::crypto::digest_hex(
                "oasis7.bft.private-retention.v1.receipt",
                &body.original.receipt,
            )?,
            original_coverage_start_height: body.original.map.coverage_start_height,
            seal_body_digest: body.digest()?,
            cut: body.cut.clone(),
            carry_entries: body
                .original
                .map
                .entries
                .iter()
                .filter(|entry| entry.key.height > body.cut.height)
                .cloned()
                .collect(),
        };
        anchor.validate_shape()?;
        Ok(anchor)
    }
    fn validate_shape(&self) -> Result<(), BftSignerErrorV3> {
        self.original_head.validate()?;
        self.cut.validate()?;
        super::state::validate_digest(&self.original_receipt_digest)?;
        super::state::validate_digest(&self.seal_body_digest)?;
        let floor = self
            .cut
            .height
            .checked_add(1)
            .ok_or_else(|| invalid("retention anchor floor overflow"))?;
        if !matches!(
            (
                self.schema_version,
                self.carry_selection_index.is_some(),
                self.original_head.schema_version
            ),
            (1, false, 1 | 2) | (2, true, 3)
        ) || self.original_coverage_start_height == 0
            || self.cut.height < self.original_coverage_start_height
            || self.carry_entries.len() > BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1
            || self.carry_entries.iter().any(|entry| {
                entry.key.height != floor
                    || entry.lifecycle != BftPrivateRetentionLifecycleV1::Ready
            })
            || self.original_head.entry_count < self.carry_entries.len() as u32
        {
            return Err(invalid("retention latest anchor shape/carry differs"));
        }
        self.carry_map()?.validate()?;
        if let Some(index) = &self.carry_selection_index {
            index.validate(&self.carry_map()?)?;
        }
        encode_bounded(self, MAX_ANCHOR)?;
        Ok(())
    }
    fn carry_map(&self) -> Result<BftPrivateRetentionMapV1, BftSignerErrorV3> {
        Ok(BftPrivateRetentionMapV1 {
            schema_version: 1,
            source: self.original_head.source.clone(),
            coverage_start_height: self
                .cut
                .height
                .checked_add(1)
                .ok_or_else(|| invalid("retention anchor floor overflow"))?,
            previous_seal_receipt_digest: Some(self.seal_body_digest.clone()),
            entries: self.carry_entries.clone(),
        })
    }
}
impl BftPrivateRetentionSealAnchorV1 {
    pub fn verify(&self, trusted: &BftPrivateRetentionAttestorV1) -> Result<(), BftSignerErrorV3> {
        self.body.validate_shape()?;
        if self.body.original_head.source != trusted.source
            || self.body.original_head.capability_digest != trusted.digest()?
            || self.body.original_coverage_start_height < trusted.coverage_start_height
        {
            return Err(invalid("retention latest anchor fixed source/role differs"));
        }
        verify_signature(
            SEAL_ANCHOR_DOMAIN,
            &self.body,
            &self.signature_hex,
            &trusted.provider_public_key,
        )
    }
}

/// Authenticates only the current map floor. A fresh signed Current checkpoint
/// must separately bind this map head to the live lease. Archive validation must
/// still require every original receipt and complete seal, starting at H.
pub(crate) fn verify_current_anchor(
    trusted: &BftPrivateRetentionAttestorV1,
    current: &BftPrivateRetentionLedgerV1,
    latest: Option<&BftPrivateRetentionSealAnchorV1>,
) -> Result<(), BftSignerErrorV3> {
    super::private_retention_selection::validate_ledger(current)?;
    match latest {
        None => {
            current.receipt.verify_map(trusted, &current.map)?;
            if current.map.previous_seal_receipt_digest.is_some() {
                return Err(invalid(
                    "current retention floor lacks latest original seal",
                ));
            }
        }
        Some(anchor) => {
            anchor.verify(trusted)?;
            if let Some(original) = &anchor.body.carry_selection_index {
                let current_index = current
                    .selection_index
                    .as_ref()
                    .ok_or_else(|| invalid("current anchor selection inventory absent"))?;
                if current_index.last_sequence < original.last_sequence
                    || original
                        .selections
                        .iter()
                        .any(|s| !current_index.selections.contains(s))
                {
                    return Err(invalid("current anchor lost original choice"));
                }
            }
            let floor = anchor
                .body
                .cut
                .height
                .checked_add(1)
                .ok_or_else(|| invalid("current retention seal floor overflow"))?;
            verify_map_at_floor(trusted, current, floor)?;
            if current.map.previous_seal_receipt_digest.as_ref()
                != Some(&anchor.body.seal_body_digest)
                || current.receipt.body.head.sequence <= anchor.body.original_head.sequence
            {
                return Err(invalid("current retention latest seal/head differs"));
            }
            for original in &anchor.body.carry_entries {
                let actual = current
                    .map
                    .entries
                    .binary_search_by(|entry| entry.key.cmp(&original.key))
                    .ok()
                    .map(|index| &current.map.entries[index]);
                if actual != Some(original) {
                    return Err(invalid(
                        "current retention latest anchor omitted original carry",
                    ));
                }
            }
        }
    }
    Ok(())
}
fn deserialize_carry<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BftPrivateRetentionEntryV1>, D::Error> {
    struct Carry;
    impl<'de> serde::de::Visitor<'de> for Carry {
        type Value = Vec<BftPrivateRetentionEntryV1>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("at most sixteen original carried facts")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq
                .size_hint()
                .is_some_and(|n| n > BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1)
            {
                return Err(serde::de::Error::custom("retention anchor carry capacity"));
            }
            let mut entries = Vec::new();
            while let Some(entry) = seq.next_element()? {
                if entries.len() == BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1 {
                    return Err(serde::de::Error::custom("retention anchor carry capacity"));
                }
                entries.push(entry);
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(Carry)
}

#[cfg(test)]
#[path = "private_retention_current_anchor_tests.rs"]
mod tests;
