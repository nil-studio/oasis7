use super::*;
use oasis7_formal_protocol::{
    incremental_material::FactLocationV1,
    incremental_state::{StateNamespaceV1, StateRootV1, StateScanBudget},
};
use oasis7_world_state::{
    incremental_cognition_history_cbor::*,
    incremental_cognition_history_lifecycle::CognitionLifecycleStateV1,
    incremental_cognition_lifecycle_index_v2::*,
};
fn parser_work() -> CognitionHistoryCborWorkV1 {
    CognitionHistoryCborWorkV1 {
        bytes_remaining: 1 << 20,
        nodes_remaining: 100000,
        source_reads_remaining: 100000,
        key_bytes_remaining: 1 << 20,
        max_depth: 16,
        max_key_bytes: 4096,
    }
}
fn work() -> CognitionLifecycleWorkV2 {
    CognitionLifecycleWorkV2 {
        key_bytes_remaining: 8 << 20,
        cells_remaining: 1000,
        max_pending_node_bytes: 1 << 20,
    }
}
pub(super) fn event(
    kind: &str,
    seq: u64,
    attempt: u64,
    parent: &str,
    keys: [&str; 5],
) -> serde_json::Value {
    let mut value = serde_json::json!({"schema_version":"cognition-journal-event.v1","journal_seq":seq,"parent_event_digest":parent,"event_kind":kind,"kind":kind,"world_id":"world","branch_id":"branch","finality_epoch":0,"finality_block_hash":null,"finality_status":"pending","reorg_epoch":0,"logical_tick":seq,"retry_seq":0,"transport_attempt":attempt,"status":"done","response_digest":null,"envelope_digest":null,"causal_refs":[]});
    for (name,value_key) in oasis7_world_state::incremental_cognition_history_lifecycle::COGNITION_LIFECYCLE_KEY_FIELDS_V1.into_iter().zip(keys){value[name]=value_key.into();}
    let mut payload = value.clone();
    for name in [
        "journal_seq",
        "parent_event_digest",
        "payload_digest",
        "event_digest",
    ] {
        payload.as_object_mut().unwrap().remove(name);
    }
    value["payload_digest"] = oasis7_world_state::cognition_recovery::cognition_digest_v1(
        "oasis7.cognition.event-payload.v1",
        &payload,
    )
    .into();
    value["event_digest"] = oasis7_world_state::cognition_recovery::cognition_digest_v1(
        "oasis7.cognition.event.v1",
        &value,
    )
    .into();
    value
}
#[test]
fn lifecycle_tuple_raw_and_zstd_cold_rows_preserve_original_keys_and_attempts() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("lifecycle");
        let long = "请求🙂e\u{301}".repeat(1000);
        let keys = [" agent ", "session", "turn", "decision", long.as_str()];
        let (root, projection, expected) = {
            let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical());
            let mut writer = store.value_session(physical());
            let mut read_io = logical();
            let mut read_keys = SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            };
            let anchor = StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap();
            let mut read = WorldFieldReadContextV1 {
                root: &anchor,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut read_io,
                },
                key_work: &mut read_keys,
                semantic_bytes: 1 << 20,
                record_bytes: 2 << 20,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let mut write_io = logical();
            let mut write_keys = SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1 << 20,
                    payload_bytes: 4096,
                    record_bytes: 2 << 20,
                },
                cancelled: || false,
            };
            let mut root = anchor.clone();
            let mut count = 0;
            let mut parent = String::new();
            let mut projection = None;
            let mut expected = None;
            for (ordinal, (kind, attempt, next_state)) in [
                ("TurnStarted", 0, CognitionLifecycleStateV1::Started),
                (
                    "ContextCaptured",
                    0,
                    CognitionLifecycleStateV1::ContextCaptured,
                ),
                (
                    "RequestDispatched",
                    1,
                    CognitionLifecycleStateV1::Dispatched,
                ),
                (
                    "RequestDispatched",
                    2,
                    CognitionLifecycleStateV1::Dispatched,
                ),
                (
                    "ResponseRecorded",
                    0,
                    CognitionLifecycleStateV1::ResponseRecorded,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                for (bad_attempt, message) in if ordinal == 1 {
                    vec![(0, "cognition_lifecycle_order_invalid")]
                } else if ordinal == 2 {
                    vec![
                        (0, "cognition_transport_attempt_invalid"),
                        (2, "cognition_transport_attempt_gap"),
                    ]
                } else {
                    vec![]
                } {
                    let invalid = event(
                        "RequestDispatched",
                        ordinal as u64 + 1,
                        bad_attempt,
                        &parent,
                        keys,
                    );
                    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&invalid).unwrap();
                    let id = *blake3::hash(&bytes).as_bytes();
                    let schema = verify_cognition_history_original_record_schema_v1(
                        CognitionOriginalCborRequestV1 {
                            field: CognitionHistoryFieldV1::JournalEvents,
                            id,
                            bytes: bytes.len() as u64,
                        },
                        &mut bytes.as_slice(),
                        &memory,
                        MemoryClass::Ordinary,
                        &mut parser_work(),
                        || false,
                    )
                    .unwrap();
                    assert!(
                        matches!(prepare_cognition_lifecycle_append_v2(&root,count,CognitionLifecycleAppendV2{schema:&schema,keys,ordinal:ordinal as u64,location:FactLocationV1{original:id,height:ordinal as u64+1,ordinal:1}},&mut read,&mut write,&mut work()),Err(MemberRecordError::Value(ValueTreeError::Invalid(actual))) if actual==message)
                    );
                }
                let value = event(kind, ordinal as u64 + 1, attempt, &parent, keys);
                parent = value["event_digest"].as_str().unwrap().to_owned();
                let bytes = oasis7_wasm_abi::encode_canonical_cbor(&value).unwrap();
                let id = *blake3::hash(&bytes).as_bytes();
                let schema = verify_cognition_history_original_record_schema_v1(
                    CognitionOriginalCborRequestV1 {
                        field: CognitionHistoryFieldV1::JournalEvents,
                        id,
                        bytes: bytes.len() as u64,
                    },
                    &mut bytes.as_slice(),
                    &memory,
                    MemoryClass::Ordinary,
                    &mut parser_work(),
                    || false,
                )
                .unwrap();
                let location = FactLocationV1 {
                    original: id,
                    height: ordinal as u64 + 1,
                    ordinal: 1,
                };
                let prepared = prepare_cognition_lifecycle_append_v2(
                    &root,
                    count,
                    CognitionLifecycleAppendV2 {
                        schema: &schema,
                        keys,
                        ordinal: ordinal as u64,
                        location,
                    },
                    &mut read,
                    &mut write,
                    &mut work(),
                )
                .unwrap();
                root = prepared.root;
                count = prepared.keys;
                assert_eq!(count, 1);
                let projected = *schema.journal_lifecycle().unwrap().keys();
                let row = read_cognition_lifecycle_lookup_v2(
                    &root,
                    &projected,
                    &mut read,
                    &mut StateScanBudget {
                        nodes_remaining: 10000,
                        read_bytes_remaining: 4 << 20,
                        leaves_remaining: 2,
                    },
                    &mut work(),
                )
                .unwrap()
                .unwrap();
                assert_eq!(row.row().state, next_state);
                assert_eq!(
                    row.row().attempt,
                    if ordinal >= 3 {
                        2
                    } else if ordinal == 2 {
                        1
                    } else {
                        0
                    }
                );
                assert_eq!(row.row().original, location);
                assert_eq!(row.keys(), &projected);
                projection = Some(projected);
                expected = Some(row.row());
            }
            (root, projection.unwrap(), expected.unwrap())
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut read = WorldFieldReadContextV1 {
            root: &root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut key_work,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let row = read_cognition_lifecycle_lookup_v2(
            &root,
            &projection,
            &mut read,
            &mut StateScanBudget {
                nodes_remaining: 10000,
                read_bytes_remaining: 4 << 20,
                leaves_remaining: 2,
            },
            &mut work(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(row.row(), expected);
        assert_eq!(row.keys(), &projection);
    }
}
