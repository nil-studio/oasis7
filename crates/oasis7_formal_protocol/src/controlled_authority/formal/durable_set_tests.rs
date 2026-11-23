use super::fixtures::*;
use super::*;

fn transition_fixture() -> (
    SignedNormalDurableSetTransitionV1,
    VerifiedAuthorityV1,
    VerifiedFormalCommitV1,
    VerifiedFormalCommitV1,
) {
    let (_, authority, initial_proof, _) = initial();
    let parent = verify_formal_qualified(&initial_proof, &authority, None, None).unwrap();
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
    successor.replica.instance.endpoint_id = "replacement-replica".into();
    successor.replica.fault_domain_id = "replacement-replica-domain".into();
    let mut body = NormalDurableSetTransitionBodyV1 {
        schema_version: 1,
        world: authority.definition().world.clone(),
        predecessor_authority_id: authority.authority_id().unwrap(),
        predecessor: parent.head(),
        boundary_position: 2,
        operation_id: h(123),
        successor,
        independent_closure_root: h(124),
    };
    body.independent_closure_root = body.copy_contract_digest().unwrap();
    body.validate(&authority).unwrap();
    let signed = SignedNormalDurableSetTransitionV1 {
        writer_signature_hex: sign(
            normal_durable_set_transition_signing_bytes(&body).unwrap(),
            4,
        ),
        body,
    };
    let freeze = frozen(&authority, &parent, None);
    let mut record = execution(&authority, &freeze);
    record.schema_version = 2;
    record.durable_set_transition_digest = Some(signed.body.digest().unwrap());
    let qualified = proof(
        record,
        &authority,
        FormalCompletionBranchV1::Normal,
        None,
        Some(freeze),
        4,
        5,
        6,
    );
    let boundary = verify_formal_qualified(&qualified, &authority, Some(&parent), None).unwrap();
    (signed, authority, parent, boundary)
}
#[test]
fn healthy_durable_set_transition_binds_exact_qualified_boundary_and_competing_prefix() {
    let (signed, authority, parent, boundary) = transition_fixture();
    let verified =
        verify_normal_durable_set_transition(&signed, &authority, &parent, &boundary).unwrap();
    assert_eq!(verified.boundary().head(), boundary.head());
    assert!(
        super::super::proof::verify_normal_parent_continuity(&authority, &boundary, 3).is_err()
    );
    let mut competing = signed.clone();
    competing.body.operation_id = h(125);
    competing.writer_signature_hex = sign(
        normal_durable_set_transition_signing_bytes(&competing.body).unwrap(),
        4,
    );
    assert_ne!(
        competing.body.digest().unwrap(),
        signed.body.digest().unwrap()
    );
    assert!(
        verify_normal_durable_set_transition(&competing, &authority, &parent, &boundary).is_err()
    );
    assert!(
        verify_normal_durable_set_transition(&signed, &authority, &boundary, &boundary).is_err()
    );
    for change in 0..5 {
        let mut bad = signed.clone();
        match change {
            0 => bad.body.successor.writer_public_key = public(10),
            1 => bad.body.successor.primary.instance.storage_id = h(126),
            2 => bad.body.successor.replica = authority.definition().replica.clone(),
            3 => bad.body.world.world_id.push_str("-fork"),
            _ => bad.body.successor.valid_from_position += 1,
        }
        assert!(bad.body.validate(&authority).is_err());
    }
}
#[test]
fn healthy_durable_set_record_version_is_strict_and_legacy_bytes_unchanged() {
    let (_, _, proof, _) = initial();
    let record = proof.decision.record;
    let encoded = serde_cbor::to_vec(&record).unwrap();
    let mut legacy_value = serde_cbor::value::to_value(&record).unwrap();
    if let serde_cbor::Value::Map(ref mut map) = legacy_value {
        assert!(
            map.remove(&serde_cbor::Value::Text(
                "durable_set_transition_digest".into()
            ))
            .is_none()
        );
    }
    let restored: CanonicalRecordV1 = serde_cbor::value::from_value(legacy_value).unwrap();
    assert_eq!(serde_cbor::to_vec(&restored).unwrap(), encoded);
    let mut invalid = record.clone();
    invalid.durable_set_transition_digest = Some(h(130));
    assert!(invalid.validate().is_err());
    invalid.schema_version = 2;
    assert!(invalid.validate().is_err()); // InitialActivation is never transition.
    let (_, _, _, boundary) = transition_fixture();
    let mut missing = boundary.record().clone();
    missing.durable_set_transition_digest = None;
    assert!(missing.validate().is_err());
}

#[test]
fn healthy_durable_set_activation_rejects_local_seal_and_unsigned_install_status() {
    use crate::controlled_authority::replicated_protocol::HeadAnchor;
    let (signed, authority, parent, boundary) = transition_fixture();
    let transition =
        verify_normal_durable_set_transition(&signed, &authority, &parent, &boundary).unwrap();
    let head = HeadAnchor {
        position: boundary.head().position,
        decision_hash: boundary.head().record_id,
        qualified: true,
    };
    let checkpoint = fencing::EndpointCheckpointBindingV1 {
        revision: 5,
        commitment: h(131),
        head,
    };
    let evidence = |instance: &fencing::EndpointInstanceBindingV1,
                    checkpoint: fencing::EndpointCheckpointBindingV1| {
        fencing::MonotonicProviderEvidenceV1 {
            statement: fencing::MonotonicProviderStatementV1 {
                schema_version: 1,
                capability: fencing::ProviderCapabilityV1::ControlledLocalTest,
                resource_binding: authority.policy().authorized_fencing_resource_bindings[0]
                    .clone(),
                instance_digest: instance.digest().unwrap(),
                checkpoint,
                epoch_floor: 2,
                seal_digest: None,
            },
            signature: "00".repeat(64),
        }
    };
    let seal = |instance: &fencing::EndpointInstanceBindingV1, signer| {
        fencing::EpochSealReceiptV1::sign(
            fencing::EpochSealBodyV1 {
                recovery_plan: None,
                writer_seal: None,
                schema_version: 1,
                instance: instance.clone(),
                recovery_operation: transition.digest().into(),
                highest_complete: checkpoint.head.clone(),
                checkpoint_before: checkpoint.clone(),
                tail: fencing::SealedTailV1::empty(),
                next_epoch_floor: 2,
            },
            evidence(instance, checkpoint.clone()),
            &key(signer),
        )
        .unwrap()
    };
    let primary_seal = seal(&authority.definition().primary.instance, 5);
    let replica_seal = seal(&authority.definition().replica.instance, 6);
    let install = |instance: &fencing::EndpointInstanceBindingV1| DurableSetInstallReceiptV1 {
        body: DurableSetInstallBodyV1 {
            schema_version: 1,
            transition_digest: transition.digest().into(),
            successor_authority_id: signed.body.successor.authority_id().unwrap(),
            instance: instance.clone(),
            boundary: boundary.head(),
            independent_closure_root: signed.body.independent_closure_root.clone(),
            independent_inventory_digest: h(141),
            checkpoint: checkpoint.clone(),
        },
        endpoint_signature_hex: "00".repeat(64),
        provider_signature_hex: "00".repeat(64),
        provider_evidence: evidence(instance, checkpoint.clone()),
    };
    let p = install(&signed.body.successor.primary.instance);
    let r = install(&signed.body.successor.replica.instance);
    assert!(
        verify_durable_set_activation(&transition, [&primary_seal, &replica_seal], [&p, &r])
            .is_err()
    );
    assert!(
        verify_durable_set_activation(&transition, [&primary_seal, &primary_seal], [&p, &p])
            .is_err()
    );
}

// Protocol signer fixture only: these test keys simulate the independently
// trusted provider contract, not a deployed nonexportable signer or native install.
fn signed_activation_fixture(
    transition: &VerifiedNormalDurableSetTransitionV1,
) -> (
    [fencing::EpochSealReceiptV1; 2],
    [DurableSetInstallReceiptV1; 2],
) {
    use crate::controlled_authority::replicated_protocol::HeadAnchor;
    let head = HeadAnchor {
        position: transition.boundary().head().position,
        decision_hash: transition.boundary().head().record_id,
        qualified: true,
    };
    let checkpoint = |revision| fencing::EndpointCheckpointBindingV1 {
        revision,
        commitment: h(revision as u8),
        head: head.clone(),
    };
    let evidence = |instance: &fencing::EndpointInstanceBindingV1, checkpoint, seal_digest| {
        let statement = fencing::MonotonicProviderStatementV1 {
            schema_version: 1,
            capability: fencing::ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
            resource_binding: transition.old.policy().authorized_fencing_resource_bindings[0]
                .clone(),
            instance_digest: instance.digest().unwrap(),
            checkpoint,
            epoch_floor: 2,
            seal_digest,
        };
        fencing::MonotonicProviderEvidenceV1 {
            signature: fencing::sign("oasis7.endpoint.monotonic-provider.v1", &statement, &key(3))
                .unwrap(),
            statement,
        }
    };
    let seal = |instance: &fencing::EndpointInstanceBindingV1, signer| {
        let body = fencing::EpochSealBodyV1 {
            recovery_plan: None,
            writer_seal: None,
            schema_version: 1,
            instance: instance.clone(),
            recovery_operation: transition.digest().into(),
            highest_complete: head.clone(),
            checkpoint_before: checkpoint(5),
            tail: fencing::SealedTailV1::empty(),
            next_epoch_floor: 2,
        };
        let provider = evidence(instance, checkpoint(6), Some(body.digest().unwrap()));
        let mut receipt = fencing::EpochSealReceiptV1::sign(body, provider, &key(signer)).unwrap();
        let request = fencing::ProtectedEndpointSigningRequestV1 {
            schema_version: 1,
            instance_digest: instance.digest().unwrap(),
            checkpoint: checkpoint(6),
            challenge: h(140),
            purpose: fencing::ProtectedEndpointSignaturePurposeV1::EpochSeal,
            preimage: fencing::epoch_seal_signing_bytes(&receipt.body).unwrap(),
        };
        let protected = fencing::ProtectedEndpointSignatureBodyV1 {
            schema_version: 1,
            custody: fencing::ProtectedSignerCustodyV1::NonexportableLeaseBoundV1,
            instance_digest: instance.digest().unwrap(),
            resource_binding: receipt.provider.statement.resource_binding.clone(),
            checkpoint: checkpoint(6),
            request_digest: request.digest().unwrap(),
            challenge: request.challenge,
            endpoint_signature: receipt.endpoint_signature.clone(),
        };
        receipt.protected_signature = Some(fencing::ProtectedEndpointSignatureV1 {
            provider_signature: sign(
                fencing::protected_endpoint_signature_signing_bytes(&protected).unwrap(),
                3,
            ),
            body: protected,
        });
        receipt
    };
    let install = |instance: &fencing::EndpointInstanceBindingV1, signer| {
        let body = DurableSetInstallBodyV1 {
            schema_version: 1,
            transition_digest: transition.digest().into(),
            successor_authority_id: transition.body().successor.authority_id().unwrap(),
            instance: instance.clone(),
            boundary: transition.boundary().head(),
            independent_closure_root: transition.body().independent_closure_root.clone(),
            independent_inventory_digest: h(141),
            checkpoint: checkpoint(7),
        };
        DurableSetInstallReceiptV1 {
            endpoint_signature_hex: sign(
                durable_set_install_endpoint_signing_bytes(&body).unwrap(),
                signer,
            ),
            provider_signature_hex: sign(
                durable_set_install_provider_signing_bytes(&body).unwrap(),
                3,
            ),
            provider_evidence: evidence(instance, body.checkpoint.clone(), None),
            body,
        }
    };
    (
        [
            seal(&transition.old.definition().primary.instance, 5),
            seal(&transition.old.definition().replica.instance, 6),
        ],
        [
            install(&transition.body().successor.primary.instance, 5),
            install(&transition.body().successor.replica.instance, 12),
        ],
    )
}
#[test]
fn healthy_durable_set_signed_activation_and_first_normal_commit() {
    let (signed, old, parent, boundary) = transition_fixture();
    let transition =
        verify_normal_durable_set_transition(&signed, &old, &parent, &boundary).unwrap();
    let (seals, installs) = signed_activation_fixture(&transition);
    let activation = verify_durable_set_activation(
        &transition,
        [&seals[0], &seals[1]],
        [&installs[0], &installs[1]],
    )
    .unwrap();
    let successor = activation.successor();
    assert_eq!(successor.activation_ref(), old.activation_ref());
    assert_eq!(
        successor.durable_set_activation_digest(),
        Some(activation.activation_digest())
    );
    let mut stripped = successor.clone();
    stripped.durable_set_activation_digest = None;
    assert!(super::super::proof::verify_normal_parent_continuity(&stripped, &boundary, 3).is_err());

    let mut freeze = frozen(successor, &boundary, None);
    // The generic recovery fixture uses new writer/P keys. Healthy replacement
    // retains original writer4/P5; sign the actual configured identities.
    freeze.reservation.signature_hex =
        sign(frozen_signing_bytes(&freeze.reservation.body).unwrap(), 4);
    freeze.primary.signature_hex = sign(
        frozen_receipt_signing_bytes(&freeze.primary.body).unwrap(),
        5,
    );
    freeze.replica.signature_hex = sign(
        frozen_receipt_signing_bytes(&freeze.replica.body).unwrap(),
        12,
    );
    verify_formal_reservation(&freeze.reservation, successor, &boundary).unwrap();
    let record = execution(successor, &freeze);
    let qualified = proof(
        record.clone(),
        successor,
        FormalCompletionBranchV1::Normal,
        None,
        Some(freeze.clone()),
        4,
        5,
        12,
    );
    let committed = verify_formal_qualified(&qualified, successor, Some(&boundary), None).unwrap();
    assert_eq!(committed.record(), &record);
    assert_eq!(committed.head().position, boundary.head().position + 1);
    assert!(verify_formal_qualified(&qualified, &old, Some(&boundary), None).is_err());
    assert!(verify_formal_qualified(&qualified, successor, Some(&parent), None).is_err());
    let mut wrong = installs.clone();
    wrong[1].body.successor_authority_id = old.authority_id().unwrap();
    assert!(
        verify_durable_set_activation(&transition, [&seals[0], &seals[1]], [&wrong[0], &wrong[1]])
            .is_err()
    );
    wrong = installs.clone();
    wrong[1].body.boundary = parent.head();
    assert!(
        verify_durable_set_activation(&transition, [&seals[0], &seals[1]], [&wrong[0], &wrong[1]])
            .is_err()
    );
    // A second independently signed proposal does not verify against the first
    // committed child as parent: no same-position append authorized by activation.
    let mut second_record = record;
    second_record.after_state_root = h(150);
    let second = proof(
        second_record,
        successor,
        FormalCompletionBranchV1::Normal,
        None,
        Some(freeze),
        4,
        5,
        12,
    );
    assert!(verify_formal_qualified(&second, successor, Some(&committed), None).is_err());
}

#[path = "durable_set_bootstrap_tests.rs"]
mod bootstrap_tests;
