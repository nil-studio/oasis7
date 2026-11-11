//! Structural delta inheritance only; a missing physical page cannot grant WorldReady.
//! Full physical inventory leases remain an independent owner obligation.
use super::*;
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_typed_record_test_support::{Pages, io};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
fn profile() -> TypedRecordProfileV1 {
    TypedRecordProfileV1 {
        field: FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: "typed_json_delta.columns",
            shape: FieldShape::Map,
            destination: FieldDestination::State(StateNamespaceV1::Business),
        },
        schema: [29; 32],
        reference_schema: None,
        labels: RecordTreeLabelsV1 {
            read: "json test read",
            pending: "json test pending",
            tree: "json test tree",
            digest: "json test digest",
            read_allowance: "json test read allowance",
            admission: "json test admission",
            bound: "json test bound",
            producer_pool: "json test pool",
            page: "json test page",
            write_allowance: "json test write allowance",
        },
    }
}
#[test]
fn typed_json_delta_inherits_large_unchanged_sibling_without_hydration() {
    let memory = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::with_limit(&memory, 65536);
    let stable = serde_json::Value::String("unchanged".repeat(32768));
    let before = serde_json::json!({"stable":stable,"changed":0});
    let after = serde_json::json!({"stable":stable,"changed":1});
    let mut write_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut pages,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    };
    let mut ww = TypedRecordWriteWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 8 << 20,
        json: crate::incremental_json_records::JsonWriteWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 10000,
            max_json_depth: 32,
            max_pending_node_bytes: 8 << 20,
        },
    };
    let old = prepare_typed_record_body_v1(&before, &profile(), &mut write, &mut ww).unwrap();
    let new = prepare_typed_record_body_v1(&after, &profile(), &mut write, &mut ww).unwrap();
    let unchanged = prepare_typed_record_body_v1(&stable, &profile(), &mut write, &mut ww).unwrap();
    let mut read_io = io();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: &old,
        state: &pages,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut read_keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut work = TypedRecordReadWorkV1 {
        json: crate::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 10000,
            max_depth: 32,
            max_payload_bytes: 2 << 20,
        },
        max_retained_trees: 32,
        max_tree_heap: 4096,
    };
    let jp = crate::incremental_json_records::JsonRecordProfileV1 {
        field: profile().field,
        schema: profile().schema,
    };
    let parent = crate::incremental_json_records::qualify_json_subtree_records_v1(
        jp,
        &old,
        &mut ctx.records(),
        &mut work.json,
    )
    .unwrap();
    pages.remove(
        blake3::Hash::from_hex(&unchanged.node_root)
            .unwrap()
            .as_bytes(),
    );
    let mut scan = StateScanBudget {
        nodes_remaining: 1000000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 10000,
    };
    let mut d = ChangeDecoder {
        ctx: &mut ctx,
        scan: &mut scan,
        work: &mut work,
        profile: profile(),
        trees_remaining: 32,
        tree_depth: 0,
    };
    accept(
        &profile(),
        Some(&TypedCellRefV1::Body(parent.root())),
        Some(&TypedCellRefV1::Body(&new)),
        &mut d,
    )
    .unwrap();
    drop(d);
    // Measure both real traversals under a generous request, then grant less
    // than their combined cost. Separate minima would incorrectly accept it.
    let mut measured = crate::incremental_json_records::JsonChangeWorkV1 {
        json: crate::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 10000,
            max_depth: 32,
            max_payload_bytes: 2 << 20,
        },
        paths_remaining: 100000,
        max_pending_pairs: 32,
    };
    crate::incremental_json_records::advance_from_qualified_root(
        jp,
        parent.root(),
        &new,
        &mut ctx.records(),
        &mut measured,
    )
    .unwrap();
    let json_spent = 100000 - measured.json.cells_remaining;
    let path_spent = 100000 - measured.paths_remaining;
    assert!(json_spent > 0 && path_spent > 0);
    let mut limited = TypedRecordReadWorkV1 {
        json: crate::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: json_spent + path_spent - 1,
            json_nodes_remaining: 10000,
            max_depth: 32,
            max_payload_bytes: 2 << 20,
        },
        max_retained_trees: 32,
        max_tree_heap: 4096,
    };
    let mut d = ChangeDecoder {
        ctx: &mut ctx,
        scan: &mut scan,
        work: &mut limited,
        profile: profile(),
        trees_remaining: 32,
        tree_depth: 0,
    };
    assert!(matches!(
        accept(
            &profile(),
            Some(&TypedCellRefV1::Body(parent.root())),
            Some(&TypedCellRefV1::Body(&new)),
            &mut d
        ),
        Err(ValueTreeError::ResourceLimited("typed JSON shared cells"))
    ));
}
