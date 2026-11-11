use super::*;

#[test]
fn final_result_pair_uses_one_frozen_root_and_appends_both_in_order() {
    let pages = Pages::default();
    let memory = memory();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [1; 32], &pages, &memory, limits()).unwrap();
    tx.write(StateNamespaceV1::Business, b"changed", Some([3; 32]))
        .unwrap();
    tx.append_fact(fact(ExecutionFactKindV1::Input, 1)).unwrap();
    let guard = tx.guard();
    let prepared = tx
        .seal_with_final_results(
            &guard,
            &live,
            9,
            || false,
            |after| {
                assert_eq!(after.simulation_tick, 9);
                assert_ne!(after.digest().unwrap(), parent.roots.digest().unwrap());
                Ok::<_, &str>([
                    ExecutionFactRefV1 {
                        kind: ExecutionFactKindV1::Result,
                        id: after.digest().unwrap(),
                    },
                    fact(ExecutionFactKindV1::Result, 7),
                ])
            },
        )
        .unwrap();
    assert_eq!(prepared.record.facts.len(), 3);
    assert_eq!(
        prepared.record.facts[0],
        fact(ExecutionFactKindV1::Input, 1)
    );
    assert_eq!(
        prepared.record.facts[1].id,
        prepared.record.after.digest().unwrap()
    );
    assert_eq!(
        prepared.record.facts[2],
        fact(ExecutionFactKindV1::Result, 7)
    );
    TransitionVerifier {
        parent: &parent,
        operation: [1; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: limits().max_pending_node_bytes,
    }
    .verify(&prepared.record)
    .unwrap();
}

#[test]
fn final_result_pair_admits_both_before_io_and_rejects_partial_or_wrong_results() {
    for mode in 0..5 {
        let pages = Pages::default();
        let memory = memory();
        let parent = empty_parent();
        let live = parent.fence().unwrap();
        let mut bounds = limits();
        if mode == 0 {
            bounds.max_facts = 2;
        }
        let mut tx = ExecutionTransaction::begin(parent, [1; 32], &pages, &memory, bounds).unwrap();
        tx.append_fact(fact(ExecutionFactKindV1::Input, 1)).unwrap();
        let guard = tx.guard();
        let called = Cell::new(false);
        let cancel = Cell::new(false);
        let result = tx.seal_with_final_results(
            &guard,
            &live,
            9,
            || cancel.get(),
            |_| {
                called.set(true);
                if mode == 3 {
                    return Err("second original failed");
                }
                cancel.set(mode == 4);
                Ok([
                    fact(
                        if mode == 1 {
                            ExecutionFactKindV1::Event
                        } else {
                            ExecutionFactKindV1::Result
                        },
                        2,
                    ),
                    fact(ExecutionFactKindV1::Result, if mode == 2 { 2 } else { 3 }),
                ])
            },
        );
        assert_eq!(called.get(), mode != 0);
        match mode {
            0 => assert!(matches!(
                result,
                Err(SealWithFinalFactError::Execution(
                    ExecutionError::ResourceLimited(_)
                ))
            )),
            1 | 2 => assert!(matches!(
                result,
                Err(SealWithFinalFactError::Execution(ExecutionError::Invalid(
                    "final Result pair"
                )))
            )),
            3 => assert!(matches!(
                result,
                Err(SealWithFinalFactError::Producer("second original failed"))
            )),
            _ => assert!(matches!(result, Err(SealWithFinalFactError::Cancelled))),
        }
    }
}

#[test]
fn final_fact_observes_frozen_actual_after_roots_and_preserves_prior_order() {
    let pages = Pages::default();
    let memory = memory();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    let build = || {
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [1; 32], &pages, &memory, limits())
                .unwrap();
        for namespace in NAMESPACES {
            tx.write(namespace, b"changed", Some([2; 32])).unwrap();
            tx.write(namespace, b"changed", Some([3; 32])).unwrap();
        }
        assert_eq!(tx.ordered_fact_count(), 0);
        tx.append_fact(fact(ExecutionFactKindV1::Input, 4)).unwrap();
        tx.append_fact(fact(ExecutionFactKindV1::Event, 5)).unwrap();
        assert_eq!(tx.ordered_fact_count(), 2);
        tx
    };
    let legacy = build();
    let guard = legacy.guard();
    let expected = legacy.seal(&guard, &live, 17).unwrap();
    let tx = build();
    let guard = tx.guard();
    let called = Cell::new(0);
    let actual = tx
        .seal_with_final_fact(
            &guard,
            &live,
            17,
            || false,
            |after| {
                called.set(called.get() + 1);
                assert_eq!(after, &expected.record.after);
                assert_ne!(after.digest().unwrap(), parent.roots.digest().unwrap());
                Ok::<_, &str>(ExecutionFactRefV1 {
                    kind: ExecutionFactKindV1::Result,
                    id: after.digest().unwrap(),
                })
            },
        )
        .unwrap();
    assert_eq!(called.get(), 1);
    assert_eq!(actual.record.before, expected.record.before);
    assert_eq!(actual.record.after, expected.record.after);
    assert_eq!(actual.record.writes, expected.record.writes);
    assert_eq!(actual.record.reads, expected.record.reads);
    assert_eq!(&actual.record.facts[..2], expected.record.facts.as_slice());
    assert_eq!(
        actual.record.facts[2].id,
        actual.record.after.digest().unwrap()
    );
    TransitionVerifier {
        parent: &parent,
        operation: [1; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: limits().max_pending_node_bytes,
    }
    .verify(&actual.record)
    .unwrap();
}
#[test]
fn final_fact_capacity_stale_guard_and_tick_fail_before_callback() {
    for mode in 0..3 {
        let pages = Pages::default();
        let memory = memory();
        let mut parent = empty_parent();
        parent.roots.simulation_tick = 2;
        let live = parent.fence().unwrap();
        let mut limits = limits();
        limits.max_facts = if mode == 2 { 2 } else { 1 };
        let mut tx = ExecutionTransaction::begin(parent, [1; 32], &pages, &memory, limits).unwrap();
        let stale = tx.guard();
        tx.append_fact(fact(ExecutionFactKindV1::Input, 4)).unwrap();
        let fresh = tx.guard();
        let called = Cell::new(false);
        let result = tx.seal_with_final_fact(
            if mode == 1 { &stale } else { &fresh },
            &live,
            if mode == 2 { 1 } else { 2 },
            || false,
            |_| {
                called.set(true);
                Ok::<_, &str>(fact(ExecutionFactKindV1::Result, 7))
            },
        );
        assert!(!called.get());
        match mode {
            1 => assert!(matches!(
                result,
                Err(SealWithFinalFactError::Execution(
                    ExecutionError::StaleGuard
                ))
            )),
            2 => assert!(matches!(
                result,
                Err(SealWithFinalFactError::Execution(ExecutionError::Invalid(
                    "tick regression"
                )))
            )),
            _ => assert!(matches!(
                result,
                Err(SealWithFinalFactError::Execution(
                    ExecutionError::ResourceLimited(_)
                ))
            )),
        }
    }
}
#[test]
fn final_fact_producer_error_and_both_cancel_boundaries_return_no_prepared_prefix() {
    for mode in 0..3 {
        let pages = Pages::default();
        let memory = memory();
        let parent = empty_parent();
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent, [1; 32], &pages, &memory, limits()).unwrap();
        tx.write(
            StateNamespaceV1::Business,
            b"successful prefix",
            Some([3; 32]),
        )
        .unwrap();
        let guard = tx.guard();
        let cancelled = Cell::new(mode == 1);
        let called = Cell::new(0);
        let result = tx.seal_with_final_fact(
            &guard,
            &live,
            0,
            || cancelled.get(),
            |_| {
                called.set(called.get() + 1);
                if mode == 0 {
                    return Err("original persistence failed");
                }
                cancelled.set(true);
                Ok(fact(ExecutionFactKindV1::Result, 7))
            },
        );
        match mode {
            0 => {
                assert_eq!(called.get(), 1);
                assert!(matches!(
                    result,
                    Err(SealWithFinalFactError::Producer(
                        "original persistence failed"
                    ))
                ));
            }
            1 => {
                assert_eq!(called.get(), 0);
                assert!(matches!(result, Err(SealWithFinalFactError::Cancelled)));
            }
            _ => {
                assert_eq!(called.get(), 1);
                assert!(matches!(result, Err(SealWithFinalFactError::Cancelled)));
            }
        }
        drop(guard);
        assert_eq!(memory.used(), 0);
    }
}
