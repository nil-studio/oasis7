//! CA-B1: committed controlled H remains closed before the first BFT commit.
use super::*;

pub(super) fn run() {
    let binary = std::env::var_os("OASIS7_FORMAL_ENDPOINT_TEST_BIN").unwrap();
    struct HandoffIndex(Vec<BusinessIndexNodeWriteV1>);
    impl BusinessIndexNodeReader for HandoffIndex {
        fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self
                .0
                .iter()
                .find(|node| node.digest == digest)
                .map(|node| node.bytes.clone()))
        }
    }

    let (fixture, initial_envelope) = super::super::super::business_tests::fixture();
    let primary_oracle = Oracle::start(&fixture, EndpointRole::Primary);
    let replica_oracle = Oracle::start(&fixture, EndpointRole::Replica);
    let authfile = fixture.path.join("oracle-auth");
    let (_, _, configs) = original_configs(&fixture, &primary_oracle, &replica_oracle, &authfile);
    let (mut primary, primary_address) = start(&binary, &configs[0]);
    let (mut replica, replica_address) = start(&binary, &configs[1]);
    let writer = seam_tests::WriterProcess::start(&fixture, &binary, &fixture.authority, 4, None);
    let genesis = fixture.authority.definition().world.genesis_head().unwrap();
    let mut coordinator = FormalCoordinator::new(
        writer.normal_endpoint(primary_address, EndpointRole::Primary, genesis.clone()),
        writer.normal_endpoint(replica_address, EndpointRole::Replica, genesis),
        fixture.authority.clone(),
        fixture.authority.clone(),
        None,
        None,
    )
    .unwrap();
    let initial_proof = coordinator.submit(fixture.initial.clone()).unwrap();
    let initial = verify_formal_qualified(&initial_proof, &fixture.authority, None, None).unwrap();
    let (mut input, material) = fixture.frozen(&initial, 44);
    let handoff = ProfileHandoffV1 {
        next_profile: "bft".into(),
        next_profile_version: 1,
        next_authority_digest: h(79),
        valid_from_position: 3,
    };
    input.body.handoff = Some(handoff.clone());
    input.signature_hex = writer.normal.sign_frozen(&input.body).unwrap();
    let executable = coordinator.freeze(input, material).unwrap();
    let original_request = executable.input().body.request_digest.clone();
    let mut candidate = fixture.candidate(&executable);
    let mut updates = vec![
        BusinessIndexLeafV1 {
            key_digest: metadata_key_digest(),
            kind: BusinessIndexLeafKindV1::CommitMetadata,
            canonical_value: b"committed handoff".to_vec(),
        },
        BusinessIndexLeafV1 {
            key_digest: original_request.clone(),
            kind: BusinessIndexLeafKindV1::Outcome,
            canonical_value: serde_cbor::to_vec(&(44u8, 2u64)).unwrap(),
        },
        FormalHistoricalCutV1::from_proof(&initial_proof)
            .unwrap()
            .leaf()
            .unwrap(),
    ];
    updates.sort_by(|a, b| a.key_digest.cmp(&b.key_digest));
    let delta = BusinessIndexDeltaV1::build(
        initial_envelope.delta.new,
        updates,
        &HandoffIndex(initial_envelope.delta.node_writes),
    )
    .unwrap();
    let envelope = super::super::super::business_tests::envelope(delta);
    super::super::super::business_tests::replace_result(
        &mut candidate.material,
        envelope.encode().unwrap(),
    );
    let record = &mut candidate.candidate.record;
    record.handoff = Some(handoff.clone());
    record.artifact_roots = candidate.material.roots.clone();
    record.result_root = envelope.delta.new_root;
    candidate.candidate.writer_signature_hex = writer
        .normal
        .sign_candidate(record, candidate.frozen.as_ref())
        .unwrap();
    let committed = coordinator.submit(candidate).unwrap();
    let cut =
        verify_formal_qualified(&committed, &fixture.authority, Some(&initial), None).unwrap();
    assert_eq!(cut.record().handoff.as_ref(), Some(&handoff));
    let (fresh, _) = fixture.frozen(&cut, 45);
    assert!(writer.normal.sign_frozen(&fresh.body).is_err());
    let pending = RecoveryPendingHandoffV1::empty();
    let forbidden_plan = plan_after_handoff(&fixture, &cut, &pending);
    // This authentic old writer signature is a negative input only. The
    // production writer service must never issue an acknowledgement for H.
    let forbidden_ack = SignedWriterRecoveryPlanAckV1::sign(
        WriterRecoveryPlanAckBodyV1 {
            schema_version: 1,
            world: fixture.authority.definition().world.clone(),
            authority_id: fixture.authority.authority_id().unwrap(),
            authority_epoch: fixture.authority.definition().epoch,
            operation_id: forbidden_plan.body.intent.operation_id.clone(),
            plan_digest: forbidden_plan.digest().unwrap(),
            pending_handoff_root: pending.root().unwrap(),
            revision: 1,
        },
        &key(4),
    )
    .unwrap();
    drop(coordinator);
    primary.kill().unwrap();
    primary.wait().unwrap();
    replica.kill().unwrap();
    replica.wait().unwrap();

    for (config, role) in configs
        .iter()
        .zip([EndpointRole::Primary, EndpointRole::Replica])
    {
        let (_reopened, address) = start(&binary, config);
        let mut endpoint = remote(address, &fixture.authority, role, 4, cut.head());
        let (input, material) = fixture.frozen(&cut, 45);
        let next_request = input.body.request_digest.clone();
        let FormalQueryOutcomeV1::Recorded(state) = endpoint.query(&original_request).unwrap()
        else {
            panic!("committed H was absent after cold reopen");
        };
        let FormalRequestStateV1::Qualified { proof, .. } = *state else {
            panic!("committed H was not qualified after cold reopen");
        };
        assert_eq!(proof, committed);
        let before = endpoint.checkpoint().unwrap();
        let error = endpoint.freeze(input, material).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("formal normal parent/profile/fresh-write boundary"),
            "{error}"
        );
        assert_eq!(endpoint.head().unwrap(), cut.head());
        assert_eq!(endpoint.checkpoint().unwrap(), before);
        assert!(
            endpoint
                .prepare_recovery_plan(
                    forbidden_plan.clone(),
                    pending.clone(),
                    forbidden_ack.clone(),
                    before.clone(),
                )
                .is_err(),
            "CA-B1: an issuer-signed controlled recovery plan cannot reopen committed H"
        );
        assert_eq!(endpoint.checkpoint().unwrap(), before);
        assert!(
            endpoint
                .seal_epoch(
                    forbidden_plan.body.intent.operation_id.clone(),
                    2,
                    before.clone()
                )
                .is_err(),
            "CA-B1: H cannot acquire a new controlled epoch seal"
        );
        assert_eq!(endpoint.checkpoint().unwrap(), before);
        assert_eq!(endpoint.head().unwrap(), cut.head());
        assert!(matches!(
            endpoint.query(&next_request).unwrap(),
            FormalQueryOutcomeV1::NotRecorded
        ));
        eprintln!(
            "CA-B1 role={role:?} controlled_profile=2 next_profile=bft/1 committed_h={} new_freeze=denied recovery_plan=denied epoch_seal=denied original_h=retained",
            cut.head().position,
        );
    }
}

fn plan_after_handoff(
    fixture: &Fixture,
    cut: &VerifiedFormalCommitV1,
    pending: &RecoveryPendingHandoffV1,
) -> SignedRecoveryPlanV1 {
    let mut successor = fixture.authority.definition().clone();
    successor.epoch = 2;
    successor.writer_public_key = public(10);
    successor.predecessor_authority_id = Some(fixture.authority.authority_id().unwrap());
    successor.valid_from_position = cut.head().position + 1;
    for (binding, n) in [
        (&mut successor.primary, 11u8),
        (&mut successor.replica, 12u8),
    ] {
        let directory = fixture.path.join(format!("forbidden-after-h-{n}"));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        binding.instance.authority_epoch = 2;
        binding.instance.endpoint_id = format!("forbidden-after-h-{n}");
        binding.instance.endpoint_public_key = public(n);
        binding.instance.instance_id = h(n + 20);
        binding.instance.storage_id = h(n + 30);
        binding.instance.storage_fingerprint = directory_fingerprint(&directory).unwrap();
        binding.fault_domain_id = format!("forbidden-after-h-domain-{n}");
    }
    let body = RecoveryPlanBodyV1::new(
        RecoveryCertificateBodyV1 {
            schema_version: 1,
            retained_context: None,
            world: successor.world.clone(),
            operation_id: h(241),
            predecessor_authority_id: fixture.authority.authority_id().unwrap(),
            predecessor_epoch: 1,
            sealed_cut: cut.head(),
            tail: RecoveryTailV1::Empty,
            fencing_evidence_digest: h(0),
            successor,
            first_normal_position: Some(cut.head().position + 1),
        },
        &fixture.authority,
        cut,
        &RecoveryTailWitnessV1::Empty,
        pending,
        RecoveryPlanAuthorityV1 {
            recovery_credential_public_key: public(28),
            isolation_evidence_digest: h(243),
        },
    )
    .unwrap();
    assert!(
        VerifiedRecoveryPlanV1::verify(
            body.clone(),
            &fixture.authority,
            cut,
            &RecoveryTailWitnessV1::Empty,
            pending,
        )
        .is_err(),
        "CA-B1: the checked issuer boundary must reject H"
    );
    // The endpoint negative has a real, valid issuer signature. It must reject
    // the already committed handoff, not merely a malformed signature.
    let plan = SignedRecoveryPlanV1 {
        signature_hex: signed(
            crate::controlled_authority::formal::crypto::bytes(
                "oasis7.formal.recovery_plan.sign.v1",
                &body,
            )
            .unwrap(),
            1,
        ),
        body,
    };
    crate::controlled_authority::formal::crypto::verify(
        "oasis7.formal.recovery_plan.sign.v1",
        &plan.body,
        &plan.signature_hex,
        &fixture.authority.policy().recovery_issuer_public_key,
    )
    .unwrap();
    assert!(plan.verify(&fixture.authority).is_err());
    plan
}
