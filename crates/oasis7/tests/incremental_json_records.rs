#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::*;
use oasis7_formal_protocol::{incremental_state::*, incremental_value::*};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
#[test]
fn neutral_json_profile_keeps_owner_namespace_schema_and_root_scopes_distinct() {
    let directory = fixture::Directory::new();
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("json"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let profile = JsonRecordProfileV1 {
        field: FieldDescriptor {
            owner: FieldOwner::Cognition,
            name: "cognition_json_test_v1",
            shape: FieldShape::Map,
            destination: FieldDestination::State(StateNamespaceV1::Authorization),
        },
        schema: *blake3::hash(b"test cognition JSON profile").as_bytes(),
    };
    let source = serde_json::json!({"":true});
    let mut writer = store.value_session(fixture::physical());
    let mut io = fixture::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let mut work = JsonWriteWorkV1 {
        cells_remaining: 11,
        json_nodes_remaining: 2,
        max_json_depth: 1,
        max_pending_node_bytes: 2 << 20,
    };
    let root = prepare_json_subtree_v1(
        profile,
        &source,
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut io,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 165,
                record_bytes: 8192,
            },
            cancelled: || false,
        },
        &mut work,
    )
    .unwrap();
    assert_eq!(root.namespace, StateNamespaceV1::Authorization);
    assert_eq!(root.leaf_count, 6);
    assert_eq!(work.cells_remaining, 0);
    assert_eq!(work.json_nodes_remaining, 0);
    let state = store
        .state_session(fixture::physical(), MemoryClass::Recovery)
        .unwrap();
    // Full recursive qualification uses the existing bounded full-scan
    // setup profile, independently of the one-node/generic projection fixture.
    // The inherited 100k read refusal is preserved at commit 93291de2d.
    let reader = store.value_session(PageIoBudget {
        reads_remaining: 1_250_000,
        ..fixture::physical()
    });
    let mut io = fixture::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: &root,
        state: &state,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || {
            state
                .take_read_error()
                .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
        },
        cancelled: || false,
    };
    let mut work = JsonReadWorkV1 {
        cells_remaining: 11,
        json_nodes_remaining: 2,
        max_depth: 1,
        max_payload_bytes: 165,
    };
    let proof = qualify_json_subtree_v1(profile, &root, &mut ctx, &mut work).unwrap();
    eprintln!(
        "neutral JSON complete qualification physical remaining={:?}",
        reader.remaining_io().unwrap()
    );
    assert_eq!(proof.exact_node_count(), Some(2));
    assert_eq!(proof.node_count_upper_bound(), 2);
    assert_eq!(work.cells_remaining, 0);
    assert_eq!(work.json_nodes_remaining, 0);
    proof.validate_binding(profile, &root).unwrap();
    let wrong_schema = JsonRecordProfileV1 {
        schema: [4; 32],
        ..profile
    };
    assert!(proof.validate_binding(wrong_schema, &root).is_err());
    let wrong_field = JsonRecordProfileV1 {
        field: FieldDescriptor {
            name: "another_json_owner",
            ..profile.field
        },
        ..profile
    };
    assert!(proof.validate_binding(wrong_field, &root).is_err());
    // A newly requested generic proof must still check nested original RootRefs;
    // it cannot relabel the existing graph with the caller's schema or field.
    for wrong in [
        wrong_schema,
        wrong_field,
        JsonRecordProfileV1 {
            field: FieldDescriptor {
                destination: FieldDestination::State(StateNamespaceV1::Effects),
                ..profile.field
            },
            ..profile
        },
    ] {
        let result = qualify_json_subtree_v1(
            wrong,
            &root,
            &mut ctx,
            &mut JsonReadWorkV1 {
                cells_remaining: 32,
                json_nodes_remaining: 4,
                max_depth: 1,
                max_payload_bytes: 165,
            },
        );
        assert!(
            matches!(result, Err(ValueTreeError::Invalid(_))),
            "{result:?}"
        );
    }
}
