use super::*;
use crate::controlled_authority::formal::test_fixtures::*;
use crate::controlled_authority::replicated_protocol::{EndpointRole, HeadAnchor};

#[test]
fn final_provider_requires_actual_healthy_qualification_or_a_recovery_plan_and_preserves_old_seal_retry()
 {
    let (_, authority, parent_proof, parent) = initial_with_profile(2);
    let mut successor = authority.definition().clone();
    successor.epoch = 2;
    successor.predecessor_authority_id = Some(authority.authority_id().unwrap());
    successor.valid_from_position = 3;
    successor.primary.instance.authority_epoch = 2;
    successor.replica.instance.authority_epoch = 2;
    successor.replica.instance.endpoint_public_key = public(12);
    successor.replica.instance.instance_id = h(120);
    successor.replica.instance.storage_id = h(121);
    successor.replica.instance.storage_fingerprint = h(122);
    successor.replica.instance.endpoint_id = "healthy-new-replica".into();
    successor.replica.fault_domain_id = "healthy-new-domain".into();
    let mut transition = NormalDurableSetTransitionBodyV1 {
        schema_version: 1,
        world: authority.definition().world.clone(),
        predecessor_authority_id: authority.authority_id().unwrap(),
        predecessor: parent.head(),
        boundary_position: 2,
        operation_id: h(123),
        successor,
        independent_closure_root: h(124),
    };
    transition.independent_closure_root = transition.copy_contract_digest().unwrap();
    let signed = SignedNormalDurableSetTransitionV1 {
        writer_signature_hex: sign(
            normal_durable_set_transition_signing_bytes(&transition).unwrap(),
            4,
        ),
        body: transition,
    };
    let frozen = frozen(&authority, &parent, None);
    let mut record = execution(&authority, &frozen);
    record.schema_version = 2;
    record.durable_set_transition_digest = Some(signed.body.digest().unwrap());
    let boundary_proof = proof(
        record,
        &authority,
        FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    let boundary =
        verify_formal_qualified(&boundary_proof, &authority, Some(&parent), None).unwrap();
    let instance = authority.definition().primary.instance.clone();
    let native = proof::NativeHotBodyV1 {
        version: 1,
        instance_digest: instance.digest().unwrap(),
        authority_id: authority.authority_id().unwrap(),
        activation_ref: authority.activation_ref().into(),
        epoch: 1,
        revision: 5,
        head: boundary.head(),
        head_node: Some(h(125)),
        imported_head_node: None,
        import_digest: None,
        base_parent: None,
        index: Some(h(126)),
        pending: None,
        decision_head: None,
        seal: None,
        healthy_seal_authorization: None,
        recovery_plan: None,
        writer_plan_ack: None,
        recovery_progress: None,
        recovery_drained: None,
        recovery_handoff: None,
        durable_set: None,
        data_closure: None,
    };
    let checkpoint = fencing::EndpointCheckpointBindingV1 {
        revision: native.revision,
        commitment: h(127),
        head: HeadAnchor {
            position: native.head.position,
            decision_hash: native.head.record_id.clone(),
            qualified: true,
        },
    };
    let seal = fencing::EpochSealBodyV1 {
        schema_version: 1,
        instance,
        recovery_operation: signed.body.digest().unwrap(),
        highest_complete: checkpoint.head.clone(),
        checkpoint_before: checkpoint.clone(),
        tail: fencing::SealedTailV1::empty(),
        next_epoch_floor: 2,
        recovery_plan: None,
        writer_seal: None,
    };
    let mut next = native.clone();
    next.revision += 1;
    next.seal = Some(seal.clone());
    assert!(validate_recovery_checkpoint_transition(&native, &next, &authority).is_err());
    let request = |purpose: fencing::ProtectedEndpointSignaturePurposeV1, preimage| {
        fencing::ProtectedEndpointSigningRequestV1 {
            schema_version: 1,
            instance_digest: native.instance_digest.clone(),
            checkpoint: checkpoint.clone(),
            challenge: h(128),
            purpose,
            preimage,
        }
    };
    assert!(
        validate_recovery_protected_signing_request(
            &native,
            &authority,
            &request(
                fencing::ProtectedEndpointSignaturePurposeV1::NativeCheckpoint,
                bytes(
                    fencing::ProtectedEndpointSignaturePurposeV1::NativeCheckpoint.domain(),
                    &next
                )
                .unwrap()
            )
        )
        .is_err()
    );
    let healthy = HealthySealAuthorizationV1 {
        schema_version: 1,
        transition: signed,
        predecessor_proof: parent_proof,
        boundary_proof,
    };
    next.healthy_seal_authorization = Some(Box::new(healthy.clone()));
    validate_recovery_checkpoint_transition(&native, &next, &authority).unwrap();
    validate_recovery_protected_signing_request(
        &native,
        &authority,
        &request(
            fencing::ProtectedEndpointSignaturePurposeV1::NativeCheckpoint,
            bytes(
                fencing::ProtectedEndpointSignaturePurposeV1::NativeCheckpoint.domain(),
                &next,
            )
            .unwrap(),
        ),
    )
    .unwrap();
    for variant in 0..4 {
        let mut changed = next.clone();
        let authorization = changed.healthy_seal_authorization.as_mut().unwrap();
        match variant {
            0 => {
                authorization
                    .predecessor_proof
                    .replica_complete
                    .signature_hex = "00".repeat(64)
            }
            1 => authorization.boundary_proof.decision.writer_signature_hex = "00".repeat(64),
            2 => {
                authorization.transition.body.operation_id = h(129);
                authorization.transition.writer_signature_hex = sign(
                    normal_durable_set_transition_signing_bytes(&authorization.transition.body)
                        .unwrap(),
                    4,
                );
            }
            _ => changed.index = Some(h(130)),
        }
        assert!(
            validate_recovery_checkpoint_transition(&native, &changed, &authority).is_err(),
            "variant {variant}"
        );
    }
    // Existing legacy sealed bytes remain readable and their exact receipt may
    // be signed again. Neither a replacement seal nor a new mutation is allowed.
    let mut legacy = next;
    legacy.healthy_seal_authorization = None;
    let exact = request(
        fencing::ProtectedEndpointSignaturePurposeV1::EpochSeal,
        fencing::epoch_seal_signing_bytes(&seal).unwrap(),
    );
    validate_recovery_protected_signing_request(&legacy, &authority, &exact).unwrap();
    let mut changed_seal = seal;
    changed_seal.recovery_operation = h(131);
    assert!(
        validate_recovery_protected_signing_request(
            &legacy,
            &authority,
            &request(
                fencing::ProtectedEndpointSignaturePurposeV1::EpochSeal,
                fencing::epoch_seal_signing_bytes(&changed_seal).unwrap()
            )
        )
        .is_err()
    );
    assert!(validate_recovery_checkpoint_transition(&legacy, &legacy, &authority).is_err());
    assert_eq!(
        healthy.boundary_proof.decision.primary.body.role,
        EndpointRole::Primary
    );
}
