//! Literal-key/source-Serde oracle. It never calls the agent visitor, schema
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
fn payload(kind: &str, value: Option<&Value>) -> (u8, Vec<u8>) {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return (1, vec![]);
    };
    match kind {
        "u" => (2, value.as_u64().unwrap().to_be_bytes().to_vec()),
        "t" => (5, value.as_str().unwrap().as_bytes().to_vec()),
        "b" => (4, vec![u8::from(value.as_bool().unwrap())]),
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
    let table: &[(&str, &str)] = match name {
        "agent_intent_ledger" => &[
            ("schema_version", "u"),
            ("agent_id", "t"),
            ("intent_id", "t"),
            ("kind", "t"),
            ("summary", "t"),
            ("target_id", "t"),
            ("effect_intent_id", "t"),
            ("intent_tick", "u"),
            ("world_id", "t"),
            ("reorg_epoch", "u"),
            ("authority_scope", "t"),
            ("status", "t"),
            ("source", "t"),
            ("logical_time", "u"),
            ("event_seq", "u"),
            ("updated_at", "u"),
            ("receipt_ref", "t"),
            ("reason_code", "t"),
            ("reason_summary", "t"),
            ("replaced_by", "t"),
            ("actor_id", "t"),
            ("request_digest", "t"),
        ],
        "agent_location_authorities" => &[
            ("agent_id", "t"),
            ("location_id", "t"),
            ("active", "b"),
            ("authority_revision", "u"),
            ("effective_at", "u"),
        ],
        "location_anchors" => &[
            ("location_id", "t"),
            ("active", "b"),
            ("authority_revision", "u"),
            ("effective_at", "u"),
        ],
        _ => panic!("original family"),
    };
    fields(&mut rows, name, ns, "record", Some(value), table);
    rows
}
pub(super) fn assert_complete_original_rows(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    class: MemoryClass,
    original: &WorldState,
) {
    // This explicit fixed caller admission precedes the independent source JSON
    // and finite expected-map allocations; the cold read pool is unchanged.
    let _oracle = memory.reserve(512 << 10, class).unwrap();
    let cases = [
        (
            "agent_intent_ledger",
            "oasis7.agent.intent-ledger.v1",
            StateNamespaceV1::Authorization,
            " outer intent ",
            serde_json::to_value(&original.agent_intent_ledger[" outer intent "]).unwrap(),
        ),
        (
            "agent_location_authorities",
            "oasis7.agent.location.v1",
            StateNamespaceV1::Business,
            " outer agent ",
            serde_json::to_value(&original.agent_location_authorities[" outer agent "]).unwrap(),
        ),
        (
            "location_anchors",
            "oasis7.location.anchor.v1",
            StateNamespaceV1::Business,
            "",
            serde_json::to_value(&original.location_anchors[""]).unwrap(),
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
        let text = label;
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

#[test]
fn literal_original_keys_profiles_and_payloads_cover_every_agent_column() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("oracle");
        let roots = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory, &source())
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        assert_complete_original_rows(&store, &memory, &roots, MemoryClass::Recovery, &source());
    }
}

#[test]
fn every_original_option_preserves_none_and_some_empty_or_zero_wire() {
    for some in [false, true] {
        let directory = Directory::new();
        let path = directory.0.join("all-options");
        let producer = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
        let _fixture = producer.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
        let mut original = source();
        let mut json =
            serde_json::to_value(&original.agent_intent_ledger[" outer intent "]).unwrap();
        for name in [
            "target_id",
            "effect_intent_id",
            "world_id",
            "authority_scope",
            "receipt_ref",
            "reason_code",
            "reason_summary",
            "replaced_by",
        ] {
            json[name] = if some {
                Value::String(String::new())
            } else {
                Value::Null
            };
        }
        for name in ["intent_tick", "reorg_epoch"] {
            json[name] = if some { Value::from(0u64) } else { Value::Null };
        }
        original.agent_intent_ledger.insert(
            " outer intent ".into(),
            serde_json::from_value(json).unwrap(),
        );
        let roots = {
            let store = IncrementalPageStore::create(&path, producer.clone(), PageCompression::Raw)
                .unwrap();
            seed(&store, &producer, &original)
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store = IncrementalPageStore::open(
            &path,
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Recovery,
        )
        .unwrap();
        assert_complete_original_rows(&store, &memory, &roots, MemoryClass::Recovery, &original);
    }
}
