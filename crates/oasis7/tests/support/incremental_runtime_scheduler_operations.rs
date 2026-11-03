use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
#[test]
fn scheduler_controls_same_transaction_and_changed_receiver() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&directory.0.join("ops"), memory.clone(), compression)
                .unwrap();
        let roots = seed(&store, &memory);
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
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            roots
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Scheduler)
                .unwrap(),
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let family = qualify_runtime_scheduler_controls_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            32,
        )
        .unwrap();
        let mut routing = open_qualified_runtime_routing_v1(&roots, &family, &mut ctx).unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
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
            take_state_error: || state.take_read_error().map(state_error),
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
        use RuntimeSchedulerControlValueRefV1 as V;
        use RuntimeSchedulerControlValueV1 as D;
        use RuntimeSchedulerScalarV1 as S;
        // Cursor Option remains distinguishable at each pending step; no full group rewrite.
        for (field, before, after, want) in [
            (S::Cursor, V::Text(""), V::Unit, D::Unit),
            (
                S::Cursor,
                V::Unit,
                V::Text("next"),
                D::Text { utf8_bytes: 4 },
            ),
            (
                S::MaxPendingActions,
                V::U64(usize::MAX as u64),
                V::U64(0),
                D::Unsigned(0),
            ),
            (
                S::PendingActionsEvicted,
                V::U64(u64::MAX),
                V::U64(0),
                D::Unsigned(0),
            ),
        ] {
            let guard = overlay.transaction.guard();
            assert!(
                change_qualified_runtime_scheduler_scalar_v1(
                    RuntimeSchedulerScalarChangeV1 {
                        field,
                        qualification: &family,
                        expected: before,
                        after,
                        max_current_payload_bytes: 32
                    },
                    &mut overlay,
                    &mut write,
                    &guard,
                    &live
                )
                .unwrap()
            );
            let guard = overlay.transaction.guard();
            let mut out = Vec::new();
            assert_eq!(
                read_qualified_runtime_scheduler_scalar_overlay_v1(
                    RuntimeSchedulerScalarRequestV1 {
                        field,
                        qualification: &family
                    },
                    &mut overlay,
                    32,
                    &mut out,
                    &guard,
                    &live
                )
                .unwrap(),
                want
            );
            if field == S::Cursor {
                assert_eq!(out, after_text(after));
            }
            let guard = overlay.transaction.guard();
            assert!(
                !change_qualified_runtime_scheduler_scalar_v1(
                    RuntimeSchedulerScalarChangeV1 {
                        field,
                        qualification: &family,
                        expected: after,
                        after,
                        max_current_payload_bytes: 32
                    },
                    &mut overlay,
                    &mut write,
                    &guard,
                    &live
                )
                .unwrap()
            );
        }
        use RuntimeRoutingColumnV1 as C;
        for (column, before, after, want) in [
            (C::OldestOverdueTicks, V::U64(0), V::Unit, D::Unit),
            (
                C::OldestOverdueTicks,
                V::Unit,
                V::U64(u64::MAX),
                D::Unsigned(u64::MAX),
            ),
            (
                C::LastDueCount,
                V::U64(usize::MAX as u64),
                V::U64(0),
                D::Unsigned(0),
            ),
        ] {
            let guard = overlay.transaction.guard();
            routing = change_qualified_runtime_routing_column_v1(
                RuntimeRoutingChangeV1 {
                    column,
                    family: &family,
                    qualification: &routing,
                    expected: before,
                    after,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            let guard = overlay.transaction.guard();
            assert_eq!(
                read_qualified_runtime_routing_overlay_v1(
                    RuntimeRoutingRequestV1 {
                        column,
                        family: &family,
                        qualification: &routing
                    },
                    &mut overlay,
                    &guard,
                    &live
                )
                .unwrap(),
                want
            );
            let guard = overlay.transaction.guard();
            assert!(
                change_qualified_runtime_routing_column_v1(
                    RuntimeRoutingChangeV1 {
                        column,
                        family: &family,
                        qualification: &routing,
                        expected: after,
                        after,
                        max_pending_node_bytes: 4 << 20
                    },
                    &mut overlay,
                    &mut write,
                    &guard,
                    &live
                )
                .unwrap()
                .is_none()
            );
        }
        let guard = overlay.transaction.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.writes.len(), 4);
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let generic = verifier.verify(&sealed.record).unwrap();
        let state2 = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values2 = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut receiver = read_ctx!(
            sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Scheduler)
                .unwrap(),
            &state2,
            &values2,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let accepted = accept_runtime_scheduler_controls_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut receiver,
            &mut 8192,
            32,
        )
        .unwrap();
        accepted
            .validate_binding(&sealed.record.after.digest().unwrap())
            .unwrap();
        for node in &generic.new_nodes {
            writer
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    memory
                        .reserve(node.bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        let state3 = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values3 = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut final_ctx = read_ctx!(
            sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Scheduler)
                .unwrap(),
            &state3,
            &values3,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let final_family = qualify_runtime_scheduler_controls_v1(
            &sealed.record.after,
            &mut final_ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            32,
        )
        .unwrap();
        assert_eq!(final_family.routing_root(), accepted.routing_root());
        let mut out = Vec::new();
        assert_eq!(
            read_qualified_runtime_scheduler_scalar_v1(
                S::Cursor,
                &sealed.record.after,
                &final_family,
                &mut final_ctx,
                32,
                &mut out
            )
            .unwrap(),
            D::Text { utf8_bytes: 4 }
        );
        assert_eq!(out, b"next");
    }
}
fn after_text(value: RuntimeSchedulerControlValueRefV1<'_>) -> &[u8] {
    if let RuntimeSchedulerControlValueRefV1::Text(v) = value {
        v.as_bytes()
    } else {
        b""
    }
}

#[test]
fn scalar_getter_n_then_noop_n_plus_one_cancels_both_channels_and_poisons_prefix() {
    use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
    use std::cell::Cell;
    let mut getter_n = 0;
    for mode in 0..3 {
        let compression = PageCompression::Raw;
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&directory.0.join("ops"), memory.clone(), compression)
                .unwrap();
        let roots = seed(&store, &memory);
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
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            roots
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Scheduler)
                .unwrap(),
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let family = qualify_runtime_scheduler_controls_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            32,
        )
        .unwrap();

        let mut tx = ExecutionTransaction::begin(
            parent,
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 16,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let active = Cell::new(false);
        let calls = Cell::new(0u64);
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
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || {
                if active.get() && mode != 2 {
                    calls.set(calls.get() + 1);
                    mode == 1 && calls.get() == getter_n + 1
                } else {
                    false
                }
            },
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
            cancelled: || {
                if active.get() && mode == 2 {
                    calls.set(calls.get() + 1);
                    calls.get() == getter_n + 1
                } else {
                    false
                }
            },
        };
        use RuntimeSchedulerControlValueRefV1 as V;
        let guard = overlay.transaction.guard();
        assert!(
            change_qualified_runtime_scheduler_scalar_v1(
                RuntimeSchedulerScalarChangeV1 {
                    field: RuntimeSchedulerScalarV1::PendingEffectsEvicted,
                    qualification: &family,
                    expected: V::U64(0),
                    after: V::U64(1),
                    max_current_payload_bytes: 8
                },
                &mut overlay,
                &mut write,
                &guard,
                &live
            )
            .unwrap()
        );
        active.set(true);
        let guard = overlay.transaction.guard();
        if mode == 0 {
            assert_eq!(
                read_qualified_runtime_scheduler_scalar_overlay_v1(
                    RuntimeSchedulerScalarRequestV1 {
                        field: RuntimeSchedulerScalarV1::MaxPendingActions,
                        qualification: &family
                    },
                    &mut overlay,
                    8,
                    &mut std::io::sink(),
                    &guard,
                    &live
                )
                .unwrap(),
                RuntimeSchedulerControlValueV1::Unsigned(usize::MAX as u64)
            );
            let count = calls.get();
            active.set(false);
            getter_n = count;
            assert!(getter_n > 0);
            let final_guard = tx.guard();
            assert_eq!(
                tx.seal(&final_guard, &live, 1).unwrap().record.writes.len(),
                1
            );
        } else {
            let result = change_qualified_runtime_scheduler_scalar_v1(
                RuntimeSchedulerScalarChangeV1 {
                    field: RuntimeSchedulerScalarV1::MaxPendingActions,
                    qualification: &family,
                    expected: V::U64(usize::MAX as u64),
                    after: V::U64(usize::MAX as u64),
                    max_current_payload_bytes: 8,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
            );
            assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                ),
                "mode={mode}: {result:?}"
            );
            assert_eq!(calls.get(), getter_n + 1);
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
        }
    }
    eprintln!(
        "scheduler scalar getterN={getter_n} NoOp cancelledN={}",
        getter_n + 1
    );
}
