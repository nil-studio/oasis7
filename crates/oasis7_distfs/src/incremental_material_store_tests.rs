//! Actual object/page source for the original-protocol structural core check.
//! No native execution closure, receipt or service readiness is asserted here.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        self,
        test_fixtures::incremental::{core_objects, setup},
    },
    incremental_material::{CoreBudget, CoreRequest, OriginalObjectReader, verify_commit_core},
};

#[test]
fn original_protocol_core_is_loaded_from_real_objects_and_pages_after_reopen() {
    let fixture_dir = Fixture::new();
    let s = setup();
    let (blobs, catalog) = core_objects(&s, &s.history);
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let store =
        IncrementalPageStore::create(&fixture_dir.store(), s.memory.clone(), PageCompression::Raw)
            .unwrap();
    let objects = IncrementalObjectStore::create(&store, &mut physical, &cancelled).unwrap();
    for bytes in blobs.iter() {
        let id = *blake3::hash(bytes).as_bytes();
        objects
            .write_object(
                &mut bytes.as_slice(),
                bytes.len() as u64,
                Some(id),
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical,
                    class: MemoryClass::Ordinary,
                    cancelled: &cancelled,
                },
            )
            .unwrap();
    }
    let mut pages = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    pages.persist_catalog_pages(&catalog).unwrap();
    drop(pages);
    drop(objects);
    drop(store);

    let store = IncrementalPageStore::open(
        &fixture_dir.store(),
        s.memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let objects =
        IncrementalObjectStore::open(&store, MemoryClass::Ordinary, &mut physical, &cancelled)
            .unwrap();
    let pages = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    let signed = formal::SignedCanonicalCandidateV1 {
        record: s.commit.record().clone(),
        writer_signature_hex: s.proof.decision.writer_signature_hex.clone(),
    };
    let candidate = formal::verify_formal_candidate(
        &signed,
        None,
        &s.authority,
        None,
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let request = CoreRequest {
        candidate: &candidate,
        original_authority: &s.authority,
        parent: None,
        max_pending_node_bytes: 4 << 20,
    };
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    let mut reader = objects.original_reader(&mut io);
    let checked = verify_commit_core(
        &request,
        &mut reader,
        &pages,
        &s.memory,
        &mut CoreBudget {
            objects_remaining: 32,
            original_bytes_remaining: 1024 * 1024,
            new_pages_remaining: 2048,
            merkle_reads_remaining: 10000,
        },
        || false,
    )
    .unwrap();
    assert_eq!(checked.body(), &s.history.body);
    assert_eq!(checked.roots(), &s.roots);
    assert_eq!(checked.catalog().after, catalog.after);
    let id = *blake3::hash(&s.config.canonical_bytes().unwrap()).as_bytes();
    let bytes = reader.read_original(&id, 4096, &s.memory).unwrap();
    assert!(bytes.uses_budget(&s.memory));
    assert!(bytes.reserved_bytes() >= bytes.capacity());
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Recovery,
        cancelled: &cancelled,
    };
    assert!(
        objects
            .original_reader(&mut io)
            .read_original(&id, 4096, &s.memory)
            .is_err()
    );
}
