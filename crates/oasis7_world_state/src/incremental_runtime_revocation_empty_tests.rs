use super::*;
#[test]
fn safe_alternatives_guard_rejects_same_transaction_changed_body_and_poisons() {
    for changed in [false, true] {
        let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
        let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
        let (pages, roots) = seed(&memory);
        let mut ri = io();
        let mut rk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let proof = qualify_runtime_revocation_v1(
            &roots,
            &mut context(&pages, &roots, &memory, &mut ri, &mut rk),
            &mut scan(),
            &mut read_work(),
        )
        .unwrap();
        assert_eq!(proof.safe_alternatives_len(), 0);
        let parent = ExecutionParent {
            roots,
            commit: [2; 32],
            authority: AuthorityBinding {
                token: [3; 32],
                epoch: 4,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent,
            [4; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 16,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        if changed {
            let mut state = crate::CapabilityRevocationState::default();
            state
                .safe_alternatives
                .insert("actual".into(), serde_json::json!({"value":1}));
            let mut sink = Sink(&pages);
            let mut rows = Vec::with_capacity(2);
            prepare_runtime_revocation_genesis_v1(
                &state,
                &mut MemberRecordWriteContext {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut io(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4 << 20,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 2 << 20,
                        record_bytes: 3 << 20,
                    },
                    cancelled: || false,
                },
                &mut write_work(),
                &mut |row| {
                    rows.push(row);
                    Ok(())
                },
            )
            .unwrap();
            for row in rows {
                tx.write(row.namespace, &row.mutation.key, row.mutation.after)
                    .unwrap();
            }
        }
        let mut oi = io();
        let mut ok = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let guard = tx.guard();
        let result = require_current_safe_alternatives_empty_v1(
            &proof,
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut oi,
                },
                key_work: &mut ok,
                semantic_bytes: 1024,
                record_bytes: 3 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
            &guard,
            &live,
        );
        if changed {
            assert!(matches!(
                result,
                Err(
                    crate::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(
                        ValueTreeError::Invalid("unsupported current safe alternatives")
                    )
                )
            ));
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
        } else {
            result.unwrap();
            let guard = tx.guard();
            tx.seal(&guard, &live, 1).unwrap();
        }
    }
}
