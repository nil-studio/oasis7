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
    WorldState, incremental_authorization_mapsets::*, incremental_field_member::*,
    incremental_field_read::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
#[path = "support/incremental_authorization_overlay.rs"]
mod overlay;
#[test]
fn authorization_mapset_identity_and_empty_groups_survive_cold_roots() {
    use AuthorizationMapSetFieldV1 as Field;
    use AuthorizationMembershipV1 as Membership;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let producer = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let mut source = WorldState::default();
        for field in Field::ALL {
            let map = match field {
                Field::DataAccessPermissions => &mut source.data_access_permissions,
                Field::ModuleReleaseRoleBindings => &mut source.module_release_role_bindings,
            };
            *map = [
                ("".into(), ["".into(), "权限\0".into()].into()),
                ("empty-group".into(), Default::default()),
                ("a:b".into(), ["c".into()].into()),
                ("a".into(), ["b:c".into()].into()),
            ]
            .into();
        }
        source
            .module_release_role_bindings
            .get_mut("")
            .unwrap()
            .insert("role-only".into());
        let roots = {
            let store = IncrementalPageStore::create(&path, producer.clone(), compression).unwrap();
            let mut values = store.value_session(physical());
            let _batch = producer.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
            let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(19);
            let mut remaining = 19;
            let summary = prepare_authorization_mapsets_genesis_v1(
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
                    assert!(rows.len() < 19);
                    assert_eq!(row.namespace, StateNamespaceV1::Authorization);
                    rows.push(row);
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(
                summary,
                AuthorizationMapSetsGenesisV1 {
                    groups: 8,
                    members: 9
                }
            );
            assert_eq!(remaining, 0);
            let field = FieldDescriptor {
                owner: FieldOwner::WorldState,
                name: "data_access_permissions",
                destination: FieldDestination::State(StateNamespaceV1::Authorization),
                shape: FieldShape::MapSet,
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
            // Independent binary preimage, not the production pair codec.
            let mut pair = Vec::from(&b"oasis7.iws.semantic-key.v1\0"[..]);
            pair.push(5);
            for text in ["a:b", "c"] {
                pair.push(1);
                pair.extend_from_slice(&(text.len() as u64).to_be_bytes());
                pair.extend_from_slice(text.as_bytes());
            }
            let entry = WorldFieldKeyV1::new(
                field,
                WorldFieldRecordKindV1::Entry,
                Some(*blake3::hash(&pair).as_bytes()),
            )
            .unwrap();
            assert!(rows.iter().any(|row| row.mutation.key == entry.as_bytes()));
            let bad = write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                &("a:b", "c"),
                MemberPayloadRefV1::U64(0),
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
                assert_eq!(delta.after.leaf_count, if variant == 1 { 18 } else { 19 });
                // Writing this 19-leaf fixture also authenticates existing CAS pages.
                // Keep a separate finite preparation allowance; cold readers retain
                // their original 4MiB memory and per-query physical IO budgets.
                let mut preparation_io = physical();
                preparation_io.reads_remaining = 1_000_000;
                let mut persistence = store.value_session(preparation_io);
                let result = persistence.put_state_pages(&delta, &producer);
                assert!(
                    result.is_ok(),
                    "state preparation: {result:?}, remaining: {:?}",
                    persistence.remaining_io()
                );
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
            for kind in Field::ALL {
                for (subject, member, expected) in [
                    (
                        "",
                        "role-only",
                        if kind == Field::ModuleReleaseRoleBindings {
                            Membership::Present
                        } else {
                            Membership::MemberAbsent
                        },
                    ),
                    ("", "", Membership::Present),
                    ("", "权限\0", Membership::Present),
                    ("empty-group", "", Membership::MemberAbsent),
                    ("missing", "", Membership::GroupAbsent),
                    ("a:b", "c", Membership::Present),
                    ("a", "b:c", Membership::Present),
                    ("a:b", "b:c", Membership::MemberAbsent),
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
                        read_authorization_membership_v1(kind, subject, member, &mut read).unwrap(),
                        expected
                    );
                    read.root = &wrong;
                    assert!(matches!(
                        read_authorization_membership_v1(kind, subject, member, &mut read),
                        Err(ValueTreeError::Invalid("field namespace"))
                    ));
                    read.root = &empty;
                    assert!(matches!(
                        read_authorization_membership_v1(kind, subject, member, &mut read),
                        Err(ValueTreeError::Invalid(
                            "authorization MapSet missing metadata"
                        ))
                    ));
                    if kind == Field::DataAccessPermissions && subject == "a:b" && member == "c" {
                        read.root = &orphan;
                        assert!(matches!(
                            read_authorization_membership_v1(kind, subject, member, &mut read),
                            Err(ValueTreeError::Invalid("authorization MapSet orphan key"))
                        ));
                        read.root = &wrong_codec;
                        assert!(matches!(
                            read_authorization_membership_v1(kind, subject, member, &mut read),
                            Err(ValueTreeError::Invalid("member expected profile"))
                        ));
                    }
                }
            }
        }
        overlay::prove_overlay(&path, compression, &roots);
    }
}

#[test]
fn authorization_mapset_genesis_stops_on_record_limit_and_cancellation() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let mut source = WorldState::default();
    source
        .data_access_permissions
        .insert("owner".into(), ["data".into()].into());
    let store_admission = memory.used();
    for (records, cancel_before, cancel_after_prefix, expected_prefix) in [
        (0, false, false, 0),
        (1, false, false, 1),
        (4, true, false, 0),
        (4, false, true, 1),
    ] {
        let cancelled = std::cell::Cell::new(cancel_before);
        let mut count = 0;
        let mut remaining = records;
        let mut values = store.value_session(physical());
        let result = prepare_authorization_mapsets_genesis_v1(
            &source,
            &mut MemberRecordWriteContext {
                sink: &mut values,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 0,
                    record_bytes: 4096,
                },
                cancelled: || cancelled.get(),
            },
            &mut remaining,
            &mut |_provisional| {
                count += 1;
                if cancel_after_prefix {
                    cancelled.set(true);
                }
                Ok(())
            },
        );
        assert_eq!(
            count, expected_prefix,
            "records={records}, cancel_before={cancel_before}, cancel_after_prefix={cancel_after_prefix}: {result:?}"
        );
        if cancel_before || cancel_after_prefix {
            assert!(matches!(
                result,
                Err(MemberRecordError::Value(ValueTreeError::Cancelled))
            ));
        } else {
            assert_eq!(remaining, 0);
            assert!(matches!(
                result,
                Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(
                    "authorization MapSet records"
                )))
            ));
        }
        // The provisional sink drops all rows; no StateRoot is ever constructed.
        drop(values);
        assert_eq!(memory.used(), store_admission);
    }
}
