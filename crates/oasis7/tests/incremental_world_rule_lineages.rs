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
    incremental_world_rule_lineages::*,
    world_rules::{SupplyLineageV1, WorldRulesStateV1},
};
use std::cell::Cell;
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
#[path = "support/incremental_world_rules_fixture.rs"]
mod rules_fixture;
use rules_fixture::*;
fn amount(target: i64, fulfilled: i64, reserved: i64) -> SupplyLineageV1 {
    SupplyLineageV1 {
        target,
        fulfilled,
        reserved,
    }
}
fn source() -> WorldState {
    WorldState {
        world_rules: Some(WorldRulesStateV1 {
            governance_effects: [("p".into(), "original".into())].into(),
            supply_lineages: [
                (" L\0é ".into(), amount(i64::MIN, -7, i64::MAX)),
                ("zero".into(), amount(0, 0, 0)),
                ("unique-noop".into(), amount(1, 2, 3)),
            ]
            .into(),
            ..WorldRulesStateV1::default()
        }),
        ..WorldState::default()
    }
}
fn semantic(original: &str, part: Option<&str>) -> Vec<u8> {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    if part.is_some() {
        bytes.push(5);
    }
    bytes.push(5);
    for text in ["supply_lineages", original] {
        bytes.push(1);
        bytes.extend_from_slice(&(text.len() as u64).to_be_bytes());
        bytes.extend_from_slice(text.as_bytes());
    }
    if let Some(part) = part {
        bytes.push(1);
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part.as_bytes());
    }
    bytes
}
fn key(original: &str, part: Option<&str>) -> WorldFieldKeyV1 {
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic(original, part)).as_bytes()),
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
    let mut records = 12;
    let count = prepare_world_rule_lineages_genesis_members_v1(
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
        &mut records,
        &mut |row| {
            assert_eq!(row.namespace, Ns::Authorization);
            assert_eq!(row.mutation.before, None);
            assert!(rows.len() < 20);
            rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(rows.len() - old, 4 * count as usize);
    rows
}
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
        cancelled: impl FnMut() -> bool,
    ) -> Result<WorldRuleLineageValueV1, ValueTreeError> {
        let state = self
            .store
            .state_session(self.state_io.clone(), self.class)
            .unwrap();
        read_world_rule_lineage_v1(
            key,
            &mut WorldFieldReadContextV1 {
                root,
                state: &state,
                values: ValueReadContext {
                    source: &self.store.value_session(physical()),
                    memory: self.memory,
                    class: self.class,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 16384,
                },
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled,
            },
        )
    }
}
#[test]
fn original_lineage_columns_reopen_as_one_complete_three_signed_value_record() {
    use WorldRuleLineageValueV1 as V;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let producer = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _source = producer.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
        let roots = {
            let store = IncrementalPageStore::create(&path, producer.clone(), compression).unwrap();
            let _batch = producer.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
            let good = produce_all(&store, &producer, &source());
            assert_eq!(good.len(), 15); // Parent/effect: 3; three closed records: 12.
            for (part, value) in [
                (None, None),
                (Some("target"), Some(i64::MIN)),
                (Some("fulfilled"), Some(-7)),
                (Some("reserved"), Some(i64::MAX)),
            ] {
                let key = key(" L\0é ", part);
                let semantic = semantic(" L\0é ", part);
                let id = good.iter().find(|(k, _)| k == key.as_bytes()).unwrap().1;
                let payload = value.map(|v| v.to_be_bytes().to_vec()).unwrap_or_default();
                let mut expected = b"oasis7.iws.field-member.v1\0".to_vec();
                expected.extend_from_slice(&(key.as_bytes().len() as u16).to_be_bytes());
                expected.push(if value.is_some() { 3 } else { 1 });
                expected.extend_from_slice(&(semantic.len() as u64).to_be_bytes());
                expected.extend_from_slice(&(payload.len() as u64).to_be_bytes());
                expected.extend_from_slice(key.as_bytes());
                expected.extend(semantic);
                expected.extend(payload);
                assert_eq!(raw(&store, &producer, &id), expected);
            }
            let mut variants = vec![
                good.clone(),
                produce_all(&store, &producer, &WorldState::default()),
                produce_all(
                    &store,
                    &producer,
                    &WorldState {
                        world_rules: Some(WorldRulesStateV1::default()),
                        ..WorldState::default()
                    },
                ),
            ];
            for missing in [
                metadata(),
                parent_key(),
                key(" L\0é ", None),
                key(" L\0é ", Some("target")),
            ] {
                let mut rows = good.clone();
                rows.retain(|(k, _)| k != missing.as_bytes());
                variants.push(rows);
            }
            variants.push(replace(&store, &producer, &good, &parent_key(), |b| {
                *b.last_mut().unwrap() = 0
            }));
            variants.push(replace(
                &store,
                &producer,
                &good,
                &key(" L\0é ", None),
                |b| b[b"oasis7.iws.field-member.v1\0".len() + 2] = 5,
            ));
            variants.push(replace(
                &store,
                &producer,
                &good,
                &key(" L\0é ", Some("target")),
                |b| b[b"oasis7.iws.field-member.v1\0".len() + 2] = 2,
            ));
            variants.push(replace(
                &store,
                &producer,
                &good,
                &key(" L\0é ", Some("fulfilled")),
                |b| {
                    b.pop();
                },
            ));
            // Persist the complete immutable seed once; malformed roots change only
            // their explicit leaves, with the same finite per-root I/O allowance.
            let seed = build(&store, &producer, &good);
            variants
                .iter()
                .map(|rows| {
                    if rows == &good {
                        seed.clone()
                    } else if rows.len() == 2 {
                        // None/Some(empty) are independent two-leaf roots, not
                        // thirteen deletions from the populated lineage seed.
                        build(&store, &producer, rows)
                    } else {
                        build_from(&store, &producer, &seed, &good, rows)
                    }
                    .canonical_bytes()
                    .unwrap()
                })
                .collect::<Vec<_>>()
        };
        drop(_source);
        drop(producer);
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
            if _full.is_some() {
                assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
            }
            let reader = Reader {
                store: &store,
                memory: &memory,
                class,
                state_io: physical(),
            };
            let good = StateRootV1::from_canonical_bytes(&roots[0]).unwrap();
            for (index, name, expected) in [
                (0, " L\0é ", V::Value(amount(i64::MIN, -7, i64::MAX))),
                (0, "zero", V::Value(amount(0, 0, 0))),
                (0, "missing", V::LineageAbsent),
                (1, " L\0é ", V::RulesAbsent),
                (2, " L\0é ", V::LineageAbsent),
            ] {
                assert_eq!(
                    reader
                        .read(
                            &StateRootV1::from_canonical_bytes(&roots[index]).unwrap(),
                            name,
                            || false
                        )
                        .unwrap(),
                    expected
                );
            }
            for root in &roots[3..] {
                assert!(matches!(
                    reader.read(
                        &StateRootV1::from_canonical_bytes(root).unwrap(),
                        " L\0é ",
                        || false
                    ),
                    Err(ValueTreeError::Invalid(_))
                ));
            }
            assert!(matches!(
                reader.read(&good, " L\0é ", || true),
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
                no_io.read(&good, "missing", || false),
                Err(ValueTreeError::ResourceLimited(_))
            ));
            assert!(matches!(
                no_io.read(
                    &StateRootV1::empty(Ns::Business).unwrap(),
                    "missing",
                    || false
                ),
                Err(ValueTreeError::Invalid("field namespace"))
            ));
        }
        overlay::exercise(&path, compression, &roots);
    }
}
#[test]
fn child_genesis_failure_discards_the_combined_parent_and_child_prefix() {
    let dir = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let _source = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let source = source();
    for case in 0..10 {
        let mut rows = produce(&store, &memory, &source);
        let prefix = rows.len();
        let cancel = Cell::new(case == 4);
        let mut records = match case {
            0 => 0,
            1 => 1,
            2 => 3,
            _ => 12,
        };
        let mut io = logical();
        if case == 8 {
            io.write_bytes_remaining = 0;
        }
        let mut bound = limits();
        if case == 7 {
            bound.payload_bytes = 0;
        }
        if case == 9 {
            bound.record_bytes = 0;
        }
        let mut keys = SemanticKeyBudget {
            bytes_remaining: if case == 6 { 0 } else { 16384 },
        };
        let result = prepare_world_rule_lineages_genesis_members_v1(
            &source,
            &mut MemberRecordWriteContext {
                sink: &mut store.value_session(physical()),
                memory: &memory,
                io: &mut io,
                key_work: &mut keys,
                limits: bound,
                cancelled: || cancel.get(),
            },
            &mut records,
            &mut |row| {
                if case == 5 {
                    return Err(ValueTreeError::Invalid("injected lineage sink"));
                }
                assert!(rows.len() < 20);
                rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
                if case == 3 {
                    cancel.set(true);
                }
                Ok(())
            },
        );
        assert!(result.is_err(), "case {case}");
        if case == 0 || case == 4 || case == 5 {
            assert_eq!(rows.len(), prefix);
        }
        // No root is built: the already-successful parent/effect prefix is also provisional.
    }
}
#[path = "support/incremental_world_rule_lineages_overlay.rs"]
mod overlay;
