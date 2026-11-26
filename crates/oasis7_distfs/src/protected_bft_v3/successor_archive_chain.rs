//! Original successor archive chain reader. An authenticated tip/root must be
//! supplied externally; the returned data never grants protected signing,
//! read-contract-4 access, a minimum checkpoint, or a FullReadOnly install.
use super::binding::{BftSignerErrorV3, invalid};
use super::{
    BFT_SUCCESSOR_WINDOW_MAX_SEGMENTS_V1, BftSuccessorWindowArchiveKindV1,
    BftSuccessorWindowArchiveManifestV1, BftSuccessorWindowArchiveRefV1,
    BftSuccessorWindowArchiveUsageV1, BftSuccessorWindowSourceV1,
    LocalBftSuccessorArchiveStoreV1,
};

const MAX_ORIGINAL_OBJECT_BYTES: usize = 512 * 1024 * 1024;

impl LocalBftSuccessorArchiveStoreV1 {
    /// Freshly read and authenticate every original manifest and referenced
    /// CAS object in one bounded, caller-pinned chain. This is **data
    /// integrity only**: the caller must independently verify the last root
    /// against a qualified H / durable successor checkpoint and authorize
    /// access to each object. It cannot fetch from a live provider or restore
    /// a missing object by inventing another source.
    ///
    /// The caller passes each (CAS reference, domain-separated manifest root)
    /// in oldest-to-newest order; the final root is an independent local pin.
    /// Public and execution inventories are verified separately.
    pub fn load_exact_successor_archive_chain_v1(
        &self,
        source: &BftSuccessorWindowSourceV1,
        kind: BftSuccessorWindowArchiveKindV1,
        chain: &[(BftSuccessorWindowArchiveRefV1, String)],
        pinned_final_root: &str,
    ) -> Result<Vec<BftSuccessorWindowArchiveManifestV1>, BftSignerErrorV3> {
        source.validate().map_err(invalid)?;
        if chain.is_empty()
            || chain.len() > BFT_SUCCESSOR_WINDOW_MAX_SEGMENTS_V1 as usize
            || chain.last().is_none_or(|(_, root)| root != pinned_final_root)
        {
            return Err(invalid("successor archive original chain length or tip pin"));
        }
        let mut previous_root: Option<String> = None;
        let mut previous_usage = BftSuccessorWindowArchiveUsageV1::default();
        let mut previous_height = source.handoff_position;
        let mut manifests = Vec::with_capacity(chain.len());

        for (index, (reference, root)) in chain.iter().enumerate() {
            let manifest = self.load_manifest(reference, root)?;
            let expected_sequence = u64::try_from(index + 1)
                .map_err(|_| invalid("successor archive sequence overflow"))?;
            if manifest.source != *source
                || manifest.kind != kind
                || manifest.segment_sequence != expected_sequence
                || manifest.predecessor_root != previous_root
                || previous_height.checked_add(1) != Some(manifest.first_height)
            {
                return Err(invalid("successor archive source/segment/predecessor mismatch"));
            }
            manifest.usage.validate_after(&previous_usage)?;

            // A verified metadata root alone cannot authenticate the underlying
            // object: reopen the original immutable bytes and check their CAS
            // length/hash before accepting this segment. Only one object is
            // retained in memory at a time (never the whole chain's payload).
            for entry in &manifest.entries {
                let original = self.load_object(&entry.object, MAX_ORIGINAL_OBJECT_BYTES)?;
                entry.object.validate_bytes(&original)?;
            }
            previous_root = Some(root.clone());
            previous_height = manifest.last_height;
            previous_usage = manifest.usage;
            manifests.push(manifest);
        }
        Ok(manifests)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::BftSuccessorWindowArchiveEntryV1;
    use super::super::successor_archive_store::tests::Scratch;
    use oasis7_proto::distributed_bft::v2::BftScopeV2;

    fn source() -> BftSuccessorWindowSourceV1 {
        BftSuccessorWindowSourceV1 {
            scope: BftScopeV2 {
                world_id: "world".into(),
                chain_id: "chain".into(),
                genesis_hash: "01".repeat(32),
            },
            binding_digest: "02".repeat(32),
            handoff_position: 2,
            handoff_record_id: "03".repeat(32),
            handoff_snapshot_root: "04".repeat(32),
            target_digest: "05".repeat(32),
            activation_digest: "06".repeat(32),
            material_identity: "07".repeat(32),
        }
    }

    fn manifest(
        source: &BftSuccessorWindowSourceV1,
        kind: BftSuccessorWindowArchiveKindV1,
        sequence: u64,
        predecessor_root: Option<String>,
        object: BftSuccessorWindowArchiveRefV1,
        total_bytes: u64,
    ) -> BftSuccessorWindowArchiveManifestV1 {
        BftSuccessorWindowArchiveManifestV1 {
            public_index: None,
            schema_version: 1,
            source: source.clone(),
            kind,
            segment_sequence: sequence,
            predecessor_root,
            first_height: 2 + sequence,
            last_height: 2 + sequence,
            entries: vec![BftSuccessorWindowArchiveEntryV1 {
                identity_digest: format!("{:02x}", sequence + 8).repeat(32),
                object,
            }],
            usage: BftSuccessorWindowArchiveUsageV1 {
                segments: sequence,
                objects: sequence,
                total_bytes,
            },
        }
    }

    #[test]
    fn cacheless_original_manifest_chain_reopens_each_object_and_exact_tip() {
        let scratch = Scratch::new();
        let source = source();
        let kind = BftSuccessorWindowArchiveKindV1::Execution;
        let store = scratch.store();
        let a = store.persist_object(b"original-a", 64).unwrap();
        let b = store.persist_object(b"original-b", 64).unwrap();
        let first = manifest(&source, kind, 1, None, a, 10);
        let root_a = first.digest().unwrap();
        let ref_a = store.persist_manifest(&first).unwrap();
        let second = manifest(&source, kind, 2, Some(root_a.clone()), b, 20);
        let root_b = second.digest().unwrap();
        let ref_b = store.persist_manifest(&second).unwrap();
        let chain = vec![(ref_a, root_a), (ref_b, root_b.clone())];

        drop(store);
        let reopened = scratch.store();
        let verified = reopened
            .load_exact_successor_archive_chain_v1(&source, kind, &chain, &root_b)
            .unwrap();
        assert_eq!(verified, vec![first, second]);
        assert!(reopened
            .load_exact_successor_archive_chain_v1(&source, kind, &chain[..1], &root_b)
            .is_err());
        assert!(reopened
            .load_exact_successor_archive_chain_v1(&source, kind, &chain, &chain[0].1)
            .is_err());
        let mut foreign = source.clone();
        foreign.scope.chain_id = "other".into();
        assert!(reopened
            .load_exact_successor_archive_chain_v1(&foreign, kind, &chain, &root_b)
            .is_err());
        assert!(reopened
            .load_exact_successor_archive_chain_v1(
                &source, BftSuccessorWindowArchiveKindV1::Public, &chain, &root_b
            )
            .is_err());
    }

    #[test]
    fn original_chain_rejects_spliced_predecessor_and_height_gap() {
        let scratch = Scratch::new();
        let source = source();
        let kind = BftSuccessorWindowArchiveKindV1::Execution;
        let store = scratch.store();
        let a = store.persist_object(b"original-a", 64).unwrap();
        let b = store.persist_object(b"original-b", 64).unwrap();
        let first = manifest(&source, kind, 1, None, a, 10);
        let root_a = first.digest().unwrap();
        let ref_a = store.persist_manifest(&first).unwrap();
        let mut second = manifest(&source, kind, 2, Some("aa".repeat(32)), b, 20);
        let broken_root = second.digest().unwrap();
        let broken_ref = store.persist_manifest(&second).unwrap();
        assert!(store
            .load_exact_successor_archive_chain_v1(
                &source,
                kind,
                &[(ref_a.clone(), root_a.clone()), (broken_ref, broken_root.clone())],
                &broken_root,
            )
            .is_err());
        second.predecessor_root = Some(root_a.clone());
        second.first_height += 1;
        second.last_height += 1;
        let gap_root = second.digest().unwrap();
        let gap_ref = store.persist_manifest(&second).unwrap();
        assert!(store
            .load_exact_successor_archive_chain_v1(
                &source,
                kind,
                &[(ref_a, root_a), (gap_ref, gap_root.clone())],
                &gap_root,
            )
            .is_err());
    }

    #[test]
    fn original_chain_rejects_cas_corruption_even_with_valid_pinned_manifest() {
        let scratch = Scratch::new();
        let source = source();
        let store = scratch.store();
        let object = store.persist_object(b"original-c", 64).unwrap();
        let manifest = manifest(
            &source,
            BftSuccessorWindowArchiveKindV1::Public,
            1,
            None,
            object.clone(),
            object.bytes,
        );
        let root = manifest.digest().unwrap();
        let reference = store.persist_manifest(&manifest).unwrap();
        let bytes = store.load_object(&object, 64).unwrap();
        assert_eq!(bytes, b"original-c");
        drop(store);
        let original_path = scratch.blob(&object);
        std::fs::write(original_path, b"corrupted!").unwrap();
        assert!(scratch
            .store()
            .load_exact_successor_archive_chain_v1(
                &source,
                BftSuccessorWindowArchiveKindV1::Public,
                &[(reference, root.clone())],
                &root,
            )
            .is_err());
    }
}
