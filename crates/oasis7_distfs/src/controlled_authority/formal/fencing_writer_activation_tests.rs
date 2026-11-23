//! A real TCP custody service consumes the original checked recovery token.
//! Endpoint signatures here are isolated crypto fixtures; the process recovery
//! matrix separately verifies their production P/R persistence and readback.
use super::*;
use crate::controlled_authority::formal::endpoint::{
    FormalCandidateMaterialV1, formal_candidate_material_digest,
};
use crate::controlled_authority::formal::fencing;
use crate::controlled_authority::replicated_protocol::HeadAnchor;

fn successor_permission(
    directory: &Path,
) -> (VerifiedRecovery, SignedRecoveryCertificateV1, ClosedRecord) {
    let (authority, cut, _, material) = material_fixture();
    let pending = RecoveryPendingHandoffV1::empty();
    let fixed = RecoveryTailWitnessV1::Empty;
    let checked = checked_plan(&authority, &cut, &fixed, &pending);
    let mut issuer = RecoveryIssuer::open(
        &directory.join("issuer"),
        authority.policy().clone(),
        fixture::key(1),
        IssuerAnchorV1::initial(authority.policy()).unwrap(),
    )
    .unwrap();
    let plan = issuer.prepare_plan(&checked).unwrap();
    let server = Server::start(initialize(&directory.join("old-writer"), &authority));
    let recovery = server.client(&authority, 18, 0);
    recovery
        .prepare_recovery_plan(&plan, &fixed, None, &pending, &authority)
        .unwrap();
    let qualified = fixture::proof(
        cut.record().clone(),
        &authority,
        FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    let drained = RecoveryDrainedProofV1 {
        schema_version: 1,
        plan_digest: plan.digest().unwrap(),
        fixed,
        qualified,
    };
    assert_eq!(drained.verify(&plan, &authority).unwrap(), cut.head());
    recovery
        .complete_drain(&plan, &drained, &authority)
        .unwrap();
    let writer_seal = recovery.seal_epoch(&plan, &drained, &authority).unwrap();
    writer_seal.verify(&plan, &authority).unwrap();
    let head = HeadAnchor {
        position: cut.head().position,
        decision_hash: cut.head().record_id,
        qualified: true,
    };
    let mut seals = Vec::new();
    for (binding, seed) in [
        (&authority.definition().primary, 5),
        (&authority.definition().replica, 6),
    ] {
        let receipt = fixture::signed_epoch_seal_receipt(
            fencing::EpochSealBodyV1 {
                schema_version: 2,
                instance: binding.instance.clone(),
                recovery_operation: plan.body.intent.operation_id.clone(),
                highest_complete: head.clone(),
                checkpoint_before: fencing::EndpointCheckpointBindingV1 {
                    revision: 9,
                    commitment: fixture::h(235),
                    head: head.clone(),
                },
                tail: fencing::SealedTailV1::empty(),
                next_epoch_floor: plan.body.intent.successor.epoch,
                recovery_plan: Some(plan.clone()),
                writer_seal: Some(writer_seal.clone()),
            },
            &fixture::key(seed),
            &fixture::key(3),
            binding.instance.storage_fingerprint.clone(),
        );
        seals.push(
            fencing::verify_epoch_seal(
                &receipt,
                &fencing::TrustedFencingPolicyV1 {
                    provider_public_key: authority.policy().fencing_provider_public_key.clone(),
                    authorized_resource_bindings: authority
                        .policy()
                        .authorized_fencing_resource_bindings
                        .clone(),
                },
            )
            .unwrap(),
        );
    }
    let mut body = plan.body.intent.clone();
    body.schema_version = 3;
    body.tail = RecoveryTailV1::Empty;
    body.fencing_evidence_digest = recovery_fencing_digest([&seals[0], &seals[1]]).unwrap();
    let intent = VerifiedRecoveryIntent::verify(
        body,
        &authority,
        &cut,
        [&seals[0], &seals[1]],
        &RecoveryTailWitnessV1::Empty,
    )
    .unwrap();
    let certificate = SignedRecoveryCertificateV1 {
        body: intent.body().clone(),
        signature_hex: fixture::sign(recovery_signing_bytes(intent.body()).unwrap(), 1),
    };
    let verified = fixture::verified_recovery(&intent, &authority);
    assert_eq!(verified.sealed_commit().record(), cut.record());
    assert_eq!(verified.body().schema_version, 3);
    (verified, certificate, material)
}

fn control_material(
    recovery: &VerifiedRecovery,
    certificate: &SignedRecoveryCertificateV1,
    mut material: ClosedRecord,
) -> FormalCandidateMaterialV1 {
    let authority = recovery.successor();
    let bytes = recovery_activation_input_bytes(certificate).unwrap();
    let input = blake3::hash(&bytes).to_hex().to_string();
    material.objects.push(ArtifactObject {
        content_hash: input.clone(),
        bytes,
        references: vec![material.roots[&ArtifactRole::Input].clone()],
    });
    material
        .objects
        .sort_by(|left, right| left.content_hash.cmp(&right.content_hash));
    material.roots.insert(ArtifactRole::Input, input.clone());
    material.payload_digest = input.clone();
    let mut record = recovery.sealed_commit().record().clone();
    let activation = authority.recovery_activation().unwrap();
    record.schema_version = 3;
    record.kind = FormalRecordKindV1::RecoveryActivation {
        certificate_digest: activation.certificate_digest.clone(),
        plan_digest: activation.plan_digest.clone(),
        pending_handoff_root: activation.pending_handoff_root.clone(),
    };
    record.position += 1;
    record.parent_record_id = recovery.sealed_commit().head().record_id;
    record.original_authority_id = authority.authority_id().unwrap();
    record.original_epoch = authority.definition().epoch;
    record.activation_ref = authority.activation_ref().into();
    record.request_digest = recovery_activation_request_digest(authority).unwrap();
    record.payload_digest = input;
    record.frozen_reservation_id = None;
    record.before_state_root = record.after_state_root.clone();
    record.artifact_roots = material.roots.clone();
    record.handoff = None;
    record.durable_set_transition_digest = None;
    verify_record_material(&record, &material).unwrap();
    FormalCandidateMaterialV1 {
        candidate: SignedCanonicalCandidateV1 {
            writer_signature_hex: fixture::sign(
                canonical_candidate_signing_bytes(&record).unwrap(),
                10,
            ),
            record,
        },
        material,
        frozen: None,
        frozen_material: None,
        branch: FormalCompletionBranchV1::Normal,
    }
}

fn successor_provider(
    path: &Path,
    recovery: &VerifiedRecovery,
    fresh: bool,
) -> ControlledFileWriterProvider {
    let open = if fresh {
        ControlledFileWriterProvider::initialize
    } else {
        ControlledFileWriterProvider::open_existing
    };
    open(
        path,
        recovery.successor().clone(),
        fixture::key(10),
        fixture::public(26),
        fixture::public(27),
        Some(recovery.clone()),
    )
    .unwrap()
}

fn endpoint_request(recovery: &VerifiedRecovery, command: Command) -> WriterEndpointRequestV1 {
    let authority = recovery.successor();
    WriterEndpointRequestV1::new(RequestBody {
        schema_version: 1,
        world: authority.definition().world.clone(),
        authority_id: authority.authority_id().unwrap(),
        instance_digest: authority.definition().primary.instance.digest().unwrap(),
        challenge: fixture::h(238),
        minimum: recovery.sealed_commit().head(),
        command,
        archive_read_grant: None,
    })
}

#[test]
fn writer_tcp_control_requires_original_dual_proof_and_cold_retry_keeps_one_candidate() {
    let directory = Directory::new();
    let (recovery, certificate, original_material) = successor_permission(&directory.0);
    let authority = recovery.successor();
    let material = control_material(&recovery, &certificate, original_material);
    let record = &material.candidate.record;
    let proof = fixture::proof(
        record.clone(),
        authority,
        FormalCompletionBranchV1::Normal,
        None,
        None,
        10,
        11,
        12,
    );
    let commit =
        verify_formal_qualified(&proof, authority, Some(recovery.sealed_commit()), None).unwrap();
    let next = fixture::frozen(authority, &commit, None).reservation.body;
    next.validate().unwrap();
    assert_eq!(
        Some(next.next_position),
        recovery.body().first_normal_position
    );
    let path = directory.0.join("successor-writer");
    let server = Server::start(successor_provider(&path, &recovery, true));
    let client = server.client(authority, 26, 0);
    assert!(client.sign_frozen(&next).is_err());
    assert_eq!(client.query().unwrap().revision, 0);

    lose_reply(
        server.address,
        &request(
            authority,
            26,
            0,
            WriterCustodyCommandV1::Candidate {
                record: Box::new(record.clone()),
                frozen: None,
            },
        ),
    );
    wait_revision(&client, 1);
    let signed_original = fs::read(path.join(LEDGER_FILE)).unwrap();
    assert!(client.sign_frozen(&next).is_err());
    drop(server);
    let server = Server::start(successor_provider(&path, &recovery, false));
    let client = server.client(authority, 26, 1);
    assert_eq!(
        client.sign_candidate(record, None).unwrap(),
        material.candidate.writer_signature_hex
    );
    assert_eq!(fs::read(path.join(LEDGER_FILE)).unwrap(), signed_original);
    let mut replacement = record.clone();
    replacement.result_root = fixture::h(239);
    replacement
        .artifact_roots
        .insert(ArtifactRole::Result, fixture::h(239));
    let alternative = SignedCanonicalCandidateV1 {
        writer_signature_hex: fixture::sign(
            canonical_candidate_signing_bytes(&replacement).unwrap(),
            10,
        ),
        record: replacement.clone(),
    };
    verify_formal_candidate(
        &alternative,
        None,
        authority,
        Some(recovery.sealed_commit()),
        None,
        FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    assert!(client.sign_candidate(&replacement, None).is_err());
    assert_eq!(fs::read(path.join(LEDGER_FILE)).unwrap(), signed_original);
    assert!(client.sign_frozen(&next).is_err());

    client
        .sign_endpoint_request(&endpoint_request(
            &recovery,
            Command::Prepare(Box::new(material.clone())),
        ))
        .unwrap();
    assert!(client.sign_frozen(&next).is_err());
    let digest = formal_candidate_material_digest(&material).unwrap();
    let prepare = |final_receipt: FormalFinalReceiptBodyV1, seed| {
        let body = FormalPrepareReceiptBodyV1 {
            final_receipt,
            candidate_material_digest: digest.clone(),
        };
        FormalPrepareReceiptV1 {
            signature_hex: fixture::sign(
                formal_prepare_receipt_signing_bytes(&body).unwrap(),
                seed,
            ),
            body,
        }
    };
    client
        .sign_endpoint_request(&endpoint_request(
            &recovery,
            Command::Decide(
                record.request_digest.clone(),
                Box::new(prepare(proof.decision.primary.body.clone(), 11)),
                Box::new(prepare(proof.decision.replica.body.clone(), 12)),
            ),
        ))
        .unwrap();
    client
        .sign_endpoint_request(&endpoint_request(
            &recovery,
            Command::InstallProof(Box::new(proof.decision.clone())),
        ))
        .unwrap();
    assert!(client.sign_frozen(&next).is_err());
    let before_complete = fs::read(path.join(LEDGER_FILE)).unwrap();
    let mut one_complete = proof.clone();
    one_complete.replica_complete.signature_hex = fixture::sign(
        formal_proof_complete_signing_bytes(&one_complete.replica_complete.body).unwrap(),
        11,
    );
    assert!(
        client
            .sign_endpoint_request(&endpoint_request(
                &recovery,
                Command::PublishQualified(Box::new(one_complete))
            ))
            .is_err()
    );
    assert!(client.sign_frozen(&next).is_err());
    assert_eq!(fs::read(path.join(LEDGER_FILE)).unwrap(), before_complete);

    let publication = endpoint_request(&recovery, Command::PublishQualified(Box::new(proof)));
    let revision = client.query().unwrap().revision;
    lose_reply(
        server.address,
        &request(
            authority,
            26,
            revision,
            WriterCustodyCommandV1::Endpoint(Box::new(publication.clone())),
        ),
    );
    wait_revision(&client, revision + 1);
    let activated = fs::read(path.join(LEDGER_FILE)).unwrap();
    drop(server);
    let server = Server::start(successor_provider(&path, &recovery, false));
    let client = server.client(authority, 26, revision + 1);
    client.sign_endpoint_request(&publication).unwrap();
    let signature = client.sign_frozen(&next).unwrap();
    crypto::verify(
        FROZEN_RESERVATION_SIGN_DOMAIN_V1,
        &next,
        &signature,
        &authority.definition().writer_public_key,
    )
    .unwrap();
    assert_eq!(fs::read(path.join(LEDGER_FILE)).unwrap(), activated);
    assert_eq!(
        client.sign_candidate(record, None).unwrap(),
        material.candidate.writer_signature_hex
    );
    assert!(client.sign_candidate(&replacement, None).is_err());
    assert_eq!(client.query().unwrap().revision, revision + 1);
}
