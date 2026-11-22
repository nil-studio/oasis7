use super::*;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};

#[test]
fn recovery_qualification_cannot_cross_into_ordinary_reader() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = seed(&store, &memory);
    let root = roots
        .state
        .iter()
        .find(|r| r.namespace == StateNamespaceV1::Scheduler)
        .unwrap();
    let recovery = store
        .state_session(physical(), MemoryClass::Recovery)
        .unwrap();
    let values = store.value_session(physical());
    let (family, routing) = {
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            root,
            &recovery,
            &values,
            &memory,
            MemoryClass::Recovery,
            &mut io,
            &mut keys
        );
        let family = qualify_runtime_scheduler_controls_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            165,
        )
        .unwrap();
        let routing = open_qualified_runtime_routing_v1(&roots, &family, &mut ctx).unwrap();
        (family, routing)
    };
    let ordinary = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = read_ctx!(
        root,
        &ordinary,
        &values,
        &memory,
        MemoryClass::Ordinary,
        &mut io,
        &mut keys
    );
    let ordinary_family = qualify_runtime_scheduler_controls_v1(
        &roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut 1000,
        165,
    )
    .unwrap();
    assert_eq!(
        read_qualified_runtime_scheduler_scalar_v1(
            RuntimeSchedulerScalarV1::MaxPendingEffects,
            &roots,
            &ordinary_family,
            &mut ctx,
            165,
            &mut std::io::sink()
        )
        .unwrap(),
        RuntimeSchedulerControlValueV1::Unsigned(0)
    );
    let result = read_qualified_runtime_scheduler_scalar_v1(
        RuntimeSchedulerScalarV1::MaxPendingEffects,
        &roots,
        &family,
        &mut ctx,
        165,
        &mut std::io::sink(),
    );
    assert!(
        matches!(result, Err(ValueTreeError::Invalid(_))),
        "Recovery qualifier crossed class: {result:?}"
    );
    assert_eq!(family.allocation_class(), MemoryClass::Recovery);
    assert_eq!(routing.allocation_class(), MemoryClass::Recovery);
    let mut wrong = roots.clone();
    wrong.height += 1;
    let foreign = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    assert!(matches!(
        readmit_runtime_scheduler_controls_ordinary_v1(&family, &wrong, &memory, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    assert!(matches!(
        readmit_runtime_scheduler_controls_ordinary_v1(&family, &roots, &foreign, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    assert!(matches!(
        readmit_runtime_scheduler_controls_ordinary_v1(&ordinary_family, &roots, &memory, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    let polls = std::cell::Cell::new(0);
    let before = memory.used();
    assert!(matches!(
        readmit_runtime_scheduler_controls_ordinary_v1(&family, &roots, &memory, || {
            polls.set(polls.get() + 1);
            polls.get() == 2
        }),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(polls.get(), 2);
    assert_eq!(memory.used(), before);
    let hold = memory
        .reserve((56 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        readmit_runtime_scheduler_controls_ordinary_v1(&family, &roots, &memory, || false),
        Err(ValueTreeError::Memory(_))
    ));
    drop(hold);
    let admitted =
        readmit_runtime_scheduler_controls_ordinary_v1(&family, &roots, &memory, || false).unwrap();
    assert_eq!(admitted.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(admitted.routing_root(), family.routing_root());
    assert_eq!(
        read_qualified_runtime_scheduler_scalar_v1(
            RuntimeSchedulerScalarV1::MaxPendingEffects,
            &roots,
            &admitted,
            &mut ctx,
            165,
            &mut std::io::sink()
        )
        .unwrap(),
        RuntimeSchedulerControlValueV1::Unsigned(0)
    );
    let mut wrong_child = routing.root().clone();
    wrong_child.leaf_count += 1;
    assert!(matches!(
        readmit_runtime_routing_ordinary_v1(&routing, &wrong_child, &memory, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    assert!(matches!(
        readmit_runtime_routing_ordinary_v1(&routing, routing.root(), &foreign, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    let child =
        readmit_runtime_routing_ordinary_v1(&routing, routing.root(), &memory, || false).unwrap();
    assert_eq!(child.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(child.root(), routing.root());
    // Both parent and nested-child class failures must abort an already-mutated transaction.
    for bad_family in [true, false] {
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent,
            [6; 32],
            &ordinary,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 16,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut overlay_io = logical();
        let mut overlay_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut overlay_io,
            },
            key_work: &mut overlay_keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || ordinary.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut writer = store.value_session(physical());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: limits(),
            cancelled: || false,
        };
        let guard = overlay.transaction.guard();
        assert!(
            change_qualified_runtime_scheduler_scalar_v1(
                RuntimeSchedulerScalarChangeV1 {
                    field: RuntimeSchedulerScalarV1::PendingEffectsEvicted,
                    qualification: &ordinary_family,
                    expected: RuntimeSchedulerControlValueRefV1::U64(0),
                    after: RuntimeSchedulerControlValueRefV1::U64(1),
                    max_current_payload_bytes: 165,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
            )
            .unwrap()
        );
        let guard = overlay.transaction.guard();
        let result = read_qualified_runtime_routing_overlay_v1(
            RuntimeRoutingRequestV1 {
                column: RuntimeRoutingColumnV1::RoutingCount,
                family: if bad_family {
                    &family
                } else {
                    &ordinary_family
                },
                qualification: if bad_family { &child } else { &routing },
            },
            &mut overlay,
            &guard,
            &live,
        );
        assert!(
            matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ),
            "{result:?}"
        );
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
    let before = memory.used();
    let old_fee = family.reserved_bytes();
    let new_fee = admitted.reserved_bytes();
    drop(family);
    assert_eq!(memory.used() + old_fee, before);
    admitted.validate_binding(&roots.digest().unwrap()).unwrap();
    drop(admitted);
    assert_eq!(memory.used() + old_fee + new_fee, before);
}
