use super::*;
use crate::service_admission::{ExecutionBinding, WorldIdentity};
use crate::{GovernanceValidatorAdmissionStatus as Status, WorldState};

fn candidate(
    id: &str,
    status: Status,
    activation: u64,
    tick: u64,
) -> GovernanceValidatorAdmissionRecord {
    GovernanceValidatorAdmissionRecord {
        candidate_id: id.into(),
        node_id: "candidate-node".into(),
        finality_signer_public_key: "22".repeat(32),
        stake: 25,
        operator_owner: "owner".into(),
        public_manifest_hash: "manifest".into(),
        activation_epoch: Some(activation),
        last_transition_tick: tick,
        status,
        ..Default::default()
    }
}
fn world(epoch: u64, candidates: Vec<GovernanceValidatorAdmissionRecord>) -> World {
    let state = WorldState {
        world_rules: None,
        time: epoch * 10,
        governance_finality_signer_registry: Some(GovernanceFinalitySignerRegistry {
            slot_id: "finality".into(),
            threshold: 1,
            signer_bindings: BTreeMap::from([("seed".into(), "11".repeat(32))]),
            validator_stakes: BTreeMap::from([("seed".into(), 100)]),
            ..Default::default()
        }),
        governance_validator_admissions: candidates
            .into_iter()
            .map(|r| (r.candidate_id.clone(), r))
            .collect(),
        ..Default::default()
    };
    let mut world = World::new_with_state(state);
    world
        .set_governance_execution_policy(crate::GovernanceExecutionPolicy {
            epoch_length_ticks: 10,
            ..Default::default()
        })
        .unwrap();
    world
        .bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .unwrap();
    world
}
fn commit(world: &World) -> CommitRef {
    let binding = world.current_cognition_runtime_binding().unwrap();
    CommitRef {
        world: WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        binding: ExecutionBinding {
            provider_world_id: "world".into(),
            branch_id: "main".into(),
            finality_ref: oasis7_client_api::world_service::request_digest(
                "finality",
                &(
                    binding.finality_epoch,
                    &binding.finality_status,
                    &binding.finality_block_hash,
                ),
            )
            .unwrap(),
            reorg_generation: 0,
            governing_manifest_ref: binding.runtime_manifest_hash.to_string(),
            authority_generation: world
                .capability_revocation_state()
                .authority_records
                .values()
                .map(|record| record.governance_epoch)
                .max()
                .unwrap_or(0),
            permission_generation: world.capability_revocation_state().epoch,
        },
        position: world.state().time,
        execution_block_hash: "block".into(),
        state_root_ref: "cas-snapshot-reference".into(),
    }
}
fn view(world: &World) -> Result<WorldGovernanceMembershipView, String> {
    WorldGovernanceMembershipView::read(
        world,
        &commit(world),
        &world.current_state_root_hash().unwrap(),
        "controlled_single_authority",
    )
}

#[test]
fn probation_effective_at_boundary_without_rewriting_raw_status() {
    let before = view(&world(
        1,
        vec![candidate("c", Status::ProbationReady, 2, 1)],
    ))
    .unwrap();
    let after = view(&world(
        2,
        vec![candidate("c", Status::ProbationReady, 2, 1)],
    ))
    .unwrap();
    assert!(
        !before
            .effective_registry
            .unwrap()
            .signer_bindings
            .contains_key("candidate-node")
    );
    assert!(
        after
            .effective_registry
            .unwrap()
            .signer_bindings
            .contains_key("candidate-node")
    );
    assert_eq!(after.candidates["c"].status, Status::ProbationReady);
    assert_ne!(
        before.effective_registry_digest,
        after.effective_registry_digest
    );
}

#[test]
fn competing_record_selection_and_revocation_reuse_resolver() {
    let active = candidate("active", Status::Active, 0, 1);
    let mut revoked = candidate("revoked", Status::Revoked, 0, 2);
    revoked.revoked_at_epoch = Some(99);
    let result = view(&world(0, vec![active, revoked])).unwrap();
    assert_eq!(result.candidates.len(), 2);
    assert_eq!(
        result.selected_records["candidate-node"].candidate_id,
        "revoked"
    );
    assert!(
        !result
            .effective_registry
            .unwrap()
            .signer_bindings
            .contains_key("candidate-node")
    );
}

#[test]
fn resolver_failure_and_mismatched_commit_root_are_explicit() {
    let mut invalid = candidate("bad", Status::Active, 0, 1);
    invalid.finality_signer_public_key = "11".repeat(32); // duplicate seed key
    assert!(
        view(&world(0, vec![invalid]))
            .unwrap_err()
            .contains("unique")
    );
    let world = world(0, vec![]);
    assert!(
        WorldGovernanceMembershipView::read(&world, &commit(&world), "wrong-root", "profile")
            .unwrap_err()
            .contains("root mismatch")
    );
    let mut wrong = commit(&world);
    wrong.world.world_id = "other-world".into();
    assert!(
        WorldGovernanceMembershipView::read(
            &world,
            &wrong,
            &world.current_state_root_hash().unwrap(),
            "profile"
        )
        .unwrap_err()
        .contains("binding mismatch")
    );
}

#[test]
fn validator_configuration_and_governance_do_not_imply_consensus_application() {
    let governance = view(&world(0, vec![candidate("active", Status::Active, 0, 1)])).unwrap();
    let runtime = RuntimeMembershipApplicationView::startup_projection(
        "validator",
        Some(governance.effective_registry_digest),
        Some("world_governance".into()),
    );
    assert!(!runtime.supports_safe_runtime_update);
    assert_eq!(runtime.applied_consensus_membership, None);
}

#[test]
fn unapproved_candidates_and_missing_registry_do_not_gain_membership() {
    for status in [Status::Applied, Status::ApprovedCandidate] {
        let result = view(&world(10, vec![candidate("candidate", status, 0, 1)])).unwrap();
        assert!(
            !result
                .effective_registry
                .unwrap()
                .signer_bindings
                .contains_key("candidate-node")
        );
    }
    let mut state = WorldState::default();
    state.governance_validator_admissions.insert(
        "candidate".into(),
        candidate("candidate", Status::Active, 0, 1),
    );
    let mut missing_registry = World::new_with_state(state);
    missing_registry
        .bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .unwrap();
    let result = view(&missing_registry).unwrap();
    assert_eq!(result.effective_registry, None);
    assert_eq!(result.candidates.len(), 1);
}

#[test]
fn future_epoch_projection_is_provenanced_and_does_not_mutate_world() {
    let world = world(1, vec![candidate("future", Status::ProbationReady, 2, 1)]);
    let source_commit = commit(&world);
    let source_root = world.current_state_root_hash().unwrap();
    let before_state = world.state().clone();
    let before_registry = world
        .resolve_governance_effective_finality_signer_registry()
        .unwrap();

    let at_activation = WorldGovernanceMembershipView::read_for_future_epoch(
        &world,
        &source_commit,
        &source_root,
        "controlled_single_authority",
        2,
    )
    .unwrap();
    let after_activation = WorldGovernanceMembershipView::read_for_future_epoch(
        &world,
        &source_commit,
        &source_root,
        "controlled_single_authority",
        3,
    )
    .unwrap();
    let committed_at_boundary = view(&self::world(
        2,
        vec![candidate("future", Status::ProbationReady, 2, 1)],
    ))
    .unwrap();

    assert_eq!(world.state(), &before_state);
    assert_eq!(
        world
            .resolve_governance_effective_finality_signer_registry()
            .unwrap(),
        before_registry
    );
    assert_eq!(at_activation.source_commit, source_commit);
    assert_eq!(at_activation.source_state_root_hash, source_root);
    assert_eq!(at_activation.source_profile, "controlled_single_authority");
    assert_eq!(at_activation.source_governance_epoch, 1);
    assert_eq!(at_activation.target_governance_epoch, 2);
    assert_eq!(after_activation.target_governance_epoch, 3);
    assert_eq!(
        at_activation.effective_registry,
        after_activation.effective_registry
    );
    assert_eq!(
        at_activation.effective_registry,
        committed_at_boundary.effective_registry
    );
    assert_eq!(
        at_activation.effective_registry_digest,
        committed_at_boundary.effective_registry_digest
    );
    assert!(
        at_activation
            .effective_registry
            .as_ref()
            .unwrap()
            .signer_bindings
            .contains_key("candidate-node")
    );
    assert!(
        !before_registry
            .as_ref()
            .unwrap()
            .signer_bindings
            .contains_key("candidate-node")
    );
    assert_eq!(
        at_activation.effective_registry_digest,
        oasis7_client_api::world_service::request_digest(
            "governance-membership-registry-v1",
            &at_activation.effective_registry,
        )
        .unwrap()
    );
    assert_ne!(
        at_activation.provenance_digest,
        after_activation.provenance_digest
    );
    assert_eq!(
        at_activation.provenance_digest,
        oasis7_client_api::world_service::request_digest(
            "governance-membership-future-registry-projection-v1",
            &(
                &at_activation.source_commit,
                &at_activation.source_state_root_hash,
                &at_activation.source_profile,
                at_activation.source_governance_epoch,
                at_activation.target_governance_epoch,
                &at_activation.effective_registry,
            ),
        )
        .unwrap()
    );
}

#[test]
fn future_epoch_projection_reuses_source_validation_and_never_falls_back() {
    let world = world(2, vec![candidate("future", Status::ProbationReady, 3, 1)]);
    let source_commit = commit(&world);
    let root = world.current_state_root_hash().unwrap();

    assert!(
        WorldGovernanceMembershipView::read_for_future_epoch(
            &world,
            &source_commit,
            &root,
            "controlled_single_authority",
            2,
        )
        .is_err()
    );
    assert!(
        WorldGovernanceMembershipView::read_for_future_epoch(
            &world,
            &source_commit,
            &root,
            "controlled_single_authority",
            1,
        )
        .is_err()
    );
    assert!(
        WorldGovernanceMembershipView::read_for_future_epoch(
            &world,
            &source_commit,
            "wrong-root",
            "controlled_single_authority",
            3,
        )
        .unwrap_err()
        .contains("root mismatch")
    );
    assert!(WorldGovernanceMembershipView::read_for_future_epoch(
        &world,
        &source_commit,
        &root,
        " ",
        3,
    )
    .unwrap_err()
    .contains("profile is missing"));
    let mut wrong_commit = source_commit.clone();
    wrong_commit.world.world_id = "other-world".into();
    assert!(
        WorldGovernanceMembershipView::read_for_future_epoch(
            &world,
            &wrong_commit,
            &root,
            "controlled_single_authority",
            3,
        )
        .unwrap_err()
        .contains("binding mismatch")
    );

    let mut no_registry_state = WorldState::default();
    no_registry_state.governance_validator_admissions.insert(
        "candidate".into(),
        candidate("candidate", Status::Active, 0, 1),
    );
    let mut no_registry = World::new_with_state(no_registry_state);
    no_registry
        .bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .unwrap();
    let projection = WorldGovernanceMembershipView::read_for_future_epoch(
        &no_registry,
        &commit(&no_registry),
        &no_registry.current_state_root_hash().unwrap(),
        "controlled_single_authority",
        1,
    )
    .unwrap();
    assert_eq!(projection.effective_registry, None);
}
