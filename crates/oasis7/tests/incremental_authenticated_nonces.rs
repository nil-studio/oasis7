#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_authenticated_nonces::*, incremental_field_member::*,
    incremental_field_read::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
#[path = "support/incremental_auth_noop_cancel.rs"]
mod noop_cancel;
struct CancelAfterPut<'a, W> {
    inner: &'a mut W,
    calls: &'a std::cell::Cell<usize>,
    at: &'a std::cell::Cell<usize>,
    cancelled: &'a std::cell::Cell<bool>,
}
impl<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter>
    oasis7_formal_protocol::incremental_value::ValueNodeWriter for CancelAfterPut<'_, W>
{
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, bytes)?;
        self.calls.set(self.calls.get() + 1);
        if self.calls.get() == self.at.get() {
            self.cancelled.set(true);
        }
        Ok(())
    }
}
#[test]
fn player_and_key_nonce_identity_and_presence_survive_cold_roots() {
    use AuthenticatedNonceV1 as Nonce;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let producer = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let source = WorldState {
            authenticated_collect_data_last_nonces: [
                (
                    "".into(),
                    [("".into(), 0), ("公钥\0".into(), u64::MAX)].into(),
                ),
                ("empty-player".into(), Default::default()),
                ("a:b".into(), [("c".into(), 17)].into()),
                ("a".into(), [("b:c".into(), 19)].into()),
            ]
            .into(),
            ..WorldState::default()
        };
        let roots = {
            let store = IncrementalPageStore::create(&path, producer.clone(), compression).unwrap();
            let mut values = store.value_session(physical());
            let _batch = producer.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
            let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(9);
            let mut remaining = 9;
            let summary = prepare_authenticated_nonces_genesis_v1(
                &source,
                &mut MemberRecordWriteContext {
                    sink: &mut values,
                    memory: &producer,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 8192,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
                &mut remaining,
                &mut |row| {
                    assert!(rows.len() < 9);
                    assert_eq!(row.namespace, StateNamespaceV1::Authorization);
                    rows.push(row);
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(
                summary,
                AuthenticatedNonceGenesisV1 {
                    players: 4,
                    keys: 4
                }
            );
            assert_eq!(remaining, 0);
            let field = FieldDescriptor {
                owner: FieldOwner::WorldState,
                name: "authenticated_collect_data_last_nonces",
                destination: FieldDestination::State(StateNamespaceV1::Authorization),
                shape: FieldShape::NestedMap,
            };
            let mut group_preimage = Vec::from(&b"oasis7.iws.semantic-key.v1\0"[..]);
            group_preimage.push(1);
            group_preimage.extend_from_slice(&3u64.to_be_bytes());
            group_preimage.extend_from_slice(b"a:b");
            let group = WorldFieldKeyV1::new(
                field,
                WorldFieldRecordKindV1::Group,
                Some(*blake3::hash(&group_preimage).as_bytes()),
            )
            .unwrap();
            assert!(rows.iter().any(|row| row.mutation.key == group.as_bytes()));
            let bad = write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                &("a:b", "c"),
                MemberPayloadRefV1::I64(17),
                &mut MemberRecordWriteContext {
                    sink: &mut values,
                    memory: &producer,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            [0, 1, 2].map(|variant| {
                let state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let workspace = producer.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
                let mut changes: Vec<_> = rows
                    .iter()
                    .filter(|row| variant != 1 || row.mutation.key != group.as_bytes())
                    .map(|row| StateWriteV1 {
                        key: row.mutation.key.clone(),
                        before: None,
                        after: if variant == 2 && row.mutation.key == bad.field_key.as_bytes() {
                            Some(bad.value_root)
                        } else {
                            row.mutation.after
                        },
                    })
                    .collect();
                changes.sort_by(|a, b| a.key.cmp(&b.key));
                let delta = workspace.retain(
                    StateDeltaV1::build(
                        StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
                        changes,
                        &state,
                        2 << 20,
                    )
                    .unwrap(),
                );
                assert_eq!(delta.after.leaf_count, if variant == 1 { 8 } else { 9 });
                store
                    .value_session(physical())
                    .put_state_pages(&delta, &producer)
                    .unwrap();
                delta.after.canonical_bytes().unwrap()
            })
        };
        assert_eq!(producer.used(), 0);
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
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let _filler = if class == MemoryClass::Recovery {
                Some(
                    memory
                        .reserve((1 << 20) - memory.used(), MemoryClass::Ordinary)
                        .unwrap(),
                )
            } else {
                None
            };
            for (player, key, expected) in [
                ("", "", Nonce::Value(0)),
                ("", "公钥\0", Nonce::Value(u64::MAX)),
                ("empty-player", "", Nonce::KeyAbsent),
                ("missing", "", Nonce::PlayerAbsent),
                ("a:b", "c", Nonce::Value(17)),
                ("a", "b:c", Nonce::Value(19)),
                ("a:b", "b:c", Nonce::KeyAbsent),
            ] {
                let root = StateRootV1::from_canonical_bytes(&roots[0]).unwrap();
                let orphan = StateRootV1::from_canonical_bytes(&roots[1]).unwrap();
                let wrong_codec = StateRootV1::from_canonical_bytes(&roots[2]).unwrap();
                let empty = StateRootV1::empty(StateNamespaceV1::Authorization).unwrap();
                let wrong = StateRootV1::empty(StateNamespaceV1::Business).unwrap();
                let state = store.state_session(physical(), class).unwrap();
                let values = store.value_session(physical());
                let mut read = WorldFieldReadContextV1 {
                    root: &root,
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 8192,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("state page failure"))
                    },
                    cancelled: || false,
                };
                assert_eq!(
                    read_authenticated_nonce_v1(player, key, &mut read).unwrap(),
                    expected
                );
                read.root = &wrong;
                assert!(matches!(
                    read_authenticated_nonce_v1(player, key, &mut read),
                    Err(ValueTreeError::Invalid("field namespace"))
                ));
                read.root = &empty;
                assert!(matches!(
                    read_authenticated_nonce_v1(player, key, &mut read),
                    Err(ValueTreeError::Invalid(
                        "authenticated nonce missing metadata"
                    ))
                ));
                if player == "a:b" && key == "c" {
                    read.root = &orphan;
                    assert!(matches!(
                        read_authenticated_nonce_v1(player, key, &mut read),
                        Err(ValueTreeError::Invalid("authenticated nonce orphan key"))
                    ));
                    read.root = &wrong_codec;
                    assert!(matches!(
                        read_authenticated_nonce_v1(player, key, &mut read),
                        Err(ValueTreeError::Invalid("member expected profile"))
                    ));
                }
            }
        }
        let next = {
            use oasis7_formal_protocol::incremental_execution::{
                AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1,
                TransactionLimits, TransitionVerifier,
            };
            use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
            use oasis7_runtime::incremental_field_overlay::{
                WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1 as Error,
            };
            let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let mut root_set = RootSetV1::empty("authenticated-nonces".into(), [3; 32]).unwrap();
            *root_set
                .state
                .iter_mut()
                .find(|root| root.namespace == StateNamespaceV1::Authorization)
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
                max_reads: 32,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            };
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical());
            let mut writer = store.value_session(physical());
            let mut transaction =
                ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits)
                    .unwrap();
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
                        bytes_remaining: 16384,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("state page failure"))
                    },
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16384,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                };
                for (player, key, expected, after, create, count) in [
                    ("", "", Nonce::Value(0), Some(1), false, 1),
                    ("", "", Nonce::Value(1), Some(3), false, 1),
                    ("", "", Nonce::Value(3), None, false, 1),
                    ("", "", Nonce::KeyAbsent, Some(0), false, 1),
                    ("new:p", "new:k", Nonce::PlayerAbsent, Some(5), true, 2),
                    ("new:p", "new:k", Nonce::Value(5), Some(6), false, 1),
                    ("new:p", "new:k", Nonce::Value(6), None, false, 1),
                    ("new:p", "new:k", Nonce::KeyAbsent, Some(0), false, 1),
                    ("empty-new", "unused", Nonce::PlayerAbsent, None, true, 1),
                    ("nobody", "k", Nonce::PlayerAbsent, None, false, 0),
                    (
                        "empty-player",
                        "public",
                        Nonce::KeyAbsent,
                        Some(u64::MAX),
                        false,
                        1,
                    ),
                    (
                        "empty-player",
                        "public",
                        Nonce::Value(u64::MAX),
                        Some(u64::MAX),
                        false,
                        0,
                    ),
                ] {
                    let guard = read.transaction.guard();
                    assert_eq!(
                        change_authenticated_nonce_overlay_v1(
                            AuthenticatedNonceChangeV1 {
                                player,
                                public_key: key,
                                expected,
                                after,
                                create_player: create
                            },
                            &mut read,
                            &mut write,
                            &guard,
                            &live
                        )
                        .unwrap(),
                        count
                    );
                    let result =
                        read_authenticated_nonce_overlay_v1(player, key, &mut read).unwrap();
                    let expected_after = after.map(Nonce::Value).unwrap_or(
                        if expected == Nonce::PlayerAbsent && !create {
                            Nonce::PlayerAbsent
                        } else {
                            Nonce::KeyAbsent
                        },
                    );
                    assert_eq!(result, expected_after);
                }
            }
            let guard = transaction.guard();
            let sealed = transaction.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.reads.len(), 11);
            assert_eq!(sealed.record.writes.len(), 4);
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
            let mut pages = store.value_session(physical());
            for node in &verified.new_nodes {
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
            for case in 0..5 {
                let state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let mut transaction =
                    ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits)
                        .unwrap();
                let calls = std::cell::Cell::new(0);
                let cancel_at = std::cell::Cell::new(usize::MAX);
                let cancel = std::cell::Cell::new(false);
                let mut counted = CancelAfterPut {
                    inner: &mut writer,
                    calls: &calls,
                    at: &cancel_at,
                    cancelled: &cancel,
                };
                let group_nodes = if case == 4 {
                    use oasis7_runtime::incremental_field_presence::{
                        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_group_v1,
                    };
                    let field = FieldDescriptor {
                        owner: FieldOwner::WorldState,
                        name: "authenticated_collect_data_last_nonces",
                        destination: FieldDestination::State(StateNamespaceV1::Authorization),
                        shape: FieldShape::NestedMap,
                    };
                    write_field_group_v1(
                        field,
                        "cancel-player",
                        &mut PresenceRecordWriteContext {
                            sink: &mut counted,
                            memory: &memory,
                            io: &mut logical(),
                            key_work: &mut SemanticKeyBudget {
                                bytes_remaining: 8192,
                            },
                            limits: PresenceRecordLimitsV1 {
                                semantic_bytes: 1024,
                                record_bytes: 4096,
                            },
                            cancelled: || false,
                        },
                    )
                    .unwrap();
                    calls.get()
                } else {
                    0
                };
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
                        record_bytes: 4096,
                        take_state_error: || {
                            state
                                .take_read_error()
                                .map(|_| ValueTreeError::Invalid("state page failure"))
                        },
                        cancelled: || false,
                    };
                    let mut write = MemberRecordWriteContext {
                        sink: &mut counted,
                        memory: &memory,
                        io: &mut logical(),
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 8192,
                        },
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 8,
                            record_bytes: 4096,
                        },
                        cancelled: || cancel.get(),
                    };
                    let guard = read.transaction.guard();
                    change_authenticated_nonce_overlay_v1(
                        AuthenticatedNonceChangeV1 {
                            player: "",
                            public_key: "",
                            expected: Nonce::Value(0),
                            after: Some(1),
                            create_player: false,
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live,
                    )
                    .unwrap();
                    let (player, expected, create) = match case {
                        0 => ("no-create", Nonce::PlayerAbsent, false),
                        1 => ("missing", Nonce::KeyAbsent, true),
                        2 => ("empty-player", Nonce::PlayerAbsent, false),
                        3 => ("partial", Nonce::PlayerAbsent, true),
                        _ => ("cancel-player", Nonce::PlayerAbsent, true),
                    };
                    if case == 3 {
                        write.limits.payload_bytes = 0;
                    }
                    if case == 4 {
                        calls.set(0);
                        cancel_at.set(group_nodes + 1);
                    }
                    let guard = read.transaction.guard();
                    let result = change_authenticated_nonce_overlay_v1(
                        AuthenticatedNonceChangeV1 {
                            player,
                            public_key: "k",
                            expected,
                            after: Some(1),
                            create_player: create,
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live,
                    );
                    if case == 4 {
                        assert!(group_nodes > 0);
                        assert_eq!(calls.get(), group_nodes + 1);
                        assert!(cancel.get());
                    }
                    match (case, result) {
                        (
                            0,
                            Err(Error::Value(ValueTreeError::Invalid(
                                "authenticated nonce missing player",
                            ))),
                        )
                        | (
                            1 | 2,
                            Err(Error::Value(ValueTreeError::Invalid(
                                "authenticated nonce expected value",
                            ))),
                        )
                        | (
                            3,
                            Err(Error::Member(MemberRecordError::Value(
                                ValueTreeError::ResourceLimited(_),
                            ))),
                        ) => {}
                        (
                            4,
                            Err(Error::Member(MemberRecordError::Value(ValueTreeError::Cancelled))),
                        ) => {}
                        (case, result) => panic!("unexpected nonce failure {case}: {result:?}"),
                    }
                }
                let guard = transaction.guard();
                assert!(transaction.seal(&guard, &live, 1).is_err());
            }
            verified
                .record
                .after
                .state
                .iter()
                .find(|root| root.namespace == StateNamespaceV1::Authorization)
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
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            for newer in [false, true] {
                let root = StateRootV1::from_canonical_bytes(if newer { &next } else { &roots[0] })
                    .unwrap();
                assert_eq!(root.leaf_count, if newer { 13 } else { 9 });
                for (player, key, expected) in [
                    ("", "", Nonce::Value(0)),
                    (
                        "new:p",
                        "new:k",
                        if newer {
                            Nonce::Value(0)
                        } else {
                            Nonce::PlayerAbsent
                        },
                    ),
                    (
                        "empty-new",
                        "other",
                        if newer {
                            Nonce::KeyAbsent
                        } else {
                            Nonce::PlayerAbsent
                        },
                    ),
                    (
                        "empty-player",
                        "public",
                        if newer {
                            Nonce::Value(u64::MAX)
                        } else {
                            Nonce::KeyAbsent
                        },
                    ),
                    ("nobody", "k", Nonce::PlayerAbsent),
                ] {
                    let state = store.state_session(physical(), class).unwrap();
                    let values = store.value_session(physical());
                    let mut read = WorldFieldReadContextV1 {
                        root: &root,
                        state: &state,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class,
                            io: &mut logical(),
                        },
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 4096,
                        },
                        semantic_bytes: 1024,
                        record_bytes: 4096,
                        take_state_error: || {
                            state
                                .take_read_error()
                                .map(|_| ValueTreeError::Invalid("state page failure"))
                        },
                        cancelled: || false,
                    };
                    assert_eq!(
                        read_authenticated_nonce_v1(player, key, &mut read).unwrap(),
                        expected
                    );
                }
            }
        }
    }
}
