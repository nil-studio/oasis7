use super::*;
use crate::{
    incremental_cognition_fields::CognitionEnvelopeRefV1,
    incremental_cognition_genesis::prepare_cognition_state_genesis_v1,
    incremental_field_member::{MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{JsonChangeWorkV1, JsonReadWorkV1, JsonWriteWorkV1},
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer, io},
    incremental_verified_world_transition::verify_world_transition_v1,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, ExecutionWriteV1, RootSetV1,
        TransactionLimits, TransitionVerifier,
    },
    incremental_state::{
        StateDeltaV1, StateNamespaceV1, StateRootV1, StateScanBudget, StateWriteV1,
    },
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

fn work() -> CognitionCurrentCompleteWorkV3 {
    CognitionCurrentCompleteWorkV3 {
        scan: StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 100_000,
        },
        json: JsonChangeWorkV1 {
            json: JsonReadWorkV1 {
                cells_remaining: 100_000,
                json_nodes_remaining: 100_000,
                max_depth: 64,
                max_payload_bytes: 8 << 20,
            },
            paths_remaining: 100_000,
            max_pending_pairs: 100_000,
        },
        outer_cells_remaining: 100_000,
    }
}
fn source() -> serde_json::Value {
    serde_json::json!({"schema_version":"cognition-persistence.v1", "cognition_journal":{"schema_version":"cognition-journal.v1","head_seq":0,"head_digest":"","events":[]},"responses":[],"commit_records":[],"receipt_registry":[],"receipt_lineage_registry":[],"feedback_outbox":{},"idempotency_index":{},"staged_actions":{},"scheduler_state":{},"continuations":[],"recovery":null,"reorg_invalidation_epoch":0})
}
fn rows(memory: &MemoryBudget, pages: &Pages) -> Vec<ReservedValue<ExecutionWriteV1>> {
    let value = source();
    let _admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let mut rows = Vec::with_capacity(128);
    let mut writer = Writer(pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    prepare_cognition_state_genesis_v1(
        CognitionEnvelopeRefV1::new(&value).unwrap(),
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut quota,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 8 << 20,
                record_bytes: 16 << 20,
            },
            cancelled: || false,
        },
        &mut 100_000,
        &mut JsonWriteWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 100_000,
            max_json_depth: 64,
            max_pending_node_bytes: 8 << 20,
        },
        &mut |row| {
            assert!(rows.len() < 128);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    rows
}
fn persist(
    memory: &MemoryBudget,
    pages: &Pages,
    nodes: &[oasis7_formal_protocol::incremental_state::StateNodeWriteV1],
) {
    for node in nodes {
        Writer(pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
}
fn install(
    memory: &MemoryBudget,
    pages: &Pages,
    rows: &[ReservedValue<ExecutionWriteV1>],
) -> RootSetV1 {
    let mut roots = RootSetV1::empty("cognition-presence-transitions".into(), [71; 32]).unwrap();
    for root in &mut roots.state {
        let mut writes: Vec<StateWriteV1> = rows
            .iter()
            .filter(|row| row.namespace == root.namespace)
            .map(|row| row.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let delta = StateDeltaV1::build(
            StateRootV1::empty(root.namespace).unwrap(),
            writes,
            pages,
            8 << 20,
        )
        .unwrap();
        persist(memory, pages, &delta.new_nodes);
        *root = delta.after;
    }
    roots
}

#[test]
fn complete_cognition_four_real_presence_transitions_and_false_selectors() {
    for (before_enabled, after_enabled) in
        [(false, false), (false, true), (true, false), (true, true)]
    {
        let memory = MemoryBudget::new(256 << 20, 0).unwrap();
        let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
        let pages = Pages::with_limit(&memory, 32768);
        let all = rows(&memory, &pages);
        let before_rows = if before_enabled { all.as_slice() } else { &[] };
        let after_rows = if after_enabled { all.as_slice() } else { &[] };
        let roots = install(&memory, &pages, before_rows);
        let presence = |enabled| {
            if enabled {
                CognitionCurrentPresenceV3::Enabled
            } else {
                CognitionCurrentPresenceV3::Disabled
            }
        };
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Allocator),
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 16 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        assert!(
            qualify_complete_cognition_current_v3(
                &roots,
                presence(!before_enabled),
                &mut ctx,
                &mut work()
            )
            .is_err(),
            "selector cannot mint a proof"
        );
        let parent_q = qualify_complete_cognition_current_v3(
            &roots,
            presence(before_enabled),
            &mut ctx,
            &mut work(),
        )
        .unwrap();
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [31; 32],
            authority: AuthorityBinding {
                token: [32; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [33; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 256,
                max_writes: 256,
                max_facts: 8,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        if before_enabled != after_enabled {
            for row in if after_enabled {
                after_rows
            } else {
                before_rows
            } {
                tx.write(
                    row.namespace,
                    &row.mutation.key,
                    if after_enabled {
                        row.mutation.after
                    } else {
                        None
                    },
                )
                .unwrap();
            }
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        persist(&memory, &pages, &sealed.new_nodes);
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [33; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: sealed.record.after.state_root(StateNamespaceV1::Allocator),
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 16 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let witness = verify_world_transition_v1(&verifier, &sealed.record, &mut ctx).unwrap();
        if before_enabled != after_enabled {
            assert!(
                accept_complete_cognition_current_verified_v3(
                    &parent_q,
                    &witness,
                    &mut ctx,
                    presence(before_enabled),
                    &mut work()
                )
                .is_err(),
                "actual writes override a false after selector"
            );
        }
        let next = accept_complete_cognition_current_verified_v3(
            &parent_q,
            &witness,
            &mut ctx,
            presence(after_enabled),
            &mut work(),
        )
        .unwrap();
        assert_eq!(next.presence(), presence(after_enabled));
        next.validate_binding(&sealed.record.after).unwrap();
        assert_eq!(next.allocation_class(), MemoryClass::Ordinary);
    }
}
