use super::*;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
    TransitionVerifier,
};
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
use std::path::Path;
struct ControlledWriter<'a, W> {
    inner: &'a mut W,
    armed: &'a Cell<bool>,
    cancelled: &'a Cell<bool>,
    put: &'a Cell<bool>,
    fail: bool,
}
impl<W: ValueNodeWriter> ValueNodeWriter for ControlledWriter<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.fail && self.armed.get() {
            return Err(ValueTreeError::Invalid("injected rule writer"));
        }
        self.inner.put_node(id, bytes)?;
        if self.armed.get() {
            self.put.set(true);
            self.cancelled.set(true);
        }
        Ok(())
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 24,
        max_writes: 8,
        max_facts: 4,
        max_pending_node_bytes: 4 << 20,
    }
}
fn make_parent(root: &StateRootV1) -> ExecutionParent {
    let mut roots = RootSetV1::empty("rule-effects".into(), [3; 32]).unwrap();
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
fn read_context<
    't,
    's,
    'v,
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: ValueNodeReader,
>(
    transaction: &'t mut ExecutionTransaction<'s, M>,
    values: &'v R,
    memory: &'v MemoryBudget,
    io: &'v mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    key_work: &'v mut SemanticKeyBudget,
    cancelled: impl FnMut() -> bool,
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
        transaction,
        values: ValueReadContext {
            source: values,
            memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work,
        semantic_bytes: 1024,
        record_bytes: 8192,
        take_state_error: || None,
        cancelled,
    }
}
pub(super) fn exercise(path: &Path, compression: PageCompression, roots: &[Vec<u8>]) {
    use WorldRuleEffectExpectedV1 as E;
    use WorldRuleEffectValueV1 as V;
    let good = StateRootV1::from_canonical_bytes(&roots[0]).unwrap();
    let after = {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _requests = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
        let store =
            IncrementalPageStore::open(path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let parent = make_parent(&good);
        let live = parent.fence().unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits())
                .unwrap();
        let values = store.value_session(physical());
        let mut writer = store.value_session(physical());
        {
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 8192,
            };
            let mut read = read_context(&mut tx, &values, &memory, &mut io, &mut keys, || false);
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                limits: super::limits(),
                cancelled: || false,
            };
            for (name, expected, after, changed) in [
                ("unique-noop", E::Value("keep"), Some("keep"), false),
                (" P\0é ", E::Value(" R\0甲 "), Some("next"), true),
                (" P\0é ", E::Value("next"), None, true),
                (" P\0é ", E::EffectAbsent, Some(""), true),
                (" P\0é ", E::Value(""), Some("new"), true),
                ("", E::Value(""), None, true),
                ("new", E::EffectAbsent, Some("fresh"), true),
                ("new", E::Value("fresh"), Some("fresh"), false),
            ] {
                let guard = read.transaction.guard();
                assert_eq!(
                    change_world_rule_effect_overlay_v1(
                        WorldRuleEffectChangeV1 {
                            key: name,
                            expected,
                            after,
                            max_current_payload_bytes: 128
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live
                    )
                    .unwrap(),
                    changed
                );
                // Do not follow the unique no-op with another public read: its
                // dependency must come from the setter itself.
                if name != "unique-noop" {
                    let mut output = Vec::with_capacity(128);
                    assert_eq!(
                        read_world_rule_effect_overlay_v1(name, &mut read, 128, &mut output)
                            .unwrap(),
                        after.map_or(V::EffectAbsent, |s| V::Value(s.len() as u64))
                    );
                    assert_eq!(output, after.unwrap_or("").as_bytes());
                }
            }
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.reads.len(), 6);
        assert_eq!(sealed.record.writes.len(), 3);
        assert!(
            sealed
                .record
                .writes
                .iter()
                .all(|w| w.namespace == Ns::Authorization
                    && w.mutation.key != metadata().as_bytes()
                    && w.mutation.key != key(None).as_bytes())
        );
        // Verification is a separate bounded operation, not a budget reset
        // inside the transaction that just sealed.
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
        // Each second failure follows an actual successful typed effect write.
        for case in 0..23 {
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut tx =
                ExecutionTransaction::begin(parent.clone(), [7; 32], &state, &memory, limits())
                    .unwrap();
            let stale = tx.guard();
            let alien = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let mut inner = store.value_session(physical());
            let armed = Cell::new(false);
            let cancelled = Cell::new(false);
            let put = Cell::new(false);
            let mut writer = ControlledWriter {
                inner: &mut inner,
                armed: &armed,
                cancelled: &cancelled,
                put: &put,
                fail: case == 15,
            };
            let mut quota = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 8192,
            };
            let mut read = read_context(&mut tx, &values, &memory, &mut quota, &mut keys, || false);
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                limits: super::limits(),
                cancelled: || false,
            };
            let guard = read.transaction.guard();
            change_world_rule_effect_overlay_v1(
                WorldRuleEffectChangeV1 {
                    key: " P\0é ",
                    expected: E::Value(" R\0甲 "),
                    after: Some("prefix"),
                    max_current_payload_bytes: 128,
                },
                &mut read,
                &mut write,
                &guard,
                &live,
            )
            .unwrap();
            drop(read);
            if case >= 16 {
                let bad = StateRootV1::from_canonical_bytes(&roots[case - 13]).unwrap();
                let target = if case == 16 {
                    metadata()
                } else if case <= 20 {
                    key(None)
                } else {
                    key(Some(" P\0é "))
                };
                let id = bad.get(target.as_bytes(), &state).unwrap();
                tx.write(Ns::Authorization, target.as_bytes(), id).unwrap();
            }
            let foreign =
                ExecutionTransaction::begin(parent.clone(), [8; 32], &state, &memory, limits())
                    .unwrap()
                    .guard();
            let ticks = Cell::new(0usize);
            let no_op_limit = Cell::new(usize::MAX);
            if case == 12 {
                let mut io = logical();
                let mut keys = SemanticKeyBudget {
                    bytes_remaining: 8192,
                };
                let mut read = read_context(&mut tx, &values, &memory, &mut io, &mut keys, || {
                    ticks.set(ticks.get() + 1);
                    false
                });
                assert_eq!(
                    read_world_rule_effect_overlay_v1(" P\0é ", &mut read, 128, &mut io::sink())
                        .unwrap(),
                    V::Value(6)
                );
                no_op_limit.set(ticks.get() + 1);
                ticks.set(0);
            }
            let mut io = logical();
            if case == 13 {
                io.read_bytes_remaining = 0;
            }
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 8192,
            };
            let mut read = read_context(&mut tx, &values, &memory, &mut io, &mut keys, || {
                ticks.set(ticks.get() + 1);
                case == 10 || cancelled.get() || (case == 12 && ticks.get() >= no_op_limit.get())
            });
            if case == 6 {
                read.values.class = MemoryClass::Recovery;
            }
            let mut write_io = logical();
            if case == 9 {
                write_io.write_bytes_remaining = 0;
            }
            let mut bound = super::limits();
            if case == 8 {
                bound.payload_bytes = 0;
            }
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: if case == 5 { &alien } else { &memory },
                io: &mut write_io,
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                limits: bound,
                cancelled: || false,
            };
            let guard = if case == 3 {
                stale
            } else if case == 4 {
                foreign
            } else {
                read.transaction.guard()
            };
            if case == 11 || case == 15 {
                armed.set(true);
            }
            let result = if case == 14 {
                struct Fail;
                impl Write for Fail {
                    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                        Err(io::Error::other("injected rule output"))
                    }
                    fn flush(&mut self) -> io::Result<()> {
                        Ok(())
                    }
                }
                read_world_rule_effect_overlay_v1(" P\0é ", &mut read, 128, &mut Fail).map(|_| true)
            } else {
                change_world_rule_effect_overlay_v1(
                    WorldRuleEffectChangeV1 {
                        key: " P\0é ",
                        expected: match case {
                            0 => E::Value(" wrong"),
                            1 => E::RulesAbsent,
                            2 => E::EffectAbsent,
                            _ => E::Value("prefix"),
                        },
                        after: Some(if case == 12 { "prefix" } else { "after" }),
                        max_current_payload_bytes: if case == 7 { 0 } else { 128 },
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live,
                )
            };
            assert!(result.is_err(), "case {case}");
            if case == 11 {
                assert!(put.get());
            }
            drop(read);
            let guard = tx.guard();
            assert!(tx.seal(&guard, &live, 1).is_err(), "case {case}");
        }
        // None stays None: a child setter cannot initialize only part of a parent.
        let absent = make_parent(&StateRootV1::from_canonical_bytes(&roots[1]).unwrap());
        let absent_live = absent.fence().unwrap();
        for create in [false, true] {
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut tx =
                ExecutionTransaction::begin(absent.clone(), [9; 32], &state, &memory, limits())
                    .unwrap();
            if create {
                tx.write(Ns::Business, b"successful-prefix", Some([9; 32]))
                    .unwrap();
            }
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 8192,
            };
            let mut read = read_context(&mut tx, &values, &memory, &mut io, &mut keys, || false);
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                limits: super::limits(),
                cancelled: || false,
            };
            let guard = read.transaction.guard();
            let result = change_world_rule_effect_overlay_v1(
                WorldRuleEffectChangeV1 {
                    key: "new",
                    expected: E::RulesAbsent,
                    after: create.then_some("not-a-parent"),
                    max_current_payload_bytes: 128,
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
            let sealed = tx.seal(&guard, &absent_live, 1);
            if create {
                assert!(sealed.is_err());
            } else {
                let sealed = sealed.unwrap();
                assert_eq!(sealed.record.reads.len(), 3);
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
        let _output = memory.reserve(16 << 10, class).unwrap();
        for (root, name, expected, text) in [
            (
                &roots[0],
                " P\0é ",
                V::Value(" R\0甲 ".len() as u64),
                " R\0甲 ",
            ),
            (&after, " P\0é ", V::Value(3), "new"),
            (&after, "", V::EffectAbsent, ""),
            (&after, "new", V::Value(5), "fresh"),
            (&after, "unique-noop", V::Value(4), "keep"),
        ] {
            let mut output = Vec::with_capacity(128);
            assert_eq!(
                reader
                    .read(
                        &StateRootV1::from_canonical_bytes(root).unwrap(),
                        name,
                        128,
                        &mut output,
                        || false
                    )
                    .unwrap(),
                expected
            );
            assert_eq!(output, text.as_bytes());
        }
    }
}
