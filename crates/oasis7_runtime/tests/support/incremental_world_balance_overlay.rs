use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
        TransitionVerifier,
    },
    incremental_value::ValueNodeWriter,
};
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::path::Path;
fn main_balance(id: &str, [a, b, c]: [u64; 3]) -> WorldBalanceRefV1<'_> {
    WorldBalanceRefV1::MainToken {
        account_id: id,
        liquid_balance: a,
        vested_balance: b,
        restricted_starter_claim_balance: c,
    }
}
fn node_balance(id: &str, [a, b, c]: [u64; 3]) -> WorldBalanceRefV1<'_> {
    WorldBalanceRefV1::NodeAsset {
        node_id: id,
        power_credit_balance: a,
        total_minted_credits: b,
        total_burned_credits: c,
    }
}
fn expected_summary(balance: WorldBalanceRefV1<'_>) -> WorldBalanceReadV1 {
    match balance {
        WorldBalanceRefV1::MainToken {
            account_id,
            liquid_balance,
            vested_balance,
            restricted_starter_claim_balance,
        } => WorldBalanceReadV1::MainToken {
            identity_utf8_bytes: account_id.len() as u64,
            liquid_balance,
            vested_balance,
            restricted_starter_claim_balance,
        },
        WorldBalanceRefV1::NodeAsset {
            node_id,
            power_credit_balance,
            total_minted_credits,
            total_burned_credits,
        } => WorldBalanceReadV1::NodeAsset {
            identity_utf8_bytes: node_id.len() as u64,
            power_credit_balance,
            total_minted_credits,
            total_burned_credits,
        },
    }
}
fn expected_identity(balance: Option<WorldBalanceRefV1<'_>>) -> &str {
    match balance {
        Some(WorldBalanceRefV1::MainToken { account_id, .. }) => account_id,
        Some(WorldBalanceRefV1::NodeAsset { node_id, .. }) => node_id,
        None => "",
    }
}
struct CancelAfterPut<'a, W> {
    inner: &'a mut W,
    armed: &'a Cell<bool>,
    cancelled: &'a Cell<bool>,
    put: &'a Cell<bool>,
}
impl<W: ValueNodeWriter> ValueNodeWriter for CancelAfterPut<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, bytes)?;
        if self.armed.get() {
            self.put.set(true);
            self.cancelled.set(true);
        }
        Ok(())
    }
}
pub(super) fn exercise(path: &Path, compression: PageCompression, roots: &[Vec<u8>]) {
    use WorldBalanceFieldV1 as F;
    let original_main = main_balance(" 原\0é ", [0, u64::MAX, 17]);
    let original_empty = main_balance("", [u64::MAX, 0, 1]);
    let original_node = node_balance("账户", [u64::MAX, 17, 0]);
    let original_zero = node_balance("", [0, 0, 0]);
    let new_main = main_balance("owner-é", [7, 8, 9]);
    let final_main = main_balance("", [0, 0, 0]);
    let new_node = node_balance("fresh-node", [17, 18, 19]);
    let inserted_node = node_balance("inner-node\0", [u64::MAX, u64::MAX, 1]);
    let inserted_main = main_balance("inner-main", [u64::MAX, 7, 8]);
    let after = {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _requests = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        let store =
            IncrementalPageStore::open(path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let mut root_set = RootSetV1::empty("world-balances".into(), [3; 32]).unwrap();
        *root_set
            .state
            .iter_mut()
            .find(|r| r.namespace == Ns::Business)
            .unwrap() = StateRootV1::from_canonical_bytes(&roots[0]).unwrap();
        let parent = ExecutionParent {
            commit: [4; 32],
            roots: root_set,
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let limits = TransactionLimits {
            max_reads: 24,
            max_writes: 8,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        };
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut writer = store.value_session(physical());
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits).unwrap();
        {
            let mut read = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || {
                    state.take_read_error().map(|e| {
                        eprintln!("balance overlay state: {e:?}");
                        ValueTreeError::Invalid("state page failure")
                    })
                },
                cancelled: || false,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 160,
                    record_bytes: 8192,
                },
                cancelled: || false,
            };
            for (field, key, expected, after, changed) in [
                (
                    F::MainToken,
                    "main-map",
                    Some(original_main),
                    Some(new_main),
                    true,
                ),
                (F::MainToken, "main-map", Some(new_main), None, true),
                (F::MainToken, "main-map", None, Some(final_main), true),
                (
                    F::MainToken,
                    "empty-id",
                    Some(original_empty),
                    Some(original_empty),
                    false,
                ),
                (
                    F::NodeAsset,
                    "node-map",
                    Some(original_node),
                    Some(new_node),
                    true,
                ),
                (F::NodeAsset, "zero", Some(original_zero), None, true),
                (F::NodeAsset, "zero", None, Some(original_zero), true),
                (F::NodeAsset, "new", None, Some(inserted_node), true),
                (F::MainToken, "new", None, Some(inserted_main), true),
            ] {
                let guard = read.transaction.guard();
                assert_eq!(
                    change_world_balance_overlay_v1(
                        WorldBalanceChangeV1 {
                            field,
                            key,
                            expected,
                            after,
                            max_current_identity_bytes: 128
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live
                    )
                    .unwrap_or_else(|e| panic!("balance update {field:?}/{key}: {e:?}")),
                    changed
                );
                // The unique empty-id no-op must add its own dependency.
                if changed {
                    let mut output = Vec::with_capacity(128);
                    assert_eq!(
                        read_world_balance_overlay_v1(field, key, &mut read, 128, &mut output)
                            .unwrap(),
                        after.map(expected_summary)
                    );
                    assert_eq!(output, expected_identity(after).as_bytes());
                }
            }
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.reads.len(), 8);
        assert_eq!(sealed.record.writes.len(), 4);
        assert!(
            sealed
                .record
                .writes
                .iter()
                .all(|w| w.namespace == Ns::Business)
        );
        let verifier = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let verified = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &verifier,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        }
        .verify(&sealed.record)
        .unwrap();
        assert_eq!(verified.record.after, sealed.record.after);
        assert_eq!(
            parent
                .roots
                .state
                .iter()
                .find(|r| r.namespace == Ns::Business)
                .unwrap()
                .leaf_count,
            6
        );
        assert_eq!(
            sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == Ns::Business)
                .unwrap()
                .leaf_count,
            8
        );
        for node in &verified.new_nodes {
            let charge = memory
                .reserve(node.bytes.len(), MemoryClass::Ordinary)
                .unwrap();
            writer
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    charge.retain(node.bytes.clone()),
                )
                .unwrap();
        }
        let same_length_wrong_identity = " WRONG! ";
        assert_eq!(
            same_length_wrong_identity.len(),
            expected_identity(Some(original_main)).len()
        );
        assert_ne!(
            same_length_wrong_identity.as_bytes(),
            expected_identity(Some(original_main)).as_bytes()
        );
        for case in 0..26 {
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut case_parent = parent.clone();
            if (16..25).contains(&case) {
                *case_parent
                    .roots
                    .state
                    .iter_mut()
                    .find(|r| r.namespace == Ns::Business)
                    .unwrap() = StateRootV1::from_canonical_bytes(&roots[case - 15]).unwrap();
            }
            let case_live = case_parent.fence().unwrap();
            let alien = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let foreign = if case == 8 {
                Some(
                    ExecutionTransaction::begin(
                        case_parent.clone(),
                        [9; 32],
                        &state,
                        &memory,
                        limits,
                    )
                    .unwrap()
                    .guard(),
                )
            } else {
                None
            };
            let mut tx =
                ExecutionTransaction::begin(case_parent, [7; 32], &state, &memory, limits).unwrap();
            let cancelled = Cell::new(false);
            let armed = Cell::new(false);
            let put = Cell::new(false);
            let read_checks = Cell::new(0usize);
            let write_checks = Cell::new(0usize);
            let cancel_at = Cell::new(None::<usize>);
            let mut cancel_writer = CancelAfterPut {
                inner: &mut writer,
                armed: &armed,
                cancelled: &cancelled,
                put: &put,
            };
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut tx,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 8192,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 8192,
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("state page failure"))
                    },
                    cancelled: || {
                        read_checks.set(read_checks.get() + 1);
                        cancelled.get()
                    },
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut cancel_writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 8192,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 160,
                        record_bytes: 8192,
                    },
                    cancelled: || {
                        if let Some(at) = cancel_at.get() {
                            write_checks.set(write_checks.get() + 1);
                            write_checks.get() >= at
                        } else {
                            false
                        }
                    },
                };
                let stale = read.transaction.guard();
                assert!(
                    change_world_balance_overlay_v1(
                        WorldBalanceChangeV1 {
                            field: F::NodeAsset,
                            key: "node-map",
                            expected: Some(original_node),
                            after: Some(node_balance("prefix", [1, 2, 3])),
                            max_current_identity_bytes: 128
                        },
                        &mut read,
                        &mut write,
                        &stale,
                        &case_live
                    )
                    .unwrap()
                );
                if case == 25 {
                    let before = read_checks.get();
                    let mut output = io::sink();
                    assert_eq!(
                        read_world_balance_overlay_v1(
                            F::MainToken,
                            "main-map",
                            &mut read,
                            128,
                            &mut output
                        )
                        .unwrap(),
                        Some(expected_summary(original_main))
                    );
                    let count = read_checks.get() - before;
                    assert!(count > 0);
                    cancel_at.set(Some(count + 1));
                }
                let guard = if case == 8 {
                    foreign.unwrap()
                } else if case == 7 {
                    stale
                } else {
                    read.transaction.guard()
                };
                let expected = match case {
                    0 => None,
                    1 => Some(main_balance(same_length_wrong_identity, [0, u64::MAX, 17])),
                    2 => Some(main_balance(" 原\0é ", [1, u64::MAX, 17])),
                    3 => Some(main_balance(" 原", [0, u64::MAX, 17])),
                    4 => Some(main_balance(" 原\0é extra", [0, u64::MAX, 17])),
                    5 => Some(node_balance(" 原\0é ", [0, u64::MAX, 17])),
                    _ => Some(original_main),
                };
                let after = if case == 6 {
                    Some(node_balance("next", [7, 8, 9]))
                } else if case == 25 {
                    expected
                } else {
                    Some(new_main)
                };
                if case == 9 {
                    write.memory = &alien;
                }
                if case == 10 {
                    read.values.class = MemoryClass::Recovery;
                }
                if case == 12 {
                    write.limits.payload_bytes = 31;
                }
                if case == 13 {
                    read.values.io.nodes_remaining = 0;
                }
                if case == 14 {
                    cancelled.set(true);
                }
                if case == 15 {
                    armed.set(true);
                }
                let result = change_world_balance_overlay_v1(
                    WorldBalanceChangeV1 {
                        field: F::MainToken,
                        key: "main-map",
                        expected,
                        after,
                        max_current_identity_bytes: if case == 11 { 0 } else { 128 },
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &case_live,
                );
                assert!(result.is_err(), "case {case}");
                if case <= 4 {
                    assert!(matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(
                            "world balance expected value"
                        )))
                    ));
                }
                if case == 5 || case == 6 {
                    assert!(matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(
                            "world balance value type"
                        )))
                    ));
                }
                if case == 15 {
                    assert!(put.get());
                    assert!(matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                            | Err(WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                                ValueTreeError::Cancelled
                            )))
                    ));
                }
                if case == 25 {
                    assert_eq!(write_checks.get(), cancel_at.get().unwrap());
                    assert!(matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                    ));
                }
            }
            let guard = tx.guard();
            assert!(
                tx.seal(&guard, &case_live, 1).is_err(),
                "case {case} retained a sealable prefix"
            );
        }
        sealed.record.after.clone()
    };
    for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
        let memory = MemoryBudget::new(
            4 << 20,
            if class == MemoryClass::Recovery {
                3 << 20
            } else {
                1 << 20
            },
        )
        .unwrap();
        let store = IncrementalPageStore::open(path, memory.clone(), compression, class).unwrap();
        let _output = memory.reserve(8192, class).unwrap();
        let _filler = if class == MemoryClass::Recovery {
            Some(
                memory
                    .reserve((1 << 20) - memory.used(), MemoryClass::Ordinary)
                    .unwrap(),
            )
        } else {
            None
        };
        let reader = Reader {
            store: &store,
            memory: &memory,
            class,
            state_io: physical(),
        };
        let old_root = StateRootV1::from_canonical_bytes(&roots[0]).unwrap();
        let new_root = after
            .state
            .iter()
            .find(|r| r.namespace == Ns::Business)
            .unwrap();
        for (field, key, old, new) in [
            (
                F::MainToken,
                "main-map",
                Some(original_main),
                Some(final_main),
            ),
            (
                F::MainToken,
                "empty-id",
                Some(original_empty),
                Some(original_empty),
            ),
            (
                F::NodeAsset,
                "node-map",
                Some(original_node),
                Some(new_node),
            ),
            (
                F::NodeAsset,
                "zero",
                Some(original_zero),
                Some(original_zero),
            ),
            (F::NodeAsset, "new", None, Some(inserted_node)),
            (F::MainToken, "new", None, Some(inserted_main)),
        ] {
            for (root, expected) in [(&old_root, old), (new_root, new)] {
                let mut output = Vec::with_capacity(128);
                assert_eq!(
                    reader
                        .read(root, field, key, 128, &mut output, || false)
                        .unwrap(),
                    expected.map(expected_summary)
                );
                assert_eq!(output, expected_identity(expected).as_bytes());
            }
        }
    }
}
