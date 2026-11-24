//! Bounded inventory of original alternative admission descriptors.
//!
//! This is a **dark** validation contract for future protected
//! ReservedPending -> Ready -> Selection -> Seal. Shape validation never
//! claims that a descriptor has been fetched, paid for, signed or admitted.
//! A caller must independently verify every original byte and obtain a
//! protected selection receipt before making any current-window decision.
use serde::{Deserialize, Serialize};

use super::binding::{BftSignerErrorV3, invalid};
use super::private_retention_input::{
    BftPrivateRetentionAdmissionInputRefV1, BftPrivateRetentionSourceV1,
};
use super::state::validate_digest;

/// At most sixteen independently pinned alternatives may be considered in
/// one bounded original-source selection transaction.
const MAX_ORIGINAL_ALTERNATIVES: usize = 16;

/// Unwiring this inventory from the signer keeps original ReservedPending
/// operations untouched until the protected selection adapter is committed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftOriginalAlternativesInventoryV1 {
    pub schema_version: u32,
    pub source: BftPrivateRetentionSourceV1,
    pub ledger_origin_digest: String,
    pub height: u64,
    /// Canonically ordered by original header_id, never by arrival order.
    pub alternatives: Vec<BftPrivateRetentionAdmissionInputRefV1>,
}

impl BftOriginalAlternativesInventoryV1 {
    /// Check all descriptor identities and bounds without opening an archive
    /// or granting eligibility. A valid inventory is still not a Ready receipt.
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        if self.schema_version != 1
            || self.height == 0
            || self.alternatives.is_empty()
            || self.alternatives.len() > MAX_ORIGINAL_ALTERNATIVES
        {
            return Err(invalid("original alternatives schema, height or count"));
        }
        self.source.validate()?;
        validate_digest(&self.ledger_origin_digest)?;
        let mut previous: Option<&str> = None;
        for alternative in &self.alternatives {
            alternative.source.validate()?;
            alternative.object.validate()?;
            validate_digest(&alternative.key.ledger_origin_digest)?;
            validate_digest(&alternative.key.header_id)?;
            validate_digest(&alternative.admission_digest)?;
            if alternative.source != self.source
                || alternative.key.ledger_origin_digest != self.ledger_origin_digest
                || alternative.key.height != self.height
            {
                return Err(invalid("original alternative source or cut differs"));
            }
            // Strict ordering rejects repeated keys, including same-header
            // entries with different original payloads/digests.
            if previous.is_some_and(|old| old >= alternative.key.header_id.as_str()) {
                return Err(invalid("original alternatives duplicate or ambiguous order"));
            }
            previous = Some(&alternative.key.header_id);
        }
        Ok(())
    }

    /// A caller's independent H/source pin must match this complete inventory.
    /// Merely matching a name must never grant protected selection authority.
    pub fn check_source(
        &self,
        independent: &BftPrivateRetentionSourceV1,
        ledger_origin_digest: &str,
        height: u64,
    ) -> Result<(), BftSignerErrorV3> {
        self.validate()?;
        independent.validate()?;
        validate_digest(ledger_origin_digest)?;
        if &self.source != independent
            || self.ledger_origin_digest != ledger_origin_digest
            || self.height != height
        {
            return Err(invalid("original alternatives independent source mismatch"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protected_bft_v3::{
        BftPrivateRetentionKeyV1, BftSuccessorWindowArchiveRefV1,
    };

    fn h(n: u8) -> String {
        hex::encode([n; 32])
    }

    fn source() -> BftPrivateRetentionSourceV1 {
        BftPrivateRetentionSourceV1 {
            binding_digest: h(1),
            handoff_digest: h(2),
            material_source_digest: h(3),
        }
    }

    fn descriptor(header: u8) -> BftPrivateRetentionAdmissionInputRefV1 {
        BftPrivateRetentionAdmissionInputRefV1 {
            source: source(),
            key: BftPrivateRetentionKeyV1 {
                ledger_origin_digest: h(4),
                height: 11,
                header_id: h(header),
            },
            object: BftSuccessorWindowArchiveRefV1 {
                digest: h(header + 30),
                bytes: 128,
            },
            admission_digest: h(header + 60),
        }
    }

    fn inventory() -> BftOriginalAlternativesInventoryV1 {
        BftOriginalAlternativesInventoryV1 {
            schema_version: 1,
            source: source(),
            ledger_origin_digest: h(4),
            height: 11,
            alternatives: vec![descriptor(10), descriptor(11)],
        }
    }

    #[test]
    fn exact_original_inventory_accepts_only_independent_matching_cut() {
        let values = inventory();
        values.validate().unwrap();
        values.check_source(&source(), &h(4), 11).unwrap();
        assert!(values.check_source(&source(), &h(5), 11).is_err());
        assert!(values.check_source(&source(), &h(4), 12).is_err());
        let mut wrong_source = source();
        wrong_source.handoff_digest = h(99);
        assert!(values.check_source(&wrong_source, &h(4), 11).is_err());
    }

    #[test]
    fn rejects_duplicate_ambiguous_or_cross_source_originals() {
        let mut v = inventory();
        v.alternatives[1].key.header_id = v.alternatives[0].key.header_id.clone();
        assert!(v.validate().is_err());
        let mut v = inventory();
        v.alternatives.reverse();
        assert!(v.validate().is_err());
        let mut v = inventory();
        v.alternatives[1].source.handoff_digest = h(90);
        assert!(v.validate().is_err());
        let mut v = inventory();
        v.alternatives[1].key.height = 12;
        assert!(v.validate().is_err());
        let mut v = inventory();
        v.alternatives[1].admission_digest = "not-a-digest".into();
        assert!(v.validate().is_err());
    }

    #[test]
    fn rejects_empty_oversized_and_invalid_archive_descriptors() {
        let mut v = inventory();
        v.alternatives.clear();
        assert!(v.validate().is_err());
        let mut v = inventory();
        v.alternatives = (1..=17).map(descriptor).collect();
        assert!(v.validate().is_err());
        let mut v = inventory();
        v.alternatives[1].object.bytes = 0;
        assert!(v.validate().is_err());
        let mut v = inventory();
        v.schema_version = 2;
        assert!(v.validate().is_err());
    }

    #[test]
    fn canonical_roundtrip_keeps_exact_descriptors_without_promotion() {
        let original = inventory();
        let encoded = serde_cbor::to_vec(&original).unwrap();
        let decoded: BftOriginalAlternativesInventoryV1 =
            serde_cbor::from_slice(&encoded).unwrap();
        assert_eq!(decoded, original);
        decoded.validate().unwrap();
        // Deliberately no selection authority, signer lease, or Ready flag.
    }
}
