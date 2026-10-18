use crate::authority;
type WorldServiceProjection = oasis7_client_api::world_read_observation::WorldReadObservationV1;
use oasis7_client_api::formal_projection::{ProjectionAudienceV1, ProjectionCommitmentsV1};
type FormalReadVerifier = super::FormalReadVerifier<WorldServiceProjection>;
use super::*;
#[path = "protocol_fixtures.rs"]
pub(super) mod fixtures;
#[path = "protocol_response_fixture.rs"]
mod response_fixture;
use ed25519_dalek::Signer;
use fixtures::*;
use oasis7_formal_protocol::controlled_authority::{
    fencing::*,
    replicated_protocol::{ArtifactRole, EndpointRole, HeadAnchor},
};
pub(super) use response_fixture::response;
use serde::Serialize;

#[path = "formal_read_durable_set_tests.rs"]
mod durable_set_tests;
#[path = "formal_read_historical_v2_tests.rs"]
mod historical_v2_tests;
#[path = "formal_read_rebase_tests.rs"]
mod rebase_tests;

fn domain_bytes<T: Serialize>(domain: &str, value: &T) -> Vec<u8> {
    let mut bytes = (domain.len() as u64).to_be_bytes().to_vec();
    bytes.extend(domain.as_bytes());
    bytes.extend(serde_cbor::to_vec(value).unwrap());
    bytes
}

fn domain_sign<T: Serialize>(domain: &str, value: &T, key_id: u8) -> String {
    hex::encode(key(key_id).sign(&domain_bytes(domain, value)).to_bytes())
}

fn successor(
    predecessor: &VerifiedAuthorityV1,
    epoch: u64,
    writer: u8,
    primary: u8,
    replica: u8,
    first_position: u64,
) -> AuthorityDefinitionV1 {
    let mut next = predecessor.definition().clone();
    next.epoch = epoch;
    next.writer_public_key = public(writer);
    next.predecessor_authority_id = Some(predecessor.authority_id().unwrap());
    next.valid_from_position = first_position;
    for (binding, key_id, role) in [
        (&mut next.primary, primary, "primary"),
        (&mut next.replica, replica, "replica"),
    ] {
        binding.instance.authority_epoch = epoch;
        binding.instance.endpoint_id = format!("retained-{epoch}-{role}");
        binding.instance.endpoint_public_key = public(key_id);
        binding.instance.instance_id = h(key_id + 10);
        binding.instance.storage_id = h(key_id + 20);
        binding.instance.storage_fingerprint = h(key_id + 30);
        binding.fault_domain_id = format!("retained-domain-{epoch}-{role}");
    }
    next
}

fn recovery_tail(witness: &RecoveryTailWitnessV1) -> RecoveryTailV1 {
    match witness {
        RecoveryTailWitnessV1::Empty => RecoveryTailV1::Empty,
        RecoveryTailWitnessV1::Frozen(frozen) => RecoveryTailV1::Frozen {
            position: frozen.reservation.body.next_position,
            reservation_id: frozen.reservation.body.reservation_id().unwrap(),
            handoff: frozen.reservation.body.handoff.clone(),
        },
        _ => panic!("focused retained recovery fixture uses the original frozen tail"),
    }
}

fn seal_tail(witness: &RecoveryTailWitnessV1, predecessor: &VerifiedAuthorityV1) -> SealedTailV1 {
    match witness {
        RecoveryTailWitnessV1::Empty => SealedTailV1::empty(),
        RecoveryTailWitnessV1::Frozen(frozen) => SealedTailV1 {
            kind: SealedTailKindV1::Frozen,
            request: Some(
                oasis7_formal_protocol::controlled_authority::LocalRequestIdentity {
                    verified_subject: "test-agent".into(),
                    operation_domain: "formal-execution".into(),
                    nonce_scope: predecessor.authority_id().unwrap(),
                    request_id: frozen.reservation.body.request_digest.clone(),
                },
            ),
            position: Some(frozen.reservation.body.next_position),
            frozen_input_digest: Some(frozen.reservation.body.reservation_id().unwrap()),
            frozen_certificate_digest: None,
            proposal_digest: None,
            replica_prepare_digest: None,
            primary_receipt_digest: None,
            replica_receipt_digest: None,
        },
        _ => panic!("focused retained recovery fixture uses the original frozen tail"),
    }
}

fn epoch_seal(
    predecessor: &VerifiedAuthorityV1,
    cut: &VerifiedFormalCommitV1,
    operation_id: &str,
    next_epoch: u64,
    witness: &RecoveryTailWitnessV1,
    key_id: u8,
    role_index: usize,
) -> EpochSealReceiptV1 {
    let instance = match role_index {
        0 => predecessor.definition().primary.instance.clone(),
        _ => predecessor.definition().replica.instance.clone(),
    };
    let complete = HeadAnchor {
        position: cut.head().position,
        decision_hash: cut.head().record_id,
        qualified: true,
    };
    let checkpoint_before = EndpointCheckpointBindingV1 {
        revision: 4,
        commitment: h(120 + key_id),
        head: complete.clone(),
    };
    let body = EpochSealBodyV1 {
        schema_version: 1,
        instance: instance.clone(),
        recovery_operation: operation_id.to_owned(),
        highest_complete: complete.clone(),
        checkpoint_before: checkpoint_before.clone(),
        tail: seal_tail(witness, predecessor),
        next_epoch_floor: next_epoch,
    };
    let checkpoint = EndpointCheckpointBindingV1 {
        revision: checkpoint_before.revision + 1,
        commitment: h(130 + key_id),
        head: complete,
    };
    let resource = predecessor.policy().authorized_fencing_resource_bindings[0].clone();
    let statement = MonotonicProviderStatementV1 {
        schema_version: 1,
        capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
        resource_binding: resource.clone(),
        instance_digest: instance.digest().unwrap(),
        checkpoint: checkpoint.clone(),
        epoch_floor: next_epoch,
        seal_digest: Some(body.digest().unwrap()),
    };
    let provider = MonotonicProviderEvidenceV1 {
        signature: domain_sign("oasis7.endpoint.monotonic-provider.v1", &statement, 3),
        statement,
    };
    let mut receipt = EpochSealReceiptV1::sign(body.clone(), provider, &key(key_id)).unwrap();
    let request = ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: instance.digest().unwrap(),
        checkpoint,
        challenge: h(140 + key_id),
        purpose: ProtectedEndpointSignaturePurposeV1::EpochSeal,
        preimage: domain_bytes("oasis7.endpoint.epoch-seal.v1", &body),
    };
    let protected_body = ProtectedEndpointSignatureBodyV1 {
        schema_version: 1,
        custody: ProtectedSignerCustodyV1::NonexportableLeaseBoundV1,
        instance_digest: instance.digest().unwrap(),
        resource_binding: resource,
        checkpoint: request.checkpoint.clone(),
        request_digest: request.digest().unwrap(),
        challenge: request.challenge.clone(),
        endpoint_signature: receipt.endpoint_signature.clone(),
    };
    receipt.protected_signature = Some(ProtectedEndpointSignatureV1 {
        provider_signature: domain_sign(
            "oasis7.formal.protected_endpoint_signature.v1",
            &protected_body,
            3,
        ),
        body: protected_body,
    });
    receipt
}

#[expect(
    clippy::too_many_arguments,
    reason = "Recovery proof fixture keeps independently pinned predecessor, cut, authority, witness, and issuer inputs explicit."
)]
fn recovery_step(
    predecessor: &VerifiedAuthorityV1,
    cut: &VerifiedFormalCommitV1,
    next: AuthorityDefinitionV1,
    witness: RecoveryTailWitnessV1,
    context: Option<&VerifiedActiveRecoveryContextV2>,
    grant_revision: u64,
    minimum_anchor: issuer::IssuerAnchorV1,
    endpoint_signers: [u8; 2],
) -> (FormalRecoveryReadStepV1, issuer::IssuerAnchorV1) {
    let operation_id = h(160 + next.epoch as u8);
    let seals = [
        epoch_seal(
            predecessor,
            cut,
            &operation_id,
            next.epoch,
            &witness,
            endpoint_signers[0],
            0,
        ),
        epoch_seal(
            predecessor,
            cut,
            &operation_id,
            next.epoch,
            &witness,
            endpoint_signers[1],
            1,
        ),
    ];
    let fencing_policy = TrustedFencingPolicyV1 {
        provider_public_key: predecessor.policy().fencing_provider_public_key.clone(),
        authorized_resource_bindings: predecessor
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let primary = verify_epoch_seal(&seals[0], &fencing_policy).unwrap();
    let replica = verify_epoch_seal(&seals[1], &fencing_policy).unwrap();
    let body = RecoveryCertificateBodyV1 {
        schema_version: if context.is_some() { 2 } else { 1 },
        world: predecessor.definition().world.clone(),
        operation_id,
        predecessor_authority_id: predecessor.authority_id().unwrap(),
        predecessor_epoch: predecessor.definition().epoch,
        sealed_cut: cut.head(),
        tail: recovery_tail(&witness),
        fencing_evidence_digest: recovery_fencing_digest([&primary, &replica]).unwrap(),
        successor: next,
        first_normal_position: Some(
            cut.head().position
                + if matches!(witness, RecoveryTailWitnessV1::Empty) {
                    1
                } else {
                    2
                },
        ),
        retained_context: context.map(|context| context.commitment().clone()),
    };
    let certificate = SignedRecoveryCertificateV1 {
        signature_hex: sign(recovery_signing_bytes(&body).unwrap(), 1),
        body,
    };
    let policy_binding = hex::encode(
        blake3::hash(&domain_bytes(
            "oasis7.formal.issuer.policy.v1",
            predecessor.policy(),
        ))
        .as_bytes(),
    );
    let current_anchor = issuer::IssuerAnchorV1 {
        revision: grant_revision,
        digest: h(170 + grant_revision as u8),
    };
    let issuance = issuer::SignedIssuanceWitnessV1::sign(
        issuer::IssuanceWitnessBodyV1 {
            schema_version: 1,
            capability: issuer::IssuerWitnessCapabilityV1::NonrollbackUniqueSuccessorV1,
            request: issuer::IssuanceRequestBodyV1 {
                schema_version: 1,
                issuer_public_key: predecessor.policy().recovery_issuer_public_key.clone(),
                policy_binding,
                predecessor_authority_id: predecessor.authority_id().unwrap(),
                certificate_digest: recovery_certificate_digest(&certificate).unwrap(),
                observation_nonce: h(180 + grant_revision as u8),
                minimum_anchor,
            },
            grant_revision,
            current_anchor: current_anchor.clone(),
        },
        &key(2),
    )
    .unwrap();
    (
        FormalRecoveryReadStepV1 {
            certificate,
            issuance,
            fencing: FormalReadFencingEvidenceV1::HealthyPair(Box::new(seals)),
            original_tail: witness,
        },
        current_anchor,
    )
}
#[test]
fn formal_header_scope_and_proof_absence_fail_closed_without_cache_changes() {
    let (world, trust, anchor, _) = protocol_initial_fixture();
    let (request, response) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let mut verifier = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    let before = verifier.anchor_ref().unwrap();
    for edit in 0..9 {
        let mut value = response.clone();
        match edit {
            0 => value.formal_proof = None,
            1 => value.version.commit.binding.authority_generation += 1,
            2 => value.version.commit.binding.permission_generation += 1,
            3 => {
                value.execution_evidence_scope =
                    Some(crate::verified_world_view::CONTROLLED_PREREQUISITE_READ_SCOPE.into())
            }
            4 => {
                value
                    .formal_proof
                    .as_mut()
                    .unwrap()
                    .projection
                    .audience
                    .reader_public_key = Some(public(7))
            }
            5 => value.continuation.era = value.continuation.era.wrapping_add(1),
            6 => value.continuation.sequence = value.continuation.sequence.wrapping_add(1),
            7 => value.continuation.stream_id = "unproved-stream".into(),
            _ => value.continuation.commit.binding.permission_generation += 1,
        }
        assert!(verifier.verify_view(&value, &request, &public(7)).is_err());
        assert_eq!(verifier.anchor_ref().unwrap(), before);
    }
    let mut fixed = request.clone();
    fixed.fixed_commit = Some(response.version.commit.clone());
    verifier.verify_view(&response, &fixed, &public(7)).unwrap();
    assert_eq!(verifier.anchor_ref().unwrap(), before);
    fixed.fixed_commit.as_mut().unwrap().position += 1;
    assert!(
        verifier
            .fixed_anchor_ref(fixed.fixed_commit.as_ref().unwrap())
            .unwrap_err()
            .starts_with("HistoryUnavailable")
    );
    let excessive = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![
            FormalReadStepV1::Commit(Box::new(protocol_initial_fixture().3));
            MAX_FORMAL_HISTORY_STEPS + 1
        ],
    };
    assert!(excessive.encode_hex().is_err());
    let bytes = serde_cbor::to_vec(&excessive).unwrap();
    assert!(FormalReadHistoryV1::decode_hex(&hex::encode(bytes)).is_err());
}

#[test]
fn configured_checkpoint_cannot_be_replaced_by_another_higher_pinned_anchor() {
    use crate::remote_world_service::{FormalReadTrustConfigV1, WorldServiceClientConfig};
    type RemoteWorldServiceClient =
        crate::remote_world_service::RemoteWorldServiceReader<WorldServiceProjection>;
    let (world, trust, anchor, _) = protocol_initial_fixture();
    let checkpoint = anchor.checkpoint();
    let config = WorldServiceClientConfig {
        formal_read_trust: Some(FormalReadTrustConfigV1 {
            initial_trust: trust.clone(),
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint: checkpoint.clone(),
        }),
        endpoint: "http://127.0.0.1:9".into(),
        trusted_service_public_key: public(9),
        expected_world: commit_ref(&anchor, &projection(&world)).world,
        scope_id: "public".into(),
        read_private_key_hex: h(7),
        timeout: std::time::Duration::from_secs(1),
        max_response_bytes: 1_048_576,
    };
    // An independently pinned checkpoint is valid only under its own external
    // pin. Its height cannot prove continuity with the client's configured pin.
    let mut other = checkpoint;
    other.head_record.position += 1;
    other.head_record.kind = FormalRecordKindV1::Execution;
    other.head_record.parent_record_id = h(88);
    other.head_record.frozen_reservation_id = Some(h(89));
    let other_anchor =
        VerifiedFormalReadAnchorV1::from_fixed_pin(&other, &other.digest().unwrap(), &trust)
            .unwrap();
    assert!(RemoteWorldServiceClient::new_formal(config, trust, other_anchor).is_err());
    let mut cache = FormalReadVerifier::new(
        &InitialActivationTrustV1 {
            issuer_public_key: public(9),
            world: anchor.authority().definition().world.clone(),
            initial_state_root: anchor.initial_certificate().body.initial_state_root.clone(),
            governing_manifest_digest: anchor
                .initial_certificate()
                .body
                .governing_manifest_digest
                .clone(),
            recovery_policy_digest: anchor.authority().policy().digest().unwrap(),
        },
        anchor,
    )
    .unwrap();
    let current = cache.anchor_ref().unwrap();
    let mut sibling = current.clone();
    sibling.record_id = h(90);
    assert!(!cache.contains_verified_anchor(&sibling).unwrap());
    assert!(cache.contains_verified_anchor(&current).unwrap());
    let (request, reply) = response(
        &world,
        &cache.current,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    cache.verify_view(&reply, &request, &public(7)).unwrap();
    let mut minimum = request;
    minimum.min_commit = Some(reply.version.commit.clone());
    cache.verify_view(&reply, &minimum, &public(7)).unwrap();
    minimum
        .min_commit
        .as_mut()
        .unwrap()
        .binding
        .permission_generation += 1;
    assert!(cache.verify_view(&reply, &minimum, &public(7)).is_err());
}

#[test]
fn typed_qualified_successor_allows_bound_governance_and_permission_evolution() {
    use oasis7_client_api::formal_projection::projection_leaf;
    use oasis7_formal_protocol::controlled_authority::replicated_protocol::{
        ArtifactRole, EndpointRole,
    };
    let (world, trust, anchor, _) = protocol_initial_fixture();
    let mut verifier = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    let (mut request, initial) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    verifier
        .verify_view(&initial, &request, &public(7))
        .unwrap();
    // These test-authority commitments exercise the light consumer contract.
    // They do not claim to execute a real governance action or qualify custody.
    let mut view = initial.view.clone();
    view.runtime_binding.as_mut().unwrap().runtime_manifest_hash =
        serde_json::from_value(serde_json::json!(format!("blake3:{}", h(91)))).unwrap();
    view.formal_commit_header
        .as_mut()
        .unwrap()
        .permission_generation += 1;
    let audience = ProjectionAudienceV1::public();
    let tree = ProjectionCommitmentsV1::from_leaves(vec![(
        audience.clone(),
        projection_leaf(&audience, &view).unwrap(),
    )])
    .unwrap();
    let authority = anchor.authority();
    let body = FormalFrozenBodyV1 {
        schema_version: 1,
        world: authority.definition().world.clone(),
        authority_id: authority.authority_id().unwrap(),
        epoch: authority.definition().epoch,
        activation_ref: authority.activation_ref().into(),
        parent: anchor.head().head(),
        next_position: anchor.head().record().position + 1,
        request_digest: h(60),
        payload_digest: h(61),
        runtime_manifest_digest: h(62),
        governing_manifest_digest: anchor.head().record().governing_manifest_digest.clone(),
        execution_policy_digest: anchor.head().record().execution_policy_digest.clone(),
        pre_state_root: anchor.head().record().after_state_root.clone(),
        frozen_material_digest: h(64),
        handoff: None,
    };
    let receipt = |role, instance: &EndpointSignerBindingV1, key| {
        let receipt = FormalFrozenReceiptBodyV1 {
            authority_id: authority.authority_id().unwrap(),
            role,
            instance_digest: instance.instance.digest().unwrap(),
            reservation_id: body.reservation_id().unwrap(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: sign(frozen_receipt_signing_bytes(&receipt).unwrap(), key),
            body: receipt,
        }
    };
    let frozen = FormalFrozenCertificateV1 {
        primary: receipt(EndpointRole::Primary, &authority.definition().primary, 5),
        replica: receipt(EndpointRole::Replica, &authority.definition().replica, 6),
        reservation: SignedFormalFrozenV1 {
            signature_hex: sign(frozen_signing_bytes(&body).unwrap(), 4),
            body: body.clone(),
        },
    };
    let mut record = anchor.head().record().clone();
    let reservation_id = body.reservation_id().unwrap();
    record.kind = FormalRecordKindV1::Execution;
    record.position = body.next_position;
    record.parent_record_id = body.parent.record_id;
    record.request_digest = body.request_digest;
    record.payload_digest = body.payload_digest;
    record.frozen_reservation_id = Some(reservation_id);
    record.runtime_manifest_digest = h(62);
    record
        .artifact_roots
        .insert(ArtifactRole::ExecutionManifest, h(62));
    record.governing_manifest_digest = h(91);
    record.projection_root = tree.root().unwrap();
    let proof = qualified_proof(
        record,
        Some(frozen),
        authority,
        Some(anchor.head()),
        None,
        4,
        5,
        6,
    );
    let committed = verify_formal_qualified(&proof, authority, Some(anchor.head()), None).unwrap();
    let successor = anchor
        .after_verified_commit(authority.clone(), committed)
        .unwrap();
    let mut reply = initial.clone();
    reply.view = view;
    reply.version.commit = commit_ref(&successor, &reply.view);
    reply.version.projection_revision =
        authority::request_digest("projection", &reply.view).unwrap();
    reply.continuation.commit = reply.version.commit.clone();
    let inclusion = tree.inclusion(&audience).unwrap();
    let sidecar = reply.formal_proof.as_mut().unwrap();
    sidecar.history_cbor_hex = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![FormalReadStepV1::Commit(Box::new(proof))],
    }
    .encode_hex()
    .unwrap();
    sidecar.projection = FormalProjectionInclusionV1 {
        schema_version: inclusion.schema_version,
        audience: FormalProjectionAudienceV1 {
            scope_id: inclusion.audience.scope_id,
            reader_public_key: inclusion.audience.reader_public_key,
        },
        leaf_index: inclusion.leaf_index,
        leaf_count: inclusion.leaf_count,
        siblings: inclusion.siblings,
    };
    request.min_commit = Some(initial.version.commit.clone());
    assert!(
        reply.validate(&request).is_err(),
        "prototype binding equality remains strict"
    );
    let before = verifier.anchor_ref().unwrap();
    let mut fork = reply.clone();
    fork.version.commit.execution_block_hash = h(92);
    fork.continuation.commit = fork.version.commit.clone();
    assert!(verifier.verify_view(&fork, &request, &public(7)).is_err());
    let mut counter = reply.clone();
    counter.version.commit.binding.permission_generation += 1;
    counter.continuation.commit = counter.version.commit.clone();
    assert!(
        verifier
            .verify_view(&counter, &request, &public(7))
            .is_err()
    );
    assert_eq!(verifier.anchor_ref().unwrap(), before);
    verifier.verify_view(&reply, &request, &public(7)).unwrap();
    assert_eq!(
        verifier.anchor_ref().unwrap().record_id,
        reply.version.commit.execution_block_hash
    );
}

#[test]
fn formal_cursor_restore_failure_is_atomic_and_existing_projection_cache_cannot_rewind() {
    use crate::remote_world_service::{FormalReadTrustConfigV1, WorldServiceClientConfig};
    type RemoteWorldServiceClient =
        crate::remote_world_service::RemoteWorldServiceReader<WorldServiceProjection>;
    let (world, trust, anchor, _) = protocol_initial_fixture();
    let (request, reply) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let mut verifier = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    verifier.verify_view(&reply, &request, &public(7)).unwrap();
    let valid = verifier.export_cursor().unwrap();
    let checkpoint = anchor.checkpoint();
    let config = WorldServiceClientConfig {
        endpoint: "http://127.0.0.1:9".into(),
        trusted_service_public_key: public(9),
        expected_world: reply.version.commit.world.clone(),
        scope_id: "public".into(),
        read_private_key_hex: h(7),
        timeout: std::time::Duration::from_secs(1),
        max_response_bytes: 16 * 1024 * 1024,
        formal_read_trust: Some(FormalReadTrustConfigV1 {
            initial_trust: trust,
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        }),
    };
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let origin = client.formal_anchor_ref().unwrap();
    let mut invalid = valid.clone();
    invalid.frames.push(valid.frames[0].clone());
    assert!(client.restore_formal_cursor(&invalid).is_err());
    assert_eq!(client.formal_anchor_ref().unwrap(), origin);
    assert!(
        client.formal_cursor().is_err(),
        "no partially authenticated frame journal"
    );
    let mut invalid = valid.clone();
    invalid.commit.binding.permission_generation += 1;
    assert!(client.restore_formal_cursor(&invalid).is_err());
    assert_eq!(client.formal_anchor_ref().unwrap(), origin);
    assert!(client.formal_cursor().is_err());
    client.restore_formal_cursor(&valid).unwrap();
    let before =
        authority::request_digest("cursor-state-test", &client.formal_cursor().unwrap()).unwrap();
    assert!(
        client.restore_formal_cursor(&valid).is_err(),
        "restore is fresh-client-only even at equal height"
    );
    assert_eq!(
        authority::request_digest("cursor-state-test", &client.formal_cursor().unwrap()).unwrap(),
        before
    );
}

#[test]
fn retained_recovery_e3_completes_original_frozen_tail_from_external_e1_pin() {
    let (world, trust, anchor, initial_proof) = protocol_initial_fixture();
    let origin_checkpoint = anchor.checkpoint();
    let origin_digest = origin_checkpoint.digest().unwrap();
    let (request, mut reply) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let original = anchor.authority();
    let mut view = reply.view.clone();
    view.runtime_binding.as_mut().unwrap().runtime_manifest_hash =
        serde_json::from_value(serde_json::json!(format!("blake3:{}", h(91)))).unwrap();
    view.formal_commit_header
        .as_mut()
        .unwrap()
        .permission_generation += 1;
    let frozen_body = FormalFrozenBodyV1 {
        schema_version: 1,
        world: original.definition().world.clone(),
        authority_id: original.authority_id().unwrap(),
        epoch: original.definition().epoch,
        activation_ref: original.activation_ref().into(),
        parent: anchor.head().head(),
        next_position: anchor.head().record().position + 1,
        request_digest: h(60),
        payload_digest: h(61),
        runtime_manifest_digest: h(62),
        governing_manifest_digest: anchor.head().record().governing_manifest_digest.clone(),
        execution_policy_digest: anchor.head().record().execution_policy_digest.clone(),
        pre_state_root: anchor.head().record().after_state_root.clone(),
        frozen_material_digest: h(64),
        handoff: None,
    };
    let frozen_receipt = |role, binding: &EndpointSignerBindingV1, key_id| {
        let body = FormalFrozenReceiptBodyV1 {
            authority_id: original.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            reservation_id: frozen_body.reservation_id().unwrap(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: sign(frozen_receipt_signing_bytes(&body).unwrap(), key_id),
            body,
        }
    };
    let frozen = FormalFrozenCertificateV1 {
        reservation: SignedFormalFrozenV1 {
            signature_hex: sign(frozen_signing_bytes(&frozen_body).unwrap(), 4),
            body: frozen_body.clone(),
        },
        primary: frozen_receipt(EndpointRole::Primary, &original.definition().primary, 5),
        replica: frozen_receipt(EndpointRole::Replica, &original.definition().replica, 6),
    };
    let witness = RecoveryTailWitnessV1::Frozen(frozen.clone());
    let next2 = successor(original, 2, 13, 14, 15, 2);
    let minimum = issuer::IssuerAnchorV1::initial(original.policy()).unwrap();
    let (step2, issuer_anchor2) = recovery_step(
        original,
        anchor.head(),
        next2,
        witness.clone(),
        None,
        1,
        minimum,
        [5, 6],
    );
    let recovery2 = history::verify_recovery_step(&step2, &anchor, None)
        .unwrap()
        .recovery;
    let context3 = VerifiedActiveRecoveryContextV2::from_previous(
        &recovery2,
        original,
        anchor.head(),
        &witness,
    )
    .unwrap();
    let next3 = successor(recovery2.successor(), 3, 23, 24, 25, 2);
    let (step3, _) = recovery_step(
        recovery2.successor(),
        anchor.head(),
        next3,
        witness.clone(),
        Some(&context3),
        2,
        issuer_anchor2,
        [14, 15],
    );
    let recovery3 = history::verify_recovery_step(&step3, &anchor, Some(&recovery2))
        .unwrap()
        .recovery;

    let audience = ProjectionAudienceV1::public();
    let mut record = anchor.head().record().clone();
    record.kind = FormalRecordKindV1::Execution;
    record.position = frozen_body.next_position;
    record.parent_record_id = frozen_body.parent.record_id.clone();
    record.request_digest = frozen_body.request_digest.clone();
    record.payload_digest = frozen_body.payload_digest.clone();
    record.frozen_reservation_id = Some(frozen_body.reservation_id().unwrap());
    record.runtime_manifest_digest = frozen_body.runtime_manifest_digest.clone();
    record.governing_manifest_digest = h(91);
    record.before_state_root = frozen_body.pre_state_root.clone();
    record.after_state_root = h(65);
    record.action_root = h(66);
    record.event_root = h(67);
    record.result_root = h(68);
    record
        .artifact_roots
        .insert(ArtifactRole::ExecutionManifest, h(62));
    let tree = ProjectionCommitmentsV1::from_leaves(vec![(
        audience.clone(),
        oasis7_client_api::formal_projection::projection_leaf(&audience, &view).unwrap(),
    )])
    .unwrap();
    record.projection_root = tree.root().unwrap();
    let candidate = SignedCanonicalCandidateV1 {
        writer_signature_hex: sign(canonical_candidate_signing_bytes(&record).unwrap(), 23),
        record: record.clone(),
    };
    let candidate_context = verify_formal_candidate(
        &candidate,
        Some(&frozen),
        original,
        Some(anchor.head()),
        Some(&recovery3),
        FormalCompletionBranchV1::ReplayFrozen,
    )
    .unwrap();
    let final_receipt = |role, key_id| {
        let body = candidate_context.receipt_body(role).unwrap();
        FormalFinalReceiptV1 {
            signature_hex: sign(formal_final_receipt_signing_bytes(&body).unwrap(), key_id),
            body,
        }
    };
    let decision = FormalDecisionProofV1 {
        schema_version: 1,
        record: record.clone(),
        branch: FormalCompletionBranchV1::ReplayFrozen,
        writer_signature_hex: candidate.writer_signature_hex,
        frozen: Some(frozen.clone()),
        primary: final_receipt(EndpointRole::Primary, 24),
        replica: final_receipt(EndpointRole::Replica, 25),
    };
    let verified_decision =
        verify_formal_decision(&decision, original, Some(anchor.head()), Some(&recovery3)).unwrap();
    let complete_receipt = |role, key_id| {
        let body = verified_decision.complete_receipt_body(role).unwrap();
        FormalProofCompleteReceiptV1 {
            signature_hex: sign(formal_proof_complete_signing_bytes(&body).unwrap(), key_id),
            body,
        }
    };
    let completion = FormalQualifiedCommitProofV1 {
        schema_version: 1,
        decision,
        primary_complete: complete_receipt(EndpointRole::Primary, 24),
        replica_complete: complete_receipt(EndpointRole::Replica, 25),
    };

    let binding = view.runtime_binding.as_ref().unwrap();
    let header = view.formal_commit_header.as_ref().unwrap();
    let era = header.event_id_era;
    let sequence = header.journal_high_watermark;
    let commit = CommitRef {
        world: reply.version.commit.world.clone(),
        position: record.position,
        execution_block_hash: record.record_id().unwrap(),
        state_root_ref: record.after_state_root.clone(),
        binding: ExecutionBinding {
            provider_world_id: binding.world_id.clone(),
            branch_id: binding.branch_id.clone(),
            finality_ref: authority::request_digest(
                "finality",
                &(
                    binding.finality_epoch,
                    &binding.finality_status,
                    &binding.finality_block_hash,
                ),
            )
            .unwrap(),
            reorg_generation: binding.reorg_epoch,
            governing_manifest_ref: binding.runtime_manifest_hash.to_string(),
            authority_generation: header.authority_generation,
            permission_generation: header.permission_generation,
        },
    };
    reply.view = view;
    reply.logical_tick = reply.view.state.time;
    reply.version.commit = commit.clone();
    reply.version.projection_revision =
        authority::request_digest("projection", &reply.view).unwrap();
    reply.continuation.commit = commit.clone();
    reply.continuation.era = era;
    reply.continuation.sequence = sequence;
    let inclusion = tree.inclusion(&audience).unwrap();
    let sidecar = reply.formal_proof.as_mut().unwrap();
    sidecar.history_cbor_hex = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![
            FormalReadStepV1::Recovery(Box::new(step2.clone())),
            FormalReadStepV1::Recovery(Box::new(step3.clone())),
            FormalReadStepV1::Commit(Box::new(completion.clone())),
        ],
    }
    .encode_hex()
    .unwrap();
    sidecar.projection = FormalProjectionInclusionV1 {
        schema_version: inclusion.schema_version,
        audience: FormalProjectionAudienceV1 {
            scope_id: inclusion.audience.scope_id,
            reader_public_key: inclusion.audience.reader_public_key,
        },
        leaf_index: inclusion.leaf_index,
        leaf_count: inclusion.leaf_count,
        siblings: inclusion.siblings,
    };

    let mut without_previous = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    let before = without_previous.anchor_ref().unwrap();
    let mut skipped = reply.clone();
    skipped.formal_proof.as_mut().unwrap().history_cbor_hex = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![FormalReadStepV1::Recovery(Box::new(step3.clone()))],
    }
    .encode_hex()
    .unwrap();
    assert!(
        without_previous
            .verify_view(&skipped, &request, &public(7))
            .unwrap_err()
            .starts_with("HistoryUnavailable")
    );
    assert_eq!(without_previous.anchor_ref().unwrap(), before);

    let mut verifier = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    let observed = verifier.verify_view(&reply, &request, &public(7)).unwrap();
    assert_eq!(observed.commit().record().position, 2);
    assert_eq!(
        observed.commit().finalizing_authority_id(),
        recovery3.successor().authority_id().unwrap()
    );
    let completed_history =
        FormalReadHistoryV1::decode_hex(&reply.formal_proof.as_ref().unwrap().history_cbor_hex)
            .unwrap();
    let advanced = verifier.anchor_ref().unwrap();
    assert_eq!(advanced.position, 2);
    assert!(verifier.checkpoint().is_err());
    let checkpoint_v2 = verifier.checkpoint_v2(&origin_digest).unwrap();
    let pinned_v2_digest = checkpoint_v2.digest().unwrap();
    let decoded = FormalReadCheckpointV2::decode_hex(&checkpoint_v2.encode_hex().unwrap()).unwrap();
    let restored = FormalReadVerifier::from_fixed_checkpoint_v2(
        &trust,
        &decoded,
        &pinned_v2_digest,
        &origin_digest,
    )
    .unwrap();
    assert_eq!(restored.anchor_ref().unwrap(), advanced);
    assert_eq!(
        restored.qualified_commits.len(),
        verifier.qualified_commits.len()
    );
    assert_eq!(restored.verified_authorities.len(), 3);
    let mut tampered = decoded.clone();
    tampered.current.head_record.result_root = h(97);
    assert!(
        FormalReadVerifier::from_fixed_checkpoint_v2(
            &trust,
            &tampered,
            &pinned_v2_digest,
            &origin_digest,
        )
        .is_err()
    );

    let (origin_request, mut origin_reply) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let origin_history = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![
            FormalReadStepV1::Recovery(Box::new(step2)),
            FormalReadStepV1::Recovery(Box::new(step3)),
        ],
    };
    origin_reply.formal_proof.as_mut().unwrap().history_cbor_hex =
        origin_history.encode_hex().unwrap();
    let mut schema2_origin_reader = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    schema2_origin_reader
        .verify_view(&origin_reply, &origin_request, &public(7))
        .unwrap();
    let schema2_origin = schema2_origin_reader.current.checkpoint();
    // A schema-2 active E3 checkpoint cannot be promoted by the legacy V1
    // import path; the V2 origin qualification below carries its E1→E2→E3
    // verified recovery lineage instead.
    assert!(
        VerifiedFormalReadAnchorV1::from_fixed_pin(
            &schema2_origin,
            &schema2_origin.digest().unwrap(),
            &trust,
        )
        .is_err()
    );
    let origin_proof_digest = formal_qualified_proof_digest(&initial_proof).unwrap();
    let origin_qualification = FormalReadOriginQualificationV2 {
        base: origin_checkpoint.clone(),
        base_digest: origin_digest.clone(),
        history: origin_history,
        origin_record: anchor.head().record().clone(),
        finalizing_authority_id: anchor.head().finalizing_authority_id().to_owned(),
        qualified_proof_digest: origin_proof_digest.clone(),
        qualified_proof: initial_proof,
    };
    let schema2_origin_digest = schema2_origin.digest().unwrap();
    let rebased = FormalReadCheckpointV2 {
        schema_version: 2,
        origin: schema2_origin.clone(),
        origin_digest: schema2_origin_digest.clone(),
        origin_qualification: Some(origin_qualification),
        history: FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
        current: schema2_origin,
    };
    let rebased_digest = rebased.digest().unwrap();
    let rebased_reader = FormalReadVerifier::from_fixed_checkpoint_v2(
        &trust,
        &rebased,
        &rebased_digest,
        &schema2_origin_digest,
    )
    .unwrap();
    assert_eq!(
        rebased_reader.anchor_ref().unwrap().active_authority_id,
        recovery3.successor().authority_id().unwrap()
    );
    let initial_qualification_key = QualifiedCommitKey {
        position: anchor.head().record().position,
        record_id: anchor.head().head().record_id.clone(),
        proof_digest: origin_proof_digest,
    };
    assert!(
        rebased_reader
            .qualified_commits
            .contains_key(&initial_qualification_key)
    );
    assert_eq!(rebased_reader.verified_authorities.len(), 3);

    let completed_origin = verifier.current.checkpoint();
    let completed_origin_digest = completed_origin.digest().unwrap();
    let replayed_non_normal_proof_digest = formal_qualified_proof_digest(&completion).unwrap();
    let replayed_non_normal_qualification = FormalReadOriginQualificationV2 {
        base: origin_checkpoint.clone(),
        base_digest: origin_digest.clone(),
        history: completed_history,
        origin_record: observed.commit().record().clone(),
        finalizing_authority_id: observed.commit().finalizing_authority_id().to_owned(),
        qualified_proof_digest: replayed_non_normal_proof_digest.clone(),
        qualified_proof: completion.clone(),
    };
    let replayed_non_normal_checkpoint = FormalReadCheckpointV2 {
        schema_version: 2,
        origin: completed_origin.clone(),
        origin_digest: completed_origin_digest.clone(),
        origin_qualification: Some(replayed_non_normal_qualification),
        history: FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
        current: completed_origin,
    };
    let replayed_non_normal_digest = replayed_non_normal_checkpoint.digest().unwrap();
    let replayed_non_normal_reader = FormalReadVerifier::from_fixed_checkpoint_v2(
        &trust,
        &replayed_non_normal_checkpoint,
        &replayed_non_normal_digest,
        &completed_origin_digest,
    )
    .unwrap();
    let replayed_non_normal_key = QualifiedCommitKey {
        position: observed.commit().record().position,
        record_id: observed.commit().head().record_id.clone(),
        proof_digest: replayed_non_normal_proof_digest,
    };
    let replayed_non_normal_token = replayed_non_normal_reader
        .qualified_commits
        .get(&replayed_non_normal_key)
        .expect("origin replay retains exact non-Normal proof context");
    assert!(replayed_non_normal_token.recovery.is_some());
    assert!(replayed_non_normal_token.retained_context.is_some());

    let without_origin_qualification = FormalReadCheckpointV2 {
        origin_qualification: None,
        ..rebased
    };
    let without_origin_digest = without_origin_qualification.digest().unwrap();
    assert!(
        FormalReadVerifier::from_fixed_checkpoint_v2(
            &trust,
            &without_origin_qualification,
            &without_origin_digest,
            &schema2_origin_digest,
        )
        .is_err()
    );
    assert!(verifier.verify_view(&reply, &request, &public(7)).is_err());
    assert_eq!(verifier.anchor_ref().unwrap(), advanced);
}

impl super::FormalReadProjection for WorldServiceProjection {
    fn validate_contents(
        response: &ReadWorldViewResponse<Self>,
        request: &ReadWorldViewRequest,
    ) -> Result<(), String> {
        response
            .validate(request)
            .map_err(|error| error.to_string())?;
        response
            .view
            .validate_bound_contents(&response.version, response.logical_tick, request)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
struct NoopSchemaProjection(WorldServiceProjection);
impl super::FormalReadProjection for NoopSchemaProjection {
    fn validate_contents(
        _: &ReadWorldViewResponse<Self>,
        _: &ReadWorldViewRequest,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn generic_formal_verifier_owns_envelope_checks_even_with_noop_schema_validator() {
    let (world, trust, anchor, _) = protocol_initial_fixture();
    let (request, original) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let reply: ReadWorldViewResponse<NoopSchemaProjection> =
        serde_json::from_value(serde_json::to_value(original).unwrap()).unwrap();
    super::FormalReadVerifier::<NoopSchemaProjection>::new(&trust, anchor.clone())
        .unwrap()
        .verify_view(&reply, &request, &public(7))
        .unwrap();

    let mut wrong_world = request.clone();
    wrong_world.world.world_id = "different-world".into();
    let mut wrong_scope = reply.clone();
    wrong_scope.version.visibility_scope = "agent:other".into();
    let mut wrong_version = reply.clone();
    wrong_version.contract_version = WORLD_SERVICE_CONTRACT_VERSION + 1;
    let mut accepted = Vec::new();
    for (name, request, reply) in [
        ("request world", wrong_world, reply.clone()),
        ("response visibility scope", request.clone(), wrong_scope),
        ("response contract version", request.clone(), wrong_version),
    ] {
        let mut verifier =
            super::FormalReadVerifier::<NoopSchemaProjection>::new(&trust, anchor.clone()).unwrap();
        if verifier.verify_view(&reply, &request, &public(7)).is_ok() {
            accepted.push(name);
        }
    }
    assert!(
        accepted.is_empty(),
        "generic verifier accepted mismatched envelope: {accepted:?}"
    );
}
