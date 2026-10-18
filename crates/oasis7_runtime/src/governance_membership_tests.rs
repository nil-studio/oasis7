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
