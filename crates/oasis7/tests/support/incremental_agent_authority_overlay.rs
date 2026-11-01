use super::*;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::cell::Cell;
fn state_error(e: oasis7_distfs::incremental_page_store::PageStoreError) -> ValueTreeError {
    use oasis7_distfs::incremental_page_store::PageStoreError as E;
    match e {
        E::Memory(error) => ValueTreeError::Memory(error),
        E::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
        E::Cancelled => ValueTreeError::Cancelled,
        E::Io(error) => ValueTreeError::Io(error),
        E::Missing(_) => ValueTreeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "agent authority physical page missing",
        )),
        E::Corrupt(reason) => ValueTreeError::Invalid(reason),
        E::UnsupportedPlatform => ValueTreeError::Invalid("agent authority physical platform"),
    }
}
fn check_noop_boundary(long_text: bool) {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("overlay"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let mut original = source();
    if long_text {
        original
            .agent_intent_ledger
            .get_mut(" outer intent ")
            .unwrap()
            .summary = "界".repeat(22_000);
    }
    let summary = original.agent_intent_ledger[" outer intent "]
        .summary
        .as_str();
    let roots = seed(&store, &memory, &original);
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical_scope());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let top = roots
        .state
        .iter()
        .find(|root| root.namespace == StateNamespaceV1::Authorization)
        .unwrap();
    let key = AgentAuthorityRecordKeyV1 {
        kind: AgentAuthorityRecordKindV1::IntentLedger,
        id: " outer intent ",
    };
    let mut context = WorldFieldReadContextV1 {
        root: top,
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: 2 << 20,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let family = qualify_agent_authority_family_v1(
        &roots,
        &mut context,
        &mut work().scan,
        &mut work().scan,
        &mut 8192,
        1 << 20,
    )
    .unwrap();
    let base = open_qualified_agent_authority_record_v1(key, &roots, &family, &mut context)
        .unwrap()
        .unwrap();
    for (column, expected, expected_value, expected_bytes) in [
        (
            IntentFieldV1::EventSeq,
            AgentAuthorityValueRefV1::U64(18),
            AgentAuthorityValueV1::Unsigned(18),
            "",
        ),
        (
            IntentFieldV1::Summary,
            AgentAuthorityValueRefV1::Text(summary),
            AgentAuthorityValueV1::Text {
                utf8_bytes: summary.len() as u64,
            },
            summary,
        ),
        (
            IntentFieldV1::TargetId,
            AgentAuthorityValueRefV1::Unit,
            AgentAuthorityValueV1::Unit,
            "",
        ),
        (
            IntentFieldV1::EffectIntentId,
            AgentAuthorityValueRefV1::Text(""),
            AgentAuthorityValueV1::Text { utf8_bytes: 0 },
            "",
        ),
        (
            IntentFieldV1::IntentTick,
            AgentAuthorityValueRefV1::U64(0),
            AgentAuthorityValueV1::Unsigned(0),
            "",
        ),
    ] {
        if long_text && column != IntentFieldV1::Summary {
            continue;
        }
        let cell = AgentAuthorityCellV1::Column(AgentAuthorityColumnV1::Intent(column));
        let getter_n = Cell::new(0usize);
        for mode in 0..4 {
            let active = Cell::new(false);
            let reads = Cell::new(0usize);
            let writes = Cell::new(0usize);
            let state = store
                .state_session(physical_scope(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical_scope());
            let mut tx = ExecutionTransaction::begin(
                parent.clone(),
                [6; 32],
                &state,
                &memory,
                TransactionLimits {
                    max_reads: 32,
                    max_writes: 8,
                    max_facts: 4,
                    max_pending_node_bytes: 4 << 20,
                },
            )
            .unwrap();
            let mut read_io = logical();
            let mut read_keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut read_io,
                },
                key_work: &mut read_keys,
                semantic_bytes: 1024,
                record_bytes: 2 << 20,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || {
                    if active.get() {
                        reads.set(reads.get() + 1);
                        if mode == 2 && reads.get() == getter_n.get() + 1 {
                            return true;
                        }
                    }
                    false
                },
            };
            let mut backend = store.value_session(physical_scope());
            let mut write_io = logical();
            let mut write_keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut backend,
                memory: &memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 1 << 20,
                    record_bytes: 2 << 20,
                },
                cancelled: || {
                    if active.get() {
                        writes.set(writes.get() + 1);
                        if mode == 3 && writes.get() == getter_n.get() + 1 {
                            return true;
                        }
                    }
                    false
                },
            };
            let guard = overlay.transaction.guard();
            let proof = change_qualified_agent_authority_cell_v1(
                AgentAuthorityCellChangeV1 {
                    key,
                    cell: AgentAuthorityCellV1::Column(AgentAuthorityColumnV1::Intent(
                        IntentFieldV1::EventSeq,
                    )),
                    qualification: &base,
                    expected: AgentAuthorityValueRefV1::U64(17),
                    after: AgentAuthorityValueRefV1::U64(18),
                    max_current_payload_bytes: 1 << 20,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            active.set(true);
            let guard = overlay.transaction.guard();
            if mode == 0 {
                let _output_charge = memory
                    .reserve(expected_bytes.len(), MemoryClass::Ordinary)
                    .unwrap();
                let mut output = Vec::with_capacity(expected_bytes.len());
                let value = read_qualified_agent_authority_cell_overlay_v1(
                    AgentAuthorityCellRequestV1 {
                        key,
                        cell,
                        qualification: &proof,
                    },
                    &mut overlay,
                    1 << 20,
                    &mut output,
                    &guard,
                    &live,
                )
                .unwrap();
                assert_eq!(value, expected_value);
                assert_eq!(output, expected_bytes.as_bytes());
                assert_eq!(writes.get(), 0);
                assert!(reads.get() > 1);
                getter_n.set(reads.get());
            } else {
                let result = change_qualified_agent_authority_cell_v1(
                    AgentAuthorityCellChangeV1 {
                        key,
                        cell,
                        qualification: &proof,
                        expected,
                        after: expected,
                        max_current_payload_bytes: 1 << 20,
                        max_pending_node_bytes: 4 << 20,
                    },
                    &mut overlay,
                    &mut write,
                    &guard,
                    &live,
                );
                assert_eq!(reads.get(), getter_n.get() + 1);
                if mode == 1 {
                    assert!(result.unwrap().is_none());
                    assert_eq!(writes.get(), getter_n.get() + 1);
                } else {
                    assert!(matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                    ));
                    let guard = overlay.transaction.guard();
                    assert!(matches!(
                        tx.seal(&guard, &live, 1),
                        Err(ExecutionError::Aborted)
                    ));
                    continue;
                }
            }
            let guard = overlay.transaction.guard();
            let sealed = tx.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.writes.len(), 1);
        }
        eprintln!(
            "agent_authority overlay cell={cell:?}: independent getter N={}, NoOp N+1={}",
            getter_n.get(),
            getter_n.get() + 1
        );
    }
}

#[test]
fn overlay_getter_sees_prefix_and_independently_calibrates_noop_final_poll() {
    check_noop_boundary(false)
}
#[test]
fn long_original_text_noop_checks_cancellation_after_full_equal_comparison() {
    check_noop_boundary(true)
}
