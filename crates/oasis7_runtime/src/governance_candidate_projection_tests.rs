use super::*;

#[test]
fn candidate_projection_is_read_only_and_matches_the_real_activation_boundary() {
    let candidate_world = world(1, vec![candidate("next", Status::ProbationReady, 2, 1)]);
    let before = candidate_world.state().clone();
    let before_root = candidate_world.current_state_root_hash().unwrap();
    let projection = candidate_world
        .project_governance_finality_registry_for_candidate_epoch(2)
        .unwrap();
    let actual = view(&world(
        2,
        vec![candidate("next", Status::ProbationReady, 2, 1)],
    ))
    .unwrap();

    assert_eq!(projection.candidate_time, 10);
    assert_eq!(projection.candidate_governance_epoch, 1);
    assert_eq!(projection.target_governance_epoch, 2);
    assert_eq!(projection.epoch_length_ticks, 10);
    assert_eq!(projection.candidate_state_root_hash, before_root);
    assert_eq!(projection.effective_registry, actual.effective_registry);
    assert_eq!(
        projection.effective_registry_digest,
        actual.effective_registry_digest
    );
    assert_eq!(candidate_world.state(), &before);
    assert_eq!(
        candidate_world.current_state_root_hash().unwrap(),
        before_root
    );
    assert!(
        !candidate_world
            .resolve_governance_effective_finality_signer_registry()
            .unwrap()
            .unwrap()
            .signer_bindings
            .contains_key("candidate-node")
    );

    let later = candidate_world
        .project_governance_finality_registry_for_candidate_epoch(3)
        .unwrap();
    assert_eq!(later.effective_registry, projection.effective_registry);
    assert_ne!(later.projection_digest, projection.projection_digest);
    assert_eq!(
        projection.projection_digest,
        oasis7_client_api::world_service::request_digest(
            "governance-membership-candidate-registry-projection-v2",
            &(
                &projection.candidate_state_root_hash,
                projection.candidate_time,
                projection.candidate_governance_epoch,
                projection.target_governance_epoch,
                projection.epoch_length_ticks,
                &projection.effective_registry,
            ),
        )
        .unwrap()
    );
}

#[test]
fn candidate_projection_rejects_past_targets_and_invalid_future_registry() {
    let mut invalid = candidate("duplicate-key", Status::ProbationReady, 2, 1);
    invalid.finality_signer_public_key = "11".repeat(32);
    let candidate_world = world(1, vec![invalid]);
    assert!(
        candidate_world
            .resolve_governance_effective_finality_signer_registry()
            .is_ok()
    );
    assert!(
        candidate_world
            .project_governance_finality_registry_for_candidate_epoch(2)
            .unwrap_err()
            .contains("unique")
    );
    for epoch in [0, 1] {
        assert!(
            candidate_world
                .project_governance_finality_registry_for_candidate_epoch(epoch)
                .unwrap_err()
                .contains("exceed")
        );
    }

    let missing = World::new_with_state(WorldState {
        world_rules: None,
        ..Default::default()
    });
    let projection = missing
        .project_governance_finality_registry_for_candidate_epoch(1)
        .unwrap();
    assert_eq!(projection.effective_registry, None);
    assert_eq!(missing.state().time, 0);
}
