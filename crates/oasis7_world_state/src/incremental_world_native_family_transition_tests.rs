use super::*;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
};
#[test]
fn native_field_ordinary_receiver_updates_inserts_deletes_and_rejects_wrong_parent() {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 65536);
    let before_rows = rows::<MetaProgressFieldV1>(
        &memory,
        &pages,
        &[
            ("outer-a".into(), meta("inner-a")),
            ("outer-b".into(), meta("inner-b")),
        ],
    );
    let roots = install(&memory, &pages, StateNamespaceV1::Business, &before_rows);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let proof = qualify_world_native_field_v1::<MetaProgressFieldV1>(
        &roots,
        &mut context(
            &memory,
            &pages,
            roots.state_root(StateNamespaceV1::Business),
            &mut quota,
            &mut keys,
        ),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    let mut changed = meta("inner-a");
    changed.total_points = -42;
    let after_rows = rows::<MetaProgressFieldV1>(
        &memory,
        &pages,
        &[
            ("outer-a".into(), changed),
            ("outer-c".into(), meta("inner-c")),
        ],
    );
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [1; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let mut transaction = ExecutionTransaction::begin(
        parent.clone(),
        [3; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    for row in &after_rows {
        if before_rows
            .iter()
            .find(|old| old.mutation.key == row.mutation.key)
            .and_then(|old| old.mutation.after)
            != row.mutation.after
        {
            transaction
                .write(row.namespace, &row.mutation.key, row.mutation.after)
                .unwrap();
        }
    }
    for row in &before_rows {
        if !after_rows
            .iter()
            .any(|new| new.mutation.key == row.mutation.key)
        {
            transaction
                .write(row.namespace, &row.mutation.key, None)
                .unwrap();
        }
    }
    let guard = transaction.guard();
    let prepared = transaction.seal(&guard, &live, 1).unwrap();
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
        operation: [3; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let mut after = context(
        &memory,
        &pages,
        prepared.record.after.state_root(StateNamespaceV1::Business),
        &mut quota,
        &mut keys,
    );
    let inherited = accept_world_native_field_transition_v1::<MetaProgressFieldV1>(
        &proof,
        &verifier,
        &prepared.record,
        &mut after,
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(inherited.records(), 2);
    inherited.validate_binding(&prepared.record.after).unwrap();
    assert!(inherited.validate_binding(&roots).is_err());
    assert!(
        accept_world_native_field_transition_v1::<MetaProgressFieldV1>(
            &inherited,
            &verifier,
            &prepared.record,
            &mut after,
            &mut scan(),
            &mut work()
        )
        .is_err()
    );
    assert!(
        readmit_world_native_field_ordinary_v1(&proof, &roots, &memory, || false).is_err(),
        "Ordinary proof must not masquerade as Recovery metadata"
    );
}
