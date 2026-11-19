//! Fixed continuity enum path is exercised independently of the emergency case.
use crate::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_codec::*,
    incremental_typed_record_test_support::{Writer, continuity_data, io, source_admission},
    incremental_world_rule_composite_family::{
        CompositeBranchCodecV1, accept_world_rule_branch_transition_v1,
        qualify_world_rule_branch_v1, test_support::*,
    },
    incremental_world_rule_continuity::set_world_rule_continuity_case_phase_v1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionTransaction, TransitionVerifier},
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::MemoryClass;
#[test]
fn continuity_fixed_phase_path_preserves_large_unreadable_case_sibling() {
    type Continuity = crate::world_rules::OrganizationContinuityStateV1;
    let _source_admission = source_admission();
    let mut source = continuity_data::fixture_OrganizationContinuityStateV1();
    source.cases.values_mut().next().unwrap().notice_receipt = Some("n".repeat(512 << 10));
    let (memory, pages, roots) = seed(&source);
    let profile = Continuity::BRANCH.profile();
    let mut cold_io = io();
    let mut cold_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut cold = context(&pages, &roots, &memory, &mut cold_io, &mut cold_keys);
    let family = qualify_world_rule_branch_v1::<Continuity, _, _, _, _>(
        &roots,
        &mut cold,
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    let path = [
        Key::Field("cases"),
        Key::Entry(" outer-key "),
        Key::Field("phase"),
        Key::Field("tag"),
    ];
    let expected = selected(family.body().unwrap(), &profile, &path, &mut cold);
    let sibling = selected(
        family.body().unwrap(),
        &profile,
        &[
            Key::Field("cases"),
            Key::Entry(" outer-key "),
            Key::Field("notice_receipt"),
        ],
        &mut cold,
    );
    pages.remove(&sibling);
    let original = parent(&roots);
    let live = original.fence().unwrap();
    let mut transaction =
        ExecutionTransaction::begin(original.clone(), [6; 32], &pages, &memory, limits()).unwrap();
    let mut read_io = io();
    let start = read_io.read_bytes_remaining;
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut write_io = io();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut writer = Writer(&pages);
    let mut output = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut write_keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    };
    let guard = transaction.guard();
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut transaction,
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
    let changed = set_world_rule_continuity_case_phase_v1(
        &family,
        &roots,
        None,
        " outer-key ",
        expected,
        crate::world_rules::OrganizationContinuityPhaseV1::Resolved,
        &pages,
        &mut overlay,
        &mut output,
        &mut scan(),
        &mut read_work(),
        &mut write_work(),
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    assert!(start - overlay.values.io.read_bytes_remaining < 512 << 10);
    drop(overlay);
    let guard = transaction.guard();
    let transition = transaction.seal(&guard, &live, 1).unwrap();
    persist(&pages, &memory, &transition.new_nodes);
    let mut receive_io = io();
    let start = receive_io.read_bytes_remaining;
    let mut receive_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut receive = context(
        &pages,
        &transition.record.after,
        &memory,
        &mut receive_io,
        &mut receive_keys,
    );
    let verifier = TransitionVerifier {
        parent: &original,
        operation: [6; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let accepted = accept_world_rule_branch_transition_v1(
        &family,
        &verifier,
        &transition.record,
        &mut receive,
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    assert_eq!(accepted.body(), Some(changed.body()));
    assert!(start - receive.values.io.read_bytes_remaining < 512 << 10);
    let id = selected(accepted.body().unwrap(), &profile, &path, &mut receive);
    let (key, _) = receive
        .records()
        .key(profile.field, WorldFieldRecordKindV1::Entry, "tag")
        .unwrap();
    let entry = oasis7_formal_protocol::incremental_state::StateEntryV1 {
        namespace: StateNamespaceV1::Authorization,
        key: key.as_bytes().to_vec(),
        value: id,
    };
    let row = crate::incremental_record_columns_read::read_column_row_v1(
        ColumnReadProfileV1 {
            field: profile.field,
            schema: profile.schema,
            groups: &[],
            reference_schema: None,
        },
        &entry,
        &mut receive.records(),
        4096,
    )
    .unwrap();
    assert!(matches!(row.cell, ColumnCellV1::U64(6)));
}
