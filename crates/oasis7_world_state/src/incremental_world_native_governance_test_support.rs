//! Tests exercise the real native metadata/outer-key envelope and cold reader.
use super::*;
use crate::{
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, io},
    incremental_world_native::{WorldNativeKeyV1, recover_world_native_record_v1},
    incremental_world_rule_composite_family::test_support::{persist, read_work, scan, write_work},
};
use oasis7_formal_protocol::{
    incremental_state::{StateDeltaV1, StateWriteV1},
    incremental_value::ValueReadContext,
};
pub(super) fn seed<F: WorldNativeFieldSpecV1>(
    key: &F::Key,
    value: &F::Value,
) -> (MemoryBudget, Pages, ReservedValue<StateRootV1>) {
    let memory = MemoryBudget::new(256 << 20, 8 << 20).unwrap();
    let mut pages = Pages::with_limit(&memory, 131_072);
    let _rows = memory.reserve(4096, MemoryClass::Ordinary).unwrap();
    let mut rows = Vec::with_capacity(2);
    let mut write_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut records = 100_000;
    let mut typed = write_work();
    let mut ctx = MemberRecordWriteContext {
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
    prepare_world_native_metadata_v1::<F>(&mut ctx, &mut records, &mut |row| {
        assert!(rows.len() < 2);
        rows.push(row);
        Ok(())
    })
    .unwrap();
    prepare_world_native_record_v1::<F>(
        key,
        value,
        &mut ctx,
        &mut records,
        &mut typed,
        &mut |row| {
            assert!(rows.len() < 2);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    drop(ctx);
    assert_eq!(records, 99_998);
    let mut writes: Vec<StateWriteV1> = rows.iter().map(|r| r.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let _delta = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
        writes,
        &pages,
        8 << 20,
    )
    .unwrap();
    persist(&pages, &memory, &delta.new_nodes);
    let root = memory
        .reserve(512, MemoryClass::Ordinary)
        .unwrap()
        .retain(delta.after.clone());
    (memory, pages, root)
}
pub(super) fn field<F: WorldNativeFieldSpecV1>(key: &F::Key, value: &F::Value) -> [u8; 32]
where
    F::Value: PartialEq + std::fmt::Debug,
{
    let (memory, pages, root) = seed::<F>(key, value);
    let mut read_io = io();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: &root,
        state: &pages,
        values: ValueReadContext {
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
    let recovered =
        recover_world_native_record_v1::<F>(key, &mut read, &mut scan(), &mut read_work())
            .unwrap()
            .unwrap();
    assert_eq!(recovered.value().value(), value);
    // Full qualification of the same selected record retains no whole DTO.
    let body = crate::incremental_world_native::read_body_v1::<F, _, _, _, _>(key, &mut read)
        .unwrap()
        .unwrap();
    let mut work = read_work();
    let trees_remaining = work.max_retained_trees;
    let profile = F::profile().unwrap();
    crate::incremental_typed_record_diff::accept_body::<
        crate::incremental_world_native::WorldNativeRecordV1<F::Value>,
        _,
        _,
        _,
        _,
    >(
        &profile,
        None,
        Some(&body),
        &mut crate::incremental_typed_record_diff::ChangeDecoder {
            ctx: &mut read,
            scan: &mut scan(),
            work: &mut work,
            profile,
            trees_remaining,
            tree_depth: 0,
        },
    )
    .unwrap();
    F::validate_record_binding(
        key.key_ref(),
        &body,
        &mut read,
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    *blake3::Hash::from_hex(&root.node_root).unwrap().as_bytes()
}
