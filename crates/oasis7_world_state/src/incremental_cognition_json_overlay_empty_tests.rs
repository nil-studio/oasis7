use super::*;
use crate::incremental_field_presence::*;
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_typed_record_test_support::{Pages, Writer, io};
use oasis7_formal_protocol::{incremental_execution::*, incremental_value::*};
use oasis7_proto::memory_budget::MemoryBudget;
#[test]
fn delegation_guard_reads_same_transaction_empty_then_nonempty_and_poisons() {
    for value in [
        serde_json::json!({}),
        serde_json::json!({"grant":{}}),
        serde_json::json!([]),
    ] {
        let memory = MemoryBudget::new(128 << 20, 0).unwrap();
        let _fee = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
        let pages = Pages::with_limit(&memory, 65536);
        let mut writer = Writer(&pages);
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let roots = RootSetV1::empty("delegation-current".into(), [1; 32]).unwrap();
        let parent = ExecutionParent {
            roots,
            commit: [2; 32],
            authority: AuthorityBinding {
                token: [3; 32],
                epoch: 1,
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
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        let mut readio = io();
        let mut readkeys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut work = JsonReadWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 100000,
            max_depth: 64,
            max_payload_bytes: 1 << 20,
        };
        let mut check = |tx: &mut ExecutionTransaction<'_, Pages>| {
            let guard = tx.guard();
            require_current_delegation_grants_empty_v1(
                &mut WorldFieldOverlayContextV1 {
                    transaction: tx,
                    values: ValueReadContext {
                        source: &pages,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut readio,
                    },
                    key_work: &mut readkeys,
                    semantic_bytes: 4096,
                    record_bytes: 2 << 20,
                    take_state_error: || None,
                    cancelled: || false,
                },
                &mut work,
                &guard,
                &live,
            )
        };
        check(&mut tx).unwrap();
        let field = CognitionJsonFieldV1::DelegationGrants;
        let metadata = write_field_metadata_v1(
            field.descriptor(),
            &mut PresenceRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut quota,
                key_work: &mut keys,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: 4096,
                    record_bytes: 2 << 20,
                },
                cancelled: || false,
            },
        )
        .unwrap();
        let mut context = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut quota,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        };
        let child = crate::incremental_json_records::prepare_json_subtree_v1(
            field.profile(),
            &value,
            &mut context,
            &mut JsonWriteWorkV1 {
                cells_remaining: 100000,
                json_nodes_remaining: 100000,
                max_json_depth: 64,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        let namespace = child.namespace;
        let entry =
            crate::incremental_cognition_json::write_root(field, child, &mut context).unwrap();

        tx.write(
            namespace,
            metadata.field_key.as_bytes(),
            Some(metadata.value_root),
        )
        .unwrap();
        tx.write(
            namespace,
            entry.field_key.as_bytes(),
            Some(entry.value_root),
        )
        .unwrap();
        let result = check(&mut tx);
        if value.is_object() && value.as_object().unwrap().is_empty() {
            result.unwrap();
            let guard = tx.guard();
            tx.seal(&guard, &live, 1).unwrap();
        } else {
            let expected = if value.is_object() {
                "nonempty delegation grants unsupported"
            } else {
                "JSON immediate root object"
            };
            assert!(
                matches!(&result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(reason))) if *reason == expected
                ),
                "delegation fixture {value:?}: {result:?}"
            );
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
        }
    }
}
