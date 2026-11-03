#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueNodeWriter, ValueReadContext, ValueTreeError, write_value},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState,
    incremental_field_member::*,
    incremental_field_read::*,
    incremental_world_rule_effects::*,
    incremental_world_rule_shipments::*,
    world_rules::{SupplyShipmentV1, WorldRulesStateV1},
};
use std::cell::Cell;
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
#[path = "support/incremental_world_rules_fixture.rs"]
mod rules_fixture;
use rules_fixture::*;
fn shipment() -> SupplyShipmentV1 {
    SupplyShipmentV1 {
        authorization_id: String::new(),
        batch_id: " b\0é ".into(),
        beneficiary_id: "beneficiary".into(),
        lineage_id: "lineage".into(),
        input_amount: i64::MIN,
        delivered_amount: i64::MAX,
        settled: false,
    }
}
fn source() -> WorldState {
    WorldState {
        world_rules: Some(WorldRulesStateV1 {
            governance_effects: [("p".into(), "original".into())].into(),
            supply_shipments: [(u64::MAX, shipment())].into(),
            ..WorldRulesStateV1::default()
        }),
        ..WorldState::default()
    }
}
fn semantic(id: u64, column: Option<&str>) -> Vec<u8> {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    if column.is_some() {
        bytes.push(5);
    }
    bytes.push(5);
    bytes.push(1);
    bytes.extend_from_slice(&16u64.to_be_bytes());
    bytes.extend_from_slice(b"supply_shipments");
    bytes.push(2);
    bytes.extend_from_slice(&id.to_be_bytes());
    if let Some(column) = column {
        bytes.push(1);
        bytes.extend_from_slice(&(column.len() as u64).to_be_bytes());
        bytes.extend_from_slice(column.as_bytes());
    }
    bytes
}
fn key(id: u64, column: Option<&str>) -> WorldFieldKeyV1 {
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic(id, column)).as_bytes()),
    )
    .unwrap()
}
fn metadata() -> WorldFieldKeyV1 {
    WorldFieldKeyV1::new(descriptor(), WorldFieldRecordKindV1::Metadata, None).unwrap()
}
fn parent_key() -> WorldFieldKeyV1 {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.push(0);
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&bytes).as_bytes()),
    )
    .unwrap()
}
fn produce_all(store: &IncrementalPageStore, memory: &MemoryBudget, source: &WorldState) -> Rows {
    let mut rows = produce(store, memory, source);
    let old = rows.len();
    let mut remaining = 8;
    let count = prepare_world_rule_shipments_genesis_members_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut store.value_session(physical()),
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16384,
            },
            limits: limits(),
            cancelled: || false,
        },
        &mut remaining,
        &mut |row| {
            assert_eq!(row.namespace, Ns::Authorization);
            assert_eq!(row.mutation.before, None);
            assert!(rows.len() < 12);
            rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(count as usize * 8, rows.len() - old);
    rows
}
type ReadResult = ReservedValue<(WorldRuleShipmentValueV1, [Vec<u8>; 4])>;
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
        id: u64,
        max_payload: u64,
        cancelled: impl FnMut() -> bool,
    ) -> Result<ReadResult, ValueTreeError> {
        let charge = self.memory.reserve(4096, self.class)?;
        let mut data: [Vec<u8>; 4] = std::array::from_fn(|_| Vec::with_capacity(256));
        let state = self
            .store
            .state_session(self.state_io.clone(), self.class)
            .map_err(state_error)?;
        let values = self.store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16384,
        };
        let [a, b, c, d] = &mut data;
        let mut outputs: [&mut dyn std::io::Write; 4] = [a, b, c, d];
        let result = read_world_rule_shipment_v1(
            id,
            &mut WorldFieldReadContextV1 {
                root,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: self.memory,
                    class: self.class,
                    io: &mut io,
                },
                key_work: &mut keys,
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled,
            },
            max_payload,
            &mut outputs,
        )?;
        Ok(charge.retain((result, data)))
    }
}
#[test]
fn original_shipment_fields_have_typed_u64_keys_and_cold_streaming_reads() {
    use WorldRuleShipmentValueV1 as V;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _batch = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
        let roots = {
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let good = produce_all(&store, &memory, &source());
            assert_eq!(good.len(), 11);
            for (column, codec, payload) in [
                (None, 1, vec![]),
                (Some("authorization_id"), 5, vec![]),
                (Some("batch_id"), 5, b" b\0\xc3\xa9 ".to_vec()),
                (Some("beneficiary_id"), 5, b"beneficiary".to_vec()),
                (Some("lineage_id"), 5, b"lineage".to_vec()),
                (Some("input_amount"), 3, i64::MIN.to_be_bytes().to_vec()),
                (Some("delivered_amount"), 3, i64::MAX.to_be_bytes().to_vec()),
                (Some("settled"), 4, vec![0]),
            ] {
                let key = key(u64::MAX, column);
                let semantic = semantic(u64::MAX, column);
                let id = good.iter().find(|(k, _)| k == key.as_bytes()).unwrap().1;
                let mut expected = b"oasis7.iws.field-member.v1\0".to_vec();
                expected.extend_from_slice(&(key.as_bytes().len() as u16).to_be_bytes());
                expected.push(codec);
                expected.extend_from_slice(&(semantic.len() as u64).to_be_bytes());
                expected.extend_from_slice(&(payload.len() as u64).to_be_bytes());
                expected.extend_from_slice(key.as_bytes());
                expected.extend(semantic);
                expected.extend(payload);
                assert_eq!(raw(&store, &memory, &id), expected);
            }
            let seed = build(&store, &memory, &good);
            let mut roots = vec![
                seed.canonical_bytes().unwrap(),
                build(
                    &store,
                    &memory,
                    &produce_all(&store, &memory, &WorldState::default()),
                )
                .canonical_bytes()
                .unwrap(),
                build(
                    &store,
                    &memory,
                    &produce_all(
                        &store,
                        &memory,
                        &WorldState {
                            world_rules: Some(WorldRulesStateV1::default()),
                            ..WorldState::default()
                        },
                    ),
                )
                .canonical_bytes()
                .unwrap(),
            ];
            let mut variants = Vec::new();
            for missing in [
                metadata(),
                parent_key(),
                key(u64::MAX, None),
                key(u64::MAX, Some("batch_id")),
            ] {
                let mut rows = good.clone();
                rows.retain(|(k, _)| k != missing.as_bytes());
                variants.push(rows);
            }
            variants.push(replace(&store, &memory, &good, &parent_key(), |b| {
                *b.last_mut().unwrap() = 0
            }));
            variants.push(replace(
                &store,
                &memory,
                &good,
                &key(u64::MAX, Some("settled")),
                |b| *b.last_mut().unwrap() = 2,
            ));
            variants.push(replace(
                &store,
                &memory,
                &good,
                &key(u64::MAX, Some("input_amount")),
                |b| b[b"oasis7.iws.field-member.v1\0".len() + 2] = 2,
            ));
            variants.push(replace(
                &store,
                &memory,
                &good,
                &key(u64::MAX, Some("batch_id")),
                |b| *b.last_mut().unwrap() = 0xff,
            ));
            variants.push(replace(
                &store,
                &memory,
                &good,
                &key(u64::MAX, Some("delivered_amount")),
                |b| {
                    b.pop();
                },
            ));
            for rows in variants {
                roots.push(
                    build_from(&store, &memory, &seed, &good, &rows)
                        .canonical_bytes()
                        .unwrap(),
                );
            }
            roots
        };
        drop(_batch);
        drop(memory);
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
            let good = StateRootV1::from_canonical_bytes(&roots[0]).unwrap();
            let result = reader.read(&good, u64::MAX, 128, || false).unwrap();
            assert_eq!(
                result.0,
                V::Value {
                    identity_utf8_bytes: [0, 6, 11, 7],
                    input_amount: i64::MIN,
                    delivered_amount: i64::MAX,
                    settled: false
                }
            );
            assert_eq!(
                result.1,
                [
                    vec![],
                    b" b\0\xc3\xa9 ".to_vec(),
                    b"beneficiary".to_vec(),
                    b"lineage".to_vec()
                ]
            );
            drop(result);
            assert_eq!(
                reader.read(&good, 0, 128, || false).unwrap().0,
                V::ShipmentAbsent
            );
            assert_eq!(
                reader
                    .read(
                        &StateRootV1::from_canonical_bytes(&roots[1]).unwrap(),
                        u64::MAX,
                        128,
                        || false
                    )
                    .unwrap()
                    .0,
                V::RulesAbsent
            );
            assert_eq!(
                reader
                    .read(
                        &StateRootV1::from_canonical_bytes(&roots[2]).unwrap(),
                        u64::MAX,
                        128,
                        || false
                    )
                    .unwrap()
                    .0,
                V::ShipmentAbsent
            );
            for root in &roots[3..] {
                assert!(matches!(
                    reader.read(
                        &StateRootV1::from_canonical_bytes(root).unwrap(),
                        u64::MAX,
                        128,
                        || false
                    ),
                    Err(ValueTreeError::Invalid(_))
                ));
            }
            assert!(reader.read(&good, u64::MAX, 0, || false).is_err());
            assert!(matches!(
                reader.read(&good, u64::MAX, 128, || true),
                Err(ValueTreeError::Cancelled)
            ));
            let no_io = Reader {
                state_io: PageIoBudget {
                    reads_remaining: 0,
                    ..physical()
                },
                ..reader
            };
            assert!(matches!(
                no_io.read(&good, 0, 128, || false),
                Err(ValueTreeError::ResourceLimited(_))
            ));
        }
        overlay::exercise(&path, compression, &roots);
    }
}
#[test]
fn shipment_genesis_failures_discard_the_combined_parent_prefix() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    for case in 0..7 {
        let mut rows = produce(&store, &memory, &source());
        let prefix = rows.len();
        let cancel = Cell::new(false);
        let mut remaining = if case == 0 { 0 } else { 8 };
        let mut io = logical();
        if case == 1 {
            io.write_bytes_remaining = 0;
        }
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: if case == 2 { 0 } else { 16384 },
        };
        let mut limit = limits();
        if case == 3 {
            limit.payload_bytes = 0;
        }
        let result = prepare_world_rule_shipments_genesis_members_v1(
            &source(),
            &mut MemberRecordWriteContext {
                sink: &mut store.value_session(physical()),
                memory: &memory,
                io: &mut io,
                key_work: &mut key_work,
                limits: limit,
                cancelled: || case == 4 || cancel.get(),
            },
            &mut remaining,
            &mut |row| {
                if case == 5 {
                    return Err(ValueTreeError::Invalid("injected shipment sink"));
                }
                rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
                if case == 6 {
                    cancel.set(true);
                }
                Ok(())
            },
        );
        assert!(result.is_err(), "case {case}");
        if matches!(case, 0 | 1 | 2 | 4 | 5) {
            assert_eq!(rows.len(), prefix);
        }
    }
}
#[path = "support/incremental_world_rule_shipments_overlay.rs"]
mod overlay;

#[test]
fn cancellation_after_an_early_changed_column_poisons_the_completed_prefix() {
    // On the original 7714 producer, the last real put was followed by three
    // cancellation callbacks, then unchecked equal Text columns. Cancel at the
    // next public callback boundary; a successful typed prefix and real puts
    // must not remain sealable merely because no later column changes.
    let (cancelled, after_put, puts, unsealable) = overlay::late_change_probe(Some(4));
    assert!(puts > 0, "must reach a real shipment value put");
    assert!(
        cancelled,
        "late cancellation escaped after {after_put} checks"
    );
    assert_eq!(after_put, 4);
    assert!(unsealable);
}
