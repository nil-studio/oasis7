use super::test_support::*;
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer, continuity_data, io, source_admission},
    incremental_world_rule_continuity::{
        EmergencySupplyOverlayV1, set_world_rule_emergency_review_decision_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction},
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
type Emergency = crate::world_rules::EmergencySupplyStateV1;
fn bind_current_emergency(
    family: &ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
    transaction: &mut ExecutionTransaction<'_, Pages>,
    live: &ExecutionFence,
) -> Result<ReservedValue<EmergencySupplyOverlayV1>, WorldFieldOverlayErrorV1> {
    let mut read_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let guard = transaction.guard();
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    crate::incremental_world_rule_composite_overlay::bind_current_world_rule_branch_v1(
        family,
        roots,
        pages,
        &mut overlay,
        &mut scan(),
        &mut read_work(),
        &guard,
        live,
    )
}
fn apply(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,
    roots: &RootSetV1,
    current: Option<&ReservedValue<EmergencySupplyOverlayV1>>,
    expected: [u8; 32],
    after: Option<bool>,
    pages: &Pages,
    memory: &MemoryBudget,
    transaction: &mut ExecutionTransaction<'_, Pages>,
    live: &ExecutionFence,
) -> Result<(Option<ReservedValue<EmergencySupplyOverlayV1>>, u64), WorldFieldOverlayErrorV1> {
    let mut read_io = io();
    let start = read_io.read_bytes_remaining;
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut write_io = io();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut writer = Writer(pages);
    let mut output = MemberRecordWriteContext {
        sink: &mut writer,
        memory,
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
        transaction,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut read_keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let result = set_world_rule_emergency_review_decision_v1(
        parent,
        roots,
        current,
        " outer-key ",
        expected,
        after,
        pages,
        &mut overlay,
        &mut output,
        &mut scan(),
        &mut read_work(),
        &mut write_work(),
        &guard,
        live,
    );
    drop(overlay);
    result.map(|token| (token, start - read_io.read_bytes_remaining))
}
#[test]
fn composite_typed_batch_current_identity_and_independent_receiver() {
    let _source_admission = source_admission();
    let mut source = continuity_data::fixture_EmergencySupplyStateV1();
    source.reviews.values_mut().next().unwrap().reason_ref = "r".repeat(512 << 10);
    let (memory, pages, roots) = seed(&source);
    let mut cold_io = io();
    let mut cold_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut cold = context(&pages, &roots, &memory, &mut cold_io, &mut cold_keys);
    let family = qualify_world_rule_branch_v1::<Emergency, _, _, _, _>(
        &roots,
        &mut cold,
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    assert_eq!(family.allocation_class(), MemoryClass::Ordinary);
    let profile = Emergency::BRANCH.profile();
    let decision = [
        Key::Field("reviews"),
        Key::Entry(" outer-key "),
        Key::Field("decision"),
    ];
    let expected = selected(family.body().unwrap(), &profile, &decision, &mut cold);
    let reason = selected(
        family.body().unwrap(),
        &profile,
        &[
            Key::Field("reviews"),
            Key::Entry(" outer-key "),
            Key::Field("reason_ref"),
        ],
        &mut cold,
    );
    pages.remove(&reason); // already qualified; no later delta may hydrate it
    let original = parent(&roots);
    let live = original.fence().unwrap();
    let mut transaction =
        ExecutionTransaction::begin(original.clone(), [6; 32], &pages, &memory, limits()).unwrap();
    let (first, used) = apply(
        &family,
        &roots,
        None,
        expected,
        None,
        &pages,
        &memory,
        &mut transaction,
        &live,
    )
    .unwrap();
    let first = first.unwrap();
    assert!(used < 512 << 10);
    // A token's owner survives unrelated transaction reads that advance its
    // revision, while the fresh ordinary guard still checks that revision.
    transaction
        .read(StateNamespaceV1::Authorization, b"unrelated-read")
        .unwrap();
    // The original mutation's token need not survive across owners. Rebind
    // the actual Current pointer and changed closure, retaining the unreadable
    // unchanged sibling above instead of hydrating the complete branch.
    let actual_body = first.body().clone();
    drop(first);
    let first =
        bind_current_emergency(&family, &roots, &pages, &memory, &mut transaction, &live).unwrap();
    assert_eq!(first.body(), &actual_body);
    let next_expected = selected(first.body(), &profile, &decision, &mut cold);
    let (second, used) = apply(
        &family,
        &roots,
        Some(&first),
        next_expected,
        Some(false),
        &pages,
        &memory,
        &mut transaction,
        &live,
    )
    .unwrap();
    let second = second.unwrap();
    assert!(used < 512 << 10);
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
    assert_eq!(accepted.body(), Some(second.body()));
    assert!(start - receive.values.io.read_bytes_remaining < 512 << 10);
    let id = selected(accepted.body().unwrap(), &profile, &decision, &mut receive);
    let (key, _) = receive
        .records()
        .key(profile.field, WorldFieldRecordKindV1::Entry, "decision")
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
    assert!(matches!(row.cell, ColumnCellV1::Bool(false)));
    // A second transaction independently reaches the identical intermediate
    // body. Matching parent+body alone must not let it borrow the first token.
    let mut other =
        ExecutionTransaction::begin(original.clone(), [6; 32], &pages, &memory, limits()).unwrap();
    let (own, _) = apply(
        &family, &roots, None, expected, None, &pages, &memory, &mut other, &live,
    )
    .unwrap();
    let own = own.unwrap();
    assert_eq!(own.body(), first.body());
    assert!(
        apply(
            &family,
            &roots,
            Some(&first),
            next_expected,
            Some(false),
            &pages,
            &memory,
            &mut other,
            &live
        )
        .is_err()
    );
    let guard = other.guard();
    assert!(other.seal(&guard, &live, 1).is_err());
    let mut foreign = roots.clone();
    foreign.genesis_config[0] ^= 1;
    assert_eq!(
        foreign.state_root(StateNamespaceV1::Authorization),
        roots.state_root(StateNamespaceV1::Authorization)
    );
    let mut tx = ExecutionTransaction::begin(original, [6; 32], &pages, &memory, limits()).unwrap();
    assert!(
        apply(
            &family, &foreign, None, expected, None, &pages, &memory, &mut tx, &live
        )
        .is_err()
    );
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}

#[test]
fn composite_recovery_needs_explicit_ordinary_readmission_and_budget() {
    let _source_admission = source_admission();
    let source = continuity_data::fixture_EmergencySupplyStateV1();
    let (memory, pages, roots) = seed(&source);
    let reader = ClassedReader {
        pages: &pages,
        memory: &memory,
        class: MemoryClass::Recovery,
        error: std::cell::RefCell::new(None),
    };
    let mut recovery_io = io();
    let mut recovery_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut recovery = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut recovery_io,
        },
        key_work: &mut recovery_keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || reader.error.borrow_mut().take(),
        cancelled: || false,
    };
    let family = qualify_world_rule_branch_v1::<Emergency, _, _, _, _>(
        &roots,
        &mut recovery,
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    assert_eq!(family.allocation_class(), MemoryClass::Recovery);
    let mut cold_io = io();
    let mut cold_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut cold = context(&pages, &roots, &memory, &mut cold_io, &mut cold_keys);
    let expected = selected(
        family.body().unwrap(),
        &Emergency::BRANCH.profile(),
        &[
            Key::Field("reviews"),
            Key::Entry(" outer-key "),
            Key::Field("decision"),
        ],
        &mut cold,
    );
    let original = parent(&roots);
    let live = original.fence().unwrap();
    let mut transaction =
        ExecutionTransaction::begin(original.clone(), [6; 32], &pages, &memory, limits()).unwrap();
    assert!(
        apply(
            &family,
            &roots,
            None,
            expected,
            None,
            &pages,
            &memory,
            &mut transaction,
            &live
        )
        .is_err()
    );
    let guard = transaction.guard();
    assert!(transaction.seal(&guard, &live, 1).is_err());
    let filler_bytes = (512usize << 20) - (8usize << 20) - memory.used() - 512;
    let filler = memory.reserve(filler_bytes, MemoryClass::Ordinary).unwrap();
    let held = memory.used();
    assert!(readmit_world_rule_branch_ordinary_v1(&family, &roots, &memory, || false).is_err());
    assert_eq!(memory.used(), held);
    drop(filler);
    let held = memory.used();
    assert!(matches!(
        readmit_world_rule_branch_ordinary_v1(&family, &roots, &memory, || true),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(memory.used(), held);
    let foreign = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
    assert!(readmit_world_rule_branch_ordinary_v1(&family, &roots, &foreign, || false).is_err());
    let ordinary =
        readmit_world_rule_branch_ordinary_v1(&family, &roots, &memory, || false).unwrap();
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(ordinary.body(), family.body());
    let mut transaction =
        ExecutionTransaction::begin(original, [6; 32], &pages, &memory, limits()).unwrap();
    assert!(
        apply(
            &ordinary,
            &roots,
            None,
            expected,
            Some(false),
            &pages,
            &memory,
            &mut transaction,
            &live
        )
        .unwrap()
        .0
        .is_some()
    );
}
