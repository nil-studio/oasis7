use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
        TransitionVerifier,
    },
    incremental_value::ValueNodeWriter,
};
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
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
pub(super) fn exercise(path: &Path, compression: PageCompression, roots: &[Vec<u8>; 5]) {
    use WorldTextMapFieldV1 as F;
    let after = {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _requests = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        let store =
            IncrementalPageStore::open(path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let mut root_set = RootSetV1::empty("text-maps".into(), [3; 32]).unwrap();
        for bytes in &roots[..2] {
            let root = StateRootV1::from_canonical_bytes(bytes).unwrap();
            let namespace = root.namespace;
            *root_set
                .state
                .iter_mut()
                .find(|r| r.namespace == namespace)
                .unwrap() = root;
        }
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
        let mut transaction =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits).unwrap();
        let values = store.value_session(physical());
        let mut writer = store.value_session(physical());
        {
            let mut read = WorldFieldOverlayContextV1 {
                transaction: &mut transaction,
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
                        eprintln!("text overlay state: {e:?}");
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
            for (field, key, expected, after, changed) in [
                (
                    F::ModuleArtifactOwners,
                    "artifact",
                    Some(" Agent\0é "),
                    Some("owner-é"),
                    true,
                ),
                (F::ModuleArtifactOwners, "", Some(""), None, true),
                (F::ModuleArtifactOwners, "", None, Some(""), true),
                (
                    F::NodeIdentityBindings,
                    "n",
                    Some("Key-A"),
                    Some("Key-B"),
                    true,
                ),
                (F::NodeIdentityBindings, "n", Some("Key-B"), None, true),
                (
                    F::NodeIdentityBindings,
                    "empty",
                    Some(""),
                    Some("身份"),
                    true,
                ),
                (
                    F::NodeMainTokenAccountBindings,
                    "n",
                    Some("acct"),
                    Some("acct"),
                    false,
                ),
                (F::NodeMainTokenAccountBindings, "new", None, Some(""), true),
                (
                    F::NodeMainTokenAccountBindings,
                    "new",
                    Some(""),
                    Some("账户"),
                    true,
                ),
            ] {
                let guard = read.transaction.guard();
                assert_eq!(
                    change_world_text_map_overlay_v1(
                        WorldTextMapChangeV1 {
                            field,
                            key,
                            expected,
                            after,
                            max_current_payload_bytes: 128
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live
                    )
                    .unwrap_or_else(|e| panic!("text update {field:?}/{key}: {e:?}")),
                    changed
                );
                let mut output = Vec::with_capacity(128);
                assert_eq!(
                    read_world_text_map_overlay_v1(field, key, &mut read, 128, &mut output)
                        .unwrap(),
                    after.map(|s| s.len() as u64)
                );
                assert_eq!(output, after.unwrap_or("").as_bytes());
            }
        }
        let guard = transaction.guard();
        let sealed = transaction.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.reads.len(), 9);
        assert_eq!(sealed.record.writes.len(), 4);
        assert_eq!(
            sealed
                .record
                .writes
                .iter()
                .filter(|w| w.namespace == Ns::Authorization)
                .count(),
            3
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
        // Every failure follows a successful contribution, so a failure must
        // poison the whole transaction rather than leave a sealable prefix.
        for case in 0..17 {
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut case_parent = parent.clone();
            if (9..12).contains(&case) {
                *case_parent
                    .roots
                    .state
                    .iter_mut()
                    .find(|r| r.namespace == Ns::Authorization)
                    .unwrap() = StateRootV1::from_canonical_bytes(&roots[case - 7]).unwrap();
            }
            let case_live = case_parent.fence().unwrap();
            let alien = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let foreign = if case == 14 {
                Some(
                    ExecutionTransaction::begin(
                        case_parent.clone(),
                        [8; 32],
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
            let read_checks = Cell::new(0usize);
            let write_checks = Cell::new(0usize);
            let cancel_at = Cell::new(None::<usize>);
            let armed = Cell::new(false);
            let put = Cell::new(false);
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
                    change_world_text_map_overlay_v1(
                        WorldTextMapChangeV1 {
                            field: F::ModuleArtifactOwners,
                            key: "artifact",
                            expected: Some(" Agent\0é "),
                            after: Some("first"),
                            max_current_payload_bytes: 128
                        },
                        &mut read,
                        &mut write,
                        &stale,
                        &case_live
                    )
                    .unwrap()
                );
                if case == 16 {
                    // Establish the complete current-value read boundary through
                    // the public reader, then cancel at its next success check.
                    // No wall-clock race or hard-coded callback count is used.
                    let before = read_checks.get();
                    assert_eq!(
                        read_world_text_map_overlay_v1(
                            F::NodeIdentityBindings,
                            "n",
                            &mut read,
                            128,
                            &mut io::sink()
                        )
                        .unwrap(),
                        Some(5)
                    );
                    let complete_read_checks = read_checks.get() - before;
                    assert!(complete_read_checks > 0);
                    cancel_at.set(Some(complete_read_checks + 1));
                }
                let guard = if case == 14 {
                    foreign.unwrap()
                } else if case == 3 {
                    stale
                } else {
                    read.transaction.guard()
                };
                let expected = match case {
                    0 => Some("Key-X"),
                    1 => Some("Key"),
                    2 => Some("Key-A-extra"),
                    4 => None,
                    _ => Some("Key-A"),
                };
                if case == 5 {
                    write.limits.payload_bytes = 0;
                }
                if case == 6 {
                    cancelled.set(true);
                }
                if case == 8 {
                    read.values.io.nodes_remaining = 0;
                }
                if case == 12 {
                    write.memory = &alien;
                }
                if case == 13 {
                    read.values.class = MemoryClass::Recovery;
                }
                if case == 15 {
                    armed.set(true);
                }
                let result = change_world_text_map_overlay_v1(
                    WorldTextMapChangeV1 {
                        field: F::NodeIdentityBindings,
                        key: "n",
                        expected,
                        after: Some(if case == 16 { "Key-A" } else { "next" }),
                        max_current_payload_bytes: if case == 7 { 0 } else { 128 },
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &case_live,
                );
                assert!(result.is_err(), "case {case}");
                if case == 16 {
                    assert_eq!(write_checks.get(), cancel_at.get().unwrap());
                    assert!(matches!(result, Err(oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))));
                }
                if case == 15 {
                    assert!(put.get(), "cancel only after a real provisional value put");
                    assert!(matches!(result, Err(oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled)) | Err(oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(ValueTreeError::Cancelled)))));
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
    // Close the entire preparation store/pool before cold old/new root reads.
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
        for (field, key, old, new) in [
            (
                F::ModuleArtifactOwners,
                "artifact",
                Some(" Agent\0é "),
                Some("owner-é"),
            ),
            (F::ModuleArtifactOwners, "", Some(""), Some("")),
            (F::NodeIdentityBindings, "n", Some("Key-A"), None),
            (F::NodeIdentityBindings, "empty", Some(""), Some("身份")),
            (
                F::NodeMainTokenAccountBindings,
                "n",
                Some("acct"),
                Some("acct"),
            ),
            (F::NodeMainTokenAccountBindings, "new", None, Some("账户")),
        ] {
            let old_root = StateRootV1::from_canonical_bytes(
                &roots[usize::from(field.namespace() == Ns::Authorization)],
            )
            .unwrap();
            let new_root = after
                .state
                .iter()
                .find(|r| r.namespace == field.namespace())
                .unwrap();
            for (root, expected) in [(&old_root, old), (new_root, new)] {
                let mut output = Vec::with_capacity(128);
                assert_eq!(
                    reader
                        .read(root, field, key, 128, &mut output, || false)
                        .unwrap(),
                    expected.map(|s| s.len() as u64)
                );
                assert_eq!(output, expected.unwrap_or("").as_bytes());
            }
        }
    }
}
