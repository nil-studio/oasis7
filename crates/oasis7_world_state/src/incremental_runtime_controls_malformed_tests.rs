use super::*;
use oasis7_formal_protocol::incremental_execution::*;

#[test]
fn generic_valid_control_deletions_wrong_types_keys_and_namespaces_are_rejected() {
    for mode in 0..5 {
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let (mut pages, roots) = seed(&memory, "authority", None);
        let mut reads = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let qualification = qualify_runtime_controls_v1(
            &roots,
            &mut WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &pages,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
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
        let field = RuntimeControlFieldV1::NextGovernanceIdentityPenaltyId;
        let metadata = mode == 0;
        let kind = if metadata {
            WorldFieldRecordKindV1::Metadata
        } else {
            WorldFieldRecordKindV1::Entry
        };
        let mut namespace = field.namespace();
        let key;
        let after;
        if metadata {
            key = WorldFieldKeyV1::new(field.descriptor(), kind, None)
                .unwrap()
                .as_bytes()
                .to_vec();
            after = None;
        } else {
            let mut writes = io();
            let mut key_work = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut writes,
                key_work: &mut key_work,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 1 << 20,
                    record_bytes: 2 << 20,
                },
                cancelled: || false,
            };
            let row = if mode == 3 {
                write_member_record_v1(
                    field.descriptor(),
                    kind,
                    &7u64,
                    MemberPayloadRefV1::U64(7),
                    &mut write,
                )
                .unwrap()
            } else {
                write_member_record_v1(
                    field.descriptor(),
                    kind,
                    &(),
                    if mode == 2 {
                        MemberPayloadRefV1::Text("wrong type")
                    } else {
                        MemberPayloadRefV1::U64(7)
                    },
                    &mut write,
                )
                .unwrap()
            };
            key = row.field_key.as_bytes().to_vec();
            after = if mode == 1 {
                None
            } else {
                Some(row.value_root)
            };
            if mode == 4 {
                namespace = StateNamespaceV1::Business;
            }
        }
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
            parent.clone(),
            [6; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 16,
                max_writes: 8,
                max_facts: 8,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        tx.write(namespace, &key, after).unwrap();
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.writes.len(), 1);
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        // These are genuinely valid changed Merkle roots, not fabricated bad hashes.
        verifier.verify(&sealed.record).unwrap();
        let mut reads = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let result = accept_runtime_controls_transition_v1(
            &qualification,
            &verifier,
            &sealed.record,
            &mut WorldFieldReadContextV1 {
                root: &sealed.record.after.state[0],
                state: &pages,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
                },
                key_work: &mut keys,
                semantic_bytes: 1024,
                record_bytes: 2 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
            1024,
        );
        assert!(result.is_err(), "invalid control profile mode {mode}");
    }
}
