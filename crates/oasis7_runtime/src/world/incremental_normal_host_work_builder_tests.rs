//! Work construction is resource admission only. No qualified state is minted.
use super::*;

#[test]
fn bounded_work_construction_refuses_insufficient_common_and_invalid_shape() {
    let memory = MemoryBudget::new(8 << 20, 64 << 10).unwrap();
    for (steps, bytes) in [(0, 1 << 20), (1 << 20, 0)] {
        let work = FormalBoundedWorkV1::new(steps, bytes).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        assert!(
            NormalHostWorkV1::new_bounded(two_agent_host_work_limits_v1(), &mut common).is_err()
        );
        assert!(!work.is_child_active());
    }
    let work = FormalBoundedWorkV1::new(1 << 20, 1 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let mut limits = two_agent_host_work_limits_v1();
    limits.driver_tail_world_delivery.max_pending_records = 257;
    assert!(matches!(
        NormalHostWorkV1::new_bounded(limits, &mut common),
        Err(BodyMutationError::Invalid(
            "normal Host pending record bound"
        ))
    ));
}

#[test]
fn host_work_keeps_original_core_and_startup_reuses_remaining_tail_accounts() {
    let memory = MemoryBudget::new(8 << 20, 64 << 10).unwrap();
    let meter = FormalBoundedWorkV1::new(1 << 20, 1 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let mut work =
        NormalHostWorkV1::new_bounded(two_agent_host_work_limits_v1(), &mut common).unwrap();
    work.validate_context(&mut common).unwrap();
    let before = work.driver.tail.world_delivery.claims.visits_remaining;
    {
        let (io, startup) = work.startup_loans();
        // These are the very same counters the later real tail will borrow.
        startup.tail.claims.visits_remaining -= 1;
        *startup.tail.grant_cells = startup.tail.grant_cells.checked_sub(1).unwrap();
        // Simultaneous IO is a disjoint borrow, not a copied context.
        io.current.io.nodes_remaining -= 2;
    }
    assert_eq!(
        work.driver.tail.world_delivery.claims.visits_remaining,
        before - 1
    );
    let after = work.driver.tail.world_delivery.fixed_cells_remaining;
    {
        let (_, startup) = work.startup_loans();
        assert_eq!(*startup.tail.grant_cells, after);
        assert_eq!(startup.tail.claims.visits_remaining, before - 1);
    }
    let foreign = FormalBoundedWorkV1::new(1 << 20, 1 << 20).unwrap();
    let mut foreign_stop = || false;
    let mut foreign_common = FormalBoundedContextV1::new(&memory, &foreign, &mut foreign_stop);
    assert!(matches!(
        work.validate_context(&mut foreign_common),
        Err(BodyMutationError::Invalid(
            "normal Host work original Core/pool"
        ))
    ));
    let other_memory = MemoryBudget::new(8 << 20, 64 << 10).unwrap();
    let mut other_stop = || false;
    let mut other_common = FormalBoundedContextV1::new(&other_memory, &meter, &mut other_stop);
    assert!(work.validate_context(&mut other_common).is_err());
}
