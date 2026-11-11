use super::*;
fn assert_invalid(error: FormalError, text: &str) {
    assert!(matches!(error,FormalError::Invalid(message) if message==text));
}
#[test]
fn insufficient_charge_debits_both_balances_and_steps_error_has_priority() {
    let work = FormalBoundedWorkV1::new(2, 3).unwrap();
    assert_invalid(
        work.charge(4, 5, &mut || false).unwrap_err(),
        "bounded archive read steps",
    );
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (0, 0)
    );
    let work = FormalBoundedWorkV1::new(4, 2).unwrap();
    assert_invalid(
        work.charge(1, 3, &mut || false).unwrap_err(),
        "bounded archive read bytes",
    );
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (3, 0)
    );
}
#[test]
fn cancellation_keeps_callback_debits_and_does_not_charge_cancelled_operation() {
    let work = FormalBoundedWorkV1::new(10, 20).unwrap();
    let error = work
        .charge(7, 15, &mut || {
            work.charge(2, 3, &mut || false).unwrap();
            true
        })
        .unwrap_err();
    assert_invalid(error, "bounded archive read cancelled");
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (8, 17)
    );
}
#[test]
fn callback_charges_are_visible_to_the_outer_current_balance_check() {
    let work = FormalBoundedWorkV1::new(5, 7).unwrap();
    let error = work
        .charge(3, 5, &mut || {
            work.charge(3, 4, &mut || false).unwrap();
            false
        })
        .unwrap_err();
    assert_invalid(error, "bounded archive read steps");
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (0, 0)
    );
}
#[test]
fn lease_is_exclusive_without_hidden_charge_or_cancellation() {
    let work = FormalBoundedWorkV1::new(10, 20).unwrap();
    let lease = work.begin_child().unwrap();
    assert!(work.is_child_active());
    assert_eq!(
        (lease.remaining_steps(), lease.remaining_read_bytes()),
        (10, 20)
    );
    assert!(work.begin_child().is_err());
    let mut called = false;
    let error = work
        .charge(1, 1, &mut || {
            called = true;
            false
        })
        .unwrap_err();
    assert_invalid(error, "bounded archive child read active");
    assert!(!called);
    lease.consume(2, 3).unwrap();
    lease.finish();
    assert!(!work.is_child_active());
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (8, 17)
    );
    work.charge(1, 2, &mut || false).unwrap();
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (7, 15)
    );
}
#[test]
fn post_callback_gate_prevents_parent_charge_when_callback_holds_child() {
    let work = FormalBoundedWorkV1::new(8, 9).unwrap();
    let mut held = None;
    let error = work
        .charge(1, 1, &mut || {
            held = Some(work.begin_child().unwrap());
            false
        })
        .unwrap_err();
    assert_invalid(error, "bounded archive child read active");
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (8, 9)
    );
    held.take().unwrap().finish();
    assert!(!work.is_child_active());
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (8, 9)
    );
}
#[test]
fn normally_finished_failed_debit_does_not_fence_unspent_other_dimension() {
    let work = FormalBoundedWorkV1::new(1, 9).unwrap();
    let lease = work.begin_child().unwrap();
    assert_invalid(
        lease.consume(2, 1).unwrap_err(),
        "bounded archive read steps",
    );
    lease.finish();
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (0, 8)
    );
    assert!(!work.is_child_active());
}
#[test]
fn abandoning_or_panicking_child_fences_both_balances_and_unlocks() {
    for panic_child in [false, true] {
        let work = FormalBoundedWorkV1::new(10, 20).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let lease = work.begin_child().unwrap();
            lease.consume(1, 2).unwrap();
            if panic_child {
                panic!("common child panic sentinel");
            }
            drop(lease);
        }));
        assert_eq!(result.is_err(), panic_child);
        assert!(!work.is_child_active());
        assert_eq!(
            (work.remaining_steps(), work.remaining_read_bytes()),
            (0, 0)
        );
        assert_invalid(
            work.consume_read_bytes(1, &mut || false).unwrap_err(),
            "bounded archive read steps",
        );
    }
}
#[test]
fn identities_are_opaque_distinct_and_counter_exhaustion_never_wraps() {
    let one = FormalBoundedWorkV1::new(1, 1).unwrap();
    let two = FormalBoundedWorkV1::new(1, 1).unwrap();
    assert_eq!(one.identity(), one.identity());
    assert_ne!(one.identity(), two.identity());
    assert_eq!(format!("{:?}", one.identity()), "WorkIdentityV1 { .. }");
    // A local allocator counter tests the checked boundary without poisoning
    // the process-global identity source used by parallel tests/consumers.
    let counter = AtomicU64::new(u64::MAX - 1);
    let last = next_identity(&counter).unwrap();
    assert_ne!(last, one.identity());
    assert_invalid(
        next_identity(&counter).unwrap_err(),
        "bounded archive meter identity exhausted",
    );
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
}
#[test]
fn contexts_and_parts_borrow_one_existing_meter_and_pool_without_new_allowance() {
    let memory = MemoryBudget::new(128, 0).unwrap();
    let work = FormalBoundedWorkV1::new(10, 20).unwrap();
    let mut stop = || false;
    {
        let mut context = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        assert!(std::ptr::eq(context.work(), &work));
        assert!(context.memory().same_pool(&memory));
        assert_eq!(
            (work.remaining_steps(), work.remaining_read_bytes()),
            (10, 20)
        );
        context.charge(2, 3).unwrap();
        let (pool, meter, cancel) = context.parts();
        assert!(pool.same_pool(&memory));
        assert_eq!(meter.identity(), work.identity());
        meter.consume_read_bytes(4, cancel).unwrap();
    }
    work.consume_steps(1, &mut stop).unwrap();
    let mut context = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    context.consume_read_bytes(2).unwrap();
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (5, 11)
    );
    assert_eq!(
        memory.used(),
        0,
        "context borrows without allocating or reserving a new pool"
    );
}
#[test]
fn a_child_on_one_meter_does_not_lock_or_alias_another_meter() {
    let one = FormalBoundedWorkV1::new(4, 5).unwrap();
    let two = FormalBoundedWorkV1::new(6, 7).unwrap();
    let lease = one.begin_child().unwrap();
    two.charge(1, 2, &mut || false).unwrap();
    assert_eq!((two.remaining_steps(), two.remaining_read_bytes()), (5, 5));
    assert_eq!(
        (lease.remaining_steps(), lease.remaining_read_bytes()),
        (4, 5)
    );
    lease.finish();
    assert_ne!(one.identity(), two.identity());
}
