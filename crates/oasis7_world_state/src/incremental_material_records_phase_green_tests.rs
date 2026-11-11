use super::*;
#[test]
fn material_explicit_selected_readmission_checks_class_binding_quota_and_cancellation() {
    let f = fixture();
    let memory = &f.memory;
    let before = memory.used();
    let ordinary = readmit_material_record_ordinary_v1(
        &f.proof,
        MaterialRecordKindV1::Profile,
        &f.owner,
        &f.body,
        memory,
        || false,
    )
    .unwrap();
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    ordinary
        .validate_binding(MaterialRecordKindV1::Profile, &f.owner, &f.body)
        .unwrap();
    assert_eq!(memory.used(), before + ordinary.reserved_bytes());
    assert!(
        readmit_material_record_ordinary_v1(
            &ordinary,
            MaterialRecordKindV1::Profile,
            &f.owner,
            &f.body,
            memory,
            || false
        )
        .is_err()
    );
    assert!(
        readmit_material_record_ordinary_v1(
            &f.proof,
            MaterialRecordKindV1::Profile,
            &[99; 32],
            &f.body,
            memory,
            || false
        )
        .is_err()
    );
    let empty = StateRootV1::empty(StateNamespaceV1::Business).unwrap();
    assert!(
        readmit_material_record_ordinary_v1(
            &f.proof,
            MaterialRecordKindV1::Profile,
            &f.owner,
            &empty,
            memory,
            || false
        )
        .is_err()
    );
    let foreign = MemoryBudget::new(2 << 20, 1 << 18).unwrap();
    assert!(
        readmit_material_record_ordinary_v1(
            &f.proof,
            MaterialRecordKindV1::Profile,
            &f.owner,
            &f.body,
            &foreign,
            || false
        )
        .is_err()
    );
    let used = memory.used();
    let fill = memory
        .reserve((128 << 20) - (16 << 20) - used - 512, MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        readmit_material_record_ordinary_v1(
            &f.proof,
            MaterialRecordKindV1::Profile,
            &f.owner,
            &f.body,
            memory,
            || false
        ),
        Err(ValueTreeError::Memory(_))
    ));
    drop(fill);
    assert_eq!(memory.used(), used);
    let mut checks = 0;
    assert!(matches!(
        readmit_material_record_ordinary_v1(
            &f.proof,
            MaterialRecordKindV1::Profile,
            &f.owner,
            &f.body,
            memory,
            || {
                checks += 1;
                checks >= 2
            }
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(memory.used(), used);
    drop(ordinary);
    assert_eq!(memory.used(), before);
}
#[test]
fn material_readmitted_selected_proof_enables_real_edit_and_noop_cancel_poison() {
    let f = fixture();
    let memory = &f.memory;
    let pages = &f.pages;
    let proof = readmit_material_record_ordinary_v1(
        &f.proof,
        MaterialRecordKindV1::Profile,
        &f.owner,
        &f.body,
        memory,
        || false,
    )
    .unwrap();
    for (cancel, after) in [(false, 9), (false, 8), (true, 8)] {
        let p = ExecutionParent {
            roots: f.roots.clone(),
            commit: [3; 32],
            authority: AuthorityBinding {
                token: [4; 32],
                epoch: 5,
            },
        };
        let live = p.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            p.clone(),
            [6; 32],
            pages,
            memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        let guard = tx.guard();
        let mut vi = io();
        let mut kw = keys();
        let mut wi = io();
        let mut wk = keys();
        let mut out = Writer(pages);
        let result = change_qualified_material_cell_v1(
            MaterialCellChangeV1 {
                key: MaterialRecordKeyV1::Profile("outer profile"),
                cell: MaterialCellV1::StackLimit,
                qualification: &proof,
                expected: Some(MaterialCellValueRefV1::I64(8)),
                after: Some(MaterialCellValueRefV1::I64(after)),
                max_current_payload_bytes: 8,
                max_pending_node_bytes: 8 << 20,
            },
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: pages,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut vi,
                },
                key_work: &mut kw,
                semantic_bytes: 2 << 20,
                record_bytes: 4 << 20,
                take_state_error: || None,
                cancelled: || cancel,
            },
            &mut MemberRecordWriteContext {
                sink: &mut out,
                memory,
                io: &mut wi,
                key_work: &mut wk,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 2 << 20,
                    payload_bytes: 2 << 20,
                    record_bytes: 4 << 20,
                },
                cancelled: || false,
            },
            &guard,
            &live,
        );
        if cancel {
            assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
            ));
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
        } else {
            let updated = result.unwrap();
            assert_eq!(updated.is_some(), after != 8);
            if let Some(updated) = updated {
                assert_eq!(updated.allocation_class(), MemoryClass::Ordinary);
            }
            let guard = tx.guard();
            let sealed = tx.seal(&guard, &live, 1).unwrap();
            TransitionVerifier {
                parent: &p,
                operation: [6; 32],
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 8 << 20,
            }
            .verify(&sealed.record)
            .unwrap();
        }
    }
}
