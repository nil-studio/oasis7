use super::*;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
    TransitionVerifier,
};
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::path::Path;
struct TraceWriter<'a, W> {
    inner: &'a mut W,
    puts: &'a Cell<usize>,
    after_put: &'a Cell<usize>,
}
impl<W: ValueNodeWriter> ValueNodeWriter for TraceWriter<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, bytes)?;
        self.puts.set(self.puts.get() + 1);
        self.after_put.set(0);
        Ok(())
    }
}
pub(super) fn late_change_probe(cancel_after: Option<usize>) -> (bool, usize, usize, bool) {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _source = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let rows = produce_all(&store, &memory, &source());
    let root = build(&store, &memory, &rows);
    let parent = make_parent(&root);
    let live = parent.fence().unwrap();
    let state = store
        .state_session(whole_io(), MemoryClass::Ordinary)
        .unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent, [71; 32], &state, &memory, txn_limits()).unwrap();
    let values = store.value_session(physical());
    let mut writer = store.value_session(physical());
    {
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32768,
        };
        let mut read = context(&mut tx, &values, &memory, &mut io, &mut keys, || false);
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
    }
    let original = shipment();
    let after = SupplyShipmentV1 {
        authorization_id: "auth".into(),
        ..original.clone()
    };
    let puts = Cell::new(0);
    let after_put = Cell::new(0);
    let mut traced = TraceWriter {
        inner: &mut writer,
        puts: &puts,
        after_put: &after_put,
    };
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32768,
    };
    let mut read = context(&mut tx, &values, &memory, &mut io, &mut keys, || {
        if puts.get() == 0 {
            return false;
        }
        after_put.set(after_put.get() + 1);
        cancel_after.is_some_and(|boundary| after_put.get() >= boundary)
    });
    let mut write = MemberRecordWriteContext {
        sink: &mut traced,
        memory: &memory,
        io: &mut logical(),
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 32768,
        },
        limits: limits(),
        cancelled: || false,
    };
    let guard = read.transaction.guard();
    let result = change_world_rule_shipment_overlay_v1(
        WorldRuleShipmentChangeV1 {
            key: u64::MAX,
            expected: WorldRuleShipmentExpectedV1::Value(&original),
            after: Some(&after),
            max_current_payload_bytes: 128,
        },
        &mut read,
        &mut write,
        &guard,
        &live,
    );
    let failed = matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
    );
    drop(read);
    let guard = tx.guard();
    let unsealable = tx.seal(&guard, &live, 1).is_err();
    (failed, after_put.get(), puts.get(), unsealable)
}
fn txn_limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 40,
        max_writes: 20,
        max_facts: 4,
        max_pending_node_bytes: 4 << 20,
    }
}
fn make_parent(root: &StateRootV1) -> ExecutionParent {
    let mut roots = RootSetV1::empty("rule-shipments".into(), [3; 32]).unwrap();
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
    PageIoBudget {
        reads_remaining: 300_000,
        ..physical()
    }
}
fn context<
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
            return Err(ValueTreeError::Invalid("injected shipment writer"));
        }
        self.inner.put_node(id, bytes)?;
        if self.armed.get() {
            self.put.set(true);
            self.cancelled.set(true);
        }
        Ok(())
    }
}
pub(super) fn exercise(path: &Path, compression: PageCompression, roots: &[Vec<u8>]) {
    use WorldRuleShipmentExpectedV1 as E;
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
        let original = shipment();
        let changed = SupplyShipmentV1 {
            authorization_id: "auth".into(),
            input_amount: 0,
            delivered_amount: -7,
            settled: true,
            ..original.clone()
        };
        {
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 32768,
            };
            let mut read = context(&mut tx, &values, &memory, &mut io, &mut keys, || false);
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
            for (id, expected, after, changed_flag) in [
                (1, E::ShipmentAbsent, None, false),
                (u64::MAX, E::Value(&original), Some(&changed), true),
                (u64::MAX, E::Value(&changed), Some(&changed), false),
                (0, E::ShipmentAbsent, Some(&original), true),
                (0, E::Value(&original), None, true),
            ] {
                let guard = read.transaction.guard();
                assert_eq!(
                    change_world_rule_shipment_overlay_v1(
                        WorldRuleShipmentChangeV1 {
                            key: id,
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
                    changed_flag
                );
            }
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.reads.len(), 26);
        assert_eq!(sealed.record.writes.len(), 4);
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
        for case in 0..18 {
            let state = store
                .state_session(whole_io(), MemoryClass::Ordinary)
                .unwrap();
            let mut tx =
                ExecutionTransaction::begin(parent.clone(), [7; 32], &state, &memory, txn_limits())
                    .unwrap();
            let stale = tx.guard();
            let alien = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            {
                let mut io = logical();
                let mut keys = SemanticKeyBudget {
                    bytes_remaining: 32768,
                };
                let mut read = context(&mut tx, &values, &memory, &mut io, &mut keys, || false);
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
            }
            if case == 12 {
                let bad = StateRootV1::from_canonical_bytes(&roots[8]).unwrap();
                let target = key(u64::MAX, Some("settled"));
                tx.write(
                    Ns::Authorization,
                    target.as_bytes(),
                    bad.get(target.as_bytes(), &state).unwrap(),
                )
                .unwrap();
            }
            let ticks = Cell::new(0);
            let boundary = Cell::new(usize::MAX);
            if case == 15 {
                let mut io = logical();
                let mut keys = SemanticKeyBudget {
                    bytes_remaining: 32768,
                };
                let mut read = context(&mut tx, &values, &memory, &mut io, &mut keys, || {
                    ticks.set(ticks.get() + 1);
                    false
                });
                let mut a = std::io::sink();
                let mut b = std::io::sink();
                let mut c = std::io::sink();
                let mut d = std::io::sink();
                read_world_rule_shipment_overlay_v1(
                    u64::MAX,
                    &mut read,
                    128,
                    &mut [&mut a, &mut b, &mut c, &mut d],
                )
                .unwrap();
                boundary.set(ticks.get() + 1);
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
                fail: case == 8,
            };
            let mut io = logical();
            if case == 10 {
                io.read_bytes_remaining = 0;
            }
            let mut keys = SemanticKeyBudget {
                bytes_remaining: if case == 11 { 0 } else { 32768 },
            };
            let mut read = context(&mut tx, &values, &memory, &mut io, &mut keys, || {
                ticks.set(ticks.get() + 1);
                case == 7 || cancelled.get() || (case == 15 && ticks.get() >= boundary.get())
            });
            if case == 16 {
                read.values.class = MemoryClass::Recovery;
            }
            let mut bound = limits();
            if case == 4 {
                bound.payload_bytes = 0;
            }
            if case == 5 {
                bound.semantic_bytes = 0;
            }
            let mut write_io = logical();
            if case == 6 {
                write_io.write_bytes_remaining = 0;
            }
            let mut write = MemberRecordWriteContext {
                sink: &mut controlled,
                memory: if case == 3 { &alien } else { &memory },
                io: &mut write_io,
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 32768,
                },
                limits: bound,
                cancelled: || false,
            };
            let mut fence = live;
            if case == 2 {
                fence.authority.epoch += 1;
            }
            let guard = if case == 1 {
                stale
            } else {
                read.transaction.guard()
            };
            if matches!(case, 8 | 9) {
                armed.set(true);
            }
            let wrong = SupplyShipmentV1 {
                batch_id: " X\0é ".into(),
                ..original.clone()
            };
            let result = change_world_rule_shipment_overlay_v1(
                WorldRuleShipmentChangeV1 {
                    key: u64::MAX,
                    expected: match case {
                        0 => E::Value(&wrong),
                        13 => E::ShipmentAbsent,
                        17 => E::RulesAbsent,
                        _ => E::Value(&original),
                    },
                    after: Some(if case == 15 { &original } else { &changed }),
                    max_current_payload_bytes: if case == 14 { 0 } else { 128 },
                },
                &mut read,
                &mut write,
                &guard,
                &fence,
            );
            assert!(result.is_err(), "case {case}");
            if case == 9 {
                assert!(put.get());
            }
            drop(read);
            let guard = tx.guard();
            assert!(tx.seal(&guard, &live, 1).is_err(), "case {case}");
        }
        let absent = make_parent(&StateRootV1::from_canonical_bytes(&roots[1]).unwrap());
        let absent_live = absent.fence().unwrap();
        for create in [false, true] {
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut tx =
                ExecutionTransaction::begin(absent.clone(), [9; 32], &state, &memory, txn_limits())
                    .unwrap();
            if create {
                tx.write(Ns::Business, b"successful-prefix", Some([9; 32]))
                    .unwrap();
            }
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 32768,
            };
            let mut read = context(&mut tx, &values, &memory, &mut io, &mut keys, || false);
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
            let result = change_world_rule_shipment_overlay_v1(
                WorldRuleShipmentChangeV1 {
                    key: 0,
                    expected: E::RulesAbsent,
                    after: create.then_some(&original),
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
            if create {
                assert!(tx.seal(&guard, &absent_live, 1).is_err());
            } else {
                let sealed = tx.seal(&guard, &absent_live, 1).unwrap();
                assert_eq!(sealed.record.reads.len(), 10);
                assert!(sealed.record.writes.is_empty());
            }
        }
        verified
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
        let _full = (class == MemoryClass::Recovery).then(|| {
            memory
                .reserve((1 << 20) - memory.used(), MemoryClass::Ordinary)
                .unwrap()
        });
        let reader = Reader {
            store: &store,
            memory: &memory,
            class,
            state_io: physical(),
        };
        assert_eq!(
            reader.read(&before, u64::MAX, 128, || false).unwrap().0,
            WorldRuleShipmentValueV1::Value {
                identity_utf8_bytes: [0, 6, 11, 7],
                input_amount: i64::MIN,
                delivered_amount: i64::MAX,
                settled: false
            }
        );
        let result = reader
            .read(
                &StateRootV1::from_canonical_bytes(&after).unwrap(),
                u64::MAX,
                128,
                || false,
            )
            .unwrap();
        assert_eq!(
            result.0,
            WorldRuleShipmentValueV1::Value {
                identity_utf8_bytes: [4, 6, 11, 7],
                input_amount: 0,
                delivered_amount: -7,
                settled: true
            }
        );
        assert_eq!(result.1[0], b"auth");
        drop(result);
        assert_eq!(
            reader
                .read(
                    &StateRootV1::from_canonical_bytes(&after).unwrap(),
                    0,
                    128,
                    || false
                )
                .unwrap()
                .0,
            WorldRuleShipmentValueV1::ShipmentAbsent
        );
    }
}
