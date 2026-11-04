use super::*;
use crate::{
    incremental_pending_action_family::{
        PendingActionFamilyWorkV1, accept_pending_actions_transition_v1, qualify_pending_actions_v1,
    },
    incremental_pending_action_original_binding::tests::{Objects, work as original_work},
    incremental_runtime_pending_action_record::tests::scheduler_fixture,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer, io},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, TransactionLimits, TransitionVerifier,
    },
    incremental_state::StateScanBudget,
};
fn scans() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10000000,
        read_bytes_remaining: 512 << 20,
        leaves_remaining: 100000,
    }
}
fn family_work() -> PendingActionFamilyWorkV1 {
    PendingActionFamilyWorkV1 {
        index: PendingActionIndexWorkV1 {
            steps_remaining: 100000,
        },
        typed: TypedRecordReadWorkV1 {
            json: crate::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 100000,
                json_nodes_remaining: 100000,
                max_depth: 32,
                max_payload_bytes: 2 << 20,
            },
            max_retained_trees: 4096,
            max_tree_heap: 2 << 20,
        },
        original: original_work(),
        selected_scan: scans(),
    }
}
fn qualify(
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
    objects: &mut Objects,
) -> ReservedValue<PendingActionQualificationV1> {
    qualify_pending_actions_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Scheduler),
            state: pages,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            semantic_bytes: 16384,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scans(),
        &mut family_work(),
        objects,
    )
    .unwrap()
}
fn typed_work() -> PendingActionWriteWorkV1 {
    PendingActionWriteWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 8 << 20,
        json: crate::incremental_json_records::JsonWriteWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 100000,
            max_json_depth: 32,
            max_pending_node_bytes: 8 << 20,
        },
    }
}
#[test]
fn actual_cursor_shared_transaction_append_pop_mixed_write_seals_and_independent_receiver_accepts()
{
    let (memory, pages, root, bytes) = scheduler_fixture();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let mut roots = RootSetV1::empty("pending-overlay".into(), [0; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Scheduler)
        .unwrap() = root;
    let mut objects = Objects {
        memory: memory.clone(),
        class: MemoryClass::Ordinary,
        bytes: bytes.clone(),
        swallow: false,
    };
    let q = qualify(&roots, &pages, &memory, &mut objects);
    let parent = ExecutionParent {
        commit: [4; 32],
        roots: roots.clone(),
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let limits = TransactionLimits {
        max_reads: 64,
        max_writes: 16,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    };
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [6; 32], &pages, &memory, limits).unwrap();
    let mut index_work = PendingActionIndexWorkV1 {
        steps_remaining: 100000,
    };
    let guard = tx.guard();
    let mut cursor = begin_pending_action_current_v1(
        &q,
        &roots,
        &mut tx,
        &memory,
        &guard,
        &live,
        &mut index_work,
        || false,
    )
    .unwrap();
    // Another family may change the same shared transaction revision.
    tx.write(StateNamespaceV1::Business, b"other-family", Some([7; 32]))
        .unwrap();
    let guard = tx.guard();
    let removed = pop_pending_action_v1(
        &mut cursor,
        &mut tx,
        &memory,
        &guard,
        &live,
        &mut index_work,
        &mut SemanticKeyBudget {
            bytes_remaining: 4096,
        },
        || false,
    )
    .unwrap()
    .unwrap();
    assert_eq!(removed.position.ordinal, 0);
    let envelope = ActionEnvelope {
        governance_authorization: None,
        committed_recipe_origin: None,
        id: 17,
        action: crate::Action::DeployModuleArtifact {
            publisher_agent_id: "publisher".into(),
            wasm_hash: "hash".into(),
            wasm_bytes: vec![0, 255, 0, 255],
        },
    };
    let original = OriginalArtifactRefV1 {
        id: *blake3::hash(&bytes).as_bytes(),
        byte_len: bytes.len() as u64,
    };
    let guard = tx.guard();
    let position = append_pending_action_v1(
        &mut cursor,
        &envelope,
        original,
        &mut tx,
        &mut MemberRecordWriteContext {
            sink: &mut Writer(&pages),
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 16384,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &guard,
        &live,
        &mut index_work,
        &mut typed_work(),
    )
    .unwrap();
    assert_eq!(position, Position { era: 1, ordinal: 0 });
    let guard = tx.guard();
    assert_eq!(
        remove_pending_action_v1(
            &mut cursor,
            position,
            &mut tx,
            &memory,
            &guard,
            &live,
            &mut index_work,
            &mut SemanticKeyBudget {
                bytes_remaining: 4096
            },
            || false
        )
        .unwrap()
        .unwrap()
        .position,
        position
    );
    let guard = tx.guard();
    let second_position = append_pending_action_v1(
        &mut cursor,
        &envelope,
        original,
        &mut tx,
        &mut MemberRecordWriteContext {
            sink: &mut Writer(&pages),
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 16384,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &guard,
        &live,
        &mut index_work,
        &mut typed_work(),
    )
    .unwrap();
    assert_eq!(second_position, Position { era: 1, ordinal: 1 });
    let guard = tx.guard();
    assert!(
        remove_pending_action_v1(
            &mut cursor,
            Position {
                era: 0,
                ordinal: 99
            },
            &mut tx,
            &memory,
            &guard,
            &live,
            &mut index_work,
            &mut SemanticKeyBudget {
                bytes_remaining: 4096
            },
            || false
        )
        .unwrap()
        .is_none()
    );
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    for node in &prepared.new_nodes {
        Writer(&pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let after = &prepared.record.after;
    let accepted = accept_pending_actions_transition_v1(
        &q,
        &verifier,
        &prepared.record,
        &mut WorldFieldReadContextV1 {
            root: after.state_root(StateNamespaceV1::Scheduler),
            state: &pages,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            semantic_bytes: 16384,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut family_work(),
        &mut objects,
    )
    .unwrap();
    assert_eq!(accepted.len(), 1);
    assert_eq!(
        accepted
            .first(&mut index_work, &mut || false)
            .unwrap()
            .unwrap()
            .position
            .ordinal,
        1
    );
}

#[test]
fn cursor_rejects_foreign_transaction_even_same_parent_and_nonce_and_poisons_it() {
    let (memory, pages, root, bytes) = scheduler_fixture();
    let mut roots = RootSetV1::empty("pending-overlay-identity".into(), [0; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Scheduler)
        .unwrap() = root;
    let mut objects = Objects {
        memory: memory.clone(),
        class: MemoryClass::Ordinary,
        bytes,
        swallow: false,
    };
    let q = qualify(&roots, &pages, &memory, &mut objects);
    let parent = ExecutionParent {
        commit: [4; 32],
        roots: roots.clone(),
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let limits = || TransactionLimits {
        max_reads: 64,
        max_writes: 16,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    };
    let mut first =
        ExecutionTransaction::begin(parent.clone(), [6; 32], &pages, &memory, limits()).unwrap();
    let mut second =
        ExecutionTransaction::begin(parent, [6; 32], &pages, &memory, limits()).unwrap();
    let mut work = PendingActionIndexWorkV1 {
        steps_remaining: 100000,
    };
    let guard = first.guard();
    let mut current = begin_pending_action_current_v1(
        &q,
        &roots,
        &mut first,
        &memory,
        &guard,
        &live,
        &mut work,
        || false,
    )
    .unwrap();
    let guard = second.guard();
    assert!(
        pop_pending_action_v1(
            &mut current,
            &mut second,
            &memory,
            &guard,
            &live,
            &mut work,
            &mut SemanticKeyBudget {
                bytes_remaining: 4096
            },
            || false
        )
        .is_err()
    );
    let guard = second.guard();
    assert!(second.seal(&guard, &live, 1).is_err());
    let guard = first.guard();
    assert!(
        pop_pending_action_v1(
            &mut current,
            &mut first,
            &memory,
            &guard,
            &live,
            &mut work,
            &mut SemanticKeyBudget {
                bytes_remaining: 4096
            },
            || true
        )
        .is_err()
    );
    let guard = first.guard();
    assert!(first.seal(&guard, &live, 1).is_err());
}
