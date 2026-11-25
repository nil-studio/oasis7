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
use super::successor_archive_store::LocalBftSuccessorArchiveStoreV1;
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
            // Reuse the actual admission reader/store bound, not the wider
            // generic 512 MiB archive object reference allowance.
            alternative.validate()?;
            if alternative.source != self.source
                || alternative.key.ledger_origin_digest != self.ledger_origin_digest
                || alternative.key.height != self.height
            {
                return Err(invalid("original alternative source or cut differs"));
            }
            // Strict ordering rejects repeated keys, including same-header
            // entries with different original payloads/digests.
            if previous.is_some_and(|old| old >= alternative.key.header_id.as_str()) {
                return Err(invalid(
                    "original alternatives duplicate or ambiguous order",
                ));
            }
            previous = Some(&alternative.key.header_id);
        }
        Ok(())
    }

    /// Independently cold-read every original admission descriptor using the
    /// existing canonical, hash-checked CAS reader. Iterate one input at a time
    /// to avoid retaining all (up to 16) potentially large originals in memory.
    ///
    /// The result proves only exact source bytes under caller-supplied H pins.
    /// It does NOT represent protected ReservedPending/Ready, selection, Seal,
    /// custody, signing eligibility, payment or a production acceptance.
    pub fn cold_verify_originals(
        &self,
        store: &LocalBftSuccessorArchiveStoreV1,
        independent: &BftPrivateRetentionSourceV1,
        ledger_origin_digest: &str,
        height: u64,
    ) -> Result<BftColdVerifiedOriginalAlternativesV1, BftSignerErrorV3> {
        self.check_source(independent, ledger_origin_digest, height)?;
        for reference in &self.alternatives {
            let input = store.load_original_admission_input(reference)?;
            // Enforce exact source, cut, first charge and every original object
            // even if the store's current decoder changes implementation.
            reference.validate_input(&input)?;
        }
        Ok(BftColdVerifiedOriginalAlternativesV1 {
            source: self.source.clone(),
            ledger_origin_digest: self.ledger_origin_digest.clone(),
            height: self.height,
            descriptors: self.alternatives.clone(),
        })
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

/// Verified *historical bytes only* from a fresh all-original CAS read.
/// No method grants protected Ready, Selection, Seal or current authority.
#[derive(Debug, Clone)]
pub struct BftColdVerifiedOriginalAlternativesV1 {
    source: BftPrivateRetentionSourceV1,
    ledger_origin_digest: String,
    height: u64,
    descriptors: Vec<BftPrivateRetentionAdmissionInputRefV1>,
}

impl BftColdVerifiedOriginalAlternativesV1 {
    pub fn count(&self) -> usize {
        self.descriptors.len()
    }

    /// Exact checked descriptor, never a signer or lease capability.
    pub fn descriptor_for_header(
        &self,
        source: &BftPrivateRetentionSourceV1,
        ledger_origin_digest: &str,
        height: u64,
        header_id: &str,
    ) -> Option<&BftPrivateRetentionAdmissionInputRefV1> {
        if source != &self.source
            || ledger_origin_digest != self.ledger_origin_digest
            || height != self.height
        {
            return None;
        }
        self.descriptors
            .iter()
            .find(|entry| entry.key.header_id == header_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protected_bft_v3::{BftPrivateRetentionKeyV1, BftSuccessorWindowArchiveRefV1};

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
        // A 70 MiB generic archive ref is legal at the object layer, but
        // cannot be written or cold-read as an original admission input.
        v.alternatives[1].object.bytes = 70 * 1024 * 1024;
        assert!(v.validate().is_err());
        let mut v = inventory();
        v.schema_version = 2;
        assert!(v.validate().is_err());
    }


    fn genuine_original(header: u8) -> super::super::private_retention_input::BftPrivateRetentionAdmissionInputV1 {
        use super::super::private_retention_input::{
            BftPrivateRetentionAdmissionInputV1, BftPrivateRetentionInputObjectKindV1,
            BftPrivateRetentionInputObjectV1,
        };
        let make_object = |kind, bytes: &'static [u8]| BftPrivateRetentionInputObjectV1 {
            kind,
            reference: BftSuccessorWindowArchiveRefV1 {
                digest: blake3::hash(bytes).to_hex().to_string(),
                bytes: bytes.len() as u64,
            },
            bytes: bytes.to_vec(),
        };
        let mut objects = vec![
            make_object(BftPrivateRetentionInputObjectKindV1::OriginalData, b"genuine-body"),
            make_object(BftPrivateRetentionInputObjectKindV1::PublicationMetadata, b"original-metadata"),
        ];
        objects.sort_by(|a, b| a.reference.digest.cmp(&b.reference.digest));
        BftPrivateRetentionAdmissionInputV1 {
            schema_version: 1,
            source: source(),
            key: BftPrivateRetentionKeyV1 {
                ledger_origin_digest: h(4),
                height: 11,
                header_id: h(header),
            },
            publication_reservation_bytes: 123,
            objects,
        }
    }

    #[test]
    fn original_alternative_cold_reads_all_real_objects_without_ready_or_seal() {
        use super::super::successor_archive_store::tests::Scratch;
        let scratch = Scratch::new();
        let a = genuine_original(10);
        let b = genuine_original(11);
        let a_ref = a.descriptor().unwrap();
        let b_ref = b.descriptor().unwrap();
        let originals = BftOriginalAlternativesInventoryV1 {
            schema_version: 1,
            source: source(),
            ledger_origin_digest: h(4),
            height: 11,
            alternatives: vec![a_ref.clone(), b_ref.clone()],
        };
        let store = scratch.store();
        assert!(originals
            .cold_verify_originals(&store, &source(), &h(4), 11)
            .is_err()); // Missing archive is never silently accepted.
        store.persist_original_admission_input(&a_ref, &a).unwrap();
        store.persist_original_admission_input(&b_ref, &b).unwrap();
        drop(store);
        let cold = scratch.store();
        let checked = originals
            .cold_verify_originals(&cold, &source(), &h(4), 11)
            .unwrap();
        assert_eq!(checked.count(), 2);
        assert_eq!(
            checked.descriptor_for_header(&source(), &h(4), 11, &h(10)),
            Some(&a_ref)
        );
        assert_eq!(
            checked.descriptor_for_header(&source(), &h(4), 11, &h(11)),
            Some(&b_ref)
        );
        assert!(checked
            .descriptor_for_header(&source(), &h(4), 12, &h(10))
            .is_none());
        let mut different = source();
        different.binding_digest = h(100);
        assert!(checked
            .descriptor_for_header(&different, &h(4), 11, &h(10))
            .is_none());
        assert!(originals
            .cold_verify_originals(&cold, &different, &h(4), 11)
            .is_err());
    }

    #[test]
    fn original_alternative_cold_read_rejects_corruption_and_restores_exact_source() {
        use super::super::successor_archive_store::tests::Scratch;
        let scratch = Scratch::new();
        let input = genuine_original(10);
        let reference = input.descriptor().unwrap();
        let originals = BftOriginalAlternativesInventoryV1 {
            schema_version: 1,
            source: source(),
            ledger_origin_digest: h(4),
            height: 11,
            alternatives: vec![reference.clone()],
        };
        let store = scratch.store();
        store.persist_original_admission_input(&reference, &input).unwrap();
        drop(store);
        std::fs::write(scratch.blob(&reference.object), b"corrupted original").unwrap();
        let broken = scratch.store();
        assert!(originals
            .cold_verify_originals(&broken, &source(), &h(4), 11)
            .is_err());
        drop(broken);
        // The original CAS pin can be repaired from independently retained
        // authentic bytes. No new source, charge, Ready or signing permission.
        let repaired = scratch.store();
        assert!(repaired
            .persist_original_admission_input(&reference, &input)
            .is_err()); // Immutable corrupt path must never be silently overwritten.
        drop(repaired);
        // Simulate an explicit operator-backed replacement from exact retained
        // originals; this is not an automatic recovery or protected decision.
        std::fs::remove_file(scratch.blob(&reference.object)).unwrap();
        let repaired = scratch.store();
        repaired.persist_original_admission_input(&reference, &input).unwrap();
        drop(repaired);
        let cold = scratch.store();
        assert!(originals
            .cold_verify_originals(&cold, &source(), &h(4), 11)
            .is_ok());
    }

    #[test]
    fn canonical_roundtrip_keeps_exact_descriptors_without_promotion() {
        let original = inventory();
        let encoded = serde_cbor::to_vec(&original).unwrap();
        let decoded: BftOriginalAlternativesInventoryV1 = serde_cbor::from_slice(&encoded).unwrap();
        assert_eq!(decoded, original);
        decoded.validate().unwrap();
        // Deliberately no selection authority, signer lease, or Ready flag.
    }
}
