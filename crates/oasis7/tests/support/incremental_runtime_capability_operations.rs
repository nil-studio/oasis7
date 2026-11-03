use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
#[test]
fn capability_same_transaction_subject_change_and_receiver() {
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
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .unwrap(),
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let family = qualify_runtime_capability_family_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            165,
        )
        .unwrap();
        use RuntimeCapabilityColumnV1 as C;
        use RuntimeCapabilityRecordKindV1 as K;
        let key = RuntimeCapabilityRecordKeyV1 {
            kind: K::Budget,
            id: "agent-key",
        };
        let mut budget =
            open_qualified_runtime_capability_record_v1(key, &roots, &family, &mut ctx)
                .unwrap()
                .unwrap();
        let old_nonce_key = RuntimeCapabilityRecordKeyV1 {
            kind: K::Nonce,
            id: "none-key",
        };
        let old_nonce =
            open_qualified_runtime_capability_record_v1(old_nonce_key, &roots, &family, &mut ctx)
                .unwrap()
                .unwrap();
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

        use RuntimeCapabilityValueRefV1 as V;
        use RuntimeCapabilityValueV1 as D;
        let guard = overlay.transaction.guard();
        budget = change_qualified_runtime_capability_cell_v1(
            RuntimeCapabilityPointChangeV1 {
                request: RuntimeCapabilityPointRequestV1 {
                    key,
                    column: C::RemainingUnits,
                    qualification: &budget,
                },
                expected: V::Signed(i64::MIN),
                after: V::Signed(17),
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
        let guard = overlay.transaction.guard();
        assert_eq!(
            read_qualified_runtime_capability_overlay_v1(
                RuntimeCapabilityPointRequestV1 {
                    key,
                    column: C::RemainingUnits,
                    qualification: &budget
                },
                &mut overlay,
                165,
                &mut std::io::sink(),
                &guard,
                &live
            )
            .unwrap(),
            D::Signed(17)
        );
        let before = CapabilitySubject::Agent {
            agent_id: "embedded-agent".into(),
            owner_binding: "".into(),
            generation: u64::MAX,
        };
        let after = CapabilitySubject::Module {
            module_id: "new-module".into(),
            module_version: "".into(),
            instance_id: "new-instance".into(),
        };
        let guard = overlay.transaction.guard();
        budget = change_qualified_runtime_capability_subject_v1(
            RuntimeCapabilitySubjectChangeV1 {
                request: RuntimeCapabilitySubjectRequestV1 {
                    key,
                    qualification: &budget,
                },
                expected: &before,
                after: &after,
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
        let guard = overlay.transaction.guard();
        let mut output = Vec::new();
        let decoded = read_qualified_runtime_capability_subject_overlay_v1(
            RuntimeCapabilitySubjectRequestV1 {
                key,
                qualification: &budget,
            },
            &mut overlay,
            165,
            &mut output,
            &guard,
            &live,
        )
        .unwrap();
        assert_eq!(
            decoded,
            [
                D::Text { utf8_bytes: 6 },
                D::Unit,
                D::Unit,
                D::Unit,
                D::Text { utf8_bytes: 10 },
                D::Text { utf8_bytes: 0 },
                D::Text { utf8_bytes: 12 },
                D::Unit,
                D::Unit
            ]
        );
        assert_eq!(output, b"modulenew-modulenew-instance");
        let guard = overlay.transaction.guard();
        assert!(
            change_qualified_runtime_capability_subject_v1(
                RuntimeCapabilitySubjectChangeV1 {
                    request: RuntimeCapabilitySubjectRequestV1 {
                        key,
                        qualification: &budget
                    },
                    expected: &after,
                    after: &after,
                    max_current_payload_bytes: 165,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live
            )
            .unwrap()
            .is_none()
        );
        let created_source = CapabilityAuthorizationNonceRecord {
            request_hash: "new-request".into(),
            outcome_hash: "".into(),
            committed_receipt_id: Some("".into()),
            state: "original-new-state".into(),
        };
        let created_key = RuntimeCapabilityRecordKeyV1 {
            kind: K::Nonce,
            id: "created-key",
        };
        let mut work = RuntimeCapabilityRecordsWorkV1 {
            cells_remaining: 14,
            max_pending_node_bytes: 4 << 20,
        };
        let guard = overlay.transaction.guard();
        let created = create_qualified_runtime_capability_record_v1(
            RuntimeCapabilityCreateV1 {
                key: created_key,
                family: &family,
                source: RuntimeCapabilityRecordRefV1::Nonce(&created_source),
            },
            &mut overlay,
            &mut write,
            &mut work,
            &guard,
            &live,
        )
        .unwrap();
        let guard = overlay.transaction.guard();
        assert_eq!(
            read_qualified_runtime_capability_overlay_v1(
                RuntimeCapabilityPointRequestV1 {
                    key: created_key,
                    column: C::CommittedReceiptId,
                    qualification: &created
                },
                &mut overlay,
                165,
                &mut std::io::sink(),
                &guard,
                &live
            )
            .unwrap(),
            D::Text { utf8_bytes: 0 }
        );
        let guard = overlay.transaction.guard();
        delete_qualified_runtime_capability_record_v1(
            RuntimeCapabilityDeleteV1 {
                key: old_nonce_key,
                qualification: &old_nonce,
            },
            &mut overlay,
            &guard,
            &live,
        )
        .unwrap();
        let guard = overlay.transaction.guard();
        create_qualified_runtime_capability_record_v1(
            RuntimeCapabilityCreateV1 {
                key: old_nonce_key,
                family: &family,
                source: RuntimeCapabilityRecordRefV1::Nonce(&created_source),
            },
            &mut overlay,
            &mut write,
            &mut work,
            &guard,
            &live,
        )
        .unwrap();
        assert_eq!(work.cells_remaining, 0);
        let guard = overlay.transaction.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.writes.len(), 3);
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
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .unwrap(),
            &state2,
            &values2,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let accepted = accept_runtime_capability_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut receiver,
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap();
        accepted
            .validate_binding(&sealed.record.after.digest().unwrap())
            .unwrap();
        assert_eq!(accepted.record_counts(), [3, 3, 1]);
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
        let mut cold = read_ctx!(
            sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .unwrap(),
            &state3,
            &values3,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let family2 = qualify_runtime_capability_family_v1(
            &sealed.record.after,
            &mut cold,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            165,
        )
        .unwrap();
        let record = open_qualified_runtime_capability_record_v1(
            key,
            &sealed.record.after,
            &family2,
            &mut cold,
        )
        .unwrap()
        .unwrap();
        for (column, want, text) in [
            (C::RemainingUnits, D::Signed(17), ""),
            (C::SubjectAgentId, D::Unit, ""),
            (C::SubjectGeneration, D::Unit, ""),
            (C::SubjectModuleId, D::Text { utf8_bytes: 10 }, "new-module"),
            (
                C::SubjectInstanceId,
                D::Text { utf8_bytes: 12 },
                "new-instance",
            ),
            (C::GrantId, D::Text { utf8_bytes: 17 }, "grant-not-map-key"),
        ] {
            let mut out = Vec::new();
            assert_eq!(
                read_qualified_runtime_capability_cell_v1(
                    RuntimeCapabilityCellRequestV1 {
                        key,
                        column,
                        qualification: &record
                    },
                    &sealed.record.after,
                    &mut cold,
                    165,
                    &mut out
                )
                .unwrap(),
                want
            );
            assert_eq!(out, text.as_bytes());
        }
    }
}

#[test]
fn capability_getter_n_then_noop_n_plus_one_poisons_successful_prefix() {
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
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .unwrap(),
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let family = qualify_runtime_capability_family_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            165,
        )
        .unwrap();

        use RuntimeCapabilityColumnV1 as C;
        use RuntimeCapabilityRecordKindV1 as K;
        let key = RuntimeCapabilityRecordKeyV1 {
            kind: K::Budget,
            id: "agent-key",
        };
        let mut budget =
            open_qualified_runtime_capability_record_v1(key, &roots, &family, &mut ctx)
                .unwrap()
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
        use RuntimeCapabilityValueRefV1 as V;
        let guard = overlay.transaction.guard();
        budget = change_qualified_runtime_capability_cell_v1(
            RuntimeCapabilityPointChangeV1 {
                request: RuntimeCapabilityPointRequestV1 {
                    key,
                    column: C::RemainingUnits,
                    qualification: &budget,
                },
                expected: V::Signed(i64::MIN),
                after: V::Signed(17),
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
            assert_eq!(
                read_qualified_runtime_capability_overlay_v1(
                    RuntimeCapabilityPointRequestV1 {
                        key,
                        column: C::ReservedUnits,
                        qualification: &budget
                    },
                    &mut overlay,
                    8,
                    &mut std::io::sink(),
                    &guard,
                    &live
                )
                .unwrap(),
                RuntimeCapabilityValueV1::Signed(0)
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
            let result = change_qualified_runtime_capability_cell_v1(
                RuntimeCapabilityPointChangeV1 {
                    request: RuntimeCapabilityPointRequestV1 {
                        key,
                        column: C::ReservedUnits,
                        qualification: &budget,
                    },
                    expected: V::Signed(0),
                    after: V::Signed(0),
                    max_current_payload_bytes: 8,
                    max_pending_node_bytes: 4 << 20,
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
        "capability scalar getterN={getter_n} NoOp cancelledN={}",
        getter_n + 1
    );
}
