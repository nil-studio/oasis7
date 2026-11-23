//! Historical schema1 images are explicit test fixtures, not new recovery
//! authority. Every subsequent open and plan CAS uses the production protected
//! endpoint and the same independently registered software provider floor.
use super::*;

fn successor(
    fixture: &Fixture,
    authority: &VerifiedAuthorityV1,
    cut: &VerifiedFormalCommitV1,
    epoch: u64,
    writer: u8,
    signers: [u8; 2],
) -> (AuthorityDefinitionV1, [PathBuf; 2]) {
    let directories = signers.map(|n| fixture.path.join(format!("legacy-cold-{epoch}-{n}")));
    let mut successor = authority.definition().clone();
    successor.epoch = epoch;
    successor.writer_public_key = public(writer);
    successor.predecessor_authority_id = Some(authority.authority_id().unwrap());
    successor.valid_from_position = cut.head().position + 1;
    for ((binding, directory), n) in [&mut successor.primary, &mut successor.replica]
        .into_iter()
        .zip(&directories)
        .zip(signers)
    {
        fs::create_dir(directory).unwrap();
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
        binding.instance.authority_epoch = epoch;
        binding.instance.endpoint_id = format!("legacy-cold-{epoch}-{n}");
        binding.instance.endpoint_public_key = public(n);
        binding.instance.instance_id = h(n + 20);
        binding.instance.storage_id = h(n + 30);
        binding.instance.storage_fingerprint = directory_fingerprint(directory).unwrap();
        binding.fault_domain_id = format!("legacy-cold-domain-{epoch}-{n}");
    }
    (successor, directories)
}

fn template(
    authority: &VerifiedAuthorityV1,
    cut: &VerifiedFormalCommitV1,
    successor: AuthorityDefinitionV1,
    operation: u8,
) -> RecoveryCertificateBodyV1 {
    RecoveryCertificateBodyV1 {
        schema_version: 1,
        retained_context: None,
        world: successor.world.clone(),
        operation_id: h(operation),
        predecessor_authority_id: authority.authority_id().unwrap(),
        predecessor_epoch: authority.definition().epoch,
        sealed_cut: cut.head(),
        tail: RecoveryTailV1::Empty,
        fencing_evidence_digest: h(0),
        first_normal_position: Some(successor.valid_from_position),
        successor,
    }
}

fn historical_archive(
    fixture: &Fixture,
    directory: &Path,
    role: EndpointRole,
    signer: u8,
    operation: &str,
) -> SignedFormalArchiveTransferV1 {
    // Reproduce the original schema1 serialization from a real Qualified
    // checkpoint. This deliberately does not call today's new-seal API.
    let original = fs::read(directory.join("formal-checkpoint-v1.cbor")).unwrap();
    let mut native = Store::verify_public(&original, &public(signer)).unwrap();
    assert!(native.pending.is_none());
    let before = Store::binding(&native, &original);
    let instance = match role {
        EndpointRole::Primary => fixture.authority.definition().primary.instance.clone(),
        EndpointRole::Replica => fixture.authority.definition().replica.instance.clone(),
    };
    let body = EpochSealBodyV1 {
        schema_version: 1,
        instance,
        recovery_operation: operation.into(),
        highest_complete: before.head.clone(),
        checkpoint_before: before,
        tail: SealedTailV1::empty(),
        next_epoch_floor: 2,
        recovery_plan: None,
        writer_seal: None,
    };
    native.revision += 1;
    native.seal = Some(body.clone());
    let checkpoint_bytes = Store::encode_provider_checkpoint(
        &native,
        &signed(Store::checkpoint_signing_bytes(&native).unwrap(), signer),
        &public(signer),
    )
    .unwrap();
    let checkpoint = Store::binding(&native, &checkpoint_bytes);
    let mut seal = oasis7_formal_protocol::controlled_authority::formal::test_fixtures::signed_epoch_seal_receipt(
        body,
        &key(signer),
        &key(3),
        fixture.authority.policy().authorized_fencing_resource_bindings[0].clone(),
    );
    // The generic signed-wire fixture uses a synthetic checkpoint commitment;
    // bind both independent attestations to these actual native image bytes.
    seal.provider.statement.checkpoint = checkpoint.clone();
    seal.provider.signature =
        sign_fencing_body(PROVIDER_DOMAIN, &seal.provider.statement, &key(3)).unwrap();
    let protected = seal.protected_signature.as_mut().unwrap();
    let request = ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: seal.body.instance.digest().unwrap(),
        checkpoint: checkpoint.clone(),
        challenge: protected.body.challenge.clone(),
        purpose: ProtectedEndpointSignaturePurposeV1::EpochSeal,
        preimage: epoch_seal_signing_bytes(&seal.body).unwrap(),
    };
    protected.body.checkpoint = checkpoint;
    protected.body.request_digest = request.digest().unwrap();
    protected.provider_signature = signed(
        protected_endpoint_signature_signing_bytes(&protected.body).unwrap(),
        3,
    );
    SignedFormalArchiveTransferV1 {
        checkpoint_bytes,
        seal,
    }
}

#[test]
fn legacy_profile2_native_owner_cold_reopens_the_exact_new_plan_and_pending_handoff() {
    let (fixture, initial_envelope) = super::super::super::business_tests::fixture();
    let mut original = fixture.coordinator();
    let initial_proof = original.submit(fixture.initial.clone()).unwrap();
    let initial_cut =
        verify_formal_qualified(&initial_proof, &fixture.authority, None, None).unwrap();
    drop(original);
    let (next, directories) =
        successor(&fixture, &fixture.authority, &initial_cut, 2, 10, [11, 12]);
    let mut legacy_body = template(&fixture.authority, &initial_cut, next, 231);
    let manifests = [
        historical_archive(
            &fixture,
            &fixture.pdir,
            EndpointRole::Primary,
            5,
            &legacy_body.operation_id,
        ),
        historical_archive(
            &fixture,
            &fixture.rdir,
            EndpointRole::Replica,
            6,
            &legacy_body.operation_id,
        ),
    ];
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: public(3),
        authorized_resource_bindings: fixture
            .authority
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let verdicts = manifests
        .each_ref()
        .map(|manifest| verify_epoch_seal(&manifest.seal, &policy).unwrap());
    legacy_body.fencing_evidence_digest =
        recovery_fencing_digest([&verdicts[0], &verdicts[1]]).unwrap();
    let intent = VerifiedRecoveryIntent::verify(
        legacy_body,
        &fixture.authority,
        &initial_cut,
        [&verdicts[0], &verdicts[1]],
        &RecoveryTailWitnessV1::Empty,
    )
    .unwrap();
    let recovery =
        oasis7_formal_protocol::controlled_authority::formal::test_fixtures::verified_recovery(
            &intent,
            &fixture.authority,
        );
    assert_eq!(recovery.body().schema_version, 1);
    let authority = recovery.successor();
    let transfers = [&fixture.pdir, &fixture.rdir]
        .into_iter()
        .enumerate()
        .map(|(i, directory)| {
            verify_formal_archive_transfer(
                &manifests[i],
                &fixture.authority,
                Arc::new(FileFormalArchiveSourceV1::new(directory.clone()).unwrap()),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let roles = [EndpointRole::Primary, EndpointRole::Replica];
    let historical = roles.map(|role| {
        let i = usize::from(role == EndpointRole::Replica);
        let instance = match role {
            EndpointRole::Primary => &authority.definition().primary.instance,
            EndpointRole::Replica => &authority.definition().replica.instance,
        };
        super::super::super::protected_signer::initial_hot_historical_fixture(
            authority,
            &fixture.authority,
            &Some(recovery.clone()),
            &Some(initial_cut.clone()),
            instance,
            Some(&transfers[i]),
        )
        .unwrap()
    });
    let oracles = roles.map(|role| {
        let i = usize::from(role == EndpointRole::Replica);
        Oracle::start_registered(
            authority.clone(),
            role,
            key(11 + i as u8),
            policy.authorized_resource_bindings[0].clone(),
            Some(historical[i].0.clone()),
        )
    });
    let open = |i: usize, credential| {
        FileFormalEndpoint::open_protected(
            &directories[i],
            FormalEndpointAuthorizationV1 {
                authority: authority.clone(),
                original: fixture.authority.clone(),
                recovery: Some(recovery.clone()),
                parent: Some(initial_cut.clone()),
            },
            roles[i],
            Arc::new(
                RemoteMonotonicEndpointProvider::new(
                    oracles[i].address,
                    Duration::from_secs(3),
                    key(credential),
                    policy.clone(),
                )
                .unwrap(),
            ),
            Some(&transfers[i]),
        )
    };
    for i in 0..2 {
        assert!(
            open(i, 17).is_err(),
            "a new legacy profile2 bootstrap must still be refused"
        );
        assert!(!directories[i].join("formal-checkpoint-v1.cbor").exists());
        // Only this explicit historical-fixture step creates old image bytes.
        // The actual provider independently checks the exact initial image;
        // none of the following production opens bypasses its floor.
        drop(
            Store::open(
                &directories[i],
                historical[i].0.clone(),
                &key(11 + i as u8),
                historical[i].1.clone(),
            )
            .unwrap(),
        );
    }
    let mut coordinator = FormalCoordinator::new(
        open(0, 17).unwrap(),
        open(1, 17).unwrap(),
        authority.clone(),
        fixture.authority.clone(),
        Some(recovery.clone()),
        Some(initial_cut.clone()),
    )
    .unwrap();
    let (mut frozen, material) = fixture.frozen(&initial_cut, 232);
    frozen.body.authority_id = authority.authority_id().unwrap();
    frozen.body.epoch = 2;
    frozen.body.activation_ref = authority.activation_ref().into();
    frozen.signature_hex = signed(frozen_signing_bytes(&frozen.body).unwrap(), 10);
    let executable = coordinator.freeze(frozen, material).unwrap();
    let mut candidate = fixture.candidate(&executable);
    candidate.candidate.record.original_epoch = 2;
    seam_tests::business_candidate(
        &fixture,
        &mut candidate,
        &initial_proof,
        &initial_envelope,
        &[&fixture.pdir],
        10,
    );
    let proof = coordinator.submit(candidate).unwrap();
    let cut = verify_formal_qualified(&proof, authority, Some(&initial_cut), None).unwrap();
    assert_eq!(cut.record().original_epoch, 2);
    assert_eq!(cut.head().position, initial_cut.head().position + 1);
    drop(coordinator);
    let (next, _) = successor(&fixture, authority, &cut, 3, 13, [14, 15]);
    let pending = RecoveryPendingHandoffV1::from_requests(vec![RecoveryPendingRequestV1 {
        identity: crate::controlled_authority::LocalRequestIdentity {
            verified_subject: "legacy-software-owner".into(),
            operation_domain: "original-authenticated-request".into(),
            nonce_scope: authority.authority_id().unwrap(),
            request_id: h(233),
        },
        // An opaque original wire fixture. These bytes receive no execution
        // authority from this storage test or from the recovery plan.
        signed_request: vec![0xff, 0x80, 0x01, 0x17],
    }])
    .unwrap();
    let plan = seam_tests::persist_plan(
        &fixture,
        authority,
        &cut,
        &RecoveryTailWitnessV1::Empty,
        &template(authority, &cut, next, 234),
        &pending,
        RecoveryPlanAuthorityV1 {
            recovery_credential_public_key: public(28),
            isolation_evidence_digest: h(235),
        },
    );
    // This test supplies a signed writer wire fixture. Actual writer custody
    // publication, draining and sealing are exercised by the process matrix.
    let ack = SignedWriterRecoveryPlanAckV1::sign(
        WriterRecoveryPlanAckBodyV1 {
            schema_version: 1,
            world: authority.definition().world.clone(),
            authority_id: authority.authority_id().unwrap(),
            authority_epoch: 2,
            operation_id: plan.body.intent.operation_id.clone(),
            plan_digest: plan.digest().unwrap(),
            pending_handoff_root: pending.root().unwrap(),
            revision: 1,
        },
        &key(10),
    )
    .unwrap();
    for i in 0..2 {
        let mut endpoint = open(i, 17).unwrap();
        let before = endpoint.checkpoint().unwrap();
        endpoint
            .prepare_recovery_plan(plan.clone(), pending.clone(), ack.clone(), &before)
            .unwrap();
        let planned = endpoint.checkpoint().unwrap();
        assert_eq!(planned.revision, before.revision + 1);
        assert_eq!(planned.head, before.head);
        drop(endpoint);
        let native_path = directories[i].join("formal-checkpoint-v1.cbor");
        let native_bytes = fs::read(&native_path).unwrap();
        let native = Store::verify_public(&native_bytes, &public(11 + i as u8)).unwrap();
        assert_eq!(native.recovery_plan.as_ref(), Some(&plan));
        assert_eq!(native.writer_plan_ack.as_ref(), Some(&ack));
        let object_path = directories[i]
            .join("formal-objects-v1")
            .join(native.recovery_handoff.as_ref().unwrap());
        let original_pending = fs::read(&object_path).unwrap();
        assert_eq!(
            serde_cbor::from_slice::<RecoveryPendingHandoffV1>(&original_pending).unwrap(),
            pending
        );
        let mut cold = open(i, 28).unwrap();
        assert_eq!(cold.checkpoint().unwrap(), planned);
        assert_eq!(cold.head().unwrap(), cut.head());
        assert_eq!(cold.recovery_pending_handoff().unwrap(), pending);
        assert_eq!(cold.recovery_drained_proof().unwrap().qualified, proof);
        cold.prepare_recovery_plan(plan.clone(), pending.clone(), ack.clone(), &planned)
            .unwrap();
        assert_eq!(cold.checkpoint().unwrap(), planned);
        let mut wrong_ack = ack.clone();
        wrong_ack.signature_hex = "00".repeat(64);
        assert!(
            cold.prepare_recovery_plan(plan.clone(), pending.clone(), wrong_ack, &planned)
                .is_err()
        );
        assert!(
            cold.prepare_recovery_plan(
                plan.clone(),
                RecoveryPendingHandoffV1::empty(),
                ack.clone(),
                &planned,
            )
            .is_err()
        );
        fs::remove_file(&object_path).unwrap();
        assert!(
            cold.prepare_recovery_plan(plan.clone(), pending.clone(), ack.clone(), &planned)
                .is_err(),
            "idempotent plan retry must still read the original private CAS object"
        );
        drop(cold);
        assert!(open(i, 28).is_err(), "missing pending cannot become empty");
        fs::write(&object_path, b"different private pending bytes").unwrap();
        fs::set_permissions(&object_path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            open(i, 28).is_err(),
            "a wrong CAS body cannot satisfy the root"
        );
        fs::write(&object_path, &original_pending).unwrap();
        fs::set_permissions(&object_path, fs::Permissions::from_mode(0o600)).unwrap();
        let mut repaired = open(i, 28).unwrap();
        assert_eq!(repaired.checkpoint().unwrap(), planned);
        assert_eq!(repaired.recovery_pending_handoff().unwrap(), pending);
        assert_eq!(repaired.recovery_drained_proof().unwrap().qualified, proof);
        assert_eq!(fs::read(native_path).unwrap(), native_bytes);
    }
}
