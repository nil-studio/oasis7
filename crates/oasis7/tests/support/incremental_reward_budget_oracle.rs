//! Independent original-Serde field table and literal key/payload oracle.
use super::*;
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use oasis7_runtime::incremental_field_presence::*;
use serde_json::Value;
use std::collections::BTreeMap;
const DOMAIN: &[u8] = b"oasis7.iws.semantic-key.v1\0";
fn text(v: &str) -> Vec<u8> {
    let mut b = vec![1];
    b.extend_from_slice(&(v.len() as u64).to_be_bytes());
    b.extend_from_slice(v.as_bytes());
    b
}
fn number(v: u64) -> Vec<u8> {
    let mut b = vec![2];
    b.extend_from_slice(&v.to_be_bytes());
    b
}
fn pair(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut v = vec![5];
    v.extend_from_slice(a);
    v.extend_from_slice(b);
    v
}
fn semantic(token: &[u8]) -> Vec<u8> {
    let mut v = DOMAIN.to_vec();
    v.extend_from_slice(token);
    v
}
fn descriptor(pool: bool) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: if pool {
            "system_order_pool_budgets"
        } else {
            "reward_mint_records"
        },
        shape: if pool {
            FieldShape::Map
        } else {
            FieldShape::Ordered
        },
        destination: FieldDestination::State(if pool {
            StateNamespaceV1::Business
        } else {
            StateNamespaceV1::Authorization
        }),
    }
}
fn key(pool: bool, token: Option<&[u8]>) -> WorldFieldKeyV1 {
    let f = descriptor(pool);
    let hash = token.map(|v| *blake3::hash(&semantic(v)).as_bytes());
    let key = WorldFieldKeyV1::new(
        f,
        if token.is_some() {
            WorldFieldRecordKindV1::Entry
        } else {
            WorldFieldRecordKindV1::Metadata
        },
        hash,
    )
    .unwrap();
    let mut raw = b"oasis7.iws.world-field-key.v1\0".to_vec();
    raw.push(1);
    let FieldDestination::State(ns) = f.destination else {
        unreachable!()
    };
    raw.extend_from_slice(&(ns as u16).to_be_bytes());
    raw.push(if pool { 2 } else { 6 });
    raw.push(if token.is_some() { 3 } else { 1 });
    raw.extend_from_slice(&(f.name.len() as u16).to_be_bytes());
    raw.extend_from_slice(f.name.as_bytes());
    raw.extend_from_slice(&hash.unwrap_or([0; 32]));
    assert_eq!(key.as_bytes(), raw);
    key
}
type Rows = BTreeMap<Vec<u8>, (Vec<u8>, u8, Vec<u8>)>;
fn put(rows: &mut Rows, pool: bool, token: Vec<u8>, codec: u8, payload: Vec<u8>) {
    assert!(
        rows.insert(
            key(pool, Some(&token)).as_bytes().to_vec(),
            (semantic(&token), codec, payload)
        )
        .is_none()
    );
}
fn expected(pool: bool, source: &Value) -> Rows {
    let mut rows = Rows::new();
    put(&mut rows, pool, vec![0], 1, vec![]);
    let columns: &[(&str, bool)] = if pool {
        &[
            ("epoch_index", false),
            ("total_credit_budget", false),
            ("remaining_credit_budget", false),
        ]
    } else {
        &[
            ("epoch_index", false),
            ("node_id", true),
            ("source_awarded_points", false),
            ("minted_power_credits", false),
            ("settlement_hash", true),
            ("signer_node_id", true),
            ("signature", true),
        ]
    };
    for (name, is_text) in columns {
        let value = &source[name];
        put(
            &mut rows,
            pool,
            pair(&[0], &text(name)),
            if *is_text { 5 } else { 2 },
            if *is_text {
                value.as_str().unwrap().as_bytes().to_vec()
            } else {
                value.as_u64().unwrap().to_be_bytes().to_vec()
            },
        );
    }
    if pool {
        for name in ["node_credit_caps", "node_credit_allocated"] {
            let prefix = pair(&[0], &text(name));
            put(&mut rows, pool, pair(&prefix, &[0]), 1, vec![]);
            for (id, value) in source[name].as_object().unwrap() {
                put(
                    &mut rows,
                    pool,
                    pair(&prefix, &text(id)),
                    2,
                    value.as_u64().unwrap().to_be_bytes().to_vec(),
                );
            }
        }
    }
    rows
}
#[test]
fn literal_ordered_and_map_keys_preserve_every_original_reward_column_and_member() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("literal");
        let roots = {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory, &source())
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let _oracle = memory.reserve(512 << 10, MemoryClass::Recovery).unwrap();
        let original = source();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut scanning = scan();
        let records = original
            .system_order_pool_budgets
            .iter()
            .map(|(index, value)| (true, *index, serde_json::to_value(value).unwrap()))
            .chain(
                original
                    .reward_mint_records
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        (false, index as u64, serde_json::to_value(value).unwrap())
                    }),
            );
        for (pool, index, original) in records {
            let FieldDestination::State(ns) = descriptor(pool).destination else {
                unreachable!()
            };
            let top = roots
                .state
                .iter()
                .find(|root| root.namespace == ns)
                .unwrap();
            let token = number(index);
            let outer = key(pool, Some(&token));
            let root = top.get(outer.as_bytes(), &state).unwrap().unwrap();
            let mut sem = [0; 128];
            let mut payload = [0; 165];
            let info = read_member_record_v1(
                &root,
                &outer,
                &mut ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Recovery,
                    io: &mut io,
                },
                MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 165,
                    record_bytes: 8192,
                },
                &mut std::io::Cursor::new(sem.as_mut_slice()),
                &mut std::io::Cursor::new(payload.as_mut_slice()),
                &mut || false,
            )
            .unwrap();
            assert_eq!(info.codec as u8, 10);
            assert_eq!(&sem[..info.semantic_bytes as usize], semantic(&token));
            let mut h = blake3::Hasher::new();
            h.update(b"oasis7.iws.reward-records.v1\0");
            h.update(descriptor(pool).name.as_bytes());
            let reference = RecordRootRefV1::from_canonical_bytes(&payload).unwrap();
            let child = reference
                .validate_binding(
                    h.finalize().as_bytes(),
                    blake3::hash(&semantic(&token)).as_bytes(),
                    ns,
                )
                .unwrap();
            let mut expected = expected(pool, &original);
            let metadata = key(pool, None);
            let mut meta_seen = false;
            StateScanContext {
                source: &state,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut scanning,
            }
            .visit_entries(
                child,
                |entry| -> Result<(), ValueTreeError> {
                    if entry.key == metadata.as_bytes() {
                        assert!(!meta_seen);
                        meta_seen = true;
                        read_field_presence_v1(
                            &entry.value,
                            &metadata,
                            &mut ValueReadContext {
                                source: &values,
                                memory: &memory,
                                class: MemoryClass::Recovery,
                                io: &mut io,
                            },
                            PresenceRecordLimitsV1 {
                                semantic_bytes: 0,
                                record_bytes: 4096,
                            },
                            &mut std::io::sink(),
                            &mut || false,
                        )
                        .unwrap();
                        return Ok(());
                    }
                    let (preimage, codec, payload) = expected
                        .remove(&entry.key)
                        .expect("unknown/duplicate literal reward row");
                    let field_key = key(pool, Some(&preimage[DOMAIN.len()..]));
                    assert_eq!(field_key.as_bytes(), entry.key);
                    let mut sem = [0; 512];
                    let mut bytes = [0; 4096];
                    let info = read_member_record_v1(
                        &entry.value,
                        &field_key,
                        &mut ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class: MemoryClass::Recovery,
                            io: &mut io,
                        },
                        MemberRecordLimitsV1 {
                            semantic_bytes: 4096,
                            payload_bytes: 4096,
                            record_bytes: 8192,
                        },
                        &mut std::io::Cursor::new(sem.as_mut_slice()),
                        &mut std::io::Cursor::new(bytes.as_mut_slice()),
                        &mut || false,
                    )
                    .unwrap();
                    assert_eq!(info.codec as u8, codec);
                    assert_eq!(&sem[..info.semantic_bytes as usize], preimage);
                    assert_eq!(&bytes[..info.payload_bytes as usize], payload);
                    Ok(())
                },
                || false,
            )
            .unwrap();
            assert!(meta_seen);
            assert!(expected.is_empty());
        }
        let top = roots
            .state
            .iter()
            .find(|root| root.namespace == StateNamespaceV1::Authorization)
            .unwrap();
        let length_key = key(
            false,
            Some(&pair(
                &text("reward_budget_record_closure_v1"),
                &text("length"),
            )),
        );
        let root = top.get(length_key.as_bytes(), &state).unwrap().unwrap();
        let mut payload = [0; 8];
        let info = read_member_record_v1(
            &root,
            &length_key,
            &mut ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 8,
                record_bytes: 8192,
            },
            &mut std::io::sink(),
            &mut std::io::Cursor::new(payload.as_mut_slice()),
            &mut || false,
        )
        .unwrap();
        assert_eq!(info.codec as u8, 2);
        assert_eq!(u64::from_be_bytes(payload), 3);
    }
}
