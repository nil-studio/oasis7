use super::*;
#[test]
fn public_row_expected_read_n_then_noop_n_plus_one_cancels_after_long_equality() {
    let mut baseline = 0;
    let mut rejected = [false; 2];
    for mode in 0..3 {
        let memory = MemoryBudget::new(96 << 20, 8 << 20).unwrap();
        let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
        let _inputs = memory.reserve(256 << 10, MemoryClass::Ordinary).unwrap();
        let (pages, roots) = fixture::seed(&memory);
        let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
        let extra = RefCell::new(Pages::default());
        let values = Values {
            base: &pages,
            extra: &extra,
        };
        let state = fixture::State::new(&pages, &memory);
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent, [12; 32], &state, &memory, limits()).unwrap();
        let mut read_io = io();
        let mut write_io = io();
        let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut sink = Sink(&extra);
        let original = crate::agent_state::BodyModuleSlot {
            slot_id: "界".repeat(22_000),
            slot_type: crate::agent_state::BodySlotType::Universal,
            installed_module: Some("".into()),
            locked: true,
        };
        let equal = original.clone();
        assert_ne!(original.slot_id.as_ptr(), equal.slot_id.as_ptr());
        let guard = tx.guard();
        assert!(
            change_agent_subtree_row_v1(
                AgentSubtreeRowChangeV1 {
                    agent: "agent α",
                    qualification: &proof,
                    ordinal: 2,
                    expected_count: 2,
                    expected_next: 2,
                    expected: None,
                    after: Some(AgentSubtreeRowRefV1::Slot(&original)),
                    max_scalar_bytes: 1 << 20,
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || false
                ),
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut write_io,
                    cancelled: || false
                },
                &guard,
                &live
            )
            .unwrap()
        );
        let polls = Cell::new(0usize);
        let guard = tx.guard();
        if mode == 0 {
            verify_agent_subtree_row_expected_overlay_v1(
                AgentSubtreeRowExpectedV1 {
                    agent: "agent α",
                    qualification: &proof,
                    ordinal: 2,
                    count: 3,
                    next: 3,
                    expected: AgentSubtreeRowRefV1::Slot(&original),
                    max_scalar_bytes: 1 << 20,
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || {
                        polls.set(polls.get() + 1);
                        false
                    }
                ),
                &guard,
                &live,
            )
            .unwrap();
            baseline = polls.get();
            assert!(baseline > 0);
            let guard = tx.guard();
            assert!(tx.seal(&guard, &live, 1).is_ok());
        } else {
            let result = change_agent_subtree_row_v1(
                AgentSubtreeRowChangeV1 {
                    agent: "agent α",
                    qualification: &proof,
                    ordinal: 2,
                    expected_count: 3,
                    expected_next: 3,
                    expected: Some(AgentSubtreeRowRefV1::Slot(&original)),
                    after: Some(AgentSubtreeRowRefV1::Slot(&equal)),
                    max_scalar_bytes: 1 << 20,
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || {
                        if mode == 1 {
                            polls.set(polls.get() + 1);
                            polls.get() == baseline + 1
                        } else {
                            false
                        }
                    }
                ),
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut write_io,
                    cancelled: || {
                        if mode == 2 {
                            polls.set(polls.get() + 1);
                            polls.get() == baseline + 1
                        } else {
                            false
                        }
                    },
                },
                &guard,
                &live,
            );
            let cancelled = matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
            );
            let guard = tx.guard();
            let aborted = matches!(tx.seal(&guard, &live, 1), Err(ExecutionError::Aborted));
            eprintln!(
                "agent row getterN={baseline} cancelN+1={} mode={mode} actualPolls={} cancelled={cancelled} prefixAborted={aborted}",
                baseline + 1,
                polls.get()
            );
            rejected[mode - 1] = cancelled && aborted;
        }
    }
    assert!(
        rejected.into_iter().all(|value| value),
        "both public N+1 channels must cancel and poison"
    );
}
