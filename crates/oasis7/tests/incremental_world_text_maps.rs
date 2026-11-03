#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError, write_value},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_field_member::*, incremental_field_read::*,
    incremental_world_text_maps::*,
};
use std::{
    cell::Cell,
    io::{self, Write},
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
struct Reader<'a> {
    store: &'a IncrementalPageStore,
    memory: &'a MemoryBudget,
    class: MemoryClass,
    state_io: PageIoBudget,
}
impl Reader<'_> {
    fn read(
        &self,
        root: &StateRootV1,
        field: WorldTextMapFieldV1,
        key: &str,
        max: u64,
        output: &mut impl Write,
        cancelled: impl FnMut() -> bool,
    ) -> Result<Option<u64>, ValueTreeError> {
        let state = self
            .store
            .state_session(self.state_io.clone(), self.class)
            .unwrap();
        let values = self.store.value_session(physical());
        read_world_text_map_v1(
            field,
            key,
            &mut WorldFieldReadContextV1 {
                root,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: self.memory,
                    class: self.class,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || {
                    state.take_read_error().map(|error| match error {
                        PageStoreError::ResourceLimited(reason) => {
                            ValueTreeError::ResourceLimited(reason)
                        }
                        _ => ValueTreeError::Invalid("state page failure"),
                    })
                },
                cancelled,
            },
            max,
            output,
        )
    }
}
struct CancelOutput<'a> {
    cancelled: &'a Cell<bool>,
    interrupted: bool,
    calls: usize,
}
impl Write for CancelOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        self.cancelled.set(true);
        if self.interrupted {
            Err(io::ErrorKind::Interrupted.into())
        } else {
            Ok(1.min(bytes.len()))
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn original_text_maps_authenticate_utf8_empty_values_and_cold_namespaces() {
    use WorldTextMapFieldV1 as Field;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let producer = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let source_admission = producer.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        let source = WorldState {
            module_artifact_owners: [
                ("".into(), "".into()),
                ("artifact".into(), " Agent\0é ".into()),
            ]
            .into(),
            node_identity_bindings: [("n".into(), "Key-A".into()), ("empty".into(), "".into())]
                .into(),
            node_main_token_account_bindings: [
                ("n".into(), "acct".into()),
                ("unicode".into(), "账户".into()),
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
            assert_eq!(
                prepare_world_text_maps_genesis_v1(
                    &source,
                    &mut MemberRecordWriteContext {
                        sink: &mut values,
                        memory: &producer,
                        io: &mut logical(),
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 8192
                        },
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 128,
                            record_bytes: 8192
                        },
                        cancelled: || false
                    },
                    &mut remaining,
                    &mut |row| {
                        assert!(rows.len() < 9);
                        rows.push(row);
                        Ok(())
                    }
                )
                .unwrap(),
                6
            );
            assert_eq!(remaining, 0);
            // Literal descriptor/key/value oracle, separate from the adapter enum.
            for (name, namespace, key, text) in [
                (
                    "module_artifact_owners",
                    Ns::Business,
                    "artifact",
                    " Agent\0é ",
                ),
                ("node_identity_bindings", Ns::Authorization, "n", "Key-A"),
                (
                    "node_main_token_account_bindings",
                    Ns::Authorization,
                    "n",
                    "acct",
                ),
            ] {
                let field = FieldDescriptor {
                    owner: FieldOwner::WorldState,
                    name,
                    shape: FieldShape::Map,
                    destination: FieldDestination::State(namespace),
                };
                let mut semantic = Vec::from(&b"oasis7.iws.semantic-key.v1\0"[..]);
                semantic.push(1);
                semantic.extend_from_slice(&(key.len() as u64).to_be_bytes());
                semantic.extend_from_slice(key.as_bytes());
                let expected = WorldFieldKeyV1::new(
                    field,
                    WorldFieldRecordKindV1::Entry,
                    Some(*blake3::hash(&semantic).as_bytes()),
                )
                .unwrap();
                let row = rows
                    .iter()
                    .find(|row| row.mutation.key == expected.as_bytes())
                    .unwrap();
                assert_eq!(row.namespace, namespace);
                let _output = producer.reserve(4096, MemoryClass::Ordinary).unwrap();
                let mut payload = Vec::with_capacity(128);
                read_member_record_profile_v1(
                    &row.mutation.after.unwrap(),
                    &expected,
                    &mut ValueReadContext {
                        source: &values,
                        memory: &producer,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    MemberReadProfileV1 {
                        codec: MemberPayloadCodecV1::Text,
                        semantic_bytes: semantic.len() as u64,
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 128,
                            record_bytes: 8192,
                        },
                    },
                    &mut io::sink(),
                    &mut payload,
                    || false,
                )
                .unwrap();
                assert_eq!(payload, text.as_bytes());
            }
            let descriptor = FieldDescriptor {
                owner: FieldOwner::WorldState,
                name: "node_identity_bindings",
                shape: FieldShape::Map,
                destination: FieldDestination::State(Ns::Authorization),
            };
            let bad = write_member_record_v1(
                descriptor,
                WorldFieldRecordKindV1::Entry,
                "n",
                MemberPayloadRefV1::U64(7),
                &mut MemberRecordWriteContext {
                    sink: &mut values,
                    memory: &producer,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 128,
                        record_bytes: 8192,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            let good = rows
                .iter()
                .find(|row| row.mutation.key == bad.field_key.as_bytes())
                .unwrap()
                .mutation
                .after
                .unwrap();
            let _raw = producer.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
            let mut raw = Vec::with_capacity(8192);
            {
                let mut read = ValueReadContext {
                    source: &values,
                    memory: &producer,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                };
                let root = read.load_root(&good).unwrap();
                read.read_range(&root, 0, root.0.logical_len, &mut raw, || false)
                    .unwrap();
            }
            let malformed = [0xff, 0xe2].map(|last| {
                let mut bytes = raw.clone();
                *bytes.last_mut().unwrap() = last;
                write_value(
                    &mut &bytes[..],
                    bytes.len() as u64,
                    &mut values,
                    &producer,
                    &mut logical(),
                    || false,
                )
                .unwrap()
                .digest()
                .unwrap()
            });
            [
                (Ns::Business, 0),
                (Ns::Authorization, 0),
                (Ns::Authorization, 1),
                (Ns::Authorization, 2),
                (Ns::Authorization, 3),
            ]
            .map(|(namespace, variant)| {
                let state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let workspace = producer.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
                let mut writes: Vec<_> = rows
                    .iter()
                    .filter(|row| row.namespace == namespace)
                    .map(|row| StateWriteV1 {
                        key: row.mutation.key.clone(),
                        before: None,
                        after: if variant != 0 && row.mutation.key == bad.field_key.as_bytes() {
                            Some(match variant {
                                1 => bad.value_root,
                                2 => malformed[0],
                                _ => malformed[1],
                            })
                        } else {
                            row.mutation.after
                        },
                    })
                    .collect();
                writes.sort_by(|a, b| a.key.cmp(&b.key));
                let delta = workspace.retain(
                    StateDeltaV1::build(
                        StateRootV1::empty(namespace).unwrap(),
                        writes,
                        &state,
                        2 << 20,
                    )
                    .unwrap(),
                );
                assert_eq!(
                    delta.after.leaf_count,
                    if namespace == Ns::Business { 3 } else { 6 }
                );
                store
                    .value_session(physical())
                    .put_state_pages(&delta, &producer)
                    .unwrap();
                delta.after.canonical_bytes().unwrap()
            })
        };
        drop(source);
        drop(source_admission);
        assert_eq!(producer.used(), 0);
        overlay::exercise(&path, compression, &roots);
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
            let reader = Reader {
                store: &store,
                memory: &memory,
                class,
                state_io: physical(),
            };
            for (field, key, text) in [
                (Field::ModuleArtifactOwners, "", Some("")),
                (Field::ModuleArtifactOwners, "artifact", Some(" Agent\0é ")),
                (Field::NodeIdentityBindings, "n", Some("Key-A")),
                (Field::NodeIdentityBindings, "empty", Some("")),
                (Field::NodeMainTokenAccountBindings, "n", Some("acct")),
                (Field::NodeMainTokenAccountBindings, "unicode", Some("账户")),
                (Field::NodeIdentityBindings, "missing", None),
            ] {
                let index = usize::from(field != Field::ModuleArtifactOwners);
                let root = StateRootV1::from_canonical_bytes(&roots[index]).unwrap();
                let _output = memory.reserve(1024, class).unwrap();
                let mut output = Vec::with_capacity(128);
                assert_eq!(
                    reader
                        .read(&root, field, key, 128, &mut output, || false)
                        .unwrap(),
                    text.map(|s| s.len() as u64)
                );
                assert_eq!(output, text.unwrap_or("").as_bytes());
                if text.is_some_and(|s| !s.is_empty()) {
                    output.clear();
                    assert!(matches!(
                        reader.read(&root, field, key, 0, &mut output, || false),
                        Err(ValueTreeError::ResourceLimited("member record lengths"))
                    ));
                    assert!(output.is_empty());
                }
            }
            let field = Field::NodeIdentityBindings;
            let good = StateRootV1::from_canonical_bytes(&roots[1]).unwrap();
            for (index, reason) in [
                (2, "member expected profile"),
                (3, "world text map UTF-8"),
                (4, "world text map UTF-8"),
            ] {
                let root = StateRootV1::from_canonical_bytes(&roots[index]).unwrap();
                assert!(
                    matches!(reader.read(&root,field,"n",128,&mut io::sink(),||false),Err(ValueTreeError::Invalid(actual)) if actual==reason)
                );
            }
            assert!(matches!(
                reader.read(
                    &StateRootV1::empty(Ns::Authorization).unwrap(),
                    field,
                    "n",
                    128,
                    &mut io::sink(),
                    || false
                ),
                Err(ValueTreeError::Invalid("world text map missing metadata"))
            ));
            assert!(matches!(
                reader.read(
                    &StateRootV1::empty(Ns::Business).unwrap(),
                    field,
                    "n",
                    128,
                    &mut io::sink(),
                    || false
                ),
                Err(ValueTreeError::Invalid("field namespace"))
            ));
            for (cancel_field, cancel_key) in [
                (field, "n"),
                (Field::NodeMainTokenAccountBindings, "unicode"),
            ] {
                for interrupted in [true, false] {
                    let cancel = Cell::new(false);
                    let mut output = CancelOutput {
                        cancelled: &cancel,
                        interrupted,
                        calls: 0,
                    };
                    assert!(matches!(
                        reader.read(&good, cancel_field, cancel_key, 128, &mut output, || cancel
                            .get()),
                        Err(ValueTreeError::Cancelled)
                    ));
                    assert_eq!(output.calls, 1);
                }
            }
            let mut none = physical();
            none.reads_remaining = 0;
            let limited = Reader {
                store: &store,
                memory: &memory,
                class,
                state_io: none,
            };
            assert!(matches!(
                limited.read(&good, field, "n", 128, &mut io::sink(), || false),
                Err(ValueTreeError::ResourceLimited(_))
            ));
        }
    }
}

#[test]
fn text_map_empty_fields_and_provisional_genesis_failures_are_distinct() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let _source = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let empty = WorldState::default();
    let mut source = WorldState::default();
    source
        .module_artifact_owners
        .insert("artifact".into(), "owner".into());
    for case in 0..6 {
        let mut values = store.value_session(physical());
        let cancel = Cell::new(case == 3);
        let mut count = 0;
        let mut records = if case == 1 {
            0
        } else if case == 2 {
            1
        } else {
            4
        };
        let result = prepare_world_text_maps_genesis_v1(
            if case == 0 { &empty } else { &source },
            &mut MemberRecordWriteContext {
                sink: &mut values,
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
                cancelled: || cancel.get(),
            },
            &mut records,
            &mut |_provisional| {
                count += 1;
                if case == 4 {
                    cancel.set(true);
                }
                if case == 5 {
                    return Err(ValueTreeError::Invalid("injected text sink"));
                }
                Ok(())
            },
        );
        match case {
            0 => {
                assert_eq!(result.unwrap(), 0);
                assert_eq!(count, 3);
            }
            1 | 2 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(
                        "world text map records"
                    )))
                ));
                assert_eq!(count, usize::from(case == 2));
            }
            3 | 4 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::Cancelled))
                ));
                assert_eq!(count, usize::from(case == 4));
            }
            _ => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::Invalid(
                        "injected text sink"
                    )))
                ));
                assert_eq!(count, 1);
            }
        }
        // Sink drops its charged provisional rows. No StateRoot is constructed.
    }
}

#[path = "support/incremental_world_text_overlay.rs"]
mod overlay;
