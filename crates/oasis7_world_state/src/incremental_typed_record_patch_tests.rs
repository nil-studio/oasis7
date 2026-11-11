//! Actual path-copy and independent receiver regressions use one page store.
use super::*;
use crate::{
    incremental_semantic_key::{SemanticKeyBudget, write_semantic_key_v1},
    incremental_typed_record_diff::{ChangeDecoder, accept_body},
    incremental_typed_record_test_support::{Pages, Writer, io, source_admission},
    incremental_world_rule_composite::WorldRuleCompositeBranchV1,
};
use oasis7_formal_protocol::incremental_value::ValueReadContext;
use oasis7_proto::memory_budget::MemoryBudget;
#[derive(Debug, PartialEq)]
struct SmallRecord {
    frozen: String,
    status: u64,
}
dto!(SmallRecord,{frozen,status});
fn write_work() -> TypedRecordWriteWorkV1 {
    TypedRecordWriteWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 8 << 20,
        json: crate::incremental_json_records::JsonWriteWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 100_000,
            max_json_depth: 16,
            max_pending_node_bytes: 8 << 20,
        },
    }
}
fn read_work() -> TypedRecordReadWorkV1 {
    TypedRecordReadWorkV1 {
        json: crate::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 100_000,
            max_depth: 16,
            max_payload_bytes: 2 << 20,
        },
        max_retained_trees: 128,
        max_tree_heap: 64 << 10,
    }
}
#[test]
fn typed_path_patch_and_changed_receiver_skip_unreadable_large_sibling() {
    let _source_admission = source_admission();
    let source = SmallRecord {
        frozen: "f".repeat(1 << 20),
        status: 3,
    };
    let memory = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let profile = WorldRuleCompositeBranchV1::OrganizationContinuity.profile();
    let before = prepare_typed_record_body_v1(
        &source,
        &profile,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut write_work(),
    )
    .unwrap();
    let mut values_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: &before,
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut values_io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut scan = StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 32 << 20,
        leaves_remaining: 1000,
    };
    let mut work = read_work();
    accept_body::<SmallRecord, _, _, _, _>(
        &profile,
        None,
        Some(&before),
        &mut ChangeDecoder {
            ctx: &mut ctx,
            scan: &mut scan,
            work: &mut work,
            profile,
            trees_remaining: 128,
            tree_depth: 0,
        },
    )
    .unwrap();
    let status = ctx
        .records()
        .key(profile.field, WorldFieldRecordKindV1::Entry, "status")
        .unwrap()
        .0;
    let frozen = ctx
        .records()
        .key(profile.field, WorldFieldRecordKindV1::Entry, "frozen")
        .unwrap()
        .0;
    let old = ctx.lookup_raw(&before, status.as_bytes()).unwrap().unwrap();
    let frozen_id = ctx.lookup_raw(&before, frozen.as_bytes()).unwrap().unwrap();
    pages.remove(&frozen_id);
    let _path_charge = memory.reserve(1024, MemoryClass::Ordinary).unwrap();
    let mut path = Vec::new();
    write_semantic_key_v1("status", &mut path).unwrap();
    let mut after = TypedTreeBuilderV1::from_root(&before, &memory).unwrap();
    let start = ctx.values.io.read_bytes_remaining;
    patch_typed_path_v1(
        &profile,
        &mut after,
        &[&path],
        Some(old),
        Some(Encoded::Scalar(MemberPayloadRefV1::U64(4))),
        &mut ctx,
        &mut MemberRecordWriteContext {
            sink: &mut Writer(&pages),
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut write_work(),
    )
    .unwrap();
    assert_ne!(after.root(), &*before);
    assert!(start - ctx.values.io.read_bytes_remaining < 512 << 10);
    let mut work = read_work();
    let start = ctx.values.io.read_bytes_remaining;
    accept_body::<SmallRecord, _, _, _, _>(
        &profile,
        Some(&before),
        Some(after.root()),
        &mut ChangeDecoder {
            ctx: &mut ctx,
            scan: &mut scan,
            work: &mut work,
            profile,
            trees_remaining: 128,
            tree_depth: 0,
        },
    )
    .unwrap();
    assert!(start - ctx.values.io.read_bytes_remaining < 512 << 10);
    // A raw path edit is deliberately not a qualification: deleting a required
    // field must still fail the independent closed-schema receiver.
    let new = ctx
        .lookup_raw(after.root(), status.as_bytes())
        .unwrap()
        .unwrap();
    patch_typed_path_v1(
        &profile,
        &mut after,
        &[&path],
        Some(new),
        None,
        &mut ctx,
        &mut MemberRecordWriteContext {
            sink: &mut Writer(&pages),
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut write_work(),
    )
    .unwrap();
    let mut work = read_work();
    assert!(
        accept_body::<SmallRecord, _, _, _, _>(
            &profile,
            Some(&before),
            Some(after.root()),
            &mut ChangeDecoder {
                ctx: &mut ctx,
                scan: &mut scan,
                work: &mut work,
                profile,
                trees_remaining: 128,
                tree_depth: 0
            }
        )
        .is_err()
    );

    drop(ctx);
    let mut pending = WorldFieldReadContextV1 {
        root: &before,
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io(),
        },
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 4096,
        },
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || {
            Some(ValueTreeError::ResourceLimited(
                "injected pending state error",
            ))
        },
        cancelled: || false,
    };
    let mut work = read_work();
    assert!(matches!(
        accept_body::<SmallRecord, _, _, _, _>(
            &profile,
            Some(&before),
            Some(&before),
            &mut ChangeDecoder {
                ctx: &mut pending,
                scan: &mut scan,
                work: &mut work,
                profile,
                trees_remaining: 128,
                tree_depth: 0
            }
        ),
        Err(ValueTreeError::ResourceLimited(
            "injected pending state error"
        ))
    ));
}
