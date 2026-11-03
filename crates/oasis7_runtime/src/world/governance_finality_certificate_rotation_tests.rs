//! Distinct service identity and historically activated finality keys.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_agent_api::world_action::*;
fn public(key: &SigningKey) -> String {
    hex::encode(key.verifying_key().to_bytes())
}
fn fixture() -> (World, SigningKey, SigningKey) {
    let old = SigningKey::from_bytes(&[31; 32]);
    let new = SigningKey::from_bytes(&[32; 32]);
    let mut world = World::new();
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "rotation-certificate".into(),
            threshold: 1,
            threshold_bps: 10000,
            signer_bindings: BTreeMap::from([("node".into(), public(&old))]),
            validator_stakes: BTreeMap::from([("node".into(), 100)]),
        })
        .unwrap();
    let admission=serde_json::from_value(serde_json::json!({"candidate_id":"admission", "node_id":"node",
        "finality_signer_public_key":public(&old),"stake":100,"operator_owner":"owner",
        "public_manifest_hash":"manifest","requested_at_epoch":0,"activation_epoch":0,"status":"active"})).unwrap();
    world
        .state
        .governance_validator_admissions
        .insert("admission".into(), admission);
    let activation = 2 * world.governance_execution_policy().epoch_length_ticks;
    let decision = activation - 2;
    let authorization = GovernanceAuthorizationV1 {
        schema_version: 1,
        world_id: "world".into(),
        chain_id: "chain".into(),
        genesis_digest: "a".repeat(64),
        parent_position: decision - 1,
        parent_record_id: "b".repeat(64),
        parent_state_root: "c".repeat(64),
        policy_digest: "d".repeat(64),
        controller_account_id: "controller".into(),
        nonce: 1,
        valid_from_position: decision,
        valid_through_position: decision,
        signatures: BTreeMap::new(),
    };
    let body = FinalityKeyRotationBodyV1 {
        schema_version: 1,
        authorization: authorization.clone(),
        rotation_id: "rotation".into(),
        candidate_id: "admission".into(),
        node_id: "node".into(),
        expected_generation: 0,
        old_public_key_hex: public(&old),
        new_public_key_hex: public(&new),
        decision_height: decision,
        activation_height: activation,
        target_epoch: 2,
    };
    let proof = |key: &SigningKey, role| FinalityKeyRotationProofV1 {
        schema_version: 1,
        role,
        signature_hex: hex::encode(key.sign(&body.signing_bytes(role).unwrap()).to_bytes()),
    };
    let action = Action::RotateGovernanceValidatorFinalityKeyV1 {
        controller_account_id: "controller".into(),
        old_key_proof: proof(&old, FinalityKeyRotationRoleV1::OldKeyAuthorization),
        new_key_proof: proof(&new, FinalityKeyRotationRoleV1::NewKeyPossession),
        body,
    };
    let mut authorization = authorization;
    authorization
        .signatures
        .insert(public(&old), "00".repeat(64));
    world.state.governance_finality_key_rotations.insert(
        "rotation".into(),
        crate::governance::FinalityKeyRotationRecordV1 {
            authorized_action: GovernanceAuthorizedActionV1 {
                action: Box::new(action),
                authorization,
            },
            applied_at_tick: decision,
        },
    );
    (world, old, new)
}
fn certificate(world: &World, epoch: u64, key: &SigningKey) -> GovernanceFinalityCertificate {
    let snapshot = world.governance_finality_epoch_snapshot_for_epoch(epoch);
    let min_unique_signers = snapshot.effective_min_unique_signers();
    let mut c = GovernanceFinalityCertificate {
        proposal_id: 71,
        manifest_hash: "manifest".into(),
        consensus_height: 5,
        epoch_id: epoch,
        validator_set_hash: snapshot.validator_set_hash,
        stake_root: snapshot.stake_root,
        threshold_bps: snapshot.threshold_bps,
        min_unique_signers,
        threshold: snapshot.threshold,
        signatures: BTreeMap::new(),
    };
    let payload = GovernanceFinalityCertificate::signing_payload_v1(
        c.proposal_id,
        &c.manifest_hash,
        c.consensus_height,
        c.epoch_id,
        &c.validator_set_hash,
        &c.stake_root,
        c.threshold_bps,
        c.effective_min_unique_signers(),
        "node",
    );
    c.signatures.insert(
        "node".into(),
        format!(
            "{}{}",
            GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,
            hex::encode(key.sign(&payload).to_bytes())
        ),
    );
    c
}
#[test]
fn finality_certificate_uses_epoch_rotation_key_without_rebinding_service_identity() {
    let (world, old, new) = fixture();
    assert_eq!(
        world.node_identity_public_key("node"),
        Some(public(&old).as_str())
    );
    let before = certificate(&world, 1, &old);
    world
        .validate_governance_finality_certificate(71, "manifest", 1, &before)
        .unwrap();
    let active = certificate(&world, 2, &new);
    world.validate_governance_finality_certificate(71,"manifest",2,&active)
        .expect("activated finality key must authenticate certificate while service identity stays original");
    let retired = certificate(&world, 2, &old);
    assert!(
        world
            .validate_governance_finality_certificate(71, "manifest", 2, &retired)
            .is_err()
    );
    assert!(
        world
            .validate_governance_finality_certificate(
                71,
                "manifest",
                1,
                &certificate(&world, 1, &new)
            )
            .is_err()
    );
    assert_eq!(
        world.node_identity_public_key("node"),
        Some(public(&old).as_str())
    );
}

#[test]
fn historical_finality_certificates_survive_later_admission_revocation() {
    let (mut world, old, new) = fixture();
    let before = certificate(&world, 1, &old);
    let active = certificate(&world, 2, &new);
    let revoked_tick = 3 * world.governance_execution_policy().epoch_length_ticks;
    let admission = world
        .state
        .governance_validator_admissions
        .get_mut("admission")
        .unwrap();
    admission.status = crate::GovernanceValidatorAdmissionStatus::Revoked;
    admission.revoked_at_epoch = Some(3);
    admission.last_transition_tick = revoked_tick;
    world
        .validate_governance_finality_certificate(71, "manifest", 1, &before)
        .expect("later revocation cannot retroactively invalidate the original epoch key");
    world
        .validate_governance_finality_certificate(71, "manifest", 2, &active)
        .expect("later revocation cannot retroactively invalidate the activated rotation key");
    assert!(
        world
            .validate_governance_finality_certificate(
                71,
                "manifest",
                3,
                &certificate(&world, 3, &new)
            )
            .is_err()
    );
    assert!(
        world
            .validate_governance_finality_certificate(
                71,
                "manifest",
                2,
                &certificate(&world, 2, &old)
            )
            .is_err()
    );
    assert_eq!(
        world.node_identity_public_key("node"),
        Some(public(&old).as_str())
    );
}

#[test]
fn historical_finality_authority_rejects_missing_revocation_boundary_and_future_activation() {
    let (mut world, _, new) = fixture();
    let certificate = certificate(&world, 2, &new);
    let admission = world
        .state
        .governance_validator_admissions
        .get_mut("admission")
        .unwrap();
    admission.status = crate::GovernanceValidatorAdmissionStatus::Revoked;
    admission.revoked_at_epoch = None;
    assert!(
        world
            .validate_governance_finality_certificate(71, "manifest", 2, &certificate)
            .is_err()
    );
    let admission = world
        .state
        .governance_validator_admissions
        .get_mut("admission")
        .unwrap();
    admission.revoked_at_epoch = Some(5);
    admission.activation_epoch = Some(4);
    assert!(
        world
            .validate_governance_finality_certificate(71, "manifest", 2, &certificate)
            .is_err()
    );
}

#[test]
fn later_unqualified_readmission_cannot_shadow_historical_rotated_authority() {
    use crate::GovernanceValidatorAdmissionStatus as Status;
    for status in [Status::Applied, Status::ApprovedCandidate, Status::Active] {
        let (mut world, old, new) = fixture();
        let original = certificate(&world, 2, &new);
        let mut later = world.state.governance_validator_admissions["admission"].clone();
        later.candidate_id = "later".into();
        later.status = status;
        later.finality_signer_public_key = public(&SigningKey::from_bytes(&[33; 32]));
        later.requested_at_epoch = 3;
        later.approved_at_epoch = Some(3);
        later.activation_epoch = if status == Status::Active {
            Some(4)
        } else {
            None
        };
        later.last_transition_tick = 4 * world.governance_execution_policy().epoch_length_ticks;
        let revoked = world
            .state
            .governance_validator_admissions
            .get_mut("admission")
            .unwrap();
        revoked.status = Status::Revoked;
        revoked.revoked_at_epoch = Some(3);
        world
            .state
            .governance_validator_admissions
            .insert("later".into(), later.clone());
        world
            .validate_governance_finality_certificate(71, "manifest", 2, &original)
            .expect("later unqualified candidate cannot shadow historical A rotation");
        assert!(
            world
                .validate_governance_finality_certificate(
                    71,
                    "manifest",
                    2,
                    &certificate(&world, 2, &old)
                )
                .is_err()
        );
        assert!(
            world
                .resolve_governance_effective_finality_signer_registry_from_admissions_at_epoch(
                    &world.state.governance_validator_admissions,
                    3
                )
                .is_err(),
            "revoked A cannot be revived by later candidate"
        );
        let future = world
            .resolve_governance_effective_finality_signer_registry_from_admissions_at_epoch(
                &world.state.governance_validator_admissions,
                4,
            );
        if status == Status::Active {
            let registry = future.unwrap().unwrap();
            assert_eq!(
                registry.signer_bindings.get("node"),
                Some(&later.finality_signer_public_key)
            );
        } else {
            assert!(
                future.is_err(),
                "unapproved candidate cannot receive current authority"
            );
        }
        assert_eq!(
            world.node_identity_public_key("node"),
            Some(public(&old).as_str())
        );
    }
}
