//! Child of native qualification tests, reusing its real all-field GEN helper.
use super::*;
#[test]
fn current_claims_select_same_transaction_insertions_and_original_missing_records() {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 131072);
    let before = crate::WorldState::default();
    let mut before_rows = native_rows(&memory, &pages, &before);
    let roots = install_native(&memory, &pages, &mut before_rows);
    let mut qi = io();
    let mut qk = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let parent = qualify_world_native_fields_v1(
        &roots,
        &mut context(
            &memory,
            &pages,
            roots.state_root(StateNamespaceV1::Authorization),
            &mut qi,
            &mut qk,
        ),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    let mut after = crate::WorldState::default();
    crate::incremental_world_native_gameplay::fixture::populate_world(&mut after);
    let expected_starter = after.starter_oc_claims["outer-starter"].clone();
    let (claim_id, expected_claim) = after
        .agent_claims
        .iter()
        .next()
        .map(|(id, value)| (id.clone(), value.clone()))
        .unwrap();
    let after_rows = native_rows(&memory, &pages, &after);
    drop(before);
    drop(after);
    let execution_parent = ExecutionParent {
        roots: roots.clone(),
        commit: [3; 32],
        authority: AuthorityBinding {
            token: [4; 32],
            epoch: 1,
        },
    };
    let live = execution_parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        execution_parent,
        [5; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 256,
            max_writes: 256,
            max_facts: 8,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    for row in after_rows {
        tx.write(row.namespace, &row.mutation.key, row.mutation.after)
            .unwrap();
    }
    let mut ri = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut read = context(
        &memory,
        &pages,
        roots.state_root(StateNamespaceV1::Authorization),
        &mut ri,
        &mut rk,
    );
    let mut selected_scan = scan();
    let mut selected_work = work().typed;
    let guard = tx.guard();
    let starter = super::super::recover_current_starter_oc_claim_v1(
        "outer-starter",
        &parent,
        &roots,
        &mut tx,
        &mut read,
        &mut selected_scan,
        &mut selected_work,
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    assert_eq!(starter.value().value(), &expected_starter);
    read.root = roots.state_root(StateNamespaceV1::Business);
    let guard = tx.guard();
    let claim = super::super::recover_current_agent_claim_v1(
        &claim_id,
        &parent,
        &roots,
        &mut tx,
        &mut read,
        &mut selected_scan,
        &mut selected_work,
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    assert_eq!(claim.value().value(), &expected_claim);
    read.root = roots.state_root(StateNamespaceV1::Authorization);
    let guard = tx.guard();
    assert!(
        super::super::recover_current_starter_oc_claim_v1(
            "missing",
            &parent,
            &roots,
            &mut tx,
            &mut read,
            &mut selected_scan,
            &mut selected_work,
            &guard,
            &live
        )
        .unwrap()
        .is_none()
    );
    let guard = tx.guard();
    tx.seal(&guard, &live, 1).unwrap();
}
