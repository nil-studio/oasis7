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
    incremental_world_balances::*, main_token::MainTokenAccountBalance,
    reward_asset::NodeAssetBalance,
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
        field: WorldBalanceFieldV1,
        key: &str,
        max: u64,
        output: &mut impl Write,
        cancelled: impl FnMut() -> bool,
    ) -> Result<Option<WorldBalanceReadV1>, ValueTreeError> {
        let state = self
            .store
            .state_session(self.state_io.clone(), self.class)
            .unwrap();
        let values = self.store.value_session(physical());
        read_world_balance_v1(
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
                record_bytes: max.saturating_add(8192),
                take_state_error: || {
                    state.take_read_error().map(|e| match e {
                        PageStoreError::ResourceLimited(r) => ValueTreeError::ResourceLimited(r),
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
fn source() -> WorldState {
    WorldState {
        main_token_balances: [
            (
                "main-map".into(),
                MainTokenAccountBalance {
                    account_id: " 原\0é ".into(),
                    liquid_balance: 0,
                    vested_balance: u64::MAX,
                    restricted_starter_claim_balance: 17,
                },
            ),
            (
                "empty-id".into(),
                MainTokenAccountBalance {
                    account_id: "".into(),
                    liquid_balance: u64::MAX,
                    vested_balance: 0,
                    restricted_starter_claim_balance: 1,
                },
            ),
        ]
        .into(),
        node_asset_balances: [
            (
                "node-map".into(),
                NodeAssetBalance {
                    node_id: "账户".into(),
                    power_credit_balance: u64::MAX,
                    total_minted_credits: 17,
                    total_burned_credits: 0,
                },
            ),
            (
                "zero".into(),
                NodeAssetBalance {
                    node_id: "".into(),
                    power_credit_balance: 0,
                    total_minted_credits: 0,
                    total_burned_credits: 0,
                },
            ),
        ]
        .into(),
        ..WorldState::default()
    }
}
fn prepare(
    source: &WorldState,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    max: u64,
) -> Vec<ReservedValue<ExecutionWriteV1>> {
    let mut writer = store.value_session(physical());
    let capacity = source.main_token_balances.len() + source.node_asset_balances.len() + 2;
    let mut rows = Vec::with_capacity(capacity);
    let mut records = capacity as u64;
    let count = prepare_world_balances_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8192,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: max + 32,
                record_bytes: max + 8192,
            },
            cancelled: || false,
        },
        &mut records,
        &mut |row| {
            assert!(rows.len() < capacity);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(count, capacity as u64 - 2);
    assert_eq!(records, 0);
    rows
}
fn build(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    rows: &[ReservedValue<ExecutionWriteV1>],
    patch: Option<(&[u8], Option<[u8; 32]>)>,
) -> Vec<u8> {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let workspace = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let mut writes: Vec<_> = rows
        .iter()
        .filter(|r| !patch.is_some_and(|(key, value)| value.is_none() && r.mutation.key == key))
        .map(|r| StateWriteV1 {
            key: r.mutation.key.clone(),
            before: None,
            after: match patch {
                Some((key, Some(value))) if r.mutation.key == key => Some(value),
                _ => r.mutation.after,
            },
        })
        .collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = workspace.retain(
        StateDeltaV1::build(
            StateRootV1::empty(Ns::Business).unwrap(),
            writes,
            &state,
            2 << 20,
        )
        .unwrap(),
    );
    store
        .value_session(physical())
        .put_state_pages(&delta, memory)
        .unwrap();
    delta.after.canonical_bytes().unwrap()
}
struct CancelOutput<'a> {
    flag: &'a Cell<bool>,
    interrupted: bool,
    calls: usize,
}
impl Write for CancelOutput<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        self.flag.set(true);
        if self.interrupted {
            Err(io::ErrorKind::Interrupted.into())
        } else {
            Ok(1.min(b.len()))
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn original_account_and_node_balances_reopen_with_independent_inner_identities() {
    use WorldBalanceFieldV1 as F;
    use WorldBalanceReadV1 as V;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let roots = {
            let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let _source = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
            let source = source();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let _batch = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
            let rows = prepare(&source, &store, &memory, 128);
            let mut values = store.value_session(physical());
            let field = FieldDescriptor {
                owner: FieldOwner::WorldState,
                name: "main_token_balances",
                shape: FieldShape::Map,
                destination: FieldDestination::State(Ns::Business),
            };
            // Independent semantic token, literal tag9 and original payload order.
            let _outputs = memory.reserve(32 << 10, MemoryClass::Ordinary).unwrap();
            for (name, key, identity, amounts) in [
                (
                    "main_token_balances",
                    "main-map",
                    " 原\0é ",
                    [0, u64::MAX, 17],
                ),
                ("node_asset_balances", "node-map", "账户", [u64::MAX, 17, 0]),
            ] {
                let descriptor = FieldDescriptor { name, ..field };
                let mut semantic = Vec::from(&b"oasis7.iws.semantic-key.v1\0"[..]);
                semantic.push(1);
                semantic.extend_from_slice(&(key.len() as u64).to_be_bytes());
                semantic.extend_from_slice(key.as_bytes());
                let expected = WorldFieldKeyV1::new(
                    descriptor,
                    WorldFieldRecordKindV1::Entry,
                    Some(*blake3::hash(&semantic).as_bytes()),
                )
                .unwrap();
                let row = rows
                    .iter()
                    .find(|r| r.mutation.key == expected.as_bytes())
                    .unwrap();
                assert_eq!(row.namespace, Ns::Business);
                let mut raw = Vec::with_capacity(8192);
                let mut read = ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                };
                let root = read.load_root(&row.mutation.after.unwrap()).unwrap();
                read.read_range(&root, 0, root.0.logical_len, &mut raw, || false)
                    .unwrap();
                let domain = b"oasis7.iws.field-member.v1\0";
                assert!(raw.starts_with(domain));
                assert_eq!(raw[domain.len() + 2], 9);
                let mut payload = Vec::with_capacity(256);
                payload.extend_from_slice(&(identity.len() as u64).to_be_bytes());
                payload.extend_from_slice(identity.as_bytes());
                for amount in amounts {
                    payload.extend_from_slice(&amount.to_be_bytes());
                }
                assert_eq!(&raw[raw.len() - payload.len()..], payload);
            }
            let mut semantic = Vec::from(&b"oasis7.iws.semantic-key.v1\0"[..]);
            semantic.push(1);
            semantic.extend_from_slice(&8u64.to_be_bytes());
            semantic.extend_from_slice(b"main-map");
            let key = WorldFieldKeyV1::new(
                field,
                WorldFieldRecordKindV1::Entry,
                Some(*blake3::hash(&semantic).as_bytes()),
            )
            .unwrap();
            let good = rows
                .iter()
                .find(|r| r.mutation.key == key.as_bytes())
                .unwrap()
                .mutation
                .after
                .unwrap();
            let wrong = write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                "main-map",
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
            let identity = " 原\0é ";
            let payload_start = raw.len() - 32 - identity.len();
            let domain = b"oasis7.iws.field-member.v1\0";
            let malformed = (0..7)
                .map(|case| {
                    let mut bytes = raw.clone();
                    match case {
                        0 => bytes[payload_start..payload_start + 8]
                            .copy_from_slice(&((identity.len() + 1) as u64).to_be_bytes()),
                        1 => bytes[payload_start + 8 + identity.len() - 1] = 0xff,
                        2 => bytes[payload_start + 8 + identity.len() - 1] = 0xe2,
                        3 => {
                            bytes.push(0);
                            bytes[domain.len() + 11..domain.len() + 19]
                                .copy_from_slice(&((33 + identity.len()) as u64).to_be_bytes());
                        }
                        4 => {
                            bytes.pop();
                            bytes[domain.len() + 11..domain.len() + 19]
                                .copy_from_slice(&((31 + identity.len()) as u64).to_be_bytes());
                        }
                        5 => bytes[payload_start..payload_start + 8]
                            .copy_from_slice(&u64::MAX.to_be_bytes()),
                        _ => {
                            bytes.truncate(payload_start + 31);
                            bytes[domain.len() + 11..domain.len() + 19]
                                .copy_from_slice(&31u64.to_be_bytes());
                        }
                    };
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
                })
                .collect::<Vec<_>>();
            let metadata =
                WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None).unwrap();
            let mut roots = Vec::with_capacity(10);
            roots.push(build(&store, &memory, &rows, None));
            roots.push(build(
                &store,
                &memory,
                &rows,
                Some((key.as_bytes(), Some(wrong.value_root))),
            ));
            for root in malformed {
                roots.push(build(
                    &store,
                    &memory,
                    &rows,
                    Some((key.as_bytes(), Some(root))),
                ));
            }
            roots.push(build(
                &store,
                &memory,
                &rows,
                Some((metadata.as_bytes(), None)),
            ));
            roots
        };
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
            let _outputs = memory.reserve(16 << 10, class).unwrap();
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
            for (field, key, expected, identity) in [
                (
                    F::MainToken,
                    "main-map",
                    Some(V::MainToken {
                        identity_utf8_bytes: " 原\0é ".len() as u64,
                        liquid_balance: 0,
                        vested_balance: u64::MAX,
                        restricted_starter_claim_balance: 17,
                    }),
                    " 原\0é ",
                ),
                (
                    F::MainToken,
                    "empty-id",
                    Some(V::MainToken {
                        identity_utf8_bytes: 0,
                        liquid_balance: u64::MAX,
                        vested_balance: 0,
                        restricted_starter_claim_balance: 1,
                    }),
                    "",
                ),
                (
                    F::NodeAsset,
                    "node-map",
                    Some(V::NodeAsset {
                        identity_utf8_bytes: 6,
                        power_credit_balance: u64::MAX,
                        total_minted_credits: 17,
                        total_burned_credits: 0,
                    }),
                    "账户",
                ),
                (
                    F::NodeAsset,
                    "zero",
                    Some(V::NodeAsset {
                        identity_utf8_bytes: 0,
                        power_credit_balance: 0,
                        total_minted_credits: 0,
                        total_burned_credits: 0,
                    }),
                    "",
                ),
                (F::MainToken, "absent", None, ""),
            ] {
                let mut output = Vec::with_capacity(128);
                assert_eq!(
                    reader
                        .read(&root, field, key, 128, &mut output, || false)
                        .unwrap(),
                    expected
                );
                assert_eq!(output, identity.as_bytes());
            }
            assert!(
                reader
                    .read(&root, F::NodeAsset, "zero", 0, &mut io::sink(), || false)
                    .unwrap()
                    .is_some()
            );
            let mut output = Vec::with_capacity(128);
            assert!(matches!(
                reader.read(&root, F::MainToken, "main-map", 0, &mut output, || false),
                Err(ValueTreeError::ResourceLimited(_))
            ));
            assert!(output.is_empty());
            for (index, bytes) in roots[1..].iter().enumerate() {
                let bad = StateRootV1::from_canonical_bytes(bytes).unwrap();
                let result =
                    reader.read(&bad, F::MainToken, "main-map", 128, &mut io::sink(), || {
                        false
                    });
                if index == 6 {
                    assert!(matches!(result, Err(ValueTreeError::ResourceLimited(_))));
                } else {
                    assert!(
                        matches!(result, Err(ValueTreeError::Invalid(_))),
                        "bad variant {index}: {result:?}"
                    );
                }
            }
            assert!(matches!(
                reader.read(
                    &StateRootV1::empty(Ns::Authorization).unwrap(),
                    F::MainToken,
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
                limited.read(&root, F::MainToken, "absent", 128, &mut io::sink(), || {
                    false
                }),
                Err(ValueTreeError::ResourceLimited(_))
            ));
            for (field, key) in [(F::MainToken, "main-map"), (F::NodeAsset, "node-map")] {
                for interrupted in [false, true] {
                    let flag = Cell::new(false);
                    let mut output = CancelOutput {
                        flag: &flag,
                        interrupted,
                        calls: 0,
                    };
                    assert!(matches!(
                        reader.read(&root, field, key, 128, &mut output, || flag.get()),
                        Err(ValueTreeError::Cancelled)
                    ));
                    assert_eq!(output.calls, 1);
                }
            }
        }
    }
}

#[test]
fn balance_genesis_empty_fields_and_all_failure_prefixes_are_provisional() {
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
    let source = source();
    for case in 0..11 {
        let mut values = store.value_session(physical());
        let cancelled = Cell::new(case == 3);
        let mut count = 0;
        let mut filler = None;
        let mut records = match case {
            1 => 0,
            2 => 1,
            _ => 6,
        };
        let mut io = logical();
        if case == 8 {
            io.nodes_remaining = 0;
        }
        let result = prepare_world_balances_genesis_v1(
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
                    payload_bytes: if case == 6 { 31 } else { 160 },
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
                    return Err(ValueTreeError::Invalid("injected balance sink"));
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
                assert_eq!(count, 2);
            }
            1 | 2 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(
                        "world balance records"
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
                        "injected balance sink"
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
        drop(filler); // No StateRoot is built from any failed prefix.
    }
}
struct HashOutput {
    hash: blake3::Hasher,
    bytes: u64,
}
impl Write for HashOutput {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.hash.update(b);
        self.bytes += b.len() as u64;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn large_inner_identity_streams_without_allocating_a_complete_read_buffer() {
    let length = 65usize << 20;
    let block = [b'x'; 8192];
    let mut expected = blake3::Hasher::new();
    for _ in 0..length / block.len() {
        expected.update(&block);
    }
    let expected = expected.finalize();
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let root = {
            let memory = MemoryBudget::new(128 << 20, 1 << 20).unwrap();
            let _source = memory
                .reserve(length + (1 << 20), MemoryClass::Ordinary)
                .unwrap();
            let source = WorldState {
                main_token_balances: [(
                    "large".into(),
                    MainTokenAccountBalance {
                        account_id: "x".repeat(length),
                        liquid_balance: u64::MAX,
                        vested_balance: 0,
                        restricted_starter_claim_balance: 17,
                    },
                )]
                .into(),
                ..WorldState::default()
            };
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let _batch = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
            let rows = prepare(&source, &store, &memory, length as u64);
            assert_eq!(rows.len(), 3);
            build(&store, &memory, &rows, None)
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
            if class == MemoryClass::Recovery {
                assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
            }
            let reader = Reader {
                store: &store,
                memory: &memory,
                class,
                state_io: physical(),
            };
            let root = StateRootV1::from_canonical_bytes(&root).unwrap();
            let mut output = HashOutput {
                hash: blake3::Hasher::new(),
                bytes: 0,
            };
            assert_eq!(
                reader
                    .read(
                        &root,
                        WorldBalanceFieldV1::MainToken,
                        "large",
                        length as u64,
                        &mut output,
                        || false
                    )
                    .unwrap(),
                Some(WorldBalanceReadV1::MainToken {
                    identity_utf8_bytes: length as u64,
                    liquid_balance: u64::MAX,
                    vested_balance: 0,
                    restricted_starter_claim_balance: 17
                })
            );
            assert_eq!(output.bytes, length as u64);
            assert_eq!(output.hash.finalize(), expected);
        }
    }
}

#[path = "support/incremental_world_balance_overlay.rs"]
mod overlay;
