//! Real World projection with cryptographic key proofs; these tests do not
//! substitute synthetic parents for live wire-3 controller-authorized commits.
use super::*;
use crate::GovernanceEvent;
use crate::GovernanceValidatorAdmissionStatus as Status;
use ed25519_dalek::{Signer, SigningKey};
fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn public(n: u8) -> String {
    hex::encode(key(n).verifying_key().to_bytes())
}
fn world() -> World {
    let mut world = World::new();
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "bootstrap".into(),
            threshold: 1,
            threshold_bps: 1,
            signer_bindings: BTreeMap::from([
                ("node".into(), public(1)),
                ("peer".into(), public(9)),
            ]),
            validator_stakes: BTreeMap::from([("node".into(), 10), ("peer".into(), 20)]),
        })
        .unwrap();
    world.state.time = 118;
    world
}
fn record(
    world: &World,
    id: &str,
    generation: u64,
    old: u8,
    new: u8,
    decision: u64,
) -> BootstrapFinalityKeyRotationRecordV1 {
    let context = GovernanceAuthorizationV1 {
        schema_version: 1,
        world_id: "world".into(),
        chain_id: "chain".into(),
        genesis_digest: "a".repeat(64),
        parent_position: decision - 1,
        parent_record_id: "b".repeat(64),
        parent_state_root: "c".repeat(64),
        policy_digest: "d".repeat(64),
        controller_account_id: "controller".into(),
        nonce: generation + 1,
        valid_from_position: decision,
        valid_through_position: decision,
        signatures: BTreeMap::new(),
    };
    let body = BootstrapFinalityKeyRotationBodyV1 {
        schema_version: 1,
        authorization: context.clone(),
        rotation_id: id.into(),
        bootstrap_registry_digest_hex: bootstrap_finality_registry_digest_v1(
            world
                .state
                .governance_finality_signer_registry
                .as_ref()
                .unwrap(),
        )
        .unwrap(),
        bootstrap_public_key_hex: public(1),
        node_id: "node".into(),
        expected_generation: generation,
        old_public_key_hex: public(old),
        new_public_key_hex: public(new),
        decision_height: decision,
        activation_height: decision + 2,
        target_epoch: (decision + 2) / 120,
    };
    let proof = |role, signer: u8| BootstrapFinalityKeyRotationProofV1 {
        schema_version: 1,
        role,
        signature_hex: hex::encode(
            key(signer)
                .sign(&body.signing_bytes(role).unwrap())
                .to_bytes(),
        ),
    };
    let action = Action::RotateGovernanceBootstrapFinalityKeyV1 {
        controller_account_id: "controller".into(),
        old_key_proof: proof(BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization, old),
        new_key_proof: proof(BootstrapFinalityKeyRotationRoleV1::NewKeyPossession, new),
        body,
    };
    let mut authorization = context;
    authorization.signatures.insert(public(8), "00".repeat(64));
    BootstrapFinalityKeyRotationRecordV1 {
        authorized_action: GovernanceAuthorizedActionV1 {
            action: Box::new(action),
            authorization,
        },
        applied_at_tick: decision,
    }
}
fn effective(world: &World, epoch: u64) -> Result<GovernanceFinalitySignerRegistry, WorldError> {
    world
        .resolve_governance_effective_finality_signer_registry_from_admissions_at_epoch(
            &world.state.governance_validator_admissions,
            epoch,
        )
        .map(|r| r.unwrap())
}
fn admission(
    id: &str,
    status: Status,
    activation: u64,
    tick: u64,
) -> GovernanceValidatorAdmissionRecord {
    GovernanceValidatorAdmissionRecord {
        candidate_id: id.into(),
        node_id: "node".into(),
        finality_signer_public_key: public(1),
        stake: 10,
        status,
        activation_epoch: Some(activation),
        last_transition_tick: tick,
        revoked_at_epoch: (status == Status::Revoked).then_some(activation),
        ..Default::default()
    }
}
#[test]
fn bootstrap_projection_keeps_origin_identity_and_activates_continuous_generations() {
    let mut world = world();
    let before = world.snapshot();
    let first = record(&world, "first", 0, 1, 2, 118);
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("first".into(), first);
    assert_eq!(
        effective(&world, 0).unwrap().signer_bindings["node"],
        public(1)
    );
    assert_eq!(
        effective(&world, 1).unwrap().signer_bindings["node"],
        public(2)
    );
    let second = record(&world, "second", 1, 2, 3, 238);
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("second".into(), second);
    assert_eq!(
        effective(&world, 1).unwrap().signer_bindings["node"],
        public(2)
    );
    assert_eq!(
        effective(&world, 2).unwrap().signer_bindings["node"],
        public(3)
    );
    assert_eq!(
        world.state.governance_finality_signer_registry,
        before.state.governance_finality_signer_registry
    );
    assert_eq!(
        world.state.node_identity_bindings,
        before.state.node_identity_bindings
    );
    assert_eq!(
        world.state.node_main_token_account_bindings,
        before.state.node_main_token_account_bindings
    );
    assert!(world.state.governance_validator_admissions.is_empty());
    let invalid = record(&world, "second", 2, 2, 3, 238);
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("second".into(), invalid);
    assert!(effective(&world, 2).is_err());
}
#[test]
fn revocation_new_application_and_same_key_admission_never_restore_bootstrap_rotation() {
    let mut world = world();
    let first = record(&world, "first", 0, 1, 2, 118);
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("first".into(), first);
    world.state.governance_validator_admissions.insert(
        "revoked".into(),
        admission("revoked", Status::Revoked, 1, 120),
    );
    world
        .state
        .governance_validator_admissions
        .insert("new".into(), admission("new", Status::Applied, 2, 121));
    assert!(
        !effective(&world, 2)
            .unwrap()
            .signer_bindings
            .contains_key("node")
    );
    // A real Admission owns the authority even with exactly the raw base key.
    world
        .state
        .governance_validator_admissions
        .insert("active".into(), admission("active", Status::Active, 2, 240));
    assert_eq!(
        effective(&world, 2).unwrap().signer_bindings["node"],
        public(1)
    );
    // Before that Admission takes effect, the prior revocation still wins.
    assert!(
        !effective(&world, 1)
            .unwrap()
            .signer_bindings
            .contains_key("node")
    );
}
#[test]
fn changed_raw_registry_and_invalid_key_proof_fail_closed_in_projection_and_restore() {
    let mut world = world();
    let first = record(&world, "first", 0, 1, 2, 118);
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("first".into(), first);
    let mut changed = world.clone();
    changed
        .state
        .governance_finality_signer_registry
        .as_mut()
        .unwrap()
        .validator_stakes
        .insert("peer".into(), 21);
    assert!(effective(&changed, 1).is_err());
    assert!(changed.capture_finality_key_rotation_parent().is_err());
    assert!(World::from_snapshot(changed.snapshot(), changed.journal.clone()).is_err());
    let record = world
        .state
        .governance_bootstrap_finality_key_rotations
        .get_mut("first")
        .unwrap();
    let Action::RotateGovernanceBootstrapFinalityKeyV1 { new_key_proof, .. } =
        record.authorized_action.action.as_mut()
    else {
        unreachable!()
    };
    new_key_proof.signature_hex = "00".repeat(64);
    assert!(effective(&world, 1).is_err());
    assert!(World::from_snapshot(world.snapshot(), world.journal.clone()).is_err());
}
#[test]
fn shared_reservations_reject_raw_keys_history_keys_ids_and_same_batch_reuse() {
    let mut world = world();
    let parent = world.capture_finality_key_rotation_parent().unwrap();
    assert!(
        world
            .validate_rotation_reservation(&parent, "fresh", &public(1))
            .is_err()
    );
    assert!(
        world
            .validate_rotation_reservation(&parent, "fresh", &public(9))
            .is_err()
    );
    assert!(
        world
            .validate_rotation_reservation(&parent, "fresh", &public(2))
            .is_ok()
    );
    let first = record(&world, "first", 0, 1, 2, 118);
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("first".into(), first);
    assert!(
        world
            .validate_rotation_reservation(&parent, "first", &public(3))
            .is_err()
    );
    assert!(
        world
            .validate_rotation_reservation(&parent, "fresh", &public(2))
            .is_err()
    );
}
#[test]
fn rejected_bootstrap_event_has_no_nonce_or_registry_publication_without_qualified_parent() {
    let mut world = world();
    let first = record(&world, "first", 0, 1, 2, 118);
    let before = world.snapshot();
    assert!(
        world
            .apply_governance_event(&GovernanceEvent::BootstrapFinalityKeyRotated {
                authorization: first.authorized_action,
            })
            .is_err()
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn admitted_and_bootstrap_records_share_ids_key_reservations_and_capacity() {
    use oasis7_agent_api::world_action::{
        FinalityKeyRotationBodyV1, FinalityKeyRotationProofV1, FinalityKeyRotationRoleV1,
    };
    let mut world = world();
    let bootstrap = record(&world, "bootstrap", 0, 1, 2, 118);
    let context = bootstrap.body().unwrap().authorization.clone();
    // This helper's oracle is namespace reservation, not admitted key-proof
    // cryptography (live admitted rotations are covered by host integration).
    let admitted = crate::governance::FinalityKeyRotationRecordV1 {
        authorized_action: GovernanceAuthorizedActionV1 {
            authorization: bootstrap.authorized_action.authorization.clone(),
            action: Box::new(Action::RotateGovernanceValidatorFinalityKeyV1 {
                controller_account_id: "controller".into(),
                body: FinalityKeyRotationBodyV1 {
                    schema_version: 1,
                    authorization: context,
                    rotation_id: "admitted".into(),
                    candidate_id: "candidate".into(),
                    node_id: "other".into(),
                    expected_generation: 0,
                    old_public_key_hex: public(4),
                    new_public_key_hex: public(5),
                    decision_height: 118,
                    activation_height: 120,
                    target_epoch: 1,
                },
                old_key_proof: FinalityKeyRotationProofV1 {
                    schema_version: 1,
                    role: FinalityKeyRotationRoleV1::OldKeyAuthorization,
                    signature_hex: "00".repeat(64),
                },
                new_key_proof: FinalityKeyRotationProofV1 {
                    schema_version: 1,
                    role: FinalityKeyRotationRoleV1::NewKeyPossession,
                    signature_hex: "00".repeat(64),
                },
            }),
        },
        applied_at_tick: 118,
    };
    let mut origin = admission("candidate", Status::Active, 0, 1);
    origin.node_id = "other".into();
    origin.finality_signer_public_key = public(4);
    world
        .state
        .governance_validator_admissions
        .insert("candidate".into(), origin);
    let parent = world.capture_finality_key_rotation_parent().unwrap();
    world
        .state
        .governance_finality_key_rotations
        .insert("admitted".into(), admitted.clone());
    assert!(
        world
            .validate_rotation_reservation(&parent, "admitted", &public(6))
            .is_err()
    );
    assert!(
        world
            .validate_rotation_reservation(&parent, "fresh", &public(4))
            .is_err()
    );
    assert!(
        world
            .validate_rotation_reservation(&parent, "fresh", &public(5))
            .is_err()
    );
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("bootstrap".into(), bootstrap.clone());
    assert!(
        world
            .validate_retained_rotation_namespace(&world.state.governance_validator_admissions)
            .is_ok()
    );
    let mut reused = bootstrap.clone();
    let Action::RotateGovernanceBootstrapFinalityKeyV1 { body, .. } =
        reused.authorized_action.action.as_mut()
    else {
        unreachable!()
    };
    body.new_public_key_hex = public(5);
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("bootstrap".into(), reused);
    assert!(
        world
            .validate_retained_rotation_namespace(&world.state.governance_validator_admissions)
            .is_err()
    );
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .clear();
    // Capacity is checked before key proofs or a provider call can occur.
    for i in 0..255 {
        world
            .state
            .governance_bootstrap_finality_key_rotations
            .insert(format!("reserved-{i}"), bootstrap.clone());
    }
    assert!(
        world
            .validate_rotation_reservation(&parent, "fresh", &public(6))
            .is_err()
    );
}

#[test]
fn later_admission_origins_do_not_retroactively_reserve_earlier_rotation_keys() {
    let mut world = world();
    let first = record(&world, "first", 0, 1, 2, 118);
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("first".into(), first);
    let second = record(&world, "second", 1, 2, 3, 238);
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("second".into(), second);
    // A later real authority record owns this node even if its original key is
    // the key that an earlier bootstrap generation legitimately installed.
    let mut takeover = admission("takeover", Status::Active, 2, 240);
    takeover.finality_signer_public_key = public(2);
    world
        .state
        .governance_validator_admissions
        .insert("takeover".into(), takeover);
    assert_eq!(
        effective(&world, 2).unwrap().signer_bindings["node"],
        public(2)
    );
    let parent = world.capture_finality_key_rotation_parent().unwrap();
    assert!(
        world
            .validate_rotation_reservation(&parent, "fresh", &public(2))
            .is_err()
    );

    // After revocation, a different node may lawfully use that original key
    // under the Admission contract. Its later arrival cannot erase old history.
    world.state.governance_validator_admissions.clear();
    let revoked = admission("revoked", Status::Revoked, 2, 240);
    world
        .state
        .governance_validator_admissions
        .insert("revoked".into(), revoked);
    let mut later = admission("later", Status::Active, 2, 241);
    later.node_id = "later-node".into();
    later.finality_signer_public_key = public(2);
    world
        .state
        .governance_validator_admissions
        .insert("later".into(), later);
    let registry = effective(&world, 2).unwrap();
    assert!(!registry.signer_bindings.contains_key("node"));
    assert_eq!(registry.signer_bindings["later-node"], public(2));
    world.state.time = 240;
    let parent = world.capture_finality_key_rotation_parent().unwrap();
    assert!(
        world
            .validate_rotation_reservation(&parent, "fresh", &public(2))
            .is_err()
    );
    assert_eq!(
        world
            .state
            .governance_bootstrap_finality_key_rotations
            .len(),
        2
    );
}

#[test]
fn later_admission_can_rotate_its_original_without_invalidating_earlier_bootstrap_history() {
    use oasis7_agent_api::world_action::{
        FinalityKeyRotationBodyV1, FinalityKeyRotationProofV1, FinalityKeyRotationRoleV1,
    };
    let mut world = world();
    let first = record(&world, "first", 0, 1, 2, 118);
    let mut context = first.body().unwrap().authorization.clone();
    world
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("first".into(), first);
    world.state.governance_validator_admissions.insert(
        "revoked".into(),
        admission("revoked", Status::Revoked, 1, 120),
    );
    let mut later = admission("later", Status::Active, 2, 240);
    later.node_id = "later-node".into();
    later.finality_signer_public_key = public(2);
    world
        .state
        .governance_validator_admissions
        .insert("later".into(), later);
    context.parent_position = 357;
    context.valid_from_position = 358;
    context.valid_through_position = 358;
    let body = FinalityKeyRotationBodyV1 {
        schema_version: 1,
        authorization: context.clone(),
        rotation_id: "later-rotation".into(),
        candidate_id: "later".into(),
        node_id: "later-node".into(),
        expected_generation: 0,
        old_public_key_hex: public(2),
        new_public_key_hex: public(4),
        decision_height: 358,
        activation_height: 360,
        target_epoch: 3,
    };
    let proof = |role, signer: u8| FinalityKeyRotationProofV1 {
        schema_version: 1,
        role,
        signature_hex: hex::encode(
            key(signer)
                .sign(&body.signing_bytes(role).unwrap())
                .to_bytes(),
        ),
    };
    let action = Action::RotateGovernanceValidatorFinalityKeyV1 {
        controller_account_id: "controller".into(),
        old_key_proof: proof(FinalityKeyRotationRoleV1::OldKeyAuthorization, 2),
        new_key_proof: proof(FinalityKeyRotationRoleV1::NewKeyPossession, 4),
        body,
    };
    context.signatures.insert(public(8), "00".repeat(64));
    world.state.governance_finality_key_rotations.insert(
        "later-rotation".into(),
        crate::governance::FinalityKeyRotationRecordV1 {
            authorized_action: GovernanceAuthorizedActionV1 {
                action: Box::new(action),
                authorization: context,
            },
            applied_at_tick: 358,
        },
    );
    assert_eq!(
        effective(&world, 2).unwrap().signer_bindings["later-node"],
        public(2)
    );
    let registry = effective(&world, 3).unwrap();
    assert_eq!(registry.signer_bindings["later-node"], public(4));
    assert!(!registry.signer_bindings.contains_key("node"));
    world.state.time = 360;
    let parent = world.capture_finality_key_rotation_parent().unwrap();
    // Origin K1 stays forbidden to fresh rotation even after its own successor
    // K2 activates; retained history accepts the lawful original chronology.
    assert!(
        world
            .validate_rotation_reservation(&parent, "fresh", &public(2))
            .is_err()
    );
}
