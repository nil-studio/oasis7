use super::*;
use crate::{
    incremental_semantic_key::{SemanticKeyBudget, hash_semantic_key_v1},
    incremental_typed_record_test_support::{Writer, io, source_admission},
    incremental_world_native::{read_body_v1, recover_world_native_record_v1},
    incremental_world_rule_composite_family::test_support::{persist, read_work, scan},
};
use oasis7_formal_protocol::{
    incremental_state::{StateDeltaV1, StateWriteV1},
    incremental_value::ValueReadContext,
};
#[test]
fn native_rotation_persisted_wrong_outer_id_is_rejected_without_leaking_output() {
    let _source = source_admission();
    type F = GovernanceFinalityKeyRotationsFieldV1;
    let (memory, pages, root) = support::seed::<F>(&"rotation".into(), &fixture::rotation());
    let _phase = memory.reserve(16384, MemoryClass::Ordinary).unwrap();
    let profile = F::profile().unwrap();
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
    let body = read_body_v1::<F, _, _, _, _>(&"rotation".into(), &mut read)
        .unwrap()
        .unwrap();
    let metadata = WorldFieldKeyV1::new(
        F::descriptor().unwrap(),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .unwrap();
    let metadata_id = read.lookup(&metadata).unwrap().unwrap();
    let wrong = "different".to_owned();
    let mut write_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let owner =
        hash_semantic_key_v1(&wrong, &memory, MemoryClass::Ordinary, &mut keys, || false).unwrap();
    let reference = RecordRootRefV1::new(profile.schema, owner.digest, (*body).clone()).unwrap();
    let row = write_member_record_v1(
        F::descriptor().unwrap(),
        WorldFieldRecordKindV1::Entry,
        &wrong,
        MemberPayloadRefV1::RecordRoot(&reference),
        &mut MemberRecordWriteContext {
            sink: &mut Writer(&pages),
            memory: &memory,
            io: &mut write_io,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let mut writes = vec![
        StateWriteV1 {
            key: metadata.as_bytes().to_vec(),
            before: None,
            after: Some(metadata_id),
        },
        StateWriteV1 {
            key: row.field_key.as_bytes().to_vec(),
            before: None,
            after: Some(row.value_root),
        },
    ];
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
    let mut io_read = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut corrupted = WorldFieldReadContextV1 {
        root: &delta.after,
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io_read,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let before = memory.used();
    assert!(
        recover_world_native_record_v1::<F>(&wrong, &mut corrupted, &mut scan(), &mut read_work())
            .is_err()
    );
    assert_eq!(memory.used(), before);
}
#[test]
fn native_rotation_key_binding_reads_only_bounded_original_id_path() {
    let _source = source_admission();
    type F = GovernanceFinalityKeyRotationsFieldV1;
    let (memory, pages, root) = support::seed::<F>(&"rotation".into(), &fixture::rotation());
    let mut io_read = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: &root,
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io_read,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let body = read_body_v1::<F, _, _, _, _>(&"rotation".into(), &mut read)
        .unwrap()
        .unwrap();
    let profile = F::profile().unwrap();
    let mut work = read_work();
    let mut traversal = scan();
    let mut d = crate::incremental_typed_record_diff::ChangeDecoder {
        ctx: &mut read,
        scan: &mut traversal,
        work: &mut work,
        profile,
        trees_remaining: 4096,
        tree_depth: 0,
    };
    let value = crate::incremental_typed_record_diff::point_typed_column_v1(
        &profile, &body, "value", &mut d, 4096,
    )
    .unwrap();
    let ColumnCellV1::Root(record) = &value.cell else {
        panic!("record wrapper")
    };
    let (key, _) = d
        .ctx
        .records()
        .key(
            profile.field,
            WorldFieldRecordKindV1::Entry,
            "applied_at_tick",
        )
        .unwrap();
    let unrelated = d.ctx.lookup_raw(record, key.as_bytes()).unwrap().unwrap();
    drop(d);
    pages.remove(&unrelated);
    // This hook grants only the exact key relation. Full record validation is
    // separately required and cannot be replaced by this point proof.
    F::validate_record_binding(
        WorldNativeKeyRefV1::Text("rotation"),
        &body,
        &mut read,
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    assert!(
        recover_world_native_record_v1::<F>(
            &"rotation".into(),
            &mut read,
            &mut scan(),
            &mut read_work()
        )
        .is_err()
    );
    let io_before = read.values.io.read_bytes_remaining;
    assert!(
        F::validate_record_binding(
            WorldNativeKeyRefV1::Text(" rotation "),
            &body,
            &mut read,
            &mut scan(),
            &mut read_work()
        )
        .is_err()
    );
    assert_eq!(read.values.io.read_bytes_remaining, io_before);
}
