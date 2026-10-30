#![cfg(all(unix, not(target_arch = "wasm32")))]

use oasis7_agent_api::world_action::ResourceKind;
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::incremental_state::{
    STATE_ROOT_BYTES, StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
};
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueReadContext};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    WorldState, incremental_field_member::*, incremental_field_presence::*,
    incremental_inventory_genesis::*, types::MaterialLedgerId,
};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oasis7-inventory-genesis-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
type WriteBatch = Vec<ReservedValue<StateWriteV1>>;
struct Writes {
    rows: ReservedValue<RefCell<WriteBatch>>,
    fail_after: usize,
    cancel: Option<Rc<Cell<bool>>>,
}
impl Writes {
    fn new(memory: &MemoryBudget) -> Self {
        Self {
            rows: memory
                .reserve(64 << 10, MemoryClass::Ordinary)
                .unwrap()
                .retain(RefCell::new(Vec::with_capacity(64))),
            fail_after: usize::MAX,
            cancel: None,
        }
    }
}
impl InventoryWriteSink for Writes {
    fn put_write(
        &mut self,
        write: ReservedValue<StateWriteV1>,
    ) -> Result<(), oasis7_formal_protocol::incremental_value::ValueTreeError> {
        let mut rows = self.rows.borrow_mut();
        assert!(rows.len() < 64);
        if rows.len() == self.fail_after {
            return Err(std::io::Error::other("injected sink failure").into());
        }
        rows.push(write);
        if let Some(flag) = &self.cancel {
            flag.set(true);
        }
        Ok(())
    }
}
fn source() -> WorldState {
    WorldState {
        time: 777,
        resources: BTreeMap::from([
            (ResourceKind::Electricity, i64::MIN),
            (ResourceKind::Data, 9),
        ]),
        materials: BTreeMap::from([(String::new(), 0), ("x:y".into(), i64::MAX)]),
        material_ledgers: BTreeMap::from([
            (MaterialLedgerId::World, BTreeMap::new()),
            (
                MaterialLedgerId::Agent("a".into()),
                BTreeMap::from([("x:y".into(), -3)]),
            ),
            (
                MaterialLedgerId::Factory("a".into()),
                BTreeMap::from([(String::new(), 42)]),
            ),
        ]),
        ..WorldState::default()
    }
}

fn build(
    source: &WorldState,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> (InventoryGenesisSummaryV1, [u8; STATE_ROOT_BYTES]) {
    let mut values = store.value_session(physical());
    let mut writes = Writes::new(memory);
    let summary = prepare_inventory_genesis_v1(
        source,
        &mut InventoryGenesisContext {
            values: &mut values,
            writes: &mut writes,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 256 << 20,
            },
            semantic_bytes: 128 << 20,
            record_bytes: 128 << 20,
            records_remaining: 64,
            cancelled: || false,
        },
    )
    .unwrap();
    let state_source = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let workspace = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let rows = writes.rows.borrow();
    let mut changes: Vec<_> = rows
        .iter()
        .map(|write| {
            assert_eq!(write.before, None);
            StateWriteV1 {
                key: write.key.clone(),
                before: None,
                after: write.after,
            }
        })
        .collect();
    changes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = workspace.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
            changes,
            &state_source,
            2 << 20,
        )
        .unwrap(),
    );
    assert_eq!(delta.after.leaf_count, rows.len() as u64);
    values.put_state_pages(&delta, memory).unwrap();
    (
        summary,
        delta.after.canonical_bytes().unwrap().try_into().unwrap(),
    )
}
fn text_token(text: &str) -> Vec<u8> {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&(text.len() as u64).to_be_bytes());
    bytes.extend_from_slice(text.as_bytes());
    bytes
}
fn ledger_token(kind: u8, id: Option<&str>) -> Vec<u8> {
    let mut bytes = vec![4, kind];
    if let Some(id) = id {
        bytes.extend(text_token(id));
    }
    bytes
}
fn pair_token(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut bytes = vec![5];
    bytes.extend_from_slice(a);
    bytes.extend_from_slice(b);
    bytes
}
fn key(
    name: &'static str,
    shape: FieldShape,
    kind: WorldFieldRecordKindV1,
    token: Option<&[u8]>,
) -> (WorldFieldKeyV1, Vec<u8>) {
    let mut semantic = Vec::new();
    let digest = token.map(|token| {
        semantic.extend_from_slice(b"oasis7.iws.semantic-key.v1\0");
        semantic.extend_from_slice(token);
        *blake3::hash(&semantic).as_bytes()
    });
    (
        WorldFieldKeyV1::new(
            FieldDescriptor {
                owner: FieldOwner::WorldState,
                name,
                destination: FieldDestination::State(StateNamespaceV1::Business),
                shape,
            },
            kind,
            digest,
        )
        .unwrap(),
        semantic,
    )
}

#[test]
fn actual_world_inventory_reopens_with_empty_groups_original_ids_and_exact_signed_quantities() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let producer_memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let root_bytes = {
            let source = producer_memory
                .reserve(1 << 20, MemoryClass::Ordinary)
                .unwrap()
                .retain(source());
            let store =
                IncrementalPageStore::create(&path, producer_memory.clone(), compression).unwrap();
            let (summary, root) = build(&source, &store, &producer_memory);
            assert_eq!(
                summary,
                InventoryGenesisSummaryV1 {
                    fields: 3,
                    groups: 3,
                    entries: 6
                }
            );
            assert_eq!(source.time, 777);
            assert!(source.material_ledgers[&MaterialLedgerId::World].is_empty());
            root
        };
        assert_eq!(producer_memory.used(), 0);
        let mut expected = Vec::new();
        for (name, shape) in [
            ("resources", FieldShape::Map),
            ("materials", FieldShape::Map),
            ("material_ledgers", FieldShape::NestedMap),
        ] {
            let (key, semantic) = key(name, shape, WorldFieldRecordKindV1::Metadata, None);
            expected.push((key, semantic, None));
        }
        for (name, shape, token, quantity) in [
            ("resources", FieldShape::Map, vec![3, 1], i64::MIN),
            ("resources", FieldShape::Map, vec![3, 2], 9),
            ("materials", FieldShape::Map, text_token(""), 0),
            ("materials", FieldShape::Map, text_token("x:y"), i64::MAX),
            (
                "material_ledgers",
                FieldShape::NestedMap,
                pair_token(&ledger_token(1, Some("a")), &text_token("x:y")),
                -3,
            ),
            (
                "material_ledgers",
                FieldShape::NestedMap,
                pair_token(&ledger_token(3, Some("a")), &text_token("")),
                42,
            ),
        ] {
            let (key, semantic) = key(name, shape, WorldFieldRecordKindV1::Entry, Some(&token));
            expected.push((key, semantic, Some(quantity)));
        }
        for token in [
            ledger_token(0, None),
            ledger_token(1, Some("a")),
            ledger_token(3, Some("a")),
        ] {
            let (key, semantic) = key(
                "material_ledgers",
                FieldShape::NestedMap,
                WorldFieldRecordKindV1::Group,
                Some(&token),
            );
            expected.push((key, semantic, None));
        }
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
            let _outputs = memory.reserve(64 << 10, class).unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let state = store.state_session(physical(), class).unwrap();
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
            let root = StateRootV1::from_canonical_bytes(&root_bytes).unwrap();
            assert_eq!(root.leaf_count, 12);
            for (field, semantic, quantity) in &expected {
                let value = {
                    let _query = memory.reserve(512 << 10, class).unwrap();
                    root.get(field.as_bytes(), &state).unwrap().unwrap()
                };
                let values = store.value_session(physical());
                let mut io = logical();
                let mut context = ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class,
                    io: &mut io,
                };
                let mut actual_key = Vec::new();
                if let Some(quantity) = quantity {
                    let mut payload = Vec::new();
                    let info = read_member_record_v1(
                        &value,
                        field,
                        &mut context,
                        MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 8,
                            record_bytes: 4096,
                        },
                        &mut actual_key,
                        &mut payload,
                        || false,
                    )
                    .unwrap();
                    assert_eq!(info.codec, MemberPayloadCodecV1::I64);
                    assert_eq!(payload, quantity.to_be_bytes());
                } else {
                    read_field_presence_v1(
                        &value,
                        field,
                        &mut context,
                        PresenceRecordLimitsV1 {
                            semantic_bytes: 1024,
                            record_bytes: 4096,
                        },
                        &mut actual_key,
                        || false,
                    )
                    .unwrap();
                }
                assert_eq!(&actual_key, semantic);
            }
        }
    }
}

#[test]
fn empty_fields_and_failure_prefixes_report_only_complete_selected_production() {
    use oasis7_formal_protocol::incremental_value::ValueTreeError;
    for mode in 0..10 {
        let directory = Directory::new();
        let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("pages"),
            memory.clone(),
            if mode == 9 {
                PageCompression::Zstd
            } else {
                PageCompression::Raw
            },
        )
        .unwrap();
        let _source = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
        let mut source = source();
        if mode != 6 {
            source.resources.clear();
            source.materials.clear();
            source.material_ledgers.clear();
        }
        let mut values = store.value_session(physical());
        let mut writes = Writes::new(&memory);
        let flag = Rc::new(Cell::new(mode == 1));
        if mode == 2 {
            writes.fail_after = 1;
        }
        if mode == 3 {
            writes.cancel = Some(flag.clone());
        }
        let _filler = if mode == 8 {
            Some(
                memory
                    .reserve((15 << 20) - memory.used(), MemoryClass::Ordinary)
                    .unwrap(),
            )
        } else {
            None
        };
        let mut io = logical();
        if mode == 5 {
            io.nodes_remaining = 0;
        }
        let mut work = SemanticKeyBudget {
            bytes_remaining: if mode == 6 { 0 } else { 4096 },
        };
        let mut context = InventoryGenesisContext {
            values: &mut values,
            writes: &mut writes,
            memory: &memory,
            io: &mut io,
            key_work: &mut work,
            semantic_bytes: 1024,
            record_bytes: if mode == 7 { 0 } else { 4096 },
            records_remaining: if mode == 4 { 2 } else { 3 },
            cancelled: || flag.get(),
        };
        let result = prepare_inventory_genesis_v1(&source, &mut context);
        if mode == 0 {
            assert_eq!(
                result.unwrap(),
                InventoryGenesisSummaryV1 {
                    fields: 3,
                    groups: 0,
                    entries: 0
                }
            );
            assert_eq!(writes.rows.borrow().len(), 3);
        } else {
            assert!(result.is_err());
            if mode == 9 {
                assert!(matches!(
                    &result,
                    Err(InventoryGenesisError::Value(ValueTreeError::Memory(_)))
                ));
            }
            if mode == 1 {
                assert!(matches!(
                    result,
                    Err(InventoryGenesisError::Value(ValueTreeError::Cancelled))
                ));
                assert!(writes.rows.borrow().is_empty());
                assert_eq!(io.nodes_remaining, 100_000);
            }
        }
        if mode != 6 {
            assert!(source.resources.is_empty());
        }
        assert_eq!(source.time, 777);
    }
}

#[test]
fn large_original_ledger_identity_streams_through_actual_world_production() {
    use std::io::Write;
    struct HashOutput {
        hash: blake3::Hasher,
        bytes: u64,
    }
    impl Write for HashOutput {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.hash.update(bytes);
            self.bytes += bytes.len() as u64;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
        let repetitions = (65 << 20) / 5 + 1;
        let length = repetitions * 5;
        let (root_bytes, group_key, entry_key, group_digest, entry_digest) = {
            let source = memory
                .reserve(length + (1 << 20), MemoryClass::Ordinary)
                .unwrap()
                .retain(WorldState {
                    resources: BTreeMap::new(),
                    materials: BTreeMap::new(),
                    material_ledgers: BTreeMap::from([(
                        MaterialLedgerId::Agent("ab甲".repeat(repetitions)),
                        BTreeMap::from([("ore".into(), i64::MIN)]),
                    )]),
                    ..WorldState::default()
                });
            let _metadata = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
            let MaterialLedgerId::Agent(id) = source.material_ledgers.keys().next().unwrap() else {
                unreachable!()
            };
            let mut group = blake3::Hasher::new();
            group.update(b"oasis7.iws.semantic-key.v1\0");
            group.update(&[4, 1, 1]);
            group.update(&(length as u64).to_be_bytes());
            group.update(id.as_bytes());
            let mut entry = blake3::Hasher::new();
            entry.update(b"oasis7.iws.semantic-key.v1\0");
            entry.update(&[5, 4, 1, 1]);
            entry.update(&(length as u64).to_be_bytes());
            entry.update(id.as_bytes());
            entry.update(&text_token("ore"));
            let group_digest = *group.finalize().as_bytes();
            let entry_digest = *entry.finalize().as_bytes();
            let descriptor = FieldDescriptor {
                owner: FieldOwner::WorldState,
                name: "material_ledgers",
                destination: FieldDestination::State(StateNamespaceV1::Business),
                shape: FieldShape::NestedMap,
            };
            let group_key = WorldFieldKeyV1::new(
                descriptor,
                WorldFieldRecordKindV1::Group,
                Some(group_digest),
            )
            .unwrap();
            let entry_key = WorldFieldKeyV1::new(
                descriptor,
                WorldFieldRecordKindV1::Entry,
                Some(entry_digest),
            )
            .unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let (summary, root) = build(&source, &store, &memory);
            assert_eq!(
                summary,
                InventoryGenesisSummaryV1 {
                    fields: 3,
                    groups: 1,
                    entries: 1
                }
            );
            (root, group_key, entry_key, group_digest, entry_digest)
        };
        assert_eq!(memory.used(), 0);
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
            let _output = memory.reserve(8192, class).unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let state = store.state_session(physical(), class).unwrap();
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
            let root = StateRootV1::from_canonical_bytes(&root_bytes).unwrap();
            assert_eq!(root.leaf_count, 5);
            for (field, digest, group) in [
                (&group_key, group_digest, true),
                (&entry_key, entry_digest, false),
            ] {
                let value = {
                    let _query = memory.reserve(512 << 10, class).unwrap();
                    root.get(field.as_bytes(), &state).unwrap().unwrap()
                };
                let values = store.value_session(physical());
                let mut io = logical();
                let mut context = ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class,
                    io: &mut io,
                };
                let mut output = HashOutput {
                    hash: blake3::Hasher::new(),
                    bytes: 0,
                };
                if group {
                    let info = read_field_presence_v1(
                        &value,
                        field,
                        &mut context,
                        PresenceRecordLimitsV1 {
                            semantic_bytes: 128 << 20,
                            record_bytes: 128 << 20,
                        },
                        &mut output,
                        || false,
                    )
                    .unwrap();
                    assert_eq!(output.bytes, info.semantic_bytes);
                } else {
                    let mut quantity = Vec::new();
                    let info = read_member_record_v1(
                        &value,
                        field,
                        &mut context,
                        MemberRecordLimitsV1 {
                            semantic_bytes: 128 << 20,
                            payload_bytes: 8,
                            record_bytes: 128 << 20,
                        },
                        &mut output,
                        &mut quantity,
                        || false,
                    )
                    .unwrap();
                    assert_eq!(info.codec, MemberPayloadCodecV1::I64);
                    assert_eq!(output.bytes, info.semantic_bytes);
                    assert_eq!(quantity, i64::MIN.to_be_bytes());
                }
                assert_eq!(output.hash.finalize().as_bytes(), &digest);
            }
        }
    }
}
