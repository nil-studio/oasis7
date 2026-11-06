use super::*;
#[path = "incremental_world_native_complete_test_support.rs"]
mod support;
use crate::incremental_semantic_key::SemanticKeyBudget;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
};
use std::cell::Cell;
use support::*;
struct Counted<'a> {
    pages: &'a Pages,
    reads: Cell<usize>,
    forbidden: Cell<bool>,
}
impl MerkleNodeReader for Counted<'_> {
    type Node = <Pages as MerkleNodeReader>::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        MerkleNodeReader::allocation_budget(self.pages)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        assert!(!self.forbidden.get(), "second generic replay");
        self.reads.set(self.reads.get() + 1);
        MerkleNodeReader::read_node(self.pages, id, max)
    }
}
#[test]
fn complete97_real_genesis_shared_scalar_delta_recovery_and_explicit_admission() {
    let memory = MemoryBudget::new(768 << 20, 64 << 20).unwrap();
    let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 131072);
    let mut source = crate::WorldState::default();
    source.world_rules = Some(crate::WorldRulesStateV1::default());
    let before_rows = rows(&memory, &pages, &source);
    let roots = install(&memory, &pages, &before_rows);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 128 << 20,
    };
    let mut outer = scan();
    let mut inner = scan();
    let mut budget = work();
    let proof = qualify_world_native_complete_v1(
        &roots,
        &mut context(
            &memory,
            &pages,
            roots.state_root(StateNamespaceV1::Business),
            &mut quota,
            &mut keys,
            MemoryClass::Ordinary,
        ),
        &mut outer,
        &mut inner,
        &mut budget,
    )
    .unwrap();
    assert_eq!(proof.field_count(), 97);
    assert_eq!(proof.family_count(), 17);
    proof.validate_binding(&roots).unwrap();
    assert!(outer.nodes_remaining < 100_000_000);
    assert!(budget.legacy_cells_remaining < 1_000_000);
    source
        .product_validation_delivery_cursor
        .routed_through_event_id = 3;
    let after_rows = rows(&memory, &pages, &source);
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
            max_reads: 64,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let mut changed = 0;
    for row in &after_rows {
        if before_rows
            .iter()
            .find(|old| old.namespace == row.namespace && old.mutation.key == row.mutation.key)
            .and_then(|old| old.mutation.after)
            != row.mutation.after
        {
            tx.write(row.namespace, &row.mutation.key, row.mutation.after)
                .unwrap();
            changed += 1;
        }
    }
    assert_eq!(changed, 1, "one original cursor scalar record changed");
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    persist(&memory, &pages, &sealed.new_nodes);
    let counted = Counted {
        pages: &pages,
        reads: Cell::new(0),
        forbidden: Cell::new(false),
    };
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [33; 32],
        source: &counted,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let mut ctx = context(
        &memory,
        &pages,
        sealed.record.after.state_root(StateNamespaceV1::Business),
        &mut quota,
        &mut keys,
        MemoryClass::Ordinary,
    );
    let witness = verify_world_transition_v1(&verifier, &sealed.record, &mut ctx).unwrap();
    assert!(counted.reads.get() > 0);
    let replay_reads = counted.reads.get();
    counted.forbidden.set(true);
    let cells = budget.controls.records_remaining;
    let next = accept_world_native_complete_verified_v1(
        &proof,
        &witness,
        &mut ctx,
        &mut inner,
        &mut budget,
    )
    .unwrap();
    next.validate_binding(&sealed.record.after).unwrap();
    assert!(budget.controls.records_remaining < cells);
    assert_eq!(counted.reads.get(), replay_reads);
    drop(ctx);
    let recovery_pages = ClassedPages::new(&pages, &memory, MemoryClass::Recovery);
    let recovered = qualify_world_native_complete_v1(
        &sealed.record.after,
        &mut context(
            &memory,
            &recovery_pages,
            sealed.record.after.state_root(StateNamespaceV1::Business),
            &mut quota,
            &mut keys,
            MemoryClass::Recovery,
        ),
        &mut outer,
        &mut inner,
        &mut budget,
    )
    .unwrap();
    assert_eq!(recovered.allocation_class(), MemoryClass::Recovery);
    // Exact same before roots eliminate root mismatch as an alternative cause.
    // A Recovery conjunction cannot enter the public Ordinary receiver even
    // for a real generic no-op, and must perform no extra generic source read.
    let after_parent = ExecutionParent {
        roots: sealed.record.after.clone(),
        commit: [41; 32],
        authority: AuthorityBinding {
            token: [42; 32],
            epoch: 8,
        },
    };
    let noop_live = after_parent.fence().unwrap();
    let noop_tx = ExecutionTransaction::begin(
        after_parent.clone(),
        [43; 32],
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
    let noop_guard = noop_tx.guard();
    let noop = noop_tx.seal(&noop_guard, &noop_live, 2).unwrap();
    let noop_verifier = TransitionVerifier {
        parent: &after_parent,
        operation: [43; 32],
        source: &counted,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let denied = accept_world_native_complete_transition_v1(
        &recovered,
        &noop_verifier,
        &noop.record,
        &mut context(
            &memory,
            &pages,
            noop.record.after.state_root(StateNamespaceV1::Business),
            &mut quota,
            &mut keys,
            MemoryClass::Ordinary,
        ),
        &mut inner,
        &mut budget,
    );
    assert!(matches!(
        denied,
        Err(WorldNativeCompleteErrorV1::Value(ValueTreeError::Invalid(
            _
        )))
    ));
    assert_eq!(counted.reads.get(), replay_reads);
    let used = memory.used();
    let admitted = readmit_world_native_complete_ordinary_v1(
        &recovered,
        &sealed.record.after,
        &mut context(
            &memory,
            &pages,
            sealed.record.after.state_root(StateNamespaceV1::Business),
            &mut quota,
            &mut keys,
            MemoryClass::Ordinary,
        ),
        &mut budget,
    )
    .unwrap();
    assert_eq!(admitted.allocation_class(), MemoryClass::Ordinary);
    admitted.validate_binding(&sealed.record.after).unwrap();
    assert!(memory.used() > used);
    drop(admitted);
    assert_eq!(memory.used(), used);
    assert_eq!(counted.reads.get(), replay_reads);
}
#[test]
fn complete97_none_family_still_requires_metadata_and_finite_work() {
    let memory = MemoryBudget::new(768 << 20, 64 << 20).unwrap();
    let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 131072);
    let mut source = crate::WorldState::default();
    source.world_rules = None;
    let mut rows = rows(&memory, &pages, &source);
    let roots = install(&memory, &pages, &rows);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 128 << 20,
    };
    let mut outer = scan();
    let mut inner = scan();
    let mut budget = work();
    let proof = qualify_world_native_complete_v1(
        &roots,
        &mut context(
            &memory,
            &pages,
            roots.state_root(StateNamespaceV1::Business),
            &mut quota,
            &mut keys,
            MemoryClass::Ordinary,
        ),
        &mut outer,
        &mut inner,
        &mut budget,
    )
    .unwrap();
    assert!(!proof.rules.rules_present());
    let mut exhausted = work();
    exhausted.primitive.records_remaining = 0;
    let used = memory.used();
    assert!(
        qualify_world_native_complete_v1(
            &roots,
            &mut context(
                &memory,
                &pages,
                roots.state_root(StateNamespaceV1::Business),
                &mut quota,
                &mut keys,
                MemoryClass::Ordinary
            ),
            &mut outer,
            &mut inner,
            &mut exhausted
        )
        .is_err()
    );
    assert_eq!(memory.used(), used);
    rows.retain(|row| {
        let key =
            crate::incremental_world_field_key::WorldFieldKeyV1::from_bytes(&row.mutation.key)
                .unwrap();
        !(key.name == "world_rules"
            && key.kind == crate::incremental_world_field_key::WorldFieldRecordKindV1::Metadata)
    });
    let missing = install(&memory, &pages, &rows);
    let used = memory.used();
    let rejected = qualify_world_native_complete_v1(
        &missing,
        &mut context(
            &memory,
            &pages,
            missing.state_root(StateNamespaceV1::Business),
            &mut quota,
            &mut keys,
            MemoryClass::Ordinary,
        ),
        &mut outer,
        &mut inner,
        &mut budget,
    );
    assert!(matches!(
        rejected,
        Err(WorldNativeCompleteErrorV1::Value(ValueTreeError::Invalid(
            _
        )))
    ));
    assert_eq!(memory.used(), used);
    assert_eq!(coverage::FIELD_FAMILIES.len(), 97);
    coverage::check().unwrap();
}
