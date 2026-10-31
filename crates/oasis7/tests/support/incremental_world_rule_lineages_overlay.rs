use super::*;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
    TransitionVerifier,
};
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
use std::path::Path;
fn txn_limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 24,
        max_writes: 16,
        max_facts: 4,
        max_pending_node_bytes: 4 << 20,
    }
}
fn make_parent(root: &StateRootV1) -> ExecutionParent {
    let mut roots = RootSetV1::empty("rule-lineages".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root.clone();
    ExecutionParent {
        commit: [4; 32],
        roots,
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    }
}
fn whole_io() -> PageIoBudget {
    // Eighteen dependency paths and eleven coalesced writes in one operation;
    // keep one finite session throughout, with the original byte allowance.
    PageIoBudget {
        reads_remaining: 300_000,
        ..physical()
    }
}
struct Writer<'a, W> {
    inner: &'a mut W,
    armed: &'a Cell<bool>,
    cancelled: &'a Cell<bool>,
    put: &'a Cell<bool>,
    fail: bool,
}
impl<W: ValueNodeWriter> ValueNodeWriter for Writer<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.armed.get() && self.fail {
            return Err(ValueTreeError::Invalid("injected lineage writer"));
        }
        self.inner.put_node(id, bytes)?;
        if self.armed.get() {
            self.put.set(true);
            self.cancelled.set(true);
        }
        Ok(())
    }
}
fn read_context<
    't,
    's,
    'v,
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
>(
    tx: &'t mut ExecutionTransaction<'s, M>,
    values: &'v R,
    memory: &'v MemoryBudget,
    io: &'v mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    keys: &'v mut SemanticKeyBudget,
    cancel: impl FnMut() -> bool,
) -> WorldFieldOverlayContextV1<
    't,
    's,
    'v,
    M,
    R,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldOverlayContextV1 {
        transaction: tx,
        values: ValueReadContext {
            source: values,
            memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work: keys,
        semantic_bytes: 1024,
        record_bytes: 8192,
        take_state_error: || None,
        cancelled: cancel,
    }
}
pub(super) fn exercise(path: &Path, compression: PageCompression, roots: &[Vec<u8>]) {
    use WorldRuleLineageExpectedV1 as E;
    use WorldRuleLineageValueV1 as V;
    let before = StateRootV1::from_canonical_bytes(&roots[0]).unwrap();
    let after = {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _requests = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
        let store =
            IncrementalPageStore::open(path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let parent = make_parent(&before);
        let live = parent.fence().unwrap();
        let state = store
            .state_session(whole_io(), MemoryClass::Ordinary)
            .unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, txn_limits())
                .unwrap();
        let values = store.value_session(physical());
        let mut writer = store.value_session(physical());
        let original = amount(i64::MIN, -7, i64::MAX);
        let swapped = amount(i64::MAX, -7, i64::MIN);
        let zero = amount(0, 0, 0);
        let keep = amount(1, 2, 3);
        let fresh = amount(9, -8, 7);
        {
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 32768,
            };
            let mut read = read_context(&mut tx, &values, &memory, &mut io, &mut keys, || false);
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 32768,
                },
                limits: limits(),
                cancelled: || false,
            };
            for (name, expected, after, changed) in [
                ("unique-noop", E::Value(&keep), Some(&keep), false),
                (" L\0é ", E::Value(&original), Some(&swapped), true),
                (" L\0é ", E::Value(&swapped), None, true),
                (" L\0é ", E::LineageAbsent, Some(&zero), true),
                ("zero", E::Value(&zero), None, true),
                ("new", E::LineageAbsent, Some(&fresh), true),
                ("new", E::Value(&fresh), Some(&fresh), false),
            ] {
                let guard = read.transaction.guard();
                assert_eq!(
                    change_world_rule_lineage_overlay_v1(
                        WorldRuleLineageChangeV1 {
                            key: name,
                            expected,
                            after
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live
                    )
                    .unwrap(),
                    changed
                );
                if name != "unique-noop" {
                    assert_eq!(
                        read_world_rule_lineage_overlay_v1(name, &mut read).unwrap(),
                        after.map_or(V::LineageAbsent, |v| V::Value(v.clone()))
                    );
                }
            }
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.reads.len(), 18);
        assert_eq!(sealed.record.writes.len(), 11);
        assert!(
            sealed
                .record
                .writes
                .iter()
                .all(|w| w.namespace == Ns::Authorization
                    && w.mutation.key != metadata().as_bytes()
                    && w.mutation.key != parent_key().as_bytes())
        );
        let verifier = store
            .state_session(whole_io(), MemoryClass::Ordinary)
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
        for case in 0..23 {
            let state = store
                .state_session(whole_io(), MemoryClass::Ordinary)
                .unwrap();
            let mut tx =
                ExecutionTransaction::begin(parent.clone(), [7; 32], &state, &memory, txn_limits())
                    .unwrap();
            let stale = tx.guard();
            let alien = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 32768,
            };
            let mut read = read_context(&mut tx, &values, &memory, &mut io, &mut keys, || false);
            let mut prefix = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 32768,
                },
                limits: limits(),
                cancelled: || false,
            };
            let guard = read.transaction.guard();
            change_world_rule_effect_overlay_v1(
                WorldRuleEffectChangeV1 {
                    key: "p",
                    expected: WorldRuleEffectExpectedV1::Value("original"),
                    after: Some("prefix"),
                    max_current_payload_bytes: 128,
                },
                &mut read,
                &mut prefix,
                &guard,
                &live,
            )
            .unwrap();
            drop(read);
            if (14..22).contains(&case) {
                let bad = StateRootV1::from_canonical_bytes(&roots[case - 11]).unwrap();
                let target = match case {
                    14 => metadata(),
                    15 | 18 => parent_key(),
                    16 | 19 => key(" L\0é ", None),
                    17 | 20 => key(" L\0é ", Some("target")),
                    _ => key(" L\0é ", Some("fulfilled")),
                };
                tx.write(
                    Ns::Authorization,
                    target.as_bytes(),
                    bad.get(target.as_bytes(), &state).unwrap(),
                )
                .unwrap();
            }
            let foreign =
                ExecutionTransaction::begin(parent.clone(), [8; 32], &state, &memory, txn_limits())
                    .unwrap()
                    .guard();
            let ticks = Cell::new(0);
            let no_op_limit = Cell::new(usize::MAX);
            if case == 11 {
                let mut io = logical();
                let mut keys = SemanticKeyBudget {
                    bytes_remaining: 32768,
                };
                let mut read = read_context(&mut tx, &values, &memory, &mut io, &mut keys, || {
                    ticks.set(ticks.get() + 1);
                    false
                });
                assert_eq!(
                    read_world_rule_lineage_overlay_v1(" L\0é ", &mut read).unwrap(),
                    V::Value(original.clone())
                );
                no_op_limit.set(ticks.get() + 1);
                ticks.set(0);
            }
            let armed = Cell::new(false);
            let cancelled = Cell::new(false);
            let put = Cell::new(false);
            let mut controlled = Writer {
                inner: &mut writer,
                armed: &armed,
                cancelled: &cancelled,
                put: &put,
                fail: case == 13,
            };
            let mut io = logical();
            if case == 12 {
                io.read_bytes_remaining = 0;
            }
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 32768,
            };
            let mut read = read_context(&mut tx, &values, &memory, &mut io, &mut keys, || {
                ticks.set(ticks.get() + 1);
                case == 9 || cancelled.get() || (case == 11 && ticks.get() >= no_op_limit.get())
            });
            if case == 5 {
                read.values.class = MemoryClass::Recovery;
            }
            let mut write_io = logical();
            if case == 8 {
                write_io.write_bytes_remaining = 0;
            }
            let mut bound = limits();
            if case == 6 {
                bound.payload_bytes = 0;
            }
            if case == 7 {
                bound.semantic_bytes = 0;
            }
            let mut write = MemberRecordWriteContext {
                sink: &mut controlled,
                memory: if case == 4 { &alien } else { &memory },
                io: &mut write_io,
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 32768,
                },
                limits: bound,
                cancelled: || false,
            };
            let guard = if case == 2 {
                stale
            } else if case == 3 {
                foreign
            } else {
                read.transaction.guard()
            };
            if case == 10 || case == 13 {
                armed.set(true);
            }
            let wrong = amount(i64::MIN, -6, i64::MAX);
            let result = change_world_rule_lineage_overlay_v1(
                WorldRuleLineageChangeV1 {
                    key: " L\0é ",
                    expected: match case {
                        0 => E::Value(&wrong),
                        1 => E::LineageAbsent,
                        22 => E::RulesAbsent,
                        _ => E::Value(&original),
                    },
                    after: Some(if case == 11 { &original } else { &swapped }),
                },
                &mut read,
                &mut write,
                &guard,
                &live,
            );
            assert!(result.is_err(), "case {case}");
            if case == 10 {
                assert!(put.get());
            }
            drop(read);
            let guard = tx.guard();
            assert!(tx.seal(&guard, &live, 1).is_err(), "case {case}");
        }
        let absent_parent = make_parent(&StateRootV1::from_canonical_bytes(&roots[1]).unwrap());
        let absent_live = absent_parent.fence().unwrap();
        for create in [false, true] {
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut tx = ExecutionTransaction::begin(
                absent_parent.clone(),
                [9; 32],
                &state,
                &memory,
                txn_limits(),
            )
            .unwrap();
            if create {
                tx.write(Ns::Business, b"successful-prefix", Some([9; 32]))
                    .unwrap();
            }
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 32768,
            };
            let mut read = read_context(&mut tx, &values, &memory, &mut io, &mut keys, || false);
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 32768,
                },
                limits: limits(),
                cancelled: || false,
            };
            let guard = read.transaction.guard();
            let result = change_world_rule_lineage_overlay_v1(
                WorldRuleLineageChangeV1 {
                    key: "new",
                    expected: E::RulesAbsent,
                    after: create.then_some(&zero),
                },
                &mut read,
                &mut write,
                &guard,
                &absent_live,
            );
            if create {
                assert!(result.is_err());
            } else {
                assert!(!result.unwrap());
            }
            drop(read);
            let guard = tx.guard();
            let result = tx.seal(&guard, &absent_live, 1);
            if create {
                assert!(result.is_err());
            } else {
                let sealed = result.unwrap();
                assert_eq!(sealed.record.reads.len(), 6);
                assert!(sealed.record.writes.is_empty());
            }
        }
        sealed
            .record
            .after
            .state
            .iter()
            .find(|r| r.namespace == Ns::Authorization)
            .unwrap()
            .canonical_bytes()
            .unwrap()
    };
    for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
        let memory = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
        let store = IncrementalPageStore::open(path, memory.clone(), compression, class).unwrap();
        let reader = Reader {
            store: &store,
            memory: &memory,
            class,
            state_io: physical(),
        };
        for (root, name, expected) in [
            (
                &roots[0],
                " L\0é ",
                V::Value(amount(i64::MIN, -7, i64::MAX)),
            ),
            (&after, " L\0é ", V::Value(amount(0, 0, 0))),
            (&after, "zero", V::LineageAbsent),
            (&after, "new", V::Value(amount(9, -8, 7))),
            (&after, "unique-noop", V::Value(amount(1, 2, 3))),
        ] {
            assert_eq!(
                reader
                    .read(
                        &StateRootV1::from_canonical_bytes(root).unwrap(),
                        name,
                        || false
                    )
                    .unwrap(),
                expected
            );
        }
    }
}
