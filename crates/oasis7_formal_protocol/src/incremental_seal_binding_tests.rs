use super::*;
struct Empty;
impl MerkleNodeReader for Empty {
    type Node = Vec<u8>;
    fn read_node(&self, _: &str, _: usize) -> Result<Option<Self::Node>, String> {
        Ok(None)
    }
}
fn parent() -> ExecutionParent {
    ExecutionParent {
        roots: RootSetV1::empty("real-seal-binding".into(), [8; 32]).unwrap(),
        commit: [7; 32],
        authority: AuthorityBinding {
            token: [9; 32],
            epoch: 1,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 128,
        max_writes: 128,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    }
}
#[test]
fn actual_seal_binding_rejects_foreign_revision_transaction_pool_and_forged_after_roots() {
    let memory = MemoryBudget::new(32 << 20, 2 << 20).unwrap();
    let source = Empty;
    let mut tx =
        ExecutionTransaction::begin(parent(), [1; 32], &source, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let old = tx.guard();
    tx.write(StateNamespaceV1::Business, b"actual-write", Some([4; 32]))
        .unwrap();
    let final_guard = tx.guard();
    let other = ExecutionTransaction::begin(parent(), [1; 32], &source, &memory, limits()).unwrap();
    let foreign = other.guard();
    let sealed = tx.seal_with_binding(&final_guard, &live, 1).unwrap();
    sealed
        .binding()
        .validate_source_guard(&final_guard, &memory)
        .unwrap();
    assert!(matches!(
        sealed.binding().validate_source_guard(&old, &memory),
        Err(ExecutionError::StaleGuard)
    ));
    assert!(matches!(
        sealed.binding().validate_source_guard(&foreign, &memory),
        Err(ExecutionError::StaleGuard)
    ));
    let another_pool = MemoryBudget::new(32 << 20, 2 << 20).unwrap();
    assert!(
        sealed
            .binding()
            .validate_source_guard(&final_guard, &another_pool)
            .is_err()
    );
    sealed
        .binding()
        .validate_after_roots(&sealed.transition().record.after, &memory)
        .unwrap();
    let mut forged = (**sealed.transition()).clone();
    forged.record.after.height += 1;
    assert!(
        sealed
            .binding()
            .validate_after_roots(&forged.record.after, &memory)
            .is_err()
    );
    assert!(
        sealed
            .binding()
            .validate_after_roots(&sealed.transition().record.after, &another_pool)
            .is_err()
    );
    let (_, binding) = sealed.into_parts();
    binding
        .validate_source_guard(&final_guard, &memory)
        .unwrap();
}
#[test]
fn failed_or_cancelled_seal_never_returns_a_binding_and_results_use_actual_frozen_roots() {
    let memory = MemoryBudget::new(32 << 20, 2 << 20).unwrap();
    let source = Empty;
    let mut tx =
        ExecutionTransaction::begin(parent(), [1; 32], &source, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    let guard = tx.guard();
    assert!(matches!(
        tx.seal_with_binding(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
    let tx = ExecutionTransaction::begin(parent(), [2; 32], &source, &memory, limits()).unwrap();
    let guard = tx.guard();
    let called = std::cell::Cell::new(false);
    let result = tx.seal_with_final_results_and_binding(
        &guard,
        &live,
        1,
        || true,
        |_| {
            called.set(true);
            Ok::<_, ExecutionError>([
                ExecutionFactRefV1 {
                    kind: ExecutionFactKindV1::Result,
                    id: [1; 32],
                },
                ExecutionFactRefV1 {
                    kind: ExecutionFactKindV1::Result,
                    id: [2; 32],
                },
            ])
        },
    );
    assert!(matches!(result, Err(SealWithFinalFactError::Cancelled)));
    assert!(!called.get());
    let tx = ExecutionTransaction::begin(parent(), [3; 32], &source, &memory, limits()).unwrap();
    let guard = tx.guard();
    let observed = std::cell::Cell::new(None);
    let sealed = tx
        .seal_with_final_results_and_binding(
            &guard,
            &live,
            1,
            || false,
            |roots| {
                observed.set(Some(roots.digest()?));
                Ok::<_, ExecutionError>([
                    ExecutionFactRefV1 {
                        kind: ExecutionFactKindV1::Result,
                        id: [1; 32],
                    },
                    ExecutionFactRefV1 {
                        kind: ExecutionFactKindV1::Result,
                        id: [2; 32],
                    },
                ])
            },
        )
        .unwrap();
    assert_eq!(observed.get(), Some(sealed.binding().after_roots_digest()));
    sealed
        .binding()
        .validate_source_guard(&guard, &memory)
        .unwrap();
    assert_eq!(sealed.transition().record.facts.len(), 2);
}

#[test]
fn cancellation_after_final_results_root_digest_never_returns_a_binding() {
    let memory = MemoryBudget::new(32 << 20, 2 << 20).unwrap();
    let source = Empty;
    let tx = ExecutionTransaction::begin(parent(), [4; 32], &source, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let producer_finished = std::cell::Cell::new(false);
    let producer_after_roots = std::cell::Cell::new(None);
    let post_producer_polls = std::cell::Cell::new(0usize);

    // seal_inner polls once after the producer and once after preparing its
    // transition. The next poll is the wrapper's final check, after it digests
    // the actual sealed after RootSet.
    const SEAL_INNER_POST_PRODUCER_POLLS: usize = 2;
    let result = tx.seal_with_final_results_and_binding(
        &guard,
        &live,
        1,
        || {
            if producer_finished.get() {
                let polls = post_producer_polls.get() + 1;
                post_producer_polls.set(polls);
                polls == SEAL_INNER_POST_PRODUCER_POLLS + 1
            } else {
                false
            }
        },
        |roots| {
            producer_after_roots.set(Some(roots.digest()?));
            producer_finished.set(true);
            Ok::<_, ExecutionError>([
                ExecutionFactRefV1 {
                    kind: ExecutionFactKindV1::Result,
                    id: [3; 32],
                },
                ExecutionFactRefV1 {
                    kind: ExecutionFactKindV1::Result,
                    id: [4; 32],
                },
            ])
        },
    );

    assert!(matches!(result, Err(SealWithFinalFactError::Cancelled)));
    assert!(producer_finished.get());
    assert!(producer_after_roots.get().is_some());
    assert_eq!(
        post_producer_polls.get(),
        SEAL_INNER_POST_PRODUCER_POLLS + 1
    );
}

#[test]
fn independent_three_result_seal_binds_actual_after_roots_and_prechecks_capacity() {
    let memory = MemoryBudget::new(32 << 20, 2 << 20).unwrap();
    let source = Empty;
    let mut tx =
        ExecutionTransaction::begin(parent(), [11; 32], &source, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    tx.write(
        StateNamespaceV1::Business,
        b"matching-action-result",
        Some([17; 32]),
    )
    .unwrap();
    let guard = tx.guard();
    let observed = std::cell::Cell::new(None);
    let sealed = tx
        .seal_with_proof_ledger_results_and_binding(
            &guard,
            &live,
            1,
            || false,
            |roots| {
                observed.set(Some(roots.digest()?));
                Ok::<_, ExecutionError>([1, 2, 3].map(|id| ExecutionFactRefV1 {
                    kind: ExecutionFactKindV1::Result,
                    id: [id; 32],
                }))
            },
        )
        .unwrap();
    assert_eq!(observed.get(), Some(sealed.binding().after_roots_digest()));
    sealed
        .binding()
        .validate_source_guard(&guard, &memory)
        .unwrap();
    sealed
        .binding()
        .validate_after_roots(&sealed.transition().record.after, &memory)
        .unwrap();
    assert_eq!(
        sealed
            .transition()
            .record
            .facts
            .iter()
            .filter(|fact| fact.kind == ExecutionFactKindV1::Result)
            .count(),
        3
    );
    drop(sealed);
    let mut restricted = limits();
    restricted.max_facts = 2;
    let tx = ExecutionTransaction::begin(parent(), [12; 32], &source, &memory, restricted).unwrap();
    let guard = tx.guard();
    let called = std::cell::Cell::new(false);
    let result = tx.seal_with_proof_ledger_results_and_binding(
        &guard,
        &live,
        1,
        || false,
        |_| {
            called.set(true);
            Ok::<_, ExecutionError>([1, 2, 3].map(|id| ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: [id; 32],
            }))
        },
    );
    assert!(result.is_err());
    assert!(!called.get());
}
#[test]
fn metered_two_and_three_results_keep_exact_original_transition_and_binding() {
    let memory = MemoryBudget::new(32 << 20, 2 << 20).unwrap();
    let source = Empty;
    for profile in [2, 3] {
        let make = || {
            let mut tx =
                ExecutionTransaction::begin(parent(), [31; 32], &source, &memory, limits())
                    .unwrap();
            tx.write(StateNamespaceV1::Business, b"meter-a", Some([1; 32]))
                .unwrap();
            tx.write(StateNamespaceV1::Scheduler, b"meter-b", Some([2; 32]))
                .unwrap();
            tx.append_fact(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Input,
                id: [3; 32],
            })
            .unwrap();
            tx
        };
        let old = make();
        let live = old.parent_fence();
        let guard = old.guard();
        let facts = [
            ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: [4; 32],
            },
            ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: [5; 32],
            },
            ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: [6; 32],
            },
        ];
        let old = if profile == 2 {
            old.seal_with_final_results_and_binding(
                &guard,
                &live,
                1,
                || false,
                |_| Ok::<_, ExecutionError>([facts[0], facts[1]]),
            )
            .unwrap()
        } else {
            old.seal_with_proof_ledger_results_and_binding(
                &guard,
                &live,
                1,
                || false,
                |_| Ok::<_, ExecutionError>(facts),
            )
            .unwrap()
        };
        let paid = make();
        let guard = paid.guard();
        let (mut steps, mut bytes) = (0, 0);
        let mut work = |s, b| {
            steps += s;
            bytes += b;
            Ok(())
        };
        let paid = if profile == 2 {
            paid.seal_with_final_results_and_binding_with_work(
                &guard,
                &live,
                1,
                || false,
                |_| Ok::<_, ExecutionError>([facts[0], facts[1]]),
                &mut work,
            )
            .unwrap()
        } else {
            paid.seal_with_proof_ledger_results_and_binding_with_work(
                &guard,
                &live,
                1,
                || false,
                |_| Ok::<_, ExecutionError>(facts),
                &mut work,
            )
            .unwrap()
        };
        assert_eq!(**old.transition(), **paid.transition());
        assert_eq!(
            old.binding().after_roots_digest(),
            paid.binding().after_roots_digest()
        );
        assert!(steps > 0 && bytes > 0);
    }
}
#[test]
fn metered_seal_first_failure_never_calls_final_result_producer() {
    let memory = MemoryBudget::new(32 << 20, 2 << 20).unwrap();
    let source = Empty;
    let tx = ExecutionTransaction::begin(parent(), [32; 32], &source, &memory, limits()).unwrap();
    let guard = tx.guard();
    let live = tx.parent_fence();
    let called = std::cell::Cell::new(false);
    let mut calls = 0;
    let result = tx.seal_with_final_results_and_binding_with_work(
        &guard,
        &live,
        1,
        || false,
        |_| {
            called.set(true);
            Ok::<_, ExecutionError>([
                ExecutionFactRefV1 {
                    kind: ExecutionFactKindV1::Result,
                    id: [4; 32],
                },
                ExecutionFactRefV1 {
                    kind: ExecutionFactKindV1::Result,
                    id: [5; 32],
                },
            ])
        },
        &mut |_, _| {
            calls += 1;
            Err(ExecutionError::Invalid("seal work first"))
        },
    );
    assert!(matches!(
        result,
        Err(SealWithFinalFactError::Execution(ExecutionError::Invalid(
            "seal work first"
        )))
    ));
    assert_eq!(calls, 1);
    assert!(!called.get());
}

#[test]
fn three_result_profile_rejects_non_results_and_repeated_ids() {
    let source = Empty;
    let memory = MemoryBudget::new(64 << 20, 0).unwrap();
    for mode in 0..3 {
        let called = std::cell::Cell::new(false);
        let tx =
            ExecutionTransaction::begin(parent(), [27; 32], &source, &memory, limits()).unwrap();
        let guard = tx.guard();
        let live = tx.parent_fence();
        let result = tx.seal_with_proof_ledger_results_and_binding(
            &guard,
            &live,
            1,
            || false,
            |_| {
                called.set(true);
                Ok::<_, ExecutionError>([
                    ExecutionFactRefV1 {
                        kind: if mode == 0 {
                            ExecutionFactKindV1::Event
                        } else {
                            ExecutionFactKindV1::Result
                        },
                        id: [31; 32],
                    },
                    ExecutionFactRefV1 {
                        kind: if mode == 1 {
                            ExecutionFactKindV1::Input
                        } else {
                            ExecutionFactKindV1::Result
                        },
                        id: if mode == 2 { [31; 32] } else { [32; 32] },
                    },
                    ExecutionFactRefV1 {
                        kind: ExecutionFactKindV1::Result,
                        id: [33; 32],
                    },
                ])
            },
        );
        assert!(
            called.get(),
            "test must reach actual final Result production"
        );
        assert!(
            matches!(result, Err(SealWithFinalFactError::Execution(_))),
            "independent Result triple accepted invalid mode {mode}"
        );
    }
}

#[test]
fn seal_binding_preserves_original_seal_bytes_and_releases_owners() {
    let memory = MemoryBudget::new(32 << 20, 0).unwrap();
    let source = Empty;
    let make = || {
        let mut tx =
            ExecutionTransaction::begin(parent(), [2; 32], &source, &memory, limits()).unwrap();
        tx.write(StateNamespaceV1::Business, b"same-write", Some([4; 32]))
            .unwrap();
        tx.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Input,
            id: [3; 32],
        })
        .unwrap();
        tx
    };
    let ordinary = make();
    let ordinary_guard = ordinary.guard();
    let live = ordinary.parent_fence();
    let ordinary = ordinary.seal(&ordinary_guard, &live, 1).unwrap();
    let wrapped = make();
    let guard = wrapped.guard();
    let wrapped = wrapped.seal_with_binding(&guard, &live, 1).unwrap();
    assert_eq!(wrapped.transition().record, ordinary.record);
    assert_eq!(wrapped.transition().new_nodes, ordinary.new_nodes);
    assert_eq!(
        wrapped.binding().after_roots_digest(),
        ordinary.record.after.digest().unwrap()
    );
    let (transition, binding) = wrapped.into_parts();
    drop(transition);
    binding.validate_source_guard(&guard, &memory).unwrap();
    drop(binding);
    drop(guard);
    drop(ordinary);
    drop(ordinary_guard);
    assert_eq!(memory.used(), 0);
}

#[test]
fn failed_seal_binding_releases_memory_without_exposing_witness() {
    for mode in 0..5 {
        let memory = MemoryBudget::new(32 << 20, 0).unwrap();
        let source = Empty;
        let mut initial = parent();
        if mode == 4 {
            initial.roots.simulation_tick = 10;
        }
        let mut tx =
            ExecutionTransaction::begin(initial, [3; 32], &source, &memory, limits()).unwrap();
        let guard = tx.guard();
        let mut live = tx.parent_fence();
        let mut held = None;
        match mode {
            0 => {
                tx.write(StateNamespaceV1::Business, b"changed", Some([1; 32]))
                    .unwrap();
            }
            1 => {
                live.authority.epoch += 1;
            }
            2 => {
                let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
            }
            3 => {
                held = Some(
                    memory
                        .reserve((32 << 20) - memory.used(), MemoryClass::Ordinary)
                        .unwrap(),
                );
            }
            _ => {}
        }
        assert!(tx.seal_with_binding(&guard, &live, 1).is_err());
        drop(held);
        drop(guard);
        assert_eq!(memory.used(), 0, "failure mode {mode}");
    }
}
