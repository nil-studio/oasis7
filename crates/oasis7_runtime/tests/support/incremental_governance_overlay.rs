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
            "governance physical page missing",
        )),
        E::Corrupt(reason) => ValueTreeError::Invalid(reason),
        E::UnsupportedPlatform => ValueTreeError::Invalid("governance physical platform"),
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
            .governance_proposals
            .get_mut(" outer proposal ")
            .unwrap()
            .description = "界".repeat(22_000);
    }
    let summary = original.governance_proposals[" outer proposal "]
        .description
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
    let key = GovernanceRecordKeyV1 {
        kind: GovernanceRecordKindV1::Identity,
        id: " outer identity ",
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
    let family = qualify_governance_family_v1(
        &roots,
        &mut context,
        &mut work().scan,
        &mut work().scan,
        &mut 8192,
        1 << 20,
    )
    .unwrap();
    let base = open_qualified_governance_record_v1(key, &roots, &family, &mut context)
        .unwrap()
        .unwrap();
    let proposal_key = GovernanceRecordKeyV1 {
        kind: GovernanceRecordKindV1::Proposal,
        id: " outer proposal ",
    };
    let proposal_base =
        open_qualified_governance_record_v1(proposal_key, &roots, &family, &mut context)
            .unwrap()
            .unwrap();
    use GovernanceCellV1 as G;
    use GovernanceColumnV1 as F;
    use GovernanceSnapshotColumnV1 as S;
    for (target_key, cell, expected, expected_value, expected_bytes) in [
        (
            key,
            G::Column(F::StakeLocked),
            Some(GovernanceValueRefV1::U64(7)),
            Some(GovernanceValueV1::Unsigned(7)),
            "",
        ),
        (
            proposal_key,
            G::Column(F::FinalizedAt),
            Some(GovernanceValueRefV1::Unit),
            Some(GovernanceValueV1::Unit),
            "",
        ),
        (
            proposal_key,
            G::Snapshot(" raw snapshot key ", S::ReputationScore),
            Some(GovernanceValueRefV1::I64(i64::MIN)),
            Some(GovernanceValueV1::Signed(i64::MIN)),
            "",
        ),
        (
            proposal_key,
            G::Option(1),
            Some(GovernanceValueRefV1::Text("")),
            Some(GovernanceValueV1::Text { utf8_bytes: 0 }),
            "",
        ),
        (
            proposal_key,
            G::Snapshot(" absent member ", S::AgentId),
            None,
            None,
            "",
        ),
        (
            proposal_key,
            G::Column(F::Description),
            Some(GovernanceValueRefV1::Text(summary)),
            Some(GovernanceValueV1::Text {
                utf8_bytes: summary.len() as u64,
            }),
            summary,
        ),
    ] {
        if (long_text && cell != G::Column(F::Description))
            || (!long_text && cell == G::Column(F::Description))
        {
            continue;
        }
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
            let proof = change_qualified_governance_cell_v1(
                GovernanceCellChangeV1 {
                    key,
                    cell: GovernanceCellV1::Column(GovernanceColumnV1::StakeLocked),
                    qualification: &base,
                    expected: Some(GovernanceValueRefV1::U64(u64::MAX)),
                    after: Some(GovernanceValueRefV1::U64(7)),
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
            let target_proof = if target_key.kind == GovernanceRecordKindV1::Identity {
                &proof
            } else {
                &proposal_base
            };
            active.set(true);
            let guard = overlay.transaction.guard();
            if mode == 0 {
                let _output_charge = memory
                    .reserve(expected_bytes.len(), MemoryClass::Ordinary)
                    .unwrap();
                let mut output = Vec::with_capacity(expected_bytes.len());
                let value = read_qualified_governance_cell_overlay_v1(
                    GovernanceCellRequestV1 {
                        key: target_key,
                        cell,
                        qualification: target_proof,
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
                let result = change_qualified_governance_cell_v1(
                    GovernanceCellChangeV1 {
                        key: target_key,
                        cell,
                        qualification: target_proof,
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
            "governance overlay cell={cell:?}: independent getter N={}, NoOp N+1={}",
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
