//! Literal-key/source-Serde oracle. It never calls the industry visitor, schema
//! enum, semantic encoder, ledger encoder or member writer under test.
use super::*;
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use oasis7_runtime::{
    incremental_field_member::read_member_record_v1, incremental_field_presence::*,
};
use serde_json::Value;
use std::collections::BTreeMap;
const DOMAIN: &[u8] = b"oasis7.iws.semantic-key.v1\0";
fn text(s: &str) -> Vec<u8> {
    let mut b = vec![1];
    b.extend_from_slice(&(s.len() as u64).to_be_bytes());
    b.extend_from_slice(s.as_bytes());
    b
}
fn pair(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = vec![5];
    out.extend_from_slice(a);
    out.extend_from_slice(b);
    out
}
fn unsigned(n: u64) -> Vec<u8> {
    let mut b = vec![2];
    b.extend_from_slice(&n.to_be_bytes());
    b
}
fn semantic(token: &[u8]) -> Vec<u8> {
    let mut b = DOMAIN.to_vec();
    b.extend_from_slice(token);
    b
}
fn digest(token: &[u8]) -> [u8; 32] {
    *blake3::hash(&semantic(token)).as_bytes()
}
pub(super) fn literal_key(
    name: &str,
    ns: StateNamespaceV1,
    kind: u8,
    token: Option<&[u8]>,
) -> Vec<u8> {
    let mut b = b"oasis7.iws.world-field-key.v1\0".to_vec();
    b.push(1);
    b.extend_from_slice(&(ns as u16).to_be_bytes());
    b.push(2);
    b.push(kind);
    b.extend_from_slice(&(name.len() as u16).to_be_bytes());
    b.extend_from_slice(name.as_bytes());
    b.extend_from_slice(&token.map(digest).unwrap_or([0; 32]));
    b
}
fn owned_key(
    name: &'static str,
    ns: StateNamespaceV1,
    kind: u8,
    token: Option<&[u8]>,
) -> WorldFieldKeyV1 {
    let field = FieldDescriptor {
        owner: FieldOwner::WorldState,
        name,
        shape: FieldShape::Map,
        destination: FieldDestination::State(ns),
    };
    let key = WorldFieldKeyV1::new(
        field,
        if kind == 1 {
            WorldFieldRecordKindV1::Metadata
        } else {
            WorldFieldRecordKindV1::Entry
        },
        token.map(digest),
    )
    .unwrap();
    assert_eq!(key.as_bytes(), literal_key(name, ns, kind, token));
    key
}
fn column(section: &str, name: &str) -> Vec<u8> {
    pair(&pair(&[0], &text(section)), &text(name))
}
fn collection(name: &str) -> Vec<u8> {
    pair(&pair(&[0], &text("record")), &text(name))
}
fn payload(kind: &str, value: Option<&Value>) -> (u8, Vec<u8>) {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return (1, vec![]);
    };
    match kind {
        "u" => (2, value.as_u64().unwrap().to_be_bytes().to_vec()),
        "i" => (3, value.as_i64().unwrap().to_be_bytes().to_vec()),
        "t" => (5, value.as_str().unwrap().as_bytes().to_vec()),
        "p" => (
            2,
            match value.as_str().unwrap() {
                "standard" => 0u64,
                "urgent" => 1,
                _ => panic!("unknown original priority"),
            }
            .to_be_bytes()
            .to_vec(),
        ),
        "l" => {
            let value = value.as_str().unwrap();
            let (tag, id) = if value == "world" {
                (0, "")
            } else if let Some(id) = value.strip_prefix("agent:") {
                (1, id)
            } else if let Some(id) = value.strip_prefix("site:") {
                (2, id)
            } else if let Some(id) = value.strip_prefix("factory:") {
                (3, id)
            } else {
                panic!("original ledger")
            };
            let mut b = b"oasis7.iws.ledger-value.v1\0".to_vec();
            b.push(tag);
            b.extend_from_slice(&(id.len() as u64).to_be_bytes());
            b.extend_from_slice(id.as_bytes());
            (6, b)
        }
        _ => panic!("oracle profile"),
    }
}
type Rows = BTreeMap<Vec<u8>, (Vec<u8>, u8, Vec<u8>)>;
fn put(rows: &mut Rows, name: &str, ns: StateNamespaceV1, token: Vec<u8>, value: (u8, Vec<u8>)) {
    let key = literal_key(name, ns, 3, Some(&token));
    assert!(
        rows.insert(key, (semantic(&token), value.0, value.1))
            .is_none()
    );
}
fn fields(
    rows: &mut Rows,
    name: &str,
    ns: StateNamespaceV1,
    section: &str,
    value: Option<&Value>,
    table: &[(&str, &str)],
) {
    for (field, kind) in table {
        put(
            rows,
            name,
            ns,
            column(section, field),
            payload(kind, value.and_then(|v| v.get(field))),
        );
    }
}
fn nested(rows: &mut Rows, name: &str, ns: StateNamespaceV1, value: &Value) {
    let industrial = value.get("industrial_root_ref").filter(|v| !v.is_null());
    if name == "pending_recipe_jobs" {
        put(
            rows,
            name,
            ns,
            column("record", "industrial_root_ref"),
            (4, vec![u8::from(industrial.is_some())]),
        );
        fields(
            rows,
            name,
            ns,
            "industrial_root_ref",
            industrial,
            &[
                ("world_binding", "t"),
                ("domain", "t"),
                ("schema", "t"),
                ("id", "t"),
                ("revision", "u"),
                ("digest", "t"),
            ],
        );
    }
    if matches!(name, "pending_recipe_jobs" | "recipe_completion_receipts") {
        let origin = value
            .get("committed_recipe_origin")
            .filter(|v| !v.is_null());
        put(
            rows,
            name,
            ns,
            column("record", "committed_recipe_origin"),
            (4, vec![u8::from(origin.is_some())]),
        );
        fields(
            rows,
            name,
            ns,
            "committed_recipe_origin",
            origin,
            &[
                ("consensus_action_id", "u"),
                ("consensus_submitter_player_id", "t"),
                ("action_payload_hash", "t"),
                ("committed_height", "u"),
                ("action_root", "t"),
            ],
        );
        fields(
            rows,
            name,
            ns,
            "submission",
            origin.and_then(|v| v.get("submission")),
            &[
                ("verified_player_id", "t"),
                ("public_key", "t"),
                ("auth_nonce", "u"),
                ("hosted_registration_nonce", "t"),
                ("requester_agent_id", "t"),
                ("factory_id", "t"),
                ("recipe_id", "t"),
            ],
        );
    }
}
fn original_rows(name: &str, ns: StateNamespaceV1, value: &Value) -> Rows {
    let mut rows = Rows::new();
    put(&mut rows, name, ns, vec![0], (1, vec![]));
    let scalar: &[(&str, &str)] = match name {
        "pending_recipe_jobs" => &[
            ("emergency_power_world_binding", "t"),
            ("job_id", "u"),
            ("requester_agent_id", "t"),
            ("factory_id", "t"),
            ("recipe_id", "t"),
            ("accepted_batches", "u"),
            ("power_required", "i"),
            ("power_owner_agent_id", "t"),
            ("duration_ticks", "u"),
            ("consume_ledger", "l"),
            ("output_ledger", "l"),
            ("ready_at", "u"),
        ],
        "pending_material_transits" => &[
            ("job_id", "u"),
            ("requester_agent_id", "t"),
            ("from_ledger", "l"),
            ("to_ledger", "l"),
            ("kind", "t"),
            ("amount", "i"),
            ("distance_km", "i"),
            ("loss_bps", "i"),
            ("priority", "p"),
            ("route_id", "t"),
            ("path_id", "t"),
            ("tariff_electricity_total", "i"),
            ("reroute_count", "u"),
            ("ready_at", "u"),
        ],
        "recipe_completion_receipts" => &[
            ("job_id", "u"),
            ("requester_agent_id", "t"),
            ("factory_id", "t"),
            ("recipe_id", "t"),
            ("accepted_batches", "u"),
            ("output_ledger", "l"),
        ],
        "factory_production_failure_dispositions" => &[
            ("action_id", "u"),
            ("requester_agent_id", "t"),
            ("factory_id", "t"),
            ("recipe_id", "t"),
            ("blocker_kind", "t"),
            ("blocker_detail", "t"),
            ("disposition_kind", "t"),
            ("consumed_power", "i"),
            ("lost_power", "i"),
            ("next_action", "t"),
            ("next_recheck", "u"),
        ],
        "logistics_settlement_receipts" => &[
            ("job_id", "u"),
            ("path_id", "t"),
            ("tariff_electricity_total", "i"),
            ("governance_tax_electricity", "i"),
        ],
        _ => panic!("oracle family"),
    };
    fields(&mut rows, name, ns, "record", Some(value), scalar);
    nested(&mut rows, name, ns, value);
    let dynamic: &[(&str, &str)] = match name {
        "pending_recipe_jobs" => &[
            ("consume", "stack"),
            ("produce", "stack"),
            ("byproducts", "stack"),
            ("bottleneck_tags", "text"),
            ("logistics_route_ids", "text"),
            ("logistics_path_ids", "text"),
        ],
        "pending_material_transits" => &[("route_ids", "text")],
        "recipe_completion_receipts" => &[
            ("produce", "stack"),
            ("byproducts", "stack"),
            ("bottleneck_tags", "text"),
            ("logistics_route_ids", "text"),
            ("logistics_path_ids", "text"),
        ],
        "factory_production_failure_dispositions" => {
            &[("consumed_inputs", "stack"), ("lost_inputs", "stack")]
        }
        "logistics_settlement_receipts" => &[("route_ids", "text"), ("owner_payouts", "map")],
        _ => unreachable!(),
    };
    for (field, shape) in dynamic {
        let value = &value[*field];
        let count = if *shape == "map" {
            value.as_object().unwrap().len()
        } else {
            value.as_array().unwrap().len()
        };
        let base = collection(field);
        put(
            &mut rows,
            name,
            ns,
            pair(&base, &[0]),
            (2, (count as u64).to_be_bytes().to_vec()),
        );
        if *shape == "map" {
            for (key, v) in value.as_object().unwrap() {
                put(
                    &mut rows,
                    name,
                    ns,
                    pair(&base, &text(key)),
                    payload("i", Some(v)),
                );
            }
        } else {
            for (index, v) in value.as_array().unwrap().iter().enumerate() {
                let item = pair(&base, &unsigned(index as u64));
                if *shape == "stack" {
                    for (part, kind) in [("kind", "t"), ("amount", "i")] {
                        put(
                            &mut rows,
                            name,
                            ns,
                            pair(&item, &text(part)),
                            payload(kind, v.get(part)),
                        );
                    }
                } else {
                    put(&mut rows, name, ns, item, payload("t", Some(v)));
                }
            }
        }
    }
    rows
}
pub(super) fn assert_complete_original_rows(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    class: MemoryClass,
) {
    // This explicit fixed caller admission precedes the independent source JSON
    // and finite expected-map allocations; the cold read pool is unchanged.
    let _oracle = memory.reserve(512 << 10, class).unwrap();
    let original = super::source();
    let cases = [
        (
            "pending_recipe_jobs",
            "recipe",
            StateNamespaceV1::Scheduler,
            u64::MAX,
            serde_json::to_value(&original.pending_recipe_jobs[&u64::MAX]).unwrap(),
        ),
        (
            "pending_material_transits",
            "transit",
            StateNamespaceV1::Scheduler,
            2,
            serde_json::to_value(&original.pending_material_transits[&2]).unwrap(),
        ),
        (
            "recipe_completion_receipts",
            "completion",
            StateNamespaceV1::Authorization,
            3,
            serde_json::to_value(&original.recipe_completion_receipts[&3]).unwrap(),
        ),
        (
            "factory_production_failure_dispositions",
            "failure",
            StateNamespaceV1::Authorization,
            4,
            serde_json::to_value(&original.factory_production_failure_dispositions[&4]).unwrap(),
        ),
        (
            "logistics_settlement_receipts",
            "settlement",
            StateNamespaceV1::Authorization,
            5,
            serde_json::to_value(&original.logistics_settlement_receipts[&5]).unwrap(),
        ),
    ];
    let state = store.state_session(physical_scope(), class).unwrap();
    let values = store.value_session(physical_scope());
    let mut io = logical();
    for (name, label, ns, id, value) in cases {
        let token = unsigned(id);
        let outer = literal_key(name, ns, 3, Some(&token));
        let key = owned_key(name, ns, 3, Some(&token));
        let root = roots.state.iter().find(|r| r.namespace == ns).unwrap();
        let value_root = root.get(&outer, &state).unwrap().unwrap();
        let mut sem = [0; 128];
        let mut body = [0; 165];
        let mut sem_out = std::io::Cursor::new(sem.as_mut_slice());
        let mut body_out = std::io::Cursor::new(body.as_mut_slice());
        let info = read_member_record_v1(
            &value_root,
            &key,
            &mut ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut io,
            },
            MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 165,
                record_bytes: 8192,
            },
            &mut sem_out,
            &mut body_out,
            &mut || false,
        )
        .unwrap();
        assert_eq!(info.codec as u8, 10);
        assert_eq!(&sem[..info.semantic_bytes as usize], semantic(&token));
        let reference = RecordRootRefV1::from_canonical_bytes(&body).unwrap();
        let mut schema = [0; 32];
        let text = format!("oasis7.industry.{label}.v1");
        schema[..text.len()].copy_from_slice(text.as_bytes());
        let child = reference
            .validate_binding(&schema, &digest(&token), ns)
            .unwrap();
        let mut expected = original_rows(name, ns, &value);
        let metadata = literal_key(name, ns, 1, None);
        let mut meta_seen = false;
        StateScanContext {
            source: &state,
            memory,
            class,
            io: &mut StateScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 256 << 20,
                leaves_remaining: 10_000,
            },
        }
        .visit_entries(
            child,
            |entry| -> Result<(), ValueTreeError> {
                if entry.key == metadata {
                    let key = owned_key(name, ns, 1, None);
                    assert!(!meta_seen);
                    meta_seen = true;
                    read_field_presence_v1(
                        &entry.value,
                        &key,
                        &mut ValueReadContext {
                            source: &values,
                            memory,
                            class,
                            io: &mut io,
                        },
                        PresenceRecordLimitsV1 {
                            semantic_bytes: 0,
                            record_bytes: 8192,
                        },
                        &mut std::io::sink(),
                        &mut || false,
                    )
                    .unwrap();
                    return Ok(());
                }
                let (semantic, codec, payload) = expected
                    .remove(&entry.key)
                    .expect("unexpected or duplicate original row");
                let key = owned_key(name, ns, 3, Some(&semantic[DOMAIN.len()..]));
                assert_eq!(key.as_bytes(), entry.key);
                let mut semantic_out = [0; 512];
                let mut payload_out = [0; 4096];
                let mut key_cursor = std::io::Cursor::new(semantic_out.as_mut_slice());
                let mut payload_cursor = std::io::Cursor::new(payload_out.as_mut_slice());
                let info = read_member_record_v1(
                    &entry.value,
                    &key,
                    &mut ValueReadContext {
                        source: &values,
                        memory,
                        class,
                        io: &mut io,
                    },
                    MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 4096,
                        record_bytes: 8192,
                    },
                    &mut key_cursor,
                    &mut payload_cursor,
                    &mut || false,
                )
                .unwrap();
                assert_eq!(info.codec as u8, codec, "{name}");
                assert_eq!(&semantic_out[..info.semantic_bytes as usize], semantic);
                assert_eq!(&payload_out[..info.payload_bytes as usize], payload);
                Ok(())
            },
            || false,
        )
        .unwrap();
        assert!(meta_seen);
        assert!(expected.is_empty(), "missing original rows: {name}");
    }
}
