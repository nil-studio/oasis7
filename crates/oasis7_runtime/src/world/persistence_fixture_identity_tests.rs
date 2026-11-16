//! A reader's feature set cannot add authority to an already sealed state.
use super::*;

#[test]
fn native_identity_snapshot_restores_exact_map_and_real_tick_under_test_features() {
    let mut native_equivalent = World::new();
    // Model the native constructor before publication. No sealed snapshot or
    // signed Tick is edited: the actual Tick below commits this exact map.
    assert!(
        native_equivalent
            .state
            .node_identity_bindings
            .remove(TEST_MODULE_SIGNER_NODE_ID)
            .is_some()
    );
    native_equivalent.step().unwrap();
    native_equivalent.verify_tick_consensus_chain().unwrap();
    let identities = native_equivalent.state.node_identity_bindings.clone();
    let root = native_equivalent.current_state_root_hash().unwrap();
    let ticks = native_equivalent.tick_consensus_records().to_vec();
    assert!(!identities.contains_key(TEST_MODULE_SIGNER_NODE_ID));
    assert!(!ticks.is_empty());
    let restored = World::from_snapshot(
        native_equivalent.snapshot(),
        native_equivalent.journal().clone(),
    )
    .unwrap();
    assert_eq!(restored.state.node_identity_bindings, identities);
    assert_eq!(restored.current_state_root_hash().unwrap(), root);
    assert_eq!(restored.tick_consensus_records(), ticks);
    restored.verify_tick_consensus_chain().unwrap();
}

#[test]
fn fresh_fixture_identity_and_already_sealed_fixture_identity_remain_available() {
    for mut fresh in [World::new(), World::new_with_state(WorldState::default())] {
        assert!(
            fresh
                .state
                .node_identity_bindings
                .contains_key(TEST_MODULE_SIGNER_NODE_ID)
        );
        fresh.step().unwrap();
        let identities = fresh.state.node_identity_bindings.clone();
        let restored = World::from_snapshot(fresh.snapshot(), fresh.journal().clone()).unwrap();
        assert_eq!(restored.state.node_identity_bindings, identities);
        restored.verify_tick_consensus_chain().unwrap();
    }
}
