use super::*;
use crate::world::incremental_module_state_family::{
    ModuleStateFamilyQualificationV1, accept_module_state_transition_v1,
};
use oasis7_world_state::incremental_module_fields::ModuleStateKeyV1;

const OPERATION: [u8; 32] = [0x96; 32];

fn write_bytes(values: &mut Values, memory: &MemoryBudget, bytes: &[u8]) -> [u8; 32] {
    crate::world::incremental_body::write_field_bytes(
        bytes,
        &mut BodyMutationContext {
            sink: values,
            memory,
            io: &mut quota(),
            cancelled: || false,
        },
    )
    .unwrap()
}

fn accept(
    parent: &ExecutionParent,
    qualification: &ReservedValue<ModuleStateFamilyQualificationV1>,
    record: &TransitionRecordV1,
    pages: &Pages,
    values: &Values,
    memory: &MemoryBudget,
    content: &mut ModuleStateContentBudgetV1,
) -> Result<ReservedValue<ModuleStateFamilyQualificationV1>, BodyMutationError> {
    let verifier = TransitionVerifier {
        parent,
        operation: OPERATION,
        source: pages,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    };
    let mut context = ModuleStateRecoveryContextV1 {
        root: record.after.state_root(Ns::Business),
        state: pages,
        values,
        memory,
        class: MemoryClass::Ordinary,
        state_io: &mut StateScanBudget {
            nodes_remaining: 100000,
            read_bytes_remaining: 16 << 20,
            leaves_remaining: 1000,
        },
        value_io: &mut quota(),
        content,
    };
    accept_module_state_transition_v1(qualification, record, &verifier, &mut context, || false)
}

#[test]
fn module_state_receiver_preserves_payload_updates_among_real_insertions_and_deletions() {
    case_mixed_membership();
}

pub(super) fn case_mixed_membership() {
    // Equal-length keys put the unchanged membership before or after both
    // membership mutations in the real sorted transition write set.
    for (keep, remove, insert) in [
        ("a-keep", "m-drop", "z-add!"),
        ("z-keep", "a-drop", "m-add!"),
    ] {
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let (_world, parent, pages, mut values) =
            setup_module_states(&memory, &[(keep, &[1]), (remove, &[2, 3])]);
        let source = values.clone();
        let qualification = qualify_module_states(&parent, &pages, &source, &memory);
        let replacement = write_bytes(&mut values, &memory, &[4, 5, 6]);
        let added = write_bytes(&mut values, &memory, &[7, 8, 9, 10]);
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), OPERATION, &pages, &memory, txn_limits())
                .unwrap();
        for (key, value) in [
            (keep, Some(replacement)),
            (remove, None),
            (insert, Some(added)),
        ] {
            txn.write(
                Ns::Business,
                ModuleStateKeyV1::new(key).unwrap().as_bytes(),
                value,
            )
            .unwrap();
        }
        let guard = txn.guard();
        let live = parent.fence().unwrap();
        let prepared = txn.seal(&guard, &live, 0).unwrap();
        let mut content = ModuleStateContentBudgetV1 {
            states_remaining: 3,
            logical_bytes_remaining: 8192,
            max_state_bytes: 128,
        };
        let accepted = accept(
            &parent,
            &qualification,
            &prepared.record,
            &pages,
            &source,
            &memory,
            &mut content,
        )
        .unwrap();
        accepted.validate_binding(&prepared.record.after).unwrap();
        assert_eq!(accepted.states(), 2);
        assert_eq!(accepted.logical_bytes(), 7);
        let mut expected = vec![keep, insert];
        expected.sort_unstable();
        assert_eq!(accepted.state_keys().collect::<Vec<_>>(), expected);
        let mut original_keys = vec![keep, remove];
        original_keys.sort_unstable();
        assert_eq!(
            qualification.state_keys().collect::<Vec<_>>(),
            original_keys
        );
        assert_eq!(content.states_remaining, 0);
        // The old/new opaque values consume ten bytes. Actual membership
        // changes must also retain their original key-copy/comparison charge.
        assert!(content.logical_bytes_remaining < 8192 - 10);
    }
}

#[test]
fn module_state_receiver_rejects_wrong_before_and_foreign_roots_before_inheriting_keys() {
    case_reject_wrong_before_and_foreign_roots();
}

pub(super) fn case_reject_wrong_before_and_foreign_roots() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let (_world, parent, pages, mut values) =
        setup_module_states(&memory, &[("stable", &[1, 2, 3])]);
    let source = values.clone();
    let qualification = qualify_module_states(&parent, &pages, &source, &memory);
    let replacement = write_bytes(&mut values, &memory, &[4, 5, 6, 7]);
    let wrong_value = write_bytes(&mut values, &memory, &[8, 9]);
    let key = ModuleStateKeyV1::new("stable").unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), OPERATION, &pages, &memory, txn_limits())
            .unwrap();
    txn.write(Ns::Business, key.as_bytes(), Some(replacement))
        .unwrap();
    let guard = txn.guard();
    let live = parent.fence().unwrap();
    let prepared = txn.seal(&guard, &live, 0).unwrap();

    // Keep the forged read/write pair internally consistent: the real replay
    // must reject its before value against the authenticated parent page.
    let mut wrong_before = prepared.record.clone();
    wrong_before
        .writes
        .iter_mut()
        .find(|write| write.mutation.key == key.as_bytes())
        .unwrap()
        .mutation
        .before = Some(wrong_value);
    wrong_before
        .reads
        .iter_mut()
        .find(|read| read.key == key.as_bytes())
        .unwrap()
        .before = Some(wrong_value);
    wrong_before.validate().unwrap();
    let initial = ModuleStateContentBudgetV1 {
        states_remaining: 1,
        logical_bytes_remaining: 1024,
        max_state_bytes: 128,
    };
    let mut content = initial;
    assert!(matches!(
        accept(
            &parent,
            &qualification,
            &wrong_before,
            &pages,
            &source,
            &memory,
            &mut content
        ),
        Err(BodyMutationError::Execution(ExecutionError::Invalid(
            "parent read value mismatch"
        )))
    ));
    assert_eq!(content, initial);

    let (_foreign_world, foreign_parent, foreign_pages, foreign_values) =
        setup_module_states(&memory, &[("stable", &[99])]);
    let foreign = qualify_module_states(&foreign_parent, &foreign_pages, &foreign_values, &memory);
    assert!(matches!(
        accept(
            &parent,
            &foreign,
            &prepared.record,
            &pages,
            &source,
            &memory,
            &mut content
        ),
        Err(BodyMutationError::Invalid("module state family binding"))
    ));
    assert_eq!(content, initial);

    let accepted = accept(
        &parent,
        &qualification,
        &prepared.record,
        &pages,
        &source,
        &memory,
        &mut content,
    )
    .unwrap();
    assert_eq!(accepted.state_keys().collect::<Vec<_>>(), ["stable"]);
    assert_eq!(accepted.states(), 1);
    assert_eq!(accepted.logical_bytes(), 4);
    assert_eq!(content.states_remaining, 0);
    assert_eq!(content.logical_bytes_remaining, 1024 - 3 - 4);
}
