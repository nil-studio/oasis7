#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_agent_api::world_action::ModuleInstallTarget as Target;
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
    incremental_installed_module_targets::*,
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
        key: &str,
        max: u64,
        output: &mut impl Write,
        cancelled: impl FnMut() -> bool,
    ) -> Result<Option<InstalledModuleTargetReadV1>, ValueTreeError> {
        let state = self
            .store
            .state_session(self.state_io.clone(), self.class)
            .unwrap();
        let values = self.store.value_session(physical());
        read_installed_module_target_v1(
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
                    state.take_read_error().map(|e| match e {
                        PageStoreError::ResourceLimited(reason) => {
                            ValueTreeError::ResourceLimited(reason)
                        }
                        PageStoreError::Cancelled => ValueTreeError::Cancelled,
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
    flag: &'a Cell<bool>,
    interrupted: bool,
    calls: usize,
}
impl Write for CancelOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        self.flag.set(true);
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
fn original_installed_targets_reopen_with_distinct_variants_and_utf8() {
    use InstalledModuleTargetReadV1 as Read;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let roots = {
            let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let _source = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
            let source = WorldState {
                installed_module_targets: [
                    ("".into(), Target::SelfAgent),
                    ("self".into(), Target::SelfAgent),
                    (
                        "empty".into(),
                        Target::LocationInfrastructure {
                            location_id: "".into(),
                        },
                    ),
                    (
                        "loc".into(),
                        Target::LocationInfrastructure {
                            location_id: " 地点\0é ".into(),
                        },
                    ),
                    (
                        "literal".into(),
                        Target::LocationInfrastructure {
                            location_id: "SelfAgent".into(),
                        },
                    ),
                    (
                        "ascii".into(),
                        Target::LocationInfrastructure {
                            location_id: "A".into(),
                        },
                    ),
                ]
                .into(),
                ..WorldState::default()
            };
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let mut values = store.value_session(physical());
            let _batch = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
            let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(7);
            let mut records = 7;
            assert_eq!(
                prepare_installed_module_targets_genesis_v1(
                    &source,
                    &mut MemberRecordWriteContext {
                        sink: &mut values,
                        memory: &memory,
                        io: &mut logical(),
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 8192
                        },
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 128,
                            record_bytes: 8192
                        },
                        cancelled: || false,
                    },
                    &mut records,
                    &mut |row| {
                        assert!(rows.len() < 7);
                        rows.push(row);
                        Ok(())
                    }
                )
                .unwrap(),
                6
            );
            assert_eq!(records, 0);
            // Independent literal descriptor, semantic token and payload oracle.
            let field = FieldDescriptor {
                owner: FieldOwner::WorldState,
                name: "installed_module_targets",
                shape: FieldShape::Map,
                destination: FieldDestination::State(Ns::Artifacts),
            };
            for (key, codec, payload) in [
                ("self", MemberPayloadCodecV1::Unit, ""),
                ("empty", MemberPayloadCodecV1::Text, ""),
                ("literal", MemberPayloadCodecV1::Text, "SelfAgent"),
                ("loc", MemberPayloadCodecV1::Text, " 地点\0é "),
            ] {
                let _output = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
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
                    .find(|r| r.mutation.key == expected.as_bytes())
                    .unwrap();
                assert_eq!(row.namespace, Ns::Artifacts);
                let mut output = Vec::with_capacity(128);
                read_member_record_profile_v1(
                    &row.mutation.after.unwrap(),
                    &expected,
                    &mut ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    MemberReadProfileV1 {
                        codec,
                        semantic_bytes: semantic.len() as u64,
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 128,
                            record_bytes: 8192,
                        },
                    },
                    &mut io::sink(),
                    &mut output,
                    || false,
                )
                .unwrap();
                assert_eq!(output, payload.as_bytes());
                let mut raw = Vec::with_capacity(4096);
                let mut raw_read = ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                };
                let root = raw_read.load_root(&row.mutation.after.unwrap()).unwrap();
                raw_read
                    .read_range(&root, 0, root.0.logical_len, &mut raw, || false)
                    .unwrap();
                let domain = b"oasis7.iws.field-member.v1\0";
                assert!(raw.starts_with(domain));
                assert_eq!(
                    raw[domain.len() + 2],
                    if codec == MemberPayloadCodecV1::Unit {
                        1
                    } else {
                        5
                    }
                );
            }
            let bad = write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                "ascii",
                MemberPayloadRefV1::U64(7),
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
                    cancelled: || false,
                },
            )
            .unwrap();
            let good = rows
                .iter()
                .find(|r| r.mutation.key == bad.field_key.as_bytes())
                .unwrap()
                .mutation
                .after
                .unwrap();
            let _raw = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
            let mut raw = Vec::with_capacity(8192);
            {
                let mut read = ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                };
                let root = read.load_root(&good).unwrap();
                read.read_range(&root, 0, root.0.logical_len, &mut raw, || false)
                    .unwrap();
            }
            let malformed = [0xff, 0xe2].map(|byte| {
                let mut bytes = raw.clone();
                *bytes.last_mut().unwrap() = byte;
                write_value(
                    &mut &bytes[..],
                    bytes.len() as u64,
                    &mut values,
                    &memory,
                    &mut logical(),
                    || false,
                )
                .unwrap()
                .digest()
                .unwrap()
            });
            let metadata =
                WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None).unwrap();
            (0..5)
                .map(|variant| {
                    let state = store
                        .state_session(physical(), MemoryClass::Ordinary)
                        .unwrap();
                    let workspace = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
                    let mut writes: Vec<_> = rows
                        .iter()
                        .filter(|r| variant != 4 || r.mutation.key != metadata.as_bytes())
                        .map(|r| StateWriteV1 {
                            key: r.mutation.key.clone(),
                            before: None,
                            after: if variant > 0
                                && variant < 4
                                && r.mutation.key == bad.field_key.as_bytes()
                            {
                                Some(match variant {
                                    1 => bad.value_root,
                                    2 => malformed[0],
                                    _ => malformed[1],
                                })
                            } else {
                                r.mutation.after
                            },
                        })
                        .collect();
                    writes.sort_by(|a, b| a.key.cmp(&b.key));
                    let delta = workspace.retain(
                        StateDeltaV1::build(
                            StateRootV1::empty(Ns::Artifacts).unwrap(),
                            writes,
                            &state,
                            2 << 20,
                        )
                        .unwrap(),
                    );
                    assert_eq!(delta.after.leaf_count, if variant == 4 { 6 } else { 7 });
                    store
                        .value_session(physical())
                        .put_state_pages(&delta, &memory)
                        .unwrap();
                    delta.after.canonical_bytes().unwrap()
                })
                .collect::<Vec<_>>()
        };
        // The source, original store and complete preparation pool are closed.
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
            let _outputs = memory.reserve(8192, class).unwrap();
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
            let root = StateRootV1::from_canonical_bytes(&roots[0]).unwrap();
            for (key, expected, payload) in [
                ("", Some(Read::SelfAgent), ""),
                ("self", Some(Read::SelfAgent), ""),
                (
                    "empty",
                    Some(Read::LocationInfrastructure { utf8_bytes: 0 }),
                    "",
                ),
                (
                    "loc",
                    Some(Read::LocationInfrastructure {
                        utf8_bytes: " 地点\0é ".len() as u64,
                    }),
                    " 地点\0é ",
                ),
                (
                    "literal",
                    Some(Read::LocationInfrastructure { utf8_bytes: 9 }),
                    "SelfAgent",
                ),
                ("absent", None, ""),
            ] {
                let mut output = Vec::with_capacity(128);
                assert_eq!(
                    reader.read(&root, key, 128, &mut output, || false).unwrap(),
                    expected
                );
                assert_eq!(output, payload.as_bytes());
            }
            assert_eq!(
                reader
                    .read(&root, "self", 0, &mut io::sink(), || false)
                    .unwrap(),
                Some(Read::SelfAgent)
            );
            assert_eq!(
                reader
                    .read(&root, "empty", 0, &mut io::sink(), || false)
                    .unwrap(),
                Some(Read::LocationInfrastructure { utf8_bytes: 0 })
            );
            assert!(matches!(
                reader.read(&root, "loc", 0, &mut io::sink(), || false),
                Err(ValueTreeError::ResourceLimited(_))
            ));
            for bytes in &roots[1..] {
                let bad = StateRootV1::from_canonical_bytes(bytes).unwrap();
                assert!(matches!(
                    reader.read(&bad, "ascii", 128, &mut io::sink(), || false),
                    Err(ValueTreeError::Invalid(_))
                ));
            }
            assert!(matches!(
                reader.read(
                    &StateRootV1::empty(Ns::Business).unwrap(),
                    "absent",
                    128,
                    &mut io::sink(),
                    || false
                ),
                Err(ValueTreeError::Invalid(_))
            ));
            let limited = Reader {
                state_io: PageIoBudget {
                    reads_remaining: 0,
                    ..physical()
                },
                ..reader
            };
            assert!(matches!(
                limited.read(&root, "absent", 128, &mut io::sink(), || false),
                Err(ValueTreeError::ResourceLimited(_))
            ));
            for interrupted in [false, true] {
                let flag = Cell::new(false);
                let mut output = CancelOutput {
                    flag: &flag,
                    interrupted,
                    calls: 0,
                };
                assert!(matches!(
                    reader.read(&root, "loc", 128, &mut output, || flag.get()),
                    Err(ValueTreeError::Cancelled)
                ));
                assert_eq!(output.calls, 1);
            }
        }
    }
}

#[test]
fn installed_target_genesis_requires_complete_records_and_never_publishes_prefixes() {
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
    let source = WorldState {
        installed_module_targets: [(
            "module".into(),
            Target::LocationInfrastructure {
                location_id: "location".into(),
            },
        )]
        .into(),
        ..WorldState::default()
    };
    for case in 0..11 {
        let mut values = store.value_session(physical());
        let cancelled = Cell::new(case == 3);
        let mut count = 0;
        let mut filler = None;
        let mut records = match case {
            1 => 0,
            2 => 1,
            _ => 2,
        };
        let mut io = logical();
        if case == 8 {
            io.nodes_remaining = 0;
        }
        let result = prepare_installed_module_targets_genesis_v1(
            if case == 0 { &empty } else { &source },
            &mut MemberRecordWriteContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io,
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: if case == 7 { 0 } else { 8192 },
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: if case == 9 { 0 } else { 1024 },
                    payload_bytes: if case == 6 { 0 } else { 128 },
                    record_bytes: 8192,
                },
                cancelled: || cancelled.get(),
            },
            &mut records,
            &mut |_row| {
                count += 1;
                if case == 4 {
                    cancelled.set(true);
                }
                if case == 5 {
                    return Err(ValueTreeError::Invalid("injected installed target sink"));
                }
                if case == 10 {
                    filler = Some(
                        memory
                            .reserve((31 << 20) - memory.used(), MemoryClass::Ordinary)
                            .unwrap(),
                    );
                }
                Ok(())
            },
        );
        match case {
            0 => {
                assert_eq!(result.unwrap(), 0);
                assert_eq!(count, 1);
            }
            1 | 2 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(
                        "installed targets records"
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
            5 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::Invalid(
                        "injected installed target sink"
                    )))
                ));
                assert_eq!(count, 1);
            }
            6 | 9 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(_)))
                ));
                assert_eq!(count, 1);
            }
            7 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Key(SemanticKeyError::ResourceLimited))
                ));
                assert_eq!(count, 1);
            }
            8 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(_)))
                ));
                assert_eq!(count, 0);
            }
            _ => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::Memory(_)))
                        | Err(MemberRecordError::Key(SemanticKeyError::Memory(_)))
                ));
                assert_eq!(count, 1);
            }
        }
        // No root is constructed. Sink rows and retained charges are dropped.
        drop(filler);
    }
}

#[path = "support/incremental_installed_target_overlay.rs"]
mod overlay;
