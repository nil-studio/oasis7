//! Literal-key/source-Serde oracle. It never calls the logistics visitor, schema
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
        "b" => (4, vec![u8::from(value.as_bool().unwrap())]),
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
fn original_rows(name: &str, ns: StateNamespaceV1, value: &Value) -> Rows {
    let mut rows = Rows::new();
    put(&mut rows, name, ns, vec![0], (1, vec![]));
    let table: &[(&str, &str)] = if name == "logistics_routes" {
        &[
            ("route_id", "t"),
            ("from_ledger", "l"),
            ("to_ledger", "l"),
            ("kind", "t"),
            ("distance_km", "i"),
            ("priority", "p"),
            ("owner_agent_id", "t"),
            ("available", "b"),
            ("capacity_units", "i"),
            ("reserved_capacity_units", "i"),
            ("tariff_electricity_per_unit", "i"),
        ]
    } else {
        &[
            ("path_id", "t"),
            ("from_ledger", "l"),
            ("to_ledger", "l"),
            ("kind", "t"),
            ("settled_amount", "i"),
            ("remaining_recipe_amount", "i"),
        ]
    };
    fields(&mut rows, name, ns, "record", Some(value), table);
    if name == "completed_logistics_paths" {
        let values = value["route_ids"].as_array().unwrap();
        let base = collection("route_ids");
        put(
            &mut rows,
            name,
            ns,
            pair(&base, &[0]),
            (2, (values.len() as u64).to_be_bytes().to_vec()),
        );
        for (index, value) in values.iter().enumerate() {
            put(
                &mut rows,
                name,
                ns,
                pair(&base, &unsigned(index as u64)),
                payload("t", Some(value)),
            );
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
            "logistics_routes",
            "route",
            StateNamespaceV1::Business,
            " outer/路线 ",
            serde_json::to_value(&original.logistics_routes[" outer/路线 "]).unwrap(),
        ),
        (
            "completed_logistics_paths",
            "path",
            StateNamespaceV1::Authorization,
            " outer/path ",
            serde_json::to_value(&original.completed_logistics_paths[" outer/path "]).unwrap(),
        ),
        (
            "completed_logistics_paths",
            "path",
            StateNamespaceV1::Authorization,
            "",
            serde_json::to_value(&original.completed_logistics_paths[""]).unwrap(),
        ),
    ];
    let state = store.state_session(physical_scope(), class).unwrap();
    let values = store.value_session(physical_scope());
    let mut io = logical();
    let mut scan = StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 10_000,
    };
    for (name, label, ns, id, value) in cases {
        let token = text(id);
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
        let text = format!("oasis7.logistics.{label}.v1");
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
            io: &mut scan,
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
