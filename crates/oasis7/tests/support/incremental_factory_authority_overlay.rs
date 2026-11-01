use super::*;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::cell::Cell;
#[test]
fn overlay_getter_sees_prefix_and_independently_calibrates_noop_final_poll() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("overlay"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let top = seed(&source(), &store, &memory);
    let mut roots = RootSetV1::empty("factory-authority-overlay".into(), [0x77; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Business)
        .unwrap() = top;
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
        .find(|root| root.namespace == StateNamespaceV1::Business)
        .unwrap();
    let key = FactoryAuthorityRecordKeyV1 {
        kind: FactoryAuthorityRecordKindV1::Site,
        id: " outer-site\0界 ",
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
        record_bytes: 4096,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let child = inspect_factory_authority_root_v1(key, &mut context)
        .unwrap()
        .unwrap();
    let base = {
        let mut inner = WorldFieldReadContextV1 {
            root: &child,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        qualify_factory_authority_body_v1(
            key.kind,
            owner(key.id),
            &mut inner,
            &mut scan(),
            &mut 1000,
            165,
        )
        .unwrap()
    };
    for (cell, expected, expected_value, expected_bytes) in [
        (
            FactoryAuthorityCellV1::AuthorizedCount,
            FactoryAuthorityCellValueRefV1::U64(3),
            FactoryAuthorityValueV1::Unsigned(3),
            "",
        ),
        (
            FactoryAuthorityCellV1::AuthorityRevision,
            FactoryAuthorityCellValueRefV1::U64(u64::MAX - 1),
            FactoryAuthorityValueV1::Unsigned(u64::MAX - 1),
            "",
        ),
        (
            FactoryAuthorityCellV1::SiteId,
            FactoryAuthorityCellValueRefV1::Text(" inner-site\0界 "),
            FactoryAuthorityValueV1::Text {
                utf8_bytes: " inner-site\0界 ".len() as u64,
            },
            " inner-site\0界 ",
        ),
    ] {
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
                record_bytes: 4096,
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
                    payload_bytes: 165,
                    record_bytes: 4096,
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
            let proof = change_qualified_factory_authority_cell_v1(
                FactoryAuthorityCellChangeV1 {
                    key,
                    cell: FactoryAuthorityCellV1::AuthorityRevision,
                    qualification: &base,
                    expected: Some(FactoryAuthorityCellValueRefV1::U64(u64::MAX)),
                    after: Some(FactoryAuthorityCellValueRefV1::U64(u64::MAX - 1)),
                    max_current_payload_bytes: 165,
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
                let mut output = Vec::new();
                let value = read_qualified_factory_authority_cell_overlay_v1(
                    FactoryAuthorityCellRequestV1 {
                        key,
                        cell,
                        qualification: &proof,
                    },
                    &mut overlay,
                    165,
                    &mut output,
                    &guard,
                    &live,
                )
                .unwrap();
                assert_eq!(value, Some(expected_value));
                assert_eq!(output, expected_bytes.as_bytes());
                assert_eq!(writes.get(), 0);
                assert!(reads.get() > 1);
                getter_n.set(reads.get());
            } else {
                let result = if cell == FactoryAuthorityCellV1::AuthorizedCount {
                    change_qualified_factory_allowlist_v1(
                        FactoryAuthorityAllowlistChangeV1 {
                            key,
                            qualification: &proof,
                            expected_len: 3,
                            edit: FactoryAuthorityAllowlistEditV1::Truncate(3),
                        },
                        &mut overlay,
                        &mut write,
                        &mut FactoryAuthorityWorkV1 {
                            cells_remaining: 100,
                            max_pending_node_bytes: 4 << 20,
                        },
                        &guard,
                        &live,
                    )
                } else {
                    change_qualified_factory_authority_cell_v1(
                        FactoryAuthorityCellChangeV1 {
                            key,
                            cell,
                            qualification: &proof,
                            expected: Some(expected),
                            after: Some(expected),
                            max_current_payload_bytes: 165,
                            max_pending_node_bytes: 4 << 20,
                        },
                        &mut overlay,
                        &mut write,
                        &guard,
                        &live,
                    )
                };
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
            "factory_authority overlay cell={cell:?}: independent getter N={}, NoOp N+1={}",
            getter_n.get(),
            getter_n.get() + 1
        );
    }
}
