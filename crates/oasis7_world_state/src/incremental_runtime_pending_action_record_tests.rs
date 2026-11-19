use super::*;
use crate::incremental_typed_record_test_support::{roundtrip, source_admission};
#[test]
fn stored_nonempty_action_cold_roundtrip_and_original_validation() {
    let _admission = source_admission();
    let record = PendingActionStoredRecordV1 {
        original_id: "a".repeat(64),
        original_bytes: 123,
        envelope: ActionEnvelope {
            governance_authorization: None,
            committed_recipe_origin: None,
            id: u64::MAX,
            action: crate::Action::DeployModuleArtifact {
                publisher_agent_id: "p".into(),
                wasm_hash: "w".into(),
                wasm_bytes: vec![0, 255, 0, 255],
            },
        },
    };
    roundtrip(&record, action::profile());
    assert_eq!(record.original().unwrap().id, [0xaa; 32]);
}
#[test]
fn malformed_original_references_are_closed() {
    let record = |original_id: String, original_bytes| PendingActionStoredRecordV1 {
        original_id,
        original_bytes,
        envelope: ActionEnvelope {
            governance_authorization: None,
            committed_recipe_origin: None,
            id: 1,
            action: crate::Action::QueryObservation {
                agent_id: "a".into(),
            },
        },
    };
    for id in [
        "".into(),
        "a".repeat(63),
        "a".repeat(65),
        "A".repeat(64),
        "g".repeat(64),
    ] {
        assert!(record(id, 1).original().is_err());
    }
    assert!(record("a".repeat(64), 0).original().is_err());
}

pub(crate) fn scheduler_fixture() -> (
    oasis7_proto::memory_budget::MemoryBudget,
    crate::incremental_typed_record_test_support::Pages,
    StateRootV1,
    Vec<u8>,
) {
    use crate::{
        incremental_semantic_key::SemanticKeyBudget,
        incremental_typed_record_test_support::{Pages, io},
    };
    use oasis7_formal_protocol::incremental_state::{StateDeltaV1, StateWriteV1};
    use oasis7_proto::memory_budget::MemoryBudget;
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::with_limit(&memory, 16384);
    let envelope = ActionEnvelope {
        governance_authorization: None,
        committed_recipe_origin: None,
        id: 17,
        action: crate::Action::DeployModuleArtifact {
            publisher_agent_id: "publisher".into(),
            wasm_hash: "hash".into(),
            wasm_bytes: vec![0, 255, 0, 255],
        },
    };
    let original = oasis7_wasm_abi::encode_canonical_cbor(&envelope).unwrap();
    let reference = OriginalArtifactRefV1 {
        id: *blake3::hash(&original).as_bytes(),
        byte_len: original.len() as u64,
    };
    let mut rows = Vec::new();
    crate::incremental_runtime_pending_actions::prepare_runtime_pending_actions_genesis_v1(
        &[envelope].into_iter().collect(),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 16384,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut 10,
        &mut PendingActionWriteWorkV1 {
            cells_remaining: 100000,
            max_pending_node_bytes: 8 << 20,
            json: crate::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 100000,
                json_nodes_remaining: 100000,
                max_json_depth: 32,
                max_pending_node_bytes: 8 << 20,
            },
        },
        &mut |_, _| Ok(reference),
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let writes = rows
        .iter()
        .map(|r| StateWriteV1 {
            key: r.mutation.key.clone(),
            before: r.mutation.before,
            after: r.mutation.after,
        })
        .collect();
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Scheduler).unwrap(),
        writes,
        &pages,
        8 << 20,
    )
    .unwrap();
    let root = delta.after.clone();
    let charged = memory
        .reserve(8 << 20, MemoryClass::Ordinary)
        .unwrap()
        .retain(delta);
    pages.put_state_pages(&charged, &memory).unwrap();
    (memory, pages, root, original)
}
pub(crate) fn recovered_fixture() -> (
    oasis7_proto::memory_budget::MemoryBudget,
    RecoveredPendingActionRecordV1,
    Vec<u8>,
) {
    use crate::{
        incremental_semantic_key::SemanticKeyBudget, incremental_typed_record_test_support::io,
    };
    use oasis7_formal_protocol::incremental_value::ValueReadContext;
    let (memory, pages, root, original) = scheduler_fixture();
    let recovered = recover_pending_action_record_v1(
        PendingActionPositionV1 { era: 0, ordinal: 0 },
        &mut WorldFieldReadContextV1 {
            root: &root,
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            semantic_bytes: 16384,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 10000000,
            read_bytes_remaining: 512 << 20,
            leaves_remaining: 100000,
        },
        &mut PendingActionReadWorkV1 {
            json: crate::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 100000,
                json_nodes_remaining: 100000,
                max_depth: 32,
                max_payload_bytes: 2 << 20,
            },
            max_retained_trees: 4096,
            max_tree_heap: 2 << 20,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        recovered.value().original().unwrap().id,
        *blake3::hash(&original).as_bytes()
    );
    (memory, recovered, original)
}
#[test]
fn actual_scheduler_record_cold_recovery() {
    let (_memory, record, _bytes) = recovered_fixture();
    assert_eq!(record.value().envelope().id, 17);
}
