//! This authenticates the fixed native subset; it deliberately does not grant WorldReady.
use super::*;
use family::WorldNativeFieldReadWorkV1;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
};
#[path = "incremental_world_native_family_test_support.rs"]
mod support;
use support::*;
fn native_rows(
    memory: &MemoryBudget,
    pages: &Pages,
    source: &crate::WorldState,
) -> Vec<ReservedValue<ExecutionWriteV1>> {
    let mut sink = Writer(pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut output = Vec::with_capacity(64);
    let mut context = MemberRecordWriteContext {
        sink: &mut sink,
        memory,
        io: &mut quota,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    };
    let mut emit = |row| {
        assert!(output.len() < 64);
        output.push(row);
        Ok(())
    };
    let mut records = 1000000;
    let mut work = write_work();
    crate::incremental_world_native_industry::prepare_world_native_industry_genesis_v1(
        source,
        &mut context,
        &mut records,
        &mut work,
        &mut emit,
    )
    .unwrap();
    crate::incremental_world_native_gameplay::prepare_world_native_gameplay_genesis_v1(
        source,
        &mut context,
        &mut records,
        &mut work,
        &mut emit,
    )
    .unwrap();
    crate::incremental_world_native_governance::prepare_world_native_governance_genesis_v1(
        source,
        &mut context,
        &mut records,
        &mut work,
        &mut emit,
    )
    .unwrap();
    output
}
fn install_native(
    memory: &MemoryBudget,
    pages: &Pages,
    rows: &mut [ReservedValue<ExecutionWriteV1>],
) -> RootSetV1 {
    rows.sort_unstable_by_key(|r| r.namespace);
    let mut roots = RootSetV1::empty("native-field-component".into(), [73; 32]).unwrap();
    for ns in [
        StateNamespaceV1::Business,
        StateNamespaceV1::Authorization,
        StateNamespaceV1::Effects,
    ] {
        let start = rows.iter().position(|r| r.namespace == ns).unwrap();
        let len = rows[start..]
            .iter()
            .take_while(|r| r.namespace == ns)
            .count();
        let partial = install(memory, pages, ns, &rows[start..start + len]);
        *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() =
            partial.state_root(ns).clone();
    }
    roots
}
#[test]
fn native_all23_scope_qualifies_replays_and_selects_original_value_without_generic_mint() {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 65536);
    let mut source = crate::WorldState::default();
    let catalogue = world_native_field_descriptors_v1().unwrap();
    assert_eq!(catalogue.len(), 23);
    for (index, descriptor) in catalogue.iter().enumerate() {
        assert!(crate::incremental_projection::WORLD_STATE_FIELDS.contains(descriptor));
        assert!(
            catalogue[..index]
                .iter()
                .all(|old| old.name != descriptor.name)
        );
    }
    let subset_rows =
        rows::<crate::incremental_world_native_gameplay::MetaProgressFieldV1>(&memory, &pages, &[]);
    let subset = install(&memory, &pages, StateNamespaceV1::Business, &subset_rows);
    let mut subset_io = io();
    let mut subset_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    assert!(
        qualify_world_native_fields_v1(
            &subset,
            &mut context(
                &memory,
                &pages,
                subset.state_root(StateNamespaceV1::Business),
                &mut subset_io,
                &mut subset_keys
            ),
            &mut scan(),
            &mut work()
        )
        .is_err(),
        "one native field must never stand in for all 23"
    );
    let mut before_rows = native_rows(&memory, &pages, &source);
    let roots = install_native(&memory, &pages, &mut before_rows);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut work = work();
    let proof = qualify_world_native_fields_v1(
        &roots,
        &mut context(
            &memory,
            &pages,
            roots.state_root(StateNamespaceV1::Business),
            &mut quota,
            &mut keys,
        ),
        &mut scan(),
        &mut work,
    )
    .unwrap();
    proof.validate_binding(&roots).unwrap();
    source.gameplay_policy.electricity_tax_bps = 65123;
    let after_rows = native_rows(&memory, &pages, &source);
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [21; 32],
        authority: AuthorityBinding {
            token: [22; 32],
            epoch: 23,
        },
    };
    let live = parent.fence().unwrap();
    let mut transaction = ExecutionTransaction::begin(
        parent.clone(),
        [24; 32],
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
            .find(|old| old.namespace == row.namespace && old.mutation.key == row.mutation.key)
            .and_then(|old| old.mutation.after)
            != row.mutation.after
        {
            transaction
                .write(row.namespace, &row.mutation.key, row.mutation.after)
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
        operation: [24; 32],
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
    let inherited = accept_world_native_fields_transition_v1(
        &proof,
        &verifier,
        &prepared.record,
        &mut after,
        &mut scan(),
        &mut work,
    )
    .unwrap();
    inherited.validate_binding(&prepared.record.after).unwrap();
    assert!(inherited.validate_binding(&roots).is_err());
    let recovered = recover_gameplay_policy_v1(&(), &mut after, &mut scan(), &mut work.typed)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.value().value().electricity_tax_bps, 65123);
    assert!(readmit_world_native_fields_ordinary_v1(&proof, &roots, &memory, || false).is_err());
}
