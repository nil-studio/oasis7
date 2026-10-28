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

#[test]
fn large_fact_streams_after_reopen_with_less_memory_than_original() {
    use oasis7_formal_protocol::{
        controlled_authority::formal::test_fixtures::{
            self as fixture, incremental::setup_with_memory,
        },
        incremental_history::HeaderPreparation,
        incremental_material::{FactBudget, verify_fact_originals},
    };
    use std::io::Read;
    let dir = Fixture::new();
    let s = setup_with_memory(MemoryBudget::new(8 << 20, 3 << 20).unwrap());
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let store =
        IncrementalPageStore::create(&dir.store(), s.memory.clone(), PageCompression::Raw).unwrap();
    let objects = IncrementalObjectStore::create(&store, &mut physical, &cancelled).unwrap();
    let length = (64 << 20) + 17;
    let id = objects
        .write_object(
            &mut std::io::repeat(0x5a).take(length),
            length,
            None,
            &mut ObjectIo {
                physical: &mut physical,
                logical: &mut logical,
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            },
        )
        .unwrap();
    let mut facts = s.history.facts.facts.clone();
    facts.push(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Result,
        id,
    });
    let history = HeaderPreparation {
        config: &s.config,
        catalog: &s.history.body.catalog_before,
        roots: &s.roots,
        parent: None,
        parent_history: None,
        transition: None,
        genesis_facts: &facts,
        operation: s.history.body.operation,
        authority: s.history.body.authority,
        execution_environment: s.config.execution_environment,
        checkpoint: true,
    }
    .prepare(&s.memory)
    .unwrap();
    let (blobs, catalog) = core_objects(&s, &history);
    let input_length = blobs
        .iter()
        .find(|bytes| blake3::hash(bytes).as_bytes() == &facts[0].id)
        .unwrap()
        .len() as u64;
    for bytes in blobs.iter() {
        objects
            .write_object(
                &mut bytes.as_slice(),
                bytes.len() as u64,
                Some(*blake3::hash(bytes).as_bytes()),
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
        &dir.store(),
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
    let mut record = s.commit.record().clone();
    record.artifact_roots.insert(
        oasis7_formal_protocol::controlled_authority::replicated_protocol::ArtifactRole::IncrementalCommitBody,
        hex::encode(history.body.digest().unwrap()),
    );
    record.artifact_roots.insert(
        oasis7_formal_protocol::controlled_authority::replicated_protocol::ArtifactRole::IncrementalHistoryPrefix,
        hex::encode(history.prefix.digest().unwrap()),
    );
    let signed = formal::SignedCanonicalCandidateV1 {
        writer_signature_hex: fixture::sign(
            formal::canonical_candidate_signing_bytes(&record).unwrap(),
            4,
        ),
        record,
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
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    let mut reader = objects.original_reader(&mut io);
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: &s.authority,
            parent: None,
            max_pending_node_bytes: 2 << 20,
        },
        &mut reader,
        &pages,
        &s.memory,
        &mut CoreBudget {
            objects_remaining: 32,
            original_bytes_remaining: 1 << 20,
            new_pages_remaining: 2048,
            merkle_reads_remaining: 10000,
        },
        || false,
    )
    .unwrap();
    // Whole-object ownership cannot fit, but streaming the same exact object can.
    assert!(
        reader
            .read_original(&id, length as usize, &s.memory)
            .is_err()
    );
    let mut quota = FactBudget {
        facts_remaining: 2,
        original_bytes_remaining: input_length + length,
        max_fact_bytes: length,
    };
    let result =
        verify_fact_originals(&core, &mut reader, &s.memory, &mut quota, || false).unwrap();
    assert_eq!(result.counts(), [1, 1, 0]);
    assert_eq!(result.original_bytes(), input_length + length);
    assert_eq!(result.body_id(), history.body.digest().unwrap());
    assert_eq!(
        (quota.facts_remaining, quota.original_bytes_remaining),
        (0, 0)
    );
}

#[path = "incremental_native_material_store_tests.rs"]
mod native;

#[path = "incremental_execution_artifact_store_tests.rs"]
mod artifacts;
