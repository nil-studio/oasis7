use super::*;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFactKindV1, ExecutionFactRefV1, PreparedTransition,
};
use oasis7_runtime::incremental_inventory_overlay_change::InventoryOverlayChangeError;
use oasis7_runtime::incremental_inventory_overlay_read::{
    InventoryOverlayReadContext, InventoryOverlayReadError,
};

fn persist(prepared: &PreparedTransition, store: &IncrementalPageStore, memory: &MemoryBudget) {
    let mut pages = store.value_session(physical());
    for node in &prepared.new_nodes {
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        pages
            .put_node(
                *blake3::hash(&node.bytes).as_bytes(),
                charge.retain(node.bytes.clone()),
            )
            .unwrap();
    }
}

#[test]
fn sequential_point_changes_coalesce_original_before_and_continue_after_cold_reopen() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _fixture = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
        let input = ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Input,
            id: [11; 32],
        };
        let result = ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Result,
            id: [12; 32],
        };
        let (original, next) = {
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let (_, original) = build(&source(), &store, &memory);
            let root = StateRootV1::from_canonical_bytes(&original).unwrap();
            let parent = parent(&root);
            let live = parent.fence().unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut transaction =
                ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits())
                    .unwrap();
            transaction.append_fact(input).unwrap();
            let old_guard = transaction.guard();
            let values = store.value_session(physical());
            let mut output = store.value_session(physical());
            let mut input_io = logical();
            let mut output_io = logical();
            let mut input_work = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut output_work = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let ledger = MaterialLedgerId::Site("new".into());
            {
                let mut read = InventoryOverlayReadContext {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut input_io,
                    },
                    key_work: &mut input_work,
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut output,
                    memory: &memory,
                    io: &mut output_io,
                    key_work: &mut output_work,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                };
                for (change, records, case, expected) in [
                    (
                        InventoryChangeRefV1::Resource {
                            key: ResourceKind::Data,
                            expected: Some(9),
                            after: Some(10),
                        },
                        1,
                        0,
                        LedgerQuantityV1::Value(10),
                    ),
                    (
                        InventoryChangeRefV1::Resource {
                            key: ResourceKind::Data,
                            expected: Some(10),
                            after: Some(13),
                        },
                        1,
                        0,
                        LedgerQuantityV1::Value(13),
                    ),
                    (
                        InventoryChangeRefV1::Material {
                            key: "",
                            expected: Some(0),
                            after: None,
                        },
                        1,
                        1,
                        LedgerQuantityV1::MaterialAbsent,
                    ),
                    (
                        InventoryChangeRefV1::Material {
                            key: "",
                            expected: None,
                            after: Some(0),
                        },
                        1,
                        1,
                        LedgerQuantityV1::Value(0),
                    ),
                    (
                        InventoryChangeRefV1::LedgerMaterial {
                            ledger: &ledger,
                            material: "ore",
                            expected: LedgerQuantityV1::LedgerAbsent,
                            after: Some(5),
                            create_ledger: true,
                        },
                        2,
                        2,
                        LedgerQuantityV1::Value(5),
                    ),
                    (
                        InventoryChangeRefV1::LedgerMaterial {
                            ledger: &ledger,
                            material: "ore",
                            expected: LedgerQuantityV1::Value(5),
                            after: Some(7),
                            create_ledger: false,
                        },
                        1,
                        2,
                        LedgerQuantityV1::Value(7),
                    ),
                    (
                        InventoryChangeRefV1::LedgerMaterial {
                            ledger: &ledger,
                            material: "ore",
                            expected: LedgerQuantityV1::Value(7),
                            after: None,
                            create_ledger: false,
                        },
                        1,
                        2,
                        LedgerQuantityV1::MaterialAbsent,
                    ),
                    (
                        InventoryChangeRefV1::LedgerMaterial {
                            ledger: &ledger,
                            material: "ore",
                            expected: LedgerQuantityV1::MaterialAbsent,
                            after: Some(0),
                            create_ledger: false,
                        },
                        1,
                        2,
                        LedgerQuantityV1::Value(0),
                    ),
                    (
                        InventoryChangeRefV1::LedgerMaterial {
                            ledger: &ledger,
                            material: "ore",
                            expected: LedgerQuantityV1::Value(0),
                            after: Some(0),
                            create_ledger: false,
                        },
                        0,
                        2,
                        LedgerQuantityV1::Value(0),
                    ),
                ] {
                    let guard = read.transaction.guard();
                    assert_eq!(
                        read.change(change, &mut write, &guard, &live).unwrap(),
                        records
                    );
                    let actual = match case {
                        0 => read
                            .resource(ResourceKind::Data)
                            .unwrap()
                            .map_or(LedgerQuantityV1::MaterialAbsent, LedgerQuantityV1::Value),
                        1 => read
                            .material("")
                            .unwrap()
                            .map_or(LedgerQuantityV1::MaterialAbsent, LedgerQuantityV1::Value),
                        _ => read.ledger(&ledger, "ore").unwrap(),
                    };
                    assert_eq!(actual, expected);
                }
                assert!(matches!(
                    read.transaction.validate_guard(&old_guard, &live),
                    Err(ExecutionError::StaleGuard)
                ));
            }
            assert!(input_work.bytes_remaining < 1 << 20);
            assert!(output_work.bytes_remaining < 1 << 20);
            assert!(input_io.nodes_remaining < 100_000);
            assert!(output_io.nodes_remaining < 100_000);
            transaction.append_fact(result).unwrap();
            let guard = transaction.guard();
            let sealed = transaction.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.reads.len(), 7);
            assert_eq!(sealed.record.writes.len(), 3);
            assert_eq!(sealed.record.facts, vec![input, result]);
            let verification = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let verified = TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &verification,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            }
            .verify(&sealed.record)
            .unwrap();
            assert_eq!(verified.record.after, sealed.record.after);
            persist(&verified, &store, &memory);
            (original, verified.record.after.canonical_bytes().unwrap())
        };
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let original = StateRootV1::from_canonical_bytes(&original).unwrap();
        let recovered = RootSetV1::from_canonical_bytes(&next).unwrap();
        assert_eq!(recovered.height, 1);
        assert_eq!(
            quantity(&original, &store, &memory, 0),
            LedgerQuantityV1::Value(9)
        );
        assert_eq!(
            quantity(&original, &store, &memory, 3),
            LedgerQuantityV1::LedgerAbsent
        );
        assert_eq!(
            quantity(&recovered.state[0], &store, &memory, 0),
            LedgerQuantityV1::Value(13)
        );
        assert_eq!(
            quantity(&recovered.state[0], &store, &memory, 1),
            LedgerQuantityV1::Value(0)
        );
        assert_eq!(
            quantity(&recovered.state[0], &store, &memory, 3),
            LedgerQuantityV1::Value(0)
        );
        let parent = ExecutionParent {
            commit: [31; 32],
            roots: recovered.clone(),
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut transaction =
            ExecutionTransaction::begin(parent, [32; 32], &state, &memory, limits()).unwrap();
        let values = store.value_session(physical());
        let mut output = store.value_session(physical());
        let mut input_io = logical();
        let mut output_io = logical();
        let mut input_work = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut output_work = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        {
            let mut read = InventoryOverlayReadContext {
                transaction: &mut transaction,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut input_io,
                },
                key_work: &mut input_work,
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || state.take_read_error().map(typed),
                cancelled: || false,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut output,
                memory: &memory,
                io: &mut output_io,
                key_work: &mut output_work,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 8,
                    record_bytes: 4096,
                },
                cancelled: || false,
            };
            let guard = read.transaction.guard();
            read.change(
                InventoryChangeRefV1::Resource {
                    key: ResourceKind::Data,
                    expected: Some(13),
                    after: Some(14),
                },
                &mut write,
                &guard,
                &live,
            )
            .unwrap();
        }
        let guard = transaction.guard();
        let sealed = transaction.seal(&guard, &live, 2).unwrap();
        assert_eq!(sealed.record.before.height, 1);
        assert_eq!(sealed.record.after.height, 2);
        assert_eq!(sealed.record.writes.len(), 1);
        persist(&sealed, &store, &memory);
        assert_eq!(
            quantity(&sealed.record.after.state[0], &store, &memory, 0),
            LedgerQuantityV1::Value(14)
        );
        assert_eq!(
            quantity(&recovered.state[0], &store, &memory, 0),
            LedgerQuantityV1::Value(13)
        );
    }
}

#[path = "incremental_inventory_overlay_change_failures.rs"]
mod failures;
