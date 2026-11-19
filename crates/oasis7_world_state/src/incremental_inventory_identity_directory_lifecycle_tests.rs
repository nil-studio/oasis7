//! Reuses the actual GEN fixture, then crosses a real seal into the next
//! transaction. No World snapshot is rebuilt for the resident directory.
use super::*;
use oasis7_formal_protocol::incremental_execution::TransitionVerifier;
#[test]
fn resident_identities_rebind_only_to_actual_seal_and_survive_next_tick_without_cold_scan() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 16384);
    let mut world = source();
    let roots = install(&world, &memory, &pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut fw = family_work();
    let mut work = directory_work();
    let family = qualify_inventory_family_v1(
        &roots,
        &mut read(&memory, &pages, &roots, &mut quota, &mut keys),
        &mut scan(),
        &mut fw,
    )
    .unwrap();
    let directory = build_inventory_identity_directory_v1(
        &family,
        &roots,
        &mut read(&memory, &pages, &roots, &mut quota, &mut keys),
        &mut scan(),
        &mut fw,
        &mut work,
    )
    .unwrap();
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [1; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [3; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    world
        .material_ledgers
        .get_mut(&MaterialLedgerId::World)
        .unwrap()
        .insert("next-tick/new-kind".into(), -101);
    let desired = rows(&world, &memory, &pages);
    let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
        &(MaterialLedgerId::World, "next-tick/new-kind"),
        &memory,
        MemoryClass::Ordinary,
        &mut keys,
        || false,
    )
    .unwrap();
    let key = WorldFieldKeyV1::new(
        descriptor(
            "material_ledgers",
            crate::incremental_projection::FieldShape::NestedMap,
        )
        .unwrap(),
        WorldFieldRecordKindV1::Entry,
        Some(semantic.digest),
    )
    .unwrap();
    let row = desired.0.iter().find(|r| r.key == key.as_bytes()).unwrap();
    tx.write(StateNamespaceV1::Business, key.as_bytes(), row.after)
        .unwrap();
    let empty = MaterialLedgerId::Agent("empty/原".into());
    world.material_ledgers.remove(&empty);
    let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
        &empty,
        &memory,
        MemoryClass::Ordinary,
        &mut keys,
        || false,
    )
    .unwrap();
    let key = WorldFieldKeyV1::new(
        descriptor(
            "material_ledgers",
            crate::incremental_projection::FieldShape::NestedMap,
        )
        .unwrap(),
        WorldFieldRecordKindV1::Group,
        Some(semantic.digest),
    )
    .unwrap();
    tx.write(StateNamespaceV1::Business, key.as_bytes(), None)
        .unwrap();
    let frozen = {
        let mut current_io = io();
        let mut current_keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let guard = tx.guard();
        let mut current = crate::incremental_field_overlay::WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut current_io,
            },
            key_work: &mut current_keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        freeze_current_inventory_identity_directory_v1(
            &directory,
            &family,
            &roots,
            &pages,
            &mut current,
            &mut fw,
            &mut work,
            &guard,
            &live,
        )
        .unwrap()
    };
    frozen.validate_guard(&tx, &memory, &live).unwrap();
    let foreign = ExecutionTransaction::begin(
        parent.clone(),
        [3; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let foreign_guard = foreign.guard();
    let foreign_sealed = foreign.seal_with_binding(&foreign_guard, &live, 1).unwrap();
    assert!(
        frozen
            .validate_sealed(
                foreign_sealed.binding(),
                &foreign_sealed.transition().record.after,
                &memory
            )
            .is_err()
    );
    let guard = tx.guard();
    let sealed = tx.seal_with_binding(&guard, &live, 1).unwrap();
    let mut forged = sealed.transition().record.after.clone();
    forged.simulation_tick += 1;
    assert!(
        frozen
            .rebind_after_seal_v1(sealed.binding(), &forged, &memory, &mut work, || false)
            .is_err()
    );
    let foreign_memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    assert!(
        frozen
            .rebind_after_seal_v1(
                sealed.binding(),
                &sealed.transition().record.after,
                &foreign_memory,
                &mut work,
                || false
            )
            .is_err()
    );
    let after_directory = frozen
        .rebind_after_seal_v1(
            sealed.binding(),
            &sealed.transition().record.after,
            &memory,
            &mut work,
            || false,
        )
        .unwrap();
    let (transition, _binding) = sealed.into_parts();
    for node in &transition.new_nodes {
        Writer(&pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let after = &transition.record.after;
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [3; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let after_family = accept_inventory_transition_v1(
        &family,
        &verifier,
        &transition.record,
        &mut read(&memory, &pages, after, &mut quota, &mut keys),
        &mut fw,
    )
    .unwrap();
    assert!(directory.validate_binding(after).is_err());
    after_directory.validate_binding(after).unwrap();
    let next_parent = ExecutionParent {
        roots: after.clone(),
        commit: [9; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    };
    let next_live = next_parent.fence().unwrap();
    let mut next = ExecutionTransaction::begin(
        next_parent,
        [4; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let absent = MaterialLedgerId::Site("absent".into());
    for ledger in world.material_ledgers.keys().chain([&empty, &absent]) {
        let actual = recover(
            ledger,
            &after_directory,
            &after_family,
            after,
            &mut next,
            &memory,
            &pages,
            &mut fw,
            &mut work,
            &next_live,
        );
        assert_eq!(actual.as_ref(), world.material_ledgers.get(ledger));
    }
}
