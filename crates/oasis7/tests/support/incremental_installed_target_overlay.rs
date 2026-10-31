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
    use ModuleInstallTargetRefV1::{LocationInfrastructure as Location, SelfAgent as SelfTarget};
    let after = {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _requests = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        let store =
            IncrementalPageStore::open(path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let root = StateRootV1::from_canonical_bytes(&roots[0]).unwrap();
        let mut root_set = RootSetV1::empty("installed-targets".into(), [3; 32]).unwrap();
        *root_set
            .state
            .iter_mut()
            .find(|r| r.namespace == Ns::Artifacts)
            .unwrap() = root;
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
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits).unwrap();
        let values = store.value_session(physical());
        let mut writer = store.value_session(physical());
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
                        eprintln!("installed target state: {e:?}");
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
                    payload_bytes: 128,
                    record_bytes: 8192,
                },
                cancelled: || false,
            };
            for (key, expected, after, changed) in [
                ("", Some(SelfTarget), Some(Location("")), true),
                ("", Some(Location("")), Some(SelfTarget), true),
                ("self", Some(SelfTarget), Some(Location("self-site")), true),
                ("self", Some(Location("self-site")), None, true),
                ("empty", Some(Location("")), Some(SelfTarget), true),
                (
                    "loc",
                    Some(Location(" 地点\0é ")),
                    Some(Location("next-é")),
                    true,
                ),
                (
                    "literal",
                    Some(Location("SelfAgent")),
                    Some(Location("SelfAgent")),
                    false,
                ),
                ("new", None, Some(SelfTarget), true),
                ("new", Some(SelfTarget), Some(Location("")), true),
            ] {
                let guard = read.transaction.guard();
                assert_eq!(
                    change_installed_module_target_overlay_v1(
                        InstalledModuleTargetChangeV1 {
                            key,
                            expected,
                            after,
                            max_current_location_bytes: 128
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live
                    )
                    .unwrap_or_else(|e| panic!("target update {key}: {e:?}")),
                    changed
                );
                // The unique literal no-op must supply its own dependency;
                // do not let a follow-up public read mask a skipped no-op read.
                if changed {
                    let mut output = Vec::with_capacity(128);
                    let actual =
                        read_installed_module_target_overlay_v1(key, &mut read, 128, &mut output)
                            .unwrap();
                    assert_eq!(
                        actual,
                        after.map(|v| match v {
                            SelfTarget => InstalledModuleTargetReadV1::SelfAgent,
                            Location(s) => InstalledModuleTargetReadV1::LocationInfrastructure {
                                utf8_bytes: s.len() as u64
                            },
                        })
                    );
                    assert_eq!(
                        output,
                        match after {
                            Some(Location(s)) => s.as_bytes(),
                            _ => &[],
                        }
                    );
                }
            }
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        // Metadata + six queried keys, including the unique no-op key.
        assert_eq!(sealed.record.reads.len(), 7);
        assert_eq!(sealed.record.writes.len(), 4);
        assert!(
            sealed
                .record
                .writes
                .iter()
                .all(|w| w.namespace == Ns::Artifacts)
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
        // Each failing contribution follows one successful typed target update.
        for case in 0..19 {
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut case_parent = parent.clone();
            if (15..18).contains(&case) {
                *case_parent
                    .roots
                    .state
                    .iter_mut()
                    .find(|r| r.namespace == Ns::Artifacts)
                    .unwrap() = StateRootV1::from_canonical_bytes(&roots[case - 14]).unwrap();
            }
            let case_live = case_parent.fence().unwrap();
            let foreign = if case == 7 {
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
            let alien = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let mut tx =
                ExecutionTransaction::begin(case_parent, [7; 32], &state, &memory, limits).unwrap();
            let cancelled = Cell::new(false);
            let armed = Cell::new(false);
            let put = Cell::new(false);
            let read_checks = Cell::new(0usize);
            let write_checks = Cell::new(0usize);
            let cancel_at = Cell::new(None::<usize>);
            let mut cancelling_writer = CancelAfterPut {
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
                    sink: &mut cancelling_writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 8192,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 128,
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
                    change_installed_module_target_overlay_v1(
                        InstalledModuleTargetChangeV1 {
                            key: "loc",
                            expected: Some(Location(" 地点\0é ")),
                            after: Some(Location("first")),
                            max_current_location_bytes: 128
                        },
                        &mut read,
                        &mut write,
                        &stale,
                        &case_live
                    )
                    .unwrap()
                );
                if case == 18 {
                    let before = read_checks.get();
                    assert_eq!(
                        read_installed_module_target_overlay_v1(
                            "literal",
                            &mut read,
                            128,
                            &mut io::sink()
                        )
                        .unwrap(),
                        Some(InstalledModuleTargetReadV1::LocationInfrastructure { utf8_bytes: 9 })
                    );
                    let complete = read_checks.get() - before;
                    assert!(complete > 0);
                    cancel_at.set(Some(complete + 1));
                }
                let guard = if case == 7 {
                    foreign.unwrap()
                } else if case == 6 {
                    stale
                } else {
                    read.transaction.guard()
                };
                let (key, expected) = match case {
                    0 => ("self", None),
                    1 => ("self", Some(Location(""))),
                    2 => ("empty", Some(SelfTarget)),
                    3 => ("literal", Some(Location("wrong"))),
                    4 => ("literal", Some(Location("Self"))),
                    5 => ("literal", Some(Location("SelfAgent-extra"))),
                    10 | 18 => ("literal", Some(Location("SelfAgent"))),
                    15..=17 => ("ascii", Some(Location("A"))),
                    _ => ("self", Some(SelfTarget)),
                };
                if case == 8 {
                    read.values.class = MemoryClass::Recovery;
                }
                if case == 9 {
                    write.memory = &alien;
                }
                if case == 11 {
                    write.limits.payload_bytes = 0;
                }
                if case == 12 {
                    read.values.io.nodes_remaining = 0;
                }
                if case == 13 {
                    cancelled.set(true);
                }
                if case == 14 {
                    armed.set(true);
                }
                let result = change_installed_module_target_overlay_v1(
                    InstalledModuleTargetChangeV1 {
                        key,
                        expected,
                        after: if case == 18 {
                            expected
                        } else {
                            Some(Location("next"))
                        },
                        max_current_location_bytes: if case == 10 { 0 } else { 128 },
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &case_live,
                );
                assert!(result.is_err(), "case {case}");
                if case == 14 {
                    assert!(put.get());
                    assert!(matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                            | Err(WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                                ValueTreeError::Cancelled
                            )))
                    ));
                }
                if case == 18 {
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
    // Close the source store/pool, then query both histories with cold budgets.
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
            .find(|r| r.namespace == Ns::Artifacts)
            .unwrap();
        for (key, old, new) in [
            ("", Some(SelfTarget), Some(SelfTarget)),
            ("self", Some(SelfTarget), None),
            ("empty", Some(Location("")), Some(SelfTarget)),
            ("loc", Some(Location(" 地点\0é ")), Some(Location("next-é"))),
            (
                "literal",
                Some(Location("SelfAgent")),
                Some(Location("SelfAgent")),
            ),
            ("new", None, Some(Location(""))),
        ] {
            for (root, expected) in [(&old_root, old), (new_root, new)] {
                let mut output = Vec::with_capacity(128);
                assert_eq!(
                    reader.read(root, key, 128, &mut output, || false).unwrap(),
                    expected.map(|t| match t {
                        SelfTarget => InstalledModuleTargetReadV1::SelfAgent,
                        Location(s) => InstalledModuleTargetReadV1::LocationInfrastructure {
                            utf8_bytes: s.len() as u64
                        },
                    })
                );
                assert_eq!(
                    output,
                    match expected {
                        Some(Location(s)) => s.as_bytes(),
                        _ => &[],
                    }
                );
            }
        }
    }
}
