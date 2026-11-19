//! Real signed parent / sole-transaction rejection checks, not an accepted lease fixture.
//! Full endpoint acceptance deliberately remains blocked by the production gate.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::test_fixtures::incremental as iws,
    incremental_execution::{ExecutionParent, TransactionLimits},
};

fn parent(setup: &iws::Setup) -> ExecutionParent {
    ExecutionParent {
        commit: setup.history.body.digest().unwrap(),
        roots: setup.roots.clone(),
        authority: authority_binding(&setup.authority, &setup.memory).unwrap(),
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 16,
        max_writes: 16,
        max_facts: 8,
        max_pending_node_bytes: 1 << 20,
    }
}
#[test]
fn native_scope_signed_parent_binding_does_not_mint_a_lease_or_change_revision() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let setup = iws::setup_with_memory(memory.clone());
    let source = iws::Empty;
    let mut tx =
        ExecutionTransaction::begin(parent(&setup), [31; 32], &source, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let before = tx.guard();
    validate_parent(
        &setup.commit,
        &setup.authority,
        &memory,
        &AtomicBool::new(false),
        &mut tx,
        &live,
    )
    .unwrap();
    tx.validate_guard(&before, &live).unwrap();
}
#[test]
fn native_scope_foreign_verified_head_rejects_and_poison_prevents_seal() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let setup = iws::setup_with_memory(memory.clone());
    let foreign = iws::setup_with_native(memory.clone(), [71; 32]);
    assert_ne!(
        setup.commit.verified_record_id(),
        foreign.commit.verified_record_id()
    );
    let source = iws::Empty;
    let mut tx =
        ExecutionTransaction::begin(parent(&setup), [32; 32], &source, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    assert!(matches!(
        validate_parent(
            &foreign.commit,
            &setup.authority,
            &memory,
            &AtomicBool::new(false),
            &mut tx,
            &live
        ),
        Err(ExecutionError::Invalid(
            "native worker exact accepted transaction parent"
        ))
    ));
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
#[test]
fn native_scope_foreign_authority_rejects_and_poison_prevents_seal() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let setup = iws::setup_with_memory(memory.clone());
    let foreign = iws::setup_with_native(memory.clone(), [72; 32]);
    assert_ne!(
        authority_binding(&setup.authority, &memory).unwrap(),
        authority_binding(&foreign.authority, &memory).unwrap()
    );
    let source = iws::Empty;
    let mut tx =
        ExecutionTransaction::begin(parent(&setup), [33; 32], &source, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    assert!(matches!(
        validate_parent(
            &setup.commit,
            &foreign.authority,
            &memory,
            &AtomicBool::new(false),
            &mut tx,
            &live
        ),
        Err(ExecutionError::Invalid(
            "native worker exact accepted transaction parent"
        ))
    ));
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
#[test]
fn native_scope_foreign_pool_and_cancel_each_poison_the_actual_transaction() {
    for cancel in [false, true] {
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let foreign = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let setup = iws::setup_with_memory(memory.clone());
        let source = iws::Empty;
        let mut tx =
            ExecutionTransaction::begin(parent(&setup), [34; 32], &source, &memory, limits())
                .unwrap();
        let live = tx.parent_fence();
        let guard = tx.guard();
        let checked_memory = if cancel { &memory } else { &foreign };
        assert!(matches!(
            validate_parent(
                &setup.commit,
                &setup.authority,
                checked_memory,
                &AtomicBool::new(cancel),
                &mut tx,
                &live
            ),
            Err(ExecutionError::Invalid(
                "native Runtime worker cancellation/pool"
            ))
        ));
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}
#[test]
fn native_scope_changed_live_fence_rejects_and_poison_prevents_seal() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let setup = iws::setup_with_memory(memory.clone());
    let source = iws::Empty;
    let mut tx =
        ExecutionTransaction::begin(parent(&setup), [35; 32], &source, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let mut stale_owner = live;
    stale_owner.authority.epoch += 1;
    assert!(matches!(
        validate_parent(
            &setup.commit,
            &setup.authority,
            &memory,
            &AtomicBool::new(false),
            &mut tx,
            &stale_owner
        ),
        Err(ExecutionError::ParentConflict)
    ));
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
