use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::test_fixtures::incremental::{genesis_config, setup_with_state},
    incremental_execution::RootSetV1,
    incremental_material::{WrittenValueBudget, verify_state_values, verify_written_values},
    incremental_state::{StateDeltaV1, StateScanBudget, StateWriteV1},
    incremental_value::{ValueNodeWriter, ValueReadContext, ValueTreeError, write_value},
};

#[test]
fn real_six_namespace_values_reopen_and_verify_with_only_recovery_headroom() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Fixture::new();
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let s = {
            let store =
                IncrementalPageStore::create(&dir.store(), memory.clone(), compression).unwrap();
            let mut values = store.value_session(physical_io());
            let value = write_value(
                &mut b"now".as_slice(),
                3,
                &mut values,
                &memory,
                &mut logical_io(),
                || false,
            )
            .unwrap();
            let scope = genesis_config([4; 32]).scope().unwrap();
            let mut roots = RootSetV1::empty(scope.world_id, scope.genesis_config).unwrap();
            let state = store
                .state_session(physical_io(), MemoryClass::Ordinary)
                .unwrap();
            for root in &mut roots.state {
                let charge = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
                let delta = charge.retain(
                    StateDeltaV1::build(
                        root.clone(),
                        vec![StateWriteV1 {
                            key: b"shared value".to_vec(),
                            before: None,
                            after: Some(value.digest().unwrap()),
                        }],
                        &state,
                        4 << 20,
                    )
                    .unwrap(),
                );
                // Persist this bounded fixture through the existing physical writer;
                // do not depend on an unmerged Runtime state-writer adapter.
                for node in &delta.new_nodes {
                    let charge = memory
                        .reserve(node.bytes.len(), MemoryClass::Ordinary)
                        .unwrap();
                    let id = *blake3::hash(&node.bytes).as_bytes();
                    assert_eq!(hex::encode(id), node.digest);
                    values
                        .put_node(id, charge.retain(node.bytes.clone()))
                        .unwrap();
                }
                *root = delta.after.clone();
            }
            let s = setup_with_state(memory.clone(), roots.state);
            let (blobs, catalog) = core_objects(&s, &s.history);
            let cancelled = AtomicBool::new(false);
            let mut physical = physical_io();
            let mut logical = logical_io();
            let objects =
                IncrementalObjectStore::create(&store, &mut physical, &cancelled).unwrap();
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
            s
        };
        let read_memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let store = IncrementalPageStore::open(
            &dir.store(),
            read_memory.clone(),
            compression,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut physical = physical_io();
        let mut logical = logical_io();
        let objects =
            IncrementalObjectStore::open(&store, MemoryClass::Ordinary, &mut physical, &cancelled)
                .unwrap();
        let state = store
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
        let core = verify_commit_core(
            &CoreRequest {
                candidate: &candidate,
                original_authority: &s.authority,
                parent: None,
                max_pending_node_bytes: 4 << 20,
            },
            &mut objects.original_reader(&mut ObjectIo {
                physical: &mut physical,
                logical: &mut logical,
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            }),
            &state,
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
        drop(state);
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let filler = if class == MemoryClass::Recovery {
                Some(
                    read_memory
                        .reserve((28 << 20) - read_memory.used(), MemoryClass::Ordinary)
                        .unwrap(),
                )
            } else {
                None
            };
            if class == MemoryClass::Recovery {
                assert!(read_memory.reserve(1, MemoryClass::Ordinary).is_err());
            }
            let state = store.state_session(physical_io(), class).unwrap();
            let values = store.value_session(physical_io());
            let mut io = logical_io();
            let mut context = ValueReadContext {
                source: &values,
                memory: &read_memory,
                class,
                io: &mut io,
            };
            let mut budget = WrittenValueBudget {
                values_remaining: 6,
                logical_bytes_remaining: 18,
                max_value_bytes: 3,
            };
            let mut scan = StateScanBudget {
                nodes_remaining: 10000,
                read_bytes_remaining: 1 << 24,
                leaves_remaining: 6,
            };
            let checked =
                verify_state_values(&core, &state, &mut context, &mut scan, &mut budget, || {
                    false
                })
                .unwrap();
            assert_eq!(checked.leaf_counts(), [1; 6]);
            assert_eq!(checked.logical_bytes(), 18);
            assert_eq!(checked.value_node_occurrences(), 12);
            assert_eq!(
                (
                    scan.leaves_remaining,
                    budget.values_remaining,
                    budget.logical_bytes_remaining
                ),
                (0, 0, 0)
            );
            assert!(checked.uses_budget(&read_memory));
            if class == MemoryClass::Ordinary {
                assert!(matches!(
                    verify_written_values(&core, &mut context, &mut budget, || false),
                    Err(ValueTreeError::Invalid("delta values require a transition"))
                ));
            }
            drop(checked);
            drop(filler);
        }
    }
}
