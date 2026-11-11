use super::*;
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_world_native::{
    prepare_world_native_metadata_v1, prepare_world_native_record_v1,
};
use family::WorldNativeFieldReadWorkV1;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, ExecutionWriteV1, TransactionLimits,
};
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueReadContext};
#[path = "incremental_world_native_family_test_support.rs"]
mod support;
use support::*;
fn index_work() -> StarterBindingIndexWorkV1 {
    StarterBindingIndexWorkV1 {
        max_rows: 64,
        max_row_text_bytes: 4096,
        max_pending_rows: 32,
        steps_remaining: 100000,
        bytes_remaining: 32 << 20,
        comparison_bytes_remaining: 32 << 20,
        moved_slots_remaining: 100000,
        pending_entries_remaining: 100000,
    }
}
fn claim(player: &str, key: Option<&str>, agent: &str) -> crate::StarterOcClaimState {
    crate::StarterOcClaimState {
        player_id: player.into(),
        public_key: key.map(str::to_owned),
        agent_id: agent.into(),
        amount: 0,
        claimed_at: 0,
        source_treasury_bucket_id: None,
    }
}
fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots: roots.clone(),
        commit: [31; 32],
        authority: AuthorityBinding {
            token: [32; 32],
            epoch: 0,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 256,
        max_writes: 64,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    }
}
fn build_index(
    memory: &MemoryBudget,
    pages: &Pages,
    roots: &RootSetV1,
) -> ReservedValue<StarterBindingIndexV1> {
    let mut io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = context(
        memory,
        pages,
        roots.state_root(StateNamespaceV1::Authorization),
        &mut io,
        &mut keys,
    );
    let mut native = work();
    let field =
        family::qualify_world_native_field_v1::<Field>(roots, &mut ctx, &mut scan(), &mut native)
            .unwrap();
    build_starter_binding_index_v1(
        &field,
        roots,
        &mut ctx,
        &mut scan(),
        &mut native,
        &mut index_work(),
    )
    .unwrap()
}
fn observe(
    memory: &MemoryBudget,
    pages: &Pages,
    roots: &RootSetV1,
    index: &ReservedValue<StarterBindingIndexV1>,
    tx: &mut ExecutionTransaction<'_, Pages>,
    player: &str,
    key: &str,
    work: &mut StarterBindingIndexWorkV1,
) -> ReservedValue<StarterBindingObservationV1> {
    let mut io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = context(
        memory,
        pages,
        roots.state_root(StateNamespaceV1::Authorization),
        &mut io,
        &mut keys,
    );
    let guard = tx.guard();
    let live = tx.parent_fence();
    observe_current_starter_binding_v1(
        player,
        key,
        index,
        roots,
        tx,
        &mut ctx,
        &mut scan(),
        &mut support::work(),
        work,
        &guard,
        &live,
    )
    .unwrap()
}
#[test]
fn real_starter_binding_index_keeps_duplicates_none_empty_and_current_outer_delta() {
    let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let initial = rows::<Field>(
        &memory,
        &pages,
        &[
            ("outer-a".into(), claim("player", Some("key"), "alice")),
            ("outer-b".into(), claim("player", Some("key"), "bob")),
            ("no-key".into(), claim("player", None, "none")),
            ("empty-key".into(), claim("player", Some(""), "empty")),
        ],
    );
    let roots = install(&memory, &pages, StateNamespaceV1::Authorization, &initial);
    let index = build_index(&memory, &pages, &roots);
    assert_eq!(index.records(), 4);
    let mut tx =
        ExecutionTransaction::begin(parent(&roots), [33; 32], &pages, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let mut work = index_work();
    let duplicate = observe(
        &memory, &pages, &roots, &index, &mut tx, "player", "key", &mut work,
    );
    assert_eq!(duplicate.matching_claims(), 2);
    assert_eq!(duplicate.unique_agent_id(), None);
    duplicate.validate(&mut tx, &memory, &live).unwrap();
    let empty = observe(
        &memory, &pages, &roots, &index, &mut tx, "player", "", &mut work,
    );
    assert_eq!(empty.matching_claims(), 1);
    assert_eq!(empty.unique_agent_id(), Some("empty"));
    assert_eq!(
        observe(
            &memory, &pages, &roots, &index, &mut tx, "absent", "key", &mut work
        )
        .matching_claims(),
        0
    );
    let key_b = &index
        .by_outer
        .iter()
        .find(|row| row.outer == "outer-b")
        .unwrap()
        .key;
    tx.write(StateNamespaceV1::Authorization, key_b.as_bytes(), None)
        .unwrap();
    let unique = observe(
        &memory, &pages, &roots, &index, &mut tx, "player", "key", &mut work,
    );
    assert_eq!(unique.matching_claims(), 1);
    assert_eq!(unique.unique_agent_id(), Some("alice"));
    let changed = rows::<Field>(
        &memory,
        &pages,
        &[
            (
                "outer-a".into(),
                claim("other-player", Some("key"), "changed"),
            ),
            ("new-outer".into(), claim("player", Some("key"), "new")),
        ],
    );
    for row in &changed {
        if WorldFieldKeyV1::from_bytes(&row.mutation.key).unwrap().kind
            == WorldFieldRecordKindV1::Entry
        {
            tx.write(row.namespace, &row.mutation.key, row.mutation.after)
                .unwrap();
        }
    }
    let current = observe(
        &memory, &pages, &roots, &index, &mut tx, "player", "key", &mut work,
    );
    assert_eq!(current.matching_claims(), 1);
    assert_eq!(current.unique_agent_id(), Some("new"));
    assert_eq!(
        observe(
            &memory,
            &pages,
            &roots,
            &index,
            &mut tx,
            "other-player",
            "key",
            &mut work
        )
        .unique_agent_id(),
        Some("changed")
    );
    let original = index
        .by_outer
        .iter()
        .find(|row| row.outer == "outer-a")
        .unwrap();
    tx.write(
        StateNamespaceV1::Authorization,
        original.key.as_bytes(),
        None,
    )
    .unwrap();
    tx.write(
        StateNamespaceV1::Authorization,
        original.key.as_bytes(),
        Some(original.member),
    )
    .unwrap();
    assert_eq!(
        observe(
            &memory, &pages, &roots, &index, &mut tx, "player", "key", &mut work
        )
        .matching_claims(),
        2
    );
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert!(sealed.record.facts.is_empty());
}
#[test]
fn starter_binding_observation_is_exact_revision_and_errors_poison() {
    let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let initial = rows::<Field>(
        &memory,
        &pages,
        &[("not-player".into(), claim("player", Some("key"), "alice"))],
    );
    let roots = install(&memory, &pages, StateNamespaceV1::Authorization, &initial);
    let index = build_index(&memory, &pages, &roots);
    let mut tx =
        ExecutionTransaction::begin(parent(&roots), [34; 32], &pages, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let mut work = index_work();
    let observation = observe(
        &memory, &pages, &roots, &index, &mut tx, "player", "key", &mut work,
    );
    tx.write(StateNamespaceV1::Scheduler, b"later", Some([9; 32]))
        .unwrap();
    assert!(observation.validate(&mut tx, &memory, &live).is_err());
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
    for case in 0..4 {
        let mut tx =
            ExecutionTransaction::begin(parent(&roots), [35; 32], &pages, &memory, limits())
                .unwrap();
        let live = tx.parent_fence();
        let guard = tx.guard();
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut ctx = context(
            &memory,
            &pages,
            roots.state_root(StateNamespaceV1::Authorization),
            &mut io,
            &mut keys,
        );
        let mut work = index_work();
        if case == 0 {
            work.steps_remaining = 0;
        }
        let mut foreign = roots.clone();
        foreign.height = 1;
        let result = if case == 1 {
            observe_current_starter_binding_v1(
                "player",
                "key",
                &index,
                &foreign,
                &mut tx,
                &mut ctx,
                &mut scan(),
                &mut support::work(),
                &mut work,
                &guard,
                &live,
            )
        } else if case == 2 {
            let row = &index.by_outer[0];
            tx.write(
                StateNamespaceV1::Authorization,
                row.key.as_bytes(),
                Some([99; 32]),
            )
            .unwrap();
            let guard = tx.guard();
            observe_current_starter_binding_v1(
                "player",
                "key",
                &index,
                &roots,
                &mut tx,
                &mut ctx,
                &mut scan(),
                &mut support::work(),
                &mut work,
                &guard,
                &live,
            )
        } else if case == 3 {
            tx.write(StateNamespaceV1::Scheduler, b"revision", Some([9; 32]))
                .unwrap();
            observe_current_starter_binding_v1(
                "player",
                "key",
                &index,
                &roots,
                &mut tx,
                &mut ctx,
                &mut scan(),
                &mut support::work(),
                &mut work,
                &guard,
                &live,
            )
        } else {
            observe_current_starter_binding_v1(
                "player",
                "key",
                &index,
                &roots,
                &mut tx,
                &mut ctx,
                &mut scan(),
                &mut support::work(),
                &mut work,
                &guard,
                &live,
            )
        };
        assert!(result.is_err());
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
    }
}
#[test]
fn verified_starter_binding_successor_uses_changed_outer_rows() {
    let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let before = rows::<Field>(
        &memory,
        &pages,
        &[
            ("outer-a".into(), claim("player", Some("key"), "alice")),
            ("outer-b".into(), claim("player", Some("key"), "bob")),
        ],
    );
    let roots = install(&memory, &pages, StateNamespaceV1::Authorization, &before);
    let index = build_index(&memory, &pages, &roots);
    let mut io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let field = {
        let mut ctx = context(
            &memory,
            &pages,
            roots.state_root(StateNamespaceV1::Authorization),
            &mut io,
            &mut keys,
        );
        family::qualify_world_native_field_v1::<Field>(&roots, &mut ctx, &mut scan(), &mut work())
            .unwrap()
    };
    let after = rows::<Field>(
        &memory,
        &pages,
        &[
            ("outer-a".into(), claim("player", Some("key"), "changed")),
            ("outer-c".into(), claim("other", None, "none")),
        ],
    );
    let p = parent(&roots);
    let mut tx =
        ExecutionTransaction::begin(p.clone(), [36; 32], &pages, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    for row in &after {
        if !before.iter().any(|old| {
            old.mutation.key == row.mutation.key && old.mutation.after == row.mutation.after
        }) {
            tx.write(row.namespace, &row.mutation.key, row.mutation.after)
                .unwrap();
        }
    }
    for row in &before {
        if !after.iter().any(|new| new.mutation.key == row.mutation.key) {
            tx.write(row.namespace, &row.mutation.key, None).unwrap();
        }
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert!(sealed.record.facts.is_empty());
    for node in &sealed.new_nodes {
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
        parent: &p,
        operation: [36; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let mut after_ctx = context(
        &memory,
        &pages,
        sealed
            .record
            .after
            .state_root(StateNamespaceV1::Authorization),
        &mut io,
        &mut keys,
    );
    let verified =
        family::verify_world_native_transition_v1(&verifier, &sealed.record, &mut after_ctx)
            .unwrap();
    let after_field = family::accept_world_native_field_verified_v1::<Field>(
        &field,
        &verified,
        &mut after_ctx,
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    let inherited = accept_starter_binding_index_verified_v1(
        &index,
        &after_field,
        &verified,
        &mut after_ctx,
        &mut scan(),
        &mut work(),
        &mut index_work(),
    )
    .unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent(&sealed.record.after),
        [37; 32],
        &pages,
        &memory,
        limits(),
    )
    .unwrap();
    let mut work = index_work();
    let observed = observe(
        &memory,
        &pages,
        &sealed.record.after,
        &inherited,
        &mut tx,
        "player",
        "key",
        &mut work,
    );
    assert_eq!(observed.matching_claims(), 1);
    assert_eq!(observed.unique_agent_id(), Some("changed"));
    assert!(
        inherited
            .validate(&roots, &memory, MemoryClass::Ordinary)
            .is_err()
    );
}

#[path = "incremental_world_native_starter_binding_index_safety_tests.rs"]
mod safety;
