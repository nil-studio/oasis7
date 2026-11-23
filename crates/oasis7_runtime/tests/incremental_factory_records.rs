#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    FactoryBuildJobState, FactoryState, WorldState, incremental_factory_records::*,
    incremental_field_member::*, incremental_field_overlay::*, incremental_field_read::*,
};
use std::{collections::BTreeMap, io};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, pair_token, physical, text_token};
fn quota() -> PageIoBudget {
    let mut value = physical();
    value.reads_remaining = 1_000_000;
    value
}
fn typed(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(e) => ValueTreeError::Memory(e),
        PageStoreError::ResourceLimited(r) => ValueTreeError::ResourceLimited(r),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(e) => ValueTreeError::Io(e),
        _ => ValueTreeError::Invalid("factory physical page"),
    }
}
fn source() -> (WorldState, serde_json::Value, serde_json::Value) {
    let spec = serde_json::json!({"factory_id":"spec-id", "display_name":" F\u{0000}\u{7532} ","tier":255,"tags":["", "dup", "dup"],"build_cost":[{"kind":"ore","amount":i64::MIN},{"kind":"ore","amount":0}],"build_time_ticks":u32::MAX,"base_power_draw":i64::MIN,"recipe_slots":u16::MAX,"throughput_bps":u32::MAX,"maintenance_per_tick":i64::MAX});
    let factory = serde_json::json!({"factory_id":"inner-factory", "site_id":" S ","builder_agent_id":"builder", "spec":spec,"input_ledger":"agent: A\u{0000}\u{7532} ","output_ledger":"factory:inside","durability_ppm":i64::MIN,"production":{"status":"paused","active_jobs":u16::MAX,"current_job_id":u64::MAX,"current_recipe_id":"", "last_started_at":0,"last_completed_at":u64::MAX,"last_blocked_at":1,"last_resumed_at":2,"current_blocker_kind":" k ","current_blocker_detail":"detail\u{0000}\u{7532}", "current_blocker_action_id":3,"completed_jobs":u64::MAX,"last_completed_recipe_id":"last","same_recipe_repeat_count":u32::MAX,"last_completed_canonical_snapshot":{"recipe_id":"snapshot","consume":[{"kind":"x","amount":-1},{"kind":"x","amount":7}],"produce":[],"byproducts":[{"kind":"","amount":i64::MAX}],"consume_ledger":"world","power_required":i64::MIN,"output_ledger":"site: z ","bottleneck_tags":["", "t"],"logistics_route_ids":["r", "r"],"logistics_path_ids":[]}},"site_authority_revision":0,"site_location_id":"", "location_anchor_revision":u64::MAX,"construction_power_profile_key":"profile", "construction_power_profile_revision":17,"built_at":u64::MAX});
    let build = serde_json::json!({"job_id":17,"builder_agent_id":"job-builder","site_id":"job-site","spec":spec,"consume_ledger":"factory:job-inner","ready_at":u64::MAX,"contract_version":255,"site_authority_revision":0,"site_location_id":"", "location_anchor_revision":u64::MAX,"construction_power_obligation":{"payer_agent_id":"payer", "profile_key":"p", "profile_revision":u64::MAX,"electricity_amount":i64::MIN,"mode":"start_only_sink","material_ledger":"site: power ","electricity_before":i64::MAX,"electricity_after":i64::MIN,"material_balances_before":{"":0,"ore":i64::MIN},"material_balances_after":{}}});
    let state = WorldState {
        factories: [(
            "outer factory".into(),
            serde_json::from_value::<FactoryState>(factory.clone()).unwrap(),
        )]
        .into(),
        pending_factory_builds: [(
            u64::MAX,
            serde_json::from_value::<FactoryBuildJobState>(build.clone()).unwrap(),
        )]
        .into(),
        ..WorldState::default()
    };
    (state, factory, build)
}
type Oracle = BTreeMap<(u16, Vec<u8>), (MemberPayloadCodecV1, Vec<u8>)>;
fn unsigned_token(value: u64) -> Vec<u8> {
    let mut bytes = vec![2];
    bytes.extend_from_slice(&value.to_be_bytes());
    bytes
}
fn field(name: &'static str, namespace: StateNamespaceV1) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name,
        destination: FieldDestination::State(namespace),
        shape: FieldShape::Map,
    }
}
// Independent literal tuple-token and JSON-source oracle. This does not call
// the production visitor, column enum, key encoder or payload writer.
fn oracle_row(
    oracle: &mut Oracle,
    field: FieldDescriptor,
    token: &[u8],
    codec: MemberPayloadCodecV1,
    payload: Vec<u8>,
) {
    let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
    semantic.extend_from_slice(token);
    let key = WorldFieldKeyV1::new(
        field,
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic).as_bytes()),
    )
    .unwrap();
    let FieldDestination::State(ns) = field.destination else {
        unreachable!()
    };
    assert!(
        oracle
            .insert((ns as u16, key.as_bytes().to_vec()), (codec, payload))
            .is_none()
    );
}
fn oracle_leaf(
    field: FieldDescriptor,
    root: &[u8],
    section: &str,
    name: &str,
    value: &serde_json::Value,
    oracle: &mut Oracle,
) {
    use MemberPayloadCodecV1 as C;
    let prefix = pair_token(&pair_token(root, &text_token(section)), &text_token(name));
    let (codec, payload) = match value {
        serde_json::Value::Null => (C::Unit, Vec::new()),
        serde_json::Value::Bool(v) => (C::Bool, vec![u8::from(*v)]),
        serde_json::Value::Number(v) => {
            if matches!(
                name,
                "durability_ppm"
                    | "base_power_draw"
                    | "maintenance_per_tick"
                    | "power_required"
                    | "electricity_amount"
                    | "electricity_before"
                    | "electricity_after"
            ) {
                (C::I64, v.as_i64().unwrap().to_be_bytes().to_vec())
            } else {
                (C::U64, v.as_u64().unwrap().to_be_bytes().to_vec())
            }
        }
        serde_json::Value::String(v) if name == "status" => (
            C::U64,
            match v.as_str() {
                "idle" => 0u64,
                "running" => 1,
                "blocked" => 2,
                "paused" => 3,
                _ => panic!("status"),
            }
            .to_be_bytes()
            .to_vec(),
        ),
        serde_json::Value::String(v) if name == "mode" => {
            assert_eq!(v, "start_only_sink");
            (C::U64, 0u64.to_be_bytes().to_vec())
        }
        serde_json::Value::String(v) if name.ends_with("ledger") => {
            let (kind, id) = if v == "world" {
                (0, "")
            } else {
                let (kind, id) = v.split_once(':').unwrap();
                (
                    match kind {
                        "agent" => 1,
                        "site" => 2,
                        "factory" => 3,
                        _ => panic!("ledger"),
                    },
                    id,
                )
            };
            let mut bytes = b"oasis7.iws.ledger-value.v1\0".to_vec();
            bytes.push(kind);
            bytes.extend_from_slice(&(id.len() as u64).to_be_bytes());
            bytes.extend_from_slice(id.as_bytes());
            (C::Ledger, bytes)
        }
        serde_json::Value::String(v) => (C::Text, v.as_bytes().to_vec()),
        serde_json::Value::Array(values) => {
            oracle_row(
                oracle,
                field,
                &pair_token(&prefix, &[0]),
                C::U64,
                (values.len() as u64).to_be_bytes().to_vec(),
            );
            for (index, value) in values.iter().enumerate() {
                let member = pair_token(&prefix, &unsigned_token(index as u64));
                if let Some(stack) = value.as_object() {
                    for (name, value) in stack {
                        let (codec, payload) = if name == "kind" {
                            (C::Text, value.as_str().unwrap().as_bytes().to_vec())
                        } else {
                            assert_eq!(name, "amount");
                            (C::I64, value.as_i64().unwrap().to_be_bytes().to_vec())
                        };
                        oracle_row(
                            oracle,
                            field,
                            &pair_token(&member, &text_token(name)),
                            codec,
                            payload,
                        );
                    }
                } else {
                    oracle_row(
                        oracle,
                        field,
                        &member,
                        C::Text,
                        value.as_str().unwrap().as_bytes().to_vec(),
                    );
                }
            }
            return;
        }
        serde_json::Value::Object(values) if name.starts_with("material_balances_") => {
            oracle_row(
                oracle,
                field,
                &pair_token(&prefix, &[0]),
                C::U64,
                (values.len() as u64).to_be_bytes().to_vec(),
            );
            for (name, value) in values {
                oracle_row(
                    oracle,
                    field,
                    &pair_token(&prefix, &text_token(name)),
                    C::I64,
                    value.as_i64().unwrap().to_be_bytes().to_vec(),
                );
            }
            return;
        }
        _ => panic!("unexpected oracle leaf {section}/{name}"),
    };
    oracle_row(oracle, field, &prefix, codec, payload);
}
fn oracle_object(
    field: FieldDescriptor,
    root: &[u8],
    section: &str,
    value: &serde_json::Value,
    oracle: &mut Oracle,
) {
    for (name, value) in value.as_object().unwrap() {
        match name.as_str() {
            "spec" => oracle_object(field, root, "spec", value, oracle),
            "production" => oracle_object(field, root, "production", value, oracle),
            "last_completed_canonical_snapshot" | "construction_power_obligation" => {
                let parent = if name == "last_completed_canonical_snapshot" {
                    "production"
                } else {
                    "construction_power_obligation"
                };
                oracle_leaf(
                    field,
                    root,
                    parent,
                    name,
                    &serde_json::Value::Bool(!value.is_null()),
                    oracle,
                );
                if !value.is_null() {
                    oracle_object(field, root, name, value, oracle);
                } else {
                    panic!("full oracle fixture uses present nested values");
                }
            }
            _ => oracle_leaf(field, root, section, name, value, oracle),
        }
    }
}
fn expected_oracle(factory: &serde_json::Value, build: &serde_json::Value) -> Oracle {
    let mut oracle = Oracle::new();
    for (field, root, section, value) in [
        (
            field("factories", StateNamespaceV1::Business),
            vec![0],
            "factory",
            factory,
        ),
        (
            field("pending_factory_builds", StateNamespaceV1::Scheduler),
            vec![0],
            "build",
            build,
        ),
    ] {
        oracle_row(
            &mut oracle,
            field,
            &root,
            MemberPayloadCodecV1::Unit,
            Vec::new(),
        );
        oracle_object(field, &root, section, value, &mut oracle);
    }
    oracle
}
fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 1 << 20,
        payload_bytes: 1 << 20,
        record_bytes: 2 << 20,
    }
}
fn factory_work() -> FactoryWorkBudgetV1 {
    FactoryWorkBudgetV1 {
        cells_remaining: 1_000_000,
        max_pending_node_bytes: 1 << 20,
        scan: StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 10_000,
        },
    }
}
fn seed(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    source: &WorldState,
    oracle: Option<&Oracle>,
) -> RootSetV1 {
    let mut values = store.value_session(quota());
    let mut work = factory_work();
    let _batch = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(256);
    assert_eq!(
        prepare_factory_records_genesis_v1(
            source,
            &mut MemberRecordWriteContext {
                sink: &mut values,
                memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 16 << 20
                },
                limits: limits(),
                cancelled: || false
            },
            &mut 256,
            &mut work,
            &mut |row| {
                assert!(rows.len() < 256);
                rows.push(row);
                Ok(())
            }
        )
        .unwrap(),
        source.factories.len() as u64 + source.pending_factory_builds.len() as u64
    );
    if let Some(oracle) = oracle {
        assert_eq!(
            rows.len(),
            source.factories.len() + source.pending_factory_builds.len() + 4
        );
        let state = store.state_session(quota(), MemoryClass::Ordinary).unwrap();
        let mut scan = StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 10_000,
        };
        let mut seen = 0;
        for row in &rows {
            let parsed = WorldFieldKeyV1::from_bytes(&row.mutation.key).unwrap();
            if parsed.kind == WorldFieldRecordKindV1::Metadata {
                continue;
            }
            let field = field(
                if row.namespace == StateNamespaceV1::Business {
                    "factories"
                } else {
                    "pending_factory_builds"
                },
                row.namespace,
            );
            let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key).unwrap();
            let mut payload = Vec::new();
            let info = read_member_record_v1(
                &row.mutation.after.unwrap(),
                &key,
                &mut ValueReadContext {
                    source: &values,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                limits(),
                &mut io::sink(),
                &mut payload,
                || false,
            )
            .unwrap();
            if info.codec == MemberPayloadCodecV1::Unit {
                assert!(payload.is_empty());
                continue;
            }
            assert_eq!(info.codec, MemberPayloadCodecV1::RecordRoot);
            let reference=oasis7_formal_protocol::incremental_record_root::RecordRootRefV1::from_canonical_bytes(&payload).unwrap();
            let schema = if row.namespace == StateNamespaceV1::Business {
                *b"oasis7.factory.state.record.v1\0\0"
            } else {
                *b"oasis7.factory.build.record.v1\0\0"
            };
            let child = reference
                .validate_binding(&schema, &parsed.semantic_key.unwrap(), row.namespace)
                .unwrap();
            StateScanContext {
                source: &state,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut scan,
            }
            .visit_entries(
                child,
                |entry| {
                    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).unwrap();
                    if parsed.kind == WorldFieldRecordKindV1::Metadata {
                        return Ok::<_, ()>(());
                    }
                    seen += 1;
                    let expected = &oracle[&(entry.namespace as u16, entry.key.clone())];
                    let key =
                        WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key).unwrap();
                    let mut payload = Vec::new();
                    let info = read_member_record_v1(
                        &entry.value,
                        &key,
                        &mut ValueReadContext {
                            source: &values,
                            memory,
                            class: MemoryClass::Ordinary,
                            io: &mut logical(),
                        },
                        limits(),
                        &mut io::sink(),
                        &mut payload,
                        || false,
                    )
                    .unwrap();
                    assert_eq!((info.codec, payload), (expected.0, expected.1.clone()));
                    Ok(())
                },
                || false,
            )
            .unwrap();
        }
        assert_eq!(seen, oracle.len());
    }
    let mut roots = RootSetV1::empty("factory-records".into(), [3; 32]).unwrap();
    for (index, namespace) in [
        (0, StateNamespaceV1::Business),
        (2, StateNamespaceV1::Scheduler),
    ] {
        let work = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
        let mut changes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == namespace)
            .map(|r| StateWriteV1 {
                key: r.mutation.key.clone(),
                before: None,
                after: r.mutation.after,
            })
            .collect();
        changes.sort_by(|a, b| a.key.cmp(&b.key));
        let state = store.state_session(quota(), MemoryClass::Ordinary).unwrap();
        let delta = work.retain(
            StateDeltaV1::build(
                StateRootV1::empty(namespace).unwrap(),
                changes,
                &state,
                4 << 20,
            )
            .unwrap(),
        );
        store
            .value_session(quota())
            .put_state_pages(&delta, memory)
            .unwrap();
        roots.state[index] = delta.after.clone();
    }
    roots
}
#[test]
fn full_factory_and_build_source_oracle_survives_cold_roots() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let (source, factory, build) = source();
        let roots = {
            let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let _fixture = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(
                &store,
                &memory,
                &source,
                Some(&expected_oracle(&factory, &build)),
            )
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
            let _caller = memory.reserve(128 << 10, class).unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            for (key, value, index) in [
                (
                    FactoryRecordKeyV1::Factory("outer factory"),
                    FactoryRecordRefV1::Factory(&source.factories["outer factory"]),
                    0,
                ),
                (
                    FactoryRecordKeyV1::Build(u64::MAX),
                    FactoryRecordRefV1::Build(&source.pending_factory_builds[&u64::MAX]),
                    2,
                ),
            ] {
                visit_factory_record_v1(
                    key,
                    value,
                    &mut |cell, payload| -> Result<(), MemberRecordError> {
                        // Each explicit public point query owns a fresh finite
                        // physical operation budget; the API never resets it.
                        let state = store.state_session(physical(), class).unwrap();
                        let values = store.value_session(physical());
                        let mut output = Vec::new();
                        let mut context = WorldFieldReadContextV1 {
                            root: &roots.state[index],
                            state: &state,
                            values: ValueReadContext {
                                source: &values,
                                memory: &memory,
                                class,
                                io: &mut logical(),
                            },
                            key_work: &mut SemanticKeyBudget {
                                bytes_remaining: 16 << 20,
                            },
                            semantic_bytes: 1 << 20,
                            record_bytes: 2 << 20,
                            take_state_error: || state.take_read_error().map(typed),
                            cancelled: || false,
                        };
                        let read =
                            read_factory_cell_v1(key, cell, &mut context, 1 << 20, &mut output)
                                .unwrap_or_else(|error| {
                                    panic!(
                                        "{key:?}/{cell:?}: {error:?}; state={:?}, values={:?}",
                                        state.remaining_io().unwrap(),
                                        values.remaining_io().unwrap()
                                    )
                                });
                        let expected = match payload {
                            MemberPayloadRefV1::Unit => FactoryValueV1::Unit,
                            MemberPayloadRefV1::U64(v) => FactoryValueV1::Unsigned(v),
                            MemberPayloadRefV1::I64(v) => FactoryValueV1::Signed(v),
                            MemberPayloadRefV1::Bool(v) => FactoryValueV1::Bool(v),
                            MemberPayloadRefV1::Text(v) => {
                                assert_eq!(output, v.as_bytes());
                                FactoryValueV1::Text {
                                    utf8_bytes: v.len() as u64,
                                }
                            }
                            MemberPayloadRefV1::Ledger(v) => {
                                let (kind, id) = match v {
                                    oasis7_runtime::types::MaterialLedgerId::World => (0, ""),
                                    oasis7_runtime::types::MaterialLedgerId::Agent(s) => {
                                        (1, s.as_str())
                                    }
                                    oasis7_runtime::types::MaterialLedgerId::Site(s) => {
                                        (2, s.as_str())
                                    }
                                    oasis7_runtime::types::MaterialLedgerId::Factory(s) => {
                                        (3, s.as_str())
                                    }
                                };
                                assert_eq!(output, id.as_bytes());
                                let FactoryCellReadV1::Value(FactoryValueV1::Ledger(info)) = read
                                else {
                                    panic!("ledger summary")
                                };
                                assert_eq!(info.kind as u8, kind);
                                assert_eq!(info.id_bytes, id.len() as u64);
                                return Ok(());
                            }
                            _ => panic!("factory codec"),
                        };
                        assert_eq!(read, FactoryCellReadV1::Value(expected));
                        Ok(())
                    },
                )
                .unwrap();
            }
        }
    }
}

#[path = "support/incremental_factory_overlay.rs"]
mod overlay;
