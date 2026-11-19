use super::*;
#[test]
fn controls_explicit_readmission_preserves_exact_root_and_releases_phase() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let (pages, roots) = seed(&memory, "authority", Some(0));
    let recovery = Classed {
        pages: &pages,
        class: MemoryClass::Recovery,
    };
    let mut values = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let qualified = qualify_runtime_controls_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &recovery,
            values: ValueReadContext {
                source: &recovery,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut values,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 100_000,
            leaves_remaining: 1000,
            read_bytes_remaining: 64 << 20,
        },
        1024,
    )
    .unwrap();
    let before = memory.used();
    let ordinary =
        readmit_runtime_controls_ordinary_v1(&qualified, &roots, &memory, || false).unwrap();
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(memory.used(), before + ordinary.reserved_bytes());
    ordinary.validate_binding(&roots).unwrap();
    assert!(readmit_runtime_controls_ordinary_v1(&ordinary, &roots, &memory, || false).is_err());
    assert!(readmit_runtime_controls_ordinary_v1(&qualified, &roots, &memory, || true).is_err());
    let other = RootSetV1::empty("other".into(), [3; 32]).unwrap();
    assert!(readmit_runtime_controls_ordinary_v1(&qualified, &other, &memory, || false).is_err());
    let foreign = MemoryBudget::new(1 << 20, 1 << 18).unwrap();
    assert!(readmit_runtime_controls_ordinary_v1(&qualified, &roots, &foreign, || false).is_err());
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let tx = ExecutionTransaction::begin(
        parent.clone(),
        [6; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let mut values = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    accept_runtime_controls_transition_v1(
        &ordinary,
        &verifier,
        &sealed.record,
        &mut WorldFieldReadContextV1 {
            root: &sealed.record.after.state[0],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut values,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        1024,
    )
    .unwrap();
    drop(guard);
    drop(sealed);
    drop(ordinary);
    assert_eq!(memory.used(), before);
}
