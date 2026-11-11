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
