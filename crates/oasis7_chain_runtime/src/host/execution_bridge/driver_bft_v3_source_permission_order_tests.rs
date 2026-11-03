//! A real authorized closure must precede this denied-reader fault case.
//! The denial double is not a substitute for the successful Noise read path.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputReadV1, BftPrivateRetentionAdmissionInputReaderV1,
    BftPrivateRetentionAdmissionInputRefV1, BftPrivateRetentionAdmissionInputV1,
    BftPrivateRetentionFactV1, BftPrivateRetentionInputObjectKindV1,
    BftPrivateRetentionOperationProofV1,
};
use std::cell::Cell;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }
pub(super) fn requested() -> bool {
    ENABLED.with(Cell::get)
}
#[test]
fn wire3_protected_archive_denies_before_loading_private_payloads() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            ENABLED.with(|enabled| enabled.set(true));
            install_case_with_retention(
                false,
                false,
                66,
                CandidateRetentionProfile::PublicationReserved,
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
const DENIED: &str = "fixture exact original-input read denied before payload";
#[derive(Default)]
pub(super) struct DenyReader {
    calls: AtomicUsize,
}
impl BftPrivateRetentionAdmissionInputReaderV1 for DenyReader {
    fn read_original(
        &self,
        _: &BftPrivateRetentionAdmissionInputRefV1,
        _: Instant,
    ) -> Result<BftPrivateRetentionAdmissionInputReadV1, BftSignerErrorV3> {
        self.calls.fetch_add(1, Ordering::Release);
        Err(BftSignerErrorV3(DENIED.into()))
    }
}
#[derive(serde::Deserialize)]
struct MetadataRefs {
    protected_retention: Option<BftSuccessorWindowArchiveRefV1>,
}
#[derive(serde::Deserialize)]
struct IndexRefs {
    h_nodes: Vec<BftSuccessorWindowArchiveRefV1>,
    operations: Vec<BftSuccessorWindowArchiveRefV1>,
}
pub(super) struct Check<'a> {
    pub archive: &'a LocalBftSuccessorArchiveStoreV1,
    pub private_root: &'a Path,
    pub metadata: &'a BftSuccessorWindowArchiveRefV1,
    pub public_state_ref: &'a BftSuccessorWindowArchiveRefV1,
    pub body: &'a BftSuccessorWindowCheckpointBodyV1,
    pub state: &'a BftSignerStateV3,
    pub verifier: &'a ActualBftSuccessorWindowDriverVerifierV1,
    pub reader: &'a DenyReader,
}
pub(super) fn exercise(check: Check<'_>) {
    let metadata: MetadataRefs = serde_cbor::from_slice(
        &check
            .archive
            .load_object(check.metadata, 1024 * 1024)
            .unwrap(),
    )
    .unwrap();
    let index: IndexRefs = serde_cbor::from_slice(
        &check
            .archive
            .load_object(
                &metadata
                    .protected_retention
                    .expect("real covered archive index"),
                1024 * 1024,
            )
            .unwrap(),
    )
    .unwrap();
    let h: BTreeSet<_> = index.h_nodes.iter().map(|r| r.digest.clone()).collect();
    let mut remove = BTreeSet::new();
    let mut originals = BTreeSet::new();
    let mut post_h = BTreeSet::new();
    for reference in index.operations {
        let proof: BftPrivateRetentionOperationProofV1 = serde_cbor::from_slice(
            &check
                .archive
                .load_object(
                    &reference,
                    oasis7_distfs::protected_bft_v3::BFT_SIGNER_MAX_FRAME_BYTES_V3,
                )
                .unwrap(),
        )
        .unwrap();
        for entry in proof.ledger.map.entries {
            let BftPrivateRetentionFactV1::Some {
                original_data_ref,
                metadata_ref,
                input_ref: Some(input_ref),
                ..
            } = entry.fact
            else {
                continue;
            };
            remove.insert(original_data_ref);
            remove.insert(metadata_ref);
            if !originals.insert(input_ref.digest.clone()) {
                continue;
            }
            let input = BftPrivateRetentionAdmissionInputV1::from_canonical_bytes(
                &check
                    .archive
                    .load_object(
                        &input_ref,
                        oasis7_distfs::protected_bft_v3::BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1,
                    )
                    .unwrap(),
            )
            .unwrap();
            remove.insert(input_ref.digest);
            for object in input.objects {
                if object.kind == BftPrivateRetentionInputObjectKindV1::Dependency
                    && !h.contains(&object.reference.digest)
                {
                    post_h.insert(object.reference.digest.clone());
                    remove.insert(object.reference.digest);
                }
            }
        }
    }
    assert!(
        !originals.is_empty(),
        "normal closure must contain real protected originals"
    );
    assert!(
        !post_h.is_empty(),
        "fixture must exercise new private dependencies beyond H"
    );
    assert!(
        remove.is_disjoint(&h),
        "the authorized H baseline must remain available"
    );
    for digest in &remove {
        let path = check
            .private_root
            .join("cas/blobs")
            .join(format!("{digest}.blob"));
        assert!(
            path.is_file(),
            "original protected archive payload must exist"
        );
        fs::remove_file(path).unwrap();
    }
    for reference in &index.h_nodes {
        check.archive.load_object(reference,
            super::super::super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES).unwrap();
    }
    let error = check
        .verifier
        .verify_closure(
            check.body,
            check.state,
            check.public_state_ref,
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap_err();
    assert!(
        error.contains(DENIED),
        "permission denial must precede missing payload: {error}"
    );
    assert!(check.reader.calls.load(Ordering::Acquire) > 0);
}
