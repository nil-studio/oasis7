use super::*;
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use oasis7_runtime::incremental_field_presence::{PresenceRecordLimitsV1, read_field_presence_v1};
use std::collections::BTreeMap;
const SEMANTIC: &[u8] = b"oasis7.iws.semantic-key.v1\0";
fn text(bytes: &mut Vec<u8>, s: &str) {
    bytes.push(1);
    bytes.extend_from_slice(&(s.len() as u64).to_be_bytes());
    bytes.extend_from_slice(s.as_bytes());
}
fn unit() -> Vec<u8> {
    let mut v = SEMANTIC.to_vec();
    v.push(0);
    v
}
fn original_text(s: &str) -> Vec<u8> {
    let mut v = SEMANTIC.to_vec();
    text(&mut v, s);
    v
}
fn original_u64(n: u64) -> Vec<u8> {
    let mut v = SEMANTIC.to_vec();
    v.push(2);
    v.extend_from_slice(&n.to_be_bytes());
    v
}
fn path(names: &[&str]) -> Vec<u8> {
    let mut v = SEMANTIC.to_vec();
    v.extend(std::iter::repeat_n(5, names.len()));
    v.push(0);
    for name in names {
        text(&mut v, name)
    }
    v
}
fn descriptor(name: &str) -> FieldDescriptor {
    let (name, shape, ns) = match name {
        "main_token_config" => (
            "main_token_config",
            FieldShape::Value,
            StateNamespaceV1::Business,
        ),
        "main_token_genesis_buckets" => (
            "main_token_genesis_buckets",
            FieldShape::Map,
            StateNamespaceV1::Business,
        ),
        "main_token_epoch_issuance_records" => (
            "main_token_epoch_issuance_records",
            FieldShape::Map,
            StateNamespaceV1::Authorization,
        ),
        "restricted_starter_claim_grants" => (
            "restricted_starter_claim_grants",
            FieldShape::Map,
            StateNamespaceV1::Authorization,
        ),
        "main_token_scheduled_policy_updates" => (
            "main_token_scheduled_policy_updates",
            FieldShape::Map,
            StateNamespaceV1::Scheduler,
        ),
        _ => panic!("unknown original descriptor"),
    };
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name,
        shape,
        destination: FieldDestination::State(ns),
    }
}
type Expected = BTreeMap<Vec<u8>, (u8, Vec<u8>)>;
fn flatten<'a>(v: &'a serde_json::Value, names: &mut Vec<&'a str>, out: &mut Expected) {
    if let serde_json::Value::Object(fields) = v {
        for (name, value) in fields {
            names.push(name);
            flatten(value, names, out);
            names.pop();
        }
        return;
    }
    let payload = match v {
        serde_json::Value::Null => (1, vec![]),
        serde_json::Value::String(s) => (5, s.as_bytes().to_vec()),
        serde_json::Value::Number(n) => (2, n.as_u64().unwrap().to_be_bytes().to_vec()),
        _ => panic!("unexpected original DTO primitive"),
    };
    assert!(out.insert(path(names), payload).is_none());
}
#[test]
fn literal_original_keys_and_every_primitive_match_raw_member_frames() {
    // Serde is an independent test oracle only; production borrows closed DTO fields.
    // The entire source/oracle is explicitly caller-admitted, not a C<T measurement.
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let _oracle = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let original = source();
    let store = IncrementalPageStore::create(
        &directory.0.join("literal"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = seed(&store, &memory, &original);
    let mut expected = BTreeMap::<(String, [u8; 32]), (Vec<u8>, serde_json::Value)>::new();
    let mut add = |name: &str, key: Vec<u8>, value: serde_json::Value| {
        let digest = *blake3::hash(&key).as_bytes();
        assert!(
            expected
                .insert((name.to_owned(), digest), (key, value))
                .is_none()
        );
    };
    add(
        "main_token_config",
        unit(),
        serde_json::to_value(&original.main_token_config).unwrap(),
    );
    for (id, v) in &original.main_token_genesis_buckets {
        add(
            "main_token_genesis_buckets",
            original_text(id),
            serde_json::to_value(v).unwrap(),
        );
    }
    for (id, v) in &original.main_token_epoch_issuance_records {
        add(
            "main_token_epoch_issuance_records",
            original_u64(*id),
            serde_json::to_value(v).unwrap(),
        );
    }
    for (id, v) in &original.restricted_starter_claim_grants {
        let mut value = serde_json::to_value(v).unwrap();
        let object = value.as_object_mut().unwrap();
        object
            .entry("status_updated_at_epoch")
            .or_insert(serde_json::Value::Null);
        object
            .entry("status_reason")
            .or_insert(serde_json::Value::Null);
        add("restricted_starter_claim_grants", original_text(id), value);
    }
    for (id, v) in &original.main_token_scheduled_policy_updates {
        add(
            "main_token_scheduled_policy_updates",
            original_u64(*id),
            serde_json::to_value(v).unwrap(),
        );
    }
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut value_io = logical();
    let mut value_context = ValueReadContext {
        source: &values,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut value_io,
    };
    let mut outer_scan = scan();
    let mut inner_scan = scan();
    let mut metadata = 0;
    let mut formats = 0;
    let mut records = 0;
    for root in &roots.state {
        StateScanContext {
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut outer_scan,
        }
        .visit_entries(
            root,
            |entry| -> Result<(), ValueTreeError> {
                let binding = WorldFieldKeyV1::from_bytes(&entry.key).unwrap();
                let field = descriptor(binding.name);
                let key = WorldFieldKeyV1::new(field, binding.kind, binding.semantic_key).unwrap();
                assert_eq!(key.as_bytes(), entry.key);
                assert_eq!(binding.namespace, entry.namespace);
                if binding.kind == WorldFieldRecordKindV1::Metadata {
                    let info = read_field_presence_v1(
                        &entry.value,
                        &key,
                        &mut value_context,
                        PresenceRecordLimitsV1 {
                            semantic_bytes: 4096,
                            record_bytes: 4096,
                        },
                        &mut std::io::sink(),
                        || false,
                    )?;
                    assert_eq!(info.semantic_bytes, 0);
                    metadata += 1;
                    return Ok(());
                }
                let mut semantic = Vec::new();
                let mut payload = Vec::new();
                let info = read_member_record_v1(
                    &entry.value,
                    &key,
                    &mut value_context,
                    MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 165,
                        record_bytes: 4096,
                    },
                    &mut semantic,
                    &mut payload,
                    || false,
                )?;
                let mut format = SEMANTIC.to_vec();
                format.push(5);
                text(&mut format, "main_token_policy_record_closure_v1");
                format.push(0);
                if semantic == format {
                    assert_eq!(info.codec as u8, 1);
                    assert!(payload.is_empty());
                    formats += 1;
                    return Ok(());
                }
                let (expected_key, json) = expected
                    .remove(&(binding.name.to_owned(), binding.semantic_key.unwrap()))
                    .expect("unrecognized original outer key");
                assert_eq!(semantic, expected_key);
                assert_eq!(info.codec as u8, 10);
                let reference = RecordRootRefV1::from_canonical_bytes(payload.as_slice()).unwrap();
                let mut schema = blake3::Hasher::new();
                schema.update(b"oasis7.iws.main-token-policy.v1\0");
                schema.update(binding.name.as_bytes());
                let child = reference
                    .validate_binding(
                        schema.finalize().as_bytes(),
                        blake3::hash(&expected_key).as_bytes(),
                        entry.namespace,
                    )
                    .unwrap();
                let mut expected_cells = Expected::new();
                expected_cells.insert(unit(), (1, vec![]));
                flatten(&json, &mut Vec::new(), &mut expected_cells);
                let mut child_meta = 0;
                StateScanContext {
                    source: &state,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut inner_scan,
                }
                .visit_entries(
                    child,
                    |entry| -> Result<(), ValueTreeError> {
                        let binding = WorldFieldKeyV1::from_bytes(&entry.key).unwrap();
                        let actual =
                            WorldFieldKeyV1::new(field, binding.kind, binding.semantic_key)
                                .unwrap();
                        assert_eq!(actual.as_bytes(), entry.key);
                        assert_eq!(entry.namespace, child.namespace);
                        if binding.kind == WorldFieldRecordKindV1::Metadata {
                            let info = read_field_presence_v1(
                                &entry.value,
                                &actual,
                                &mut value_context,
                                PresenceRecordLimitsV1 {
                                    semantic_bytes: 4096,
                                    record_bytes: 4096,
                                },
                                &mut std::io::sink(),
                                || false,
                            )?;
                            assert_eq!(info.semantic_bytes, 0);
                            child_meta += 1;
                            return Ok(());
                        }
                        let mut semantic = Vec::new();
                        let mut payload = Vec::new();
                        let info = read_member_record_v1(
                            &entry.value,
                            &actual,
                            &mut value_context,
                            MemberRecordLimitsV1 {
                                semantic_bytes: 4096,
                                payload_bytes: 165,
                                record_bytes: 4096,
                            },
                            &mut semantic,
                            &mut payload,
                            || false,
                        )?;
                        let (codec, expected_payload) = expected_cells
                            .remove(&semantic)
                            .expect("unknown or duplicate original column path");
                        assert_eq!(info.codec as u8, codec);
                        assert_eq!(payload, expected_payload);
                        Ok(())
                    },
                    || false,
                )
                .unwrap();
                assert_eq!(child_meta, 1);
                assert!(
                    expected_cells.is_empty(),
                    "missing original columns: {expected_cells:?}"
                );
                records += 1;
                Ok(())
            },
            || false,
        )
        .unwrap();
    }
    assert_eq!((metadata, formats, records), (5, 5, 6));
    assert!(expected.is_empty());
}
