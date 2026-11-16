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
