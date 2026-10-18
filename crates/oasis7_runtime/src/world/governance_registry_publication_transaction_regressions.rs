use super::super::*;
use super::World;
use std::collections::{BTreeMap, BTreeSet};

const CONTROLLER: &str = "msig.genesis.v1";
const LIVEOPS: &str = "liveops";
const CANDIDATE: &str = "candidate.fixture";
const NODE: &str = "validator.fixture";
const PUBLIC_KEY: &str = "3333333333333333333333333333333333333333333333333333333333333333";

fn seed_controller_registry(world: &mut World) {
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: CONTROLLER.into(),
            treasury_bucket_controller_slots: BTreeMap::from([(
                MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.into(),
                LIVEOPS.into(),
            )]),
            restricted_starter_claim_admin_account_ids: BTreeSet::from([LIVEOPS.into()]),
            controller_signer_policies: BTreeMap::from([
                (
                    CONTROLLER.into(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([
                            "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                                .into(),
                        ]),
                    },
                ),
                (
                    LIVEOPS.into(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([
                            "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                                .into(),
                        ]),
                    },
                ),
                (
                    "ops.backup".into(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([
                            "24c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                                .into(),
                        ]),
                    },
                ),
            ]),
        })
        .expect("seed controller registry");
}

fn submitted(candidate_id: &str, node_id: &str, public_key: &str) -> GovernanceEvent {
    GovernanceEvent::ValidatorAdmissionSubmitted {
        controller_account_id: CONTROLLER.into(),
        candidate_id: candidate_id.into(),
        node_id: node_id.into(),
        finality_signer_public_key: public_key.into(),
        stake: 25,
        operator_owner: "ops.team".into(),
        public_manifest_hash: format!("manifest:{candidate_id}"),
        requested_at_epoch: 0,
    }
}

fn approved(candidate_id: &str) -> GovernanceEvent {
    GovernanceEvent::ValidatorAdmissionApproved {
        controller_account_id: CONTROLLER.into(),
        candidate_id: candidate_id.into(),
        approved_at_epoch: 0,
    }
}

fn activated(candidate_id: &str) -> GovernanceEvent {
    GovernanceEvent::ValidatorAdmissionActivated {
        controller_account_id: CONTROLLER.into(),
        candidate_id: candidate_id.into(),
        activation_epoch: 0,
    }
}

fn revoked(candidate_id: &str, node_id: &str) -> GovernanceEvent {
    GovernanceEvent::ValidatorAdmissionRevoked {
        controller_account_id: CONTROLLER.into(),
        candidate_id: candidate_id.into(),
        node_id: node_id.into(),
        revoked_at_epoch: 1,
        reason: "fixture rotation".into(),
    }
}

fn admin_update(previous: Vec<&str>, next: Vec<&str>) -> GovernanceEvent {
    GovernanceEvent::RestrictedStarterClaimAdminRegistryUpdated {
        controller_account_id: LIVEOPS.into(),
        previous_admin_account_ids: previous.into_iter().map(str::to_string).collect(),
        next_admin_account_ids: next.into_iter().map(str::to_string).collect(),
    }
}

fn apply_direct(world: &mut World, event: &GovernanceEvent) {
    world
        .apply_governance_event(event)
        .expect("apply governance fixture");
}

fn assert_world_unchanged(world: &World, expected: &World, expected_root: &str) {
    assert_eq!(world.snapshot(), expected.snapshot());
    assert_eq!(
        world.state.governance_main_token_controller_registry,
        expected.state.governance_main_token_controller_registry
    );
    assert_eq!(
        world.state.governance_validator_admissions,
        expected.state.governance_validator_admissions
    );
    assert_eq!(
        world.state.node_identity_bindings,
        expected.state.node_identity_bindings
    );
    assert_eq!(
        world.state.node_main_token_account_bindings,
        expected.state.node_main_token_account_bindings
    );
    assert_eq!(world.journal(), expected.journal());
    assert_eq!(
        (world.next_event_id, world.next_event_id_era),
        (expected.next_event_id, expected.next_event_id_era)
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected.tick_consensus_records()
    );
    assert_eq!(world.state.time, expected.state.time);
    assert_eq!(
        world.current_state_root_hash().expect("current state root"),
        expected_root
    );
}

fn assert_injected(error: &WorldError) {
    assert!(
        matches!(
            error,
            WorldError::ResourceBalanceInvalid { reason }
                if reason == "injected append_event failure after publication preparation"
        ),
        "unexpected error: {error:?}"
    );
}

#[test]
fn every_raw_governance_registry_event_post_prepare_failure_preserves_world() {
    for case in ["admin", "submit", "approve", "activate", "revoke"] {
        let mut world = World::new();
        seed_controller_registry(&mut world);
        if matches!(case, "approve" | "activate" | "revoke") {
            apply_direct(&mut world, &submitted(CANDIDATE, NODE, PUBLIC_KEY));
        }
        if case == "activate" {
            apply_direct(&mut world, &approved(CANDIDATE));
        }
        let body = match case {
            "admin" => admin_update(vec![LIVEOPS], vec![LIVEOPS, "ops.backup"]),
            "submit" => submitted(CANDIDATE, NODE, PUBLIC_KEY),
            "approve" => approved(CANDIDATE),
            "activate" => activated(CANDIDATE),
            "revoke" => revoked(CANDIDATE, NODE),
            _ => unreachable!(),
        };
        let before = world.clone();
        let root_before = world.current_state_root_hash().expect("state root before");
        world.fail_next_append_after_publication_prepare_for_test();

        let error = world
            .append_event(WorldEventBody::Governance(body), None)
            .expect_err("post-prepare fault must reject governance registry event");

        assert_injected(&error);
        assert_world_unchanged(&world, &before, &root_before);
    }
}

#[test]
fn duplicate_candidate_error_precedes_identity_mismatch_without_mutation() {
    let mut world = World::new();
    seed_controller_registry(&mut world);
    apply_direct(&mut world, &submitted(CANDIDATE, NODE, PUBLIC_KEY));
    let before = world.clone();
    let root_before = world.current_state_root_hash().expect("state root before");

    let error = world
        .append_event(
            WorldEventBody::Governance(submitted(
                CANDIDATE,
                NODE,
                "4444444444444444444444444444444444444444444444444444444444444444",
            )),
            None,
        )
        .expect_err("duplicate candidate must win over identity mismatch");
    assert!(matches!(
        error,
        WorldError::GovernancePolicyInvalid { reason }
            if reason == "validator admission candidate already exists candidate_id=candidate.fixture"
    ));
    assert_world_unchanged(&world, &before, &root_before);
}

#[test]
fn admin_registry_drift_error_is_exact_and_nonmutating() {
    let mut world = World::new();
    seed_controller_registry(&mut world);
    let before = world.clone();
    let root_before = world.current_state_root_hash().expect("state root before");
    let error = world
        .append_event(
            WorldEventBody::Governance(admin_update(
                vec!["stale.admin"],
                vec![LIVEOPS, "ops.backup"],
            )),
            None,
        )
        .expect_err("stale previous admin registry must fail");
    assert!(matches!(
        error,
        WorldError::GovernancePolicyInvalid { reason }
            if reason == "restricted claim admin registry drift before apply: expected_previous=[\"stale.admin\"] actual_current=[\"liveops\"]"
    ));
    assert_world_unchanged(&world, &before, &root_before);
}

#[test]
fn revoke_node_mismatch_is_exact_and_nonmutating() {
    let mut world = World::new();
    seed_controller_registry(&mut world);
    apply_direct(&mut world, &submitted(CANDIDATE, NODE, PUBLIC_KEY));
    world
        .bind_node_identity(
            "validator.other",
            "5555555555555555555555555555555555555555555555555555555555555555",
        )
        .expect("seed other node identity");
    let before = world.clone();
    let root_before = world.current_state_root_hash().expect("state root before");

    let error = world
        .append_event(
            WorldEventBody::Governance(revoked(CANDIDATE, "validator.other")),
            None,
        )
        .expect_err("candidate/node mismatch must fail");
    assert!(matches!(
        error,
        WorldError::GovernancePolicyInvalid { reason }
            if reason == "validator admission revoke node_id mismatch candidate_id=candidate.fixture expected=validator.fixture actual=validator.other"
    ));
    assert_world_unchanged(&world, &before, &root_before);
}

#[test]
fn raw_registry_lifecycle_and_synthetic_revoke_replay_to_identical_root() {
    let mut world = World::new();
    seed_controller_registry(&mut world);
    world
        .bind_node_identity(
            "validator.legacy",
            "6666666666666666666666666666666666666666666666666666666666666666",
        )
        .expect("seed legacy node identity");
    let baseline = world.snapshot();
    for event in [
        admin_update(vec![LIVEOPS], vec![LIVEOPS, "ops.backup"]),
        submitted(CANDIDATE, NODE, PUBLIC_KEY),
        approved(CANDIDATE),
        activated(CANDIDATE),
        revoked(CANDIDATE, NODE),
        revoked("missing-candidate", "validator.legacy"),
    ] {
        world
            .append_event(WorldEventBody::Governance(event), None)
            .expect("append raw governance registry lifecycle event");
    }

    let candidate = &world.state.governance_validator_admissions[CANDIDATE];
    assert_eq!(
        candidate.status,
        GovernanceValidatorAdmissionStatus::Revoked
    );
    assert_eq!(
        world.state.governance_validator_admissions["legacy-revoked:validator.legacy"].status,
        GovernanceValidatorAdmissionStatus::Revoked
    );
    assert_eq!(world.state.node_identity_bindings[NODE], PUBLIC_KEY);
    assert!(
        world
            .state
            .node_main_token_account_bindings
            .contains_key(NODE)
    );
    let replay = World::from_snapshot(baseline, world.journal().clone()).expect("replay lifecycle");
    assert_eq!(
        replay.state.governance_main_token_controller_registry,
        world.state.governance_main_token_controller_registry
    );
    assert_eq!(
        replay.state.governance_validator_admissions,
        world.state.governance_validator_admissions
    );
    assert_eq!(
        replay.state.node_identity_bindings,
        world.state.node_identity_bindings
    );
    assert_eq!(
        replay.state.node_main_token_account_bindings,
        world.state.node_main_token_account_bindings
    );
    assert_eq!(
        replay.current_state_root_hash().expect("replay root"),
        world.current_state_root_hash().expect("live root")
    );
}
