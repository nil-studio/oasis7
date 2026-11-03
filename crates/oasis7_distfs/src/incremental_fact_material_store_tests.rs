use super::*;
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
    for (compression, limit, reserve) in [
        (PageCompression::Raw, 8 << 20, 3 << 20),
        (PageCompression::Zstd, 32 << 20, 4 << 20),
    ] {
        let dir = Fixture::new();
        let s = setup_with_memory(MemoryBudget::new(limit, reserve).unwrap());
        let cancelled = AtomicBool::new(false);
        let mut physical = physical_io();
        let mut logical = logical_io();
        let store =
            IncrementalPageStore::create(&dir.store(), s.memory.clone(), compression).unwrap();
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
        let read_memory = MemoryBudget::new(limit, reserve).unwrap();
        let store = IncrementalPageStore::open(
            &dir.store(),
            read_memory.clone(),
            compression,
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
            &read_memory,
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
        assert!(matches!(
            reader.read_original(&id, length as usize, &read_memory),
            Err(PageStoreError::Memory(_))
        ));
        let mut quota = FactBudget {
            facts_remaining: 2,
            original_bytes_remaining: input_length + length,
            max_fact_bytes: length,
        };
        let result =
            verify_fact_originals(&core, &mut reader, &read_memory, &mut quota, || false).unwrap();
        assert_eq!(result.counts(), [1, 1, 0]);
        assert_eq!(result.original_bytes(), input_length + length);
        assert_eq!(result.body_id(), history.body.digest().unwrap());
        assert_eq!(
            (quota.facts_remaining, quota.original_bytes_remaining),
            (0, 0)
        );
    }
}
