use super::*;

#[test]
fn real_current_agent_identity_point_reads_respect_parent_and_budget() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let (pages, roots) = fixture::seed(&memory);
    let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
    let state = fixture::State::new(&pages, &memory);
    let extra = RefCell::new(Pages::default());
    let values = Values {
        base: &pages,
        extra: &extra,
    };
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent,
        [0x39; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 1024,
            max_writes: 16,
            ..limits()
        },
    )
    .unwrap();
    let mut read_io = io();
    let mut original_io = io();
    let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut original_keys = crate::incremental_semantic_key::SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut original = crate::incremental_field_read::WorldFieldReadContextV1 {
        root: roots.state_root(AgentFieldV1::Identity.namespace()),
        state: &state,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut original_io,
        },
        key_work: &mut original_keys,
        semantic_bytes: 1024,
        record_bytes: 1 << 20,
        take_state_error: || state.take(),
        cancelled: || false,
    };
    let mut pending_entries = 1024;
    let guard = tx.guard();
    let existing = read_current_agent_presence_v1(
        "agent α",
        &proof,
        &roots,
        &mut overlay!(
            &mut tx,
            &state,
            &values,
            &memory,
            &mut read_io,
            &mut keys,
            || false
        ),
        &mut original,
        &mut pending_entries,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(existing, CurrentAgentPresenceV1::Existing);
    assert!(proof.uses_budget(&memory));

    // The genuine GEN-seeded parent has no other agent. An absent identity
    // is not inferred from a caller-supplied decoded key or from a fake view.
    let guard = tx.guard();
    let absent = read_current_agent_presence_v1(
        "unknown agent",
        &proof,
        &roots,
        &mut overlay!(
            &mut tx,
            &state,
            &values,
            &memory,
            &mut read_io,
            &mut keys,
            || false
        ),
        &mut original,
        &mut pending_entries,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(absent, CurrentAgentPresenceV1::Absent);
    assert!(pending_entries <= 1024);
}
