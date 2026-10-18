use super::super::*;
use std::collections::{BTreeMap, BTreeSet};

fn seed_governance_world(world: &mut World) {
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            epoch_length_ticks: 10,
            ..GovernanceExecutionPolicy::default()
        })
        .expect("set governance policy");
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "governance.finality.v1".to_string(),
            threshold: 2,
            threshold_bps: 0,
            signer_bindings: BTreeMap::from([
                (
                    "validator-a".to_string(),
                    "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                ),
                (
                    "validator-b".to_string(),
                    "2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                ),
            ]),
            validator_stakes: BTreeMap::from([
                ("validator-a".to_string(), 60),
                ("validator-b".to_string(), 40),
            ]),
        })
        .expect("set finality registry");
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::from([(
                MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.to_string(),
                "liveops".to_string(),
            )]),
            restricted_starter_claim_admin_account_ids: BTreeSet::from(["liveops".to_string()]),
            controller_signer_policies: BTreeMap::from([
                (
                    "msig.genesis.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([
                            "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "liveops".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([
                            "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                                .to_string(),
                        ]),
                    },
                ),
            ]),
        })
        .expect("set controller registry");
}

fn submit_candidate(world: &mut World, candidate_id: &str, node_id: &str, public_key_hex: &str) {
    world.submit_action(Action::SubmitGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: candidate_id.to_string(),
        node_id: node_id.to_string(),
        finality_signer_public_key: public_key_hex.to_string(),
        stake: 25,
        operator_owner: "ops.team".to_string(),
        public_manifest_hash: format!("manifest:{candidate_id}"),
    });
    world.step().expect("submit validator admission");
}

#[test]
fn validator_admission_lifecycle_updates_effective_registry_and_allows_reapply_after_revoke() {
    let mut world = World::new();
    seed_governance_world(&mut world);

    submit_candidate(
        &mut world,
        "candidate-c",
        "validator-c",
        "3333333333333333333333333333333333333333333333333333333333333333",
    );
    world.submit_action(Action::ApproveGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-c".to_string(),
    });
    world.step().expect("approve validator admission");
    world.submit_action(Action::ActivateGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-c".to_string(),
        activation_epoch: 0,
    });
    world.step().expect("activate validator admission");

    let active_registry = world
        .resolve_governance_effective_finality_signer_registry()
        .expect("resolve effective registry")
        .expect("effective registry");
    assert_eq!(active_registry.signer_bindings.len(), 3);
    assert_eq!(
        active_registry.validator_stakes.get("validator-c"),
        Some(&25)
    );
    assert_eq!(
        active_registry
            .signer_bindings
            .get("validator-c")
            .map(String::as_str),
        Some("3333333333333333333333333333333333333333333333333333333333333333")
    );

    world.submit_action(Action::RevokeGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-c".to_string(),
        node_id: "validator-c".to_string(),
        reason: "operator rotation".to_string(),
    });
    world.step().expect("revoke validator admission");

    let revoked_registry = world
        .resolve_governance_effective_finality_signer_registry()
        .expect("resolve effective registry after revoke")
        .expect("effective registry after revoke");
    assert!(!revoked_registry.signer_bindings.contains_key("validator-c"));
    assert!(
        !revoked_registry
            .validator_stakes
            .contains_key("validator-c")
    );

    submit_candidate(
        &mut world,
        "candidate-c-reapply",
        "validator-c",
        "3333333333333333333333333333333333333333333333333333333333333333",
    );
    let reapplied = world
        .governance_validator_admissions()
        .get("candidate-c-reapply")
        .expect("reapplied candidate");
    assert_eq!(
        reapplied.status,
        GovernanceValidatorAdmissionStatus::Applied
    );

    world.submit_action(Action::ApproveGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-c-reapply".to_string(),
    });
    world.step().expect("approve reapply validator admission");
    world.submit_action(Action::ActivateGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-c-reapply".to_string(),
        activation_epoch: 0,
    });
    world.step().expect("activate reapply validator admission");

    let reactivated_registry = world
        .resolve_governance_effective_finality_signer_registry()
        .expect("resolve effective registry after reapply")
        .expect("effective registry after reapply");
    assert_eq!(
        reactivated_registry
            .signer_bindings
            .get("validator-c")
            .map(String::as_str),
        Some("3333333333333333333333333333333333333333333333333333333333333333")
    );
    assert_eq!(
        reactivated_registry.validator_stakes.get("validator-c"),
        Some(&25)
    );
}

#[test]
fn validator_admission_probation_becomes_effective_once_activation_epoch_is_due() {
    let mut world = World::new();
    seed_governance_world(&mut world);

    submit_candidate(
        &mut world,
        "candidate-future",
        "validator-future",
        "4444444444444444444444444444444444444444444444444444444444444444",
    );
    world.submit_action(Action::ApproveGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-future".to_string(),
    });
    world.step().expect("approve future validator admission");
    world.submit_action(Action::ActivateGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-future".to_string(),
        activation_epoch: 1,
    });
    world.step().expect("schedule future validator admission");

    let probationary = world
        .governance_validator_admissions()
        .get("candidate-future")
        .expect("candidate-future");
    assert_eq!(
        probationary.status,
        GovernanceValidatorAdmissionStatus::ProbationReady
    );
    let pre_epoch_registry = world
        .resolve_governance_effective_finality_signer_registry()
        .expect("resolve pre-epoch registry")
        .expect("pre-epoch registry");
    assert!(
        !pre_epoch_registry
            .signer_bindings
            .contains_key("validator-future")
    );

    for _ in 0..7 {
        world.step().expect("advance governance epoch");
    }

    let post_epoch_registry = world
        .resolve_governance_effective_finality_signer_registry()
        .expect("resolve post-epoch registry")
        .expect("post-epoch registry");
    assert_eq!(
        post_epoch_registry
            .signer_bindings
            .get("validator-future")
            .map(String::as_str),
        Some("4444444444444444444444444444444444444444444444444444444444444444")
    );
    assert_eq!(
        post_epoch_registry.validator_stakes.get("validator-future"),
        Some(&25)
    );
}

#[test]
fn validator_admission_activate_with_already_due_epoch_becomes_active_immediately() {
    let mut world = World::new();
    seed_governance_world(&mut world);

    submit_candidate(
        &mut world,
        "candidate-late",
        "validator-late",
        "5555555555555555555555555555555555555555555555555555555555555555",
    );
    world.submit_action(Action::ApproveGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-late".to_string(),
    });
    world.step().expect("approve late validator admission");
    for _ in 0..15 {
        world
            .step()
            .expect("advance governance epoch before late activation");
    }

    world.submit_action(Action::ActivateGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-late".to_string(),
        activation_epoch: 1,
    });
    world
        .step()
        .expect("activate already-due validator admission");

    let late_record = world
        .governance_validator_admissions()
        .get("candidate-late")
        .expect("late candidate");
    assert_eq!(
        late_record.status,
        GovernanceValidatorAdmissionStatus::Active
    );
    let registry = world
        .resolve_governance_effective_finality_signer_registry()
        .expect("resolve effective registry for late activation")
        .expect("effective registry for late activation");
    assert_eq!(
        registry
            .signer_bindings
            .get("validator-late")
            .map(String::as_str),
        Some("5555555555555555555555555555555555555555555555555555555555555555")
    );
    assert_eq!(registry.validator_stakes.get("validator-late"), Some(&25));
}

#[test]
fn validator_admission_rejects_zero_stake() {
    let mut world = World::new();
    seed_governance_world(&mut world);

    world.submit_action(Action::SubmitGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-zero".to_string(),
        node_id: "validator-zero".to_string(),
        finality_signer_public_key:
            "6666666666666666666666666666666666666666666666666666666666666666".to_string(),
        stake: 0,
        operator_owner: "ops.team".to_string(),
        public_manifest_hash: "manifest:zero".to_string(),
    });
    world.step().expect("reject zero stake admission");

    assert!(
        !world
            .governance_validator_admissions()
            .contains_key("candidate-zero")
    );
}
