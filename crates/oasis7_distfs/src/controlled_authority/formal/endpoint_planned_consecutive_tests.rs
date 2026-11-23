//! A second real profile2 recovery consumes only the preceding owner's copied
//! local archive. Legacy retained-tail coverage remains in its original child.
use super::*;

pub(super) struct Previous<'a> {
    pub fixture: &'a Fixture,
    pub binary: &'a std::ffi::OsStr,
    pub recovery: &'a VerifiedRecovery,
    pub anchor: &'a VerifiedFormalReadAnchorV1,
    pub proof: &'a FormalQualifiedCommitProofV1,
    pub candidate: &'a FormalCandidateMaterialV1,
    pub envelope: &'a FormalBusinessResultEnvelopeV1,
    pub configs: &'a [PathBuf],
    pub writer: &'a mut seam_tests::WriterProcess,
    pub old_proofs: &'a [&'a FormalQualifiedCommitProofV1],
}

pub(super) fn run(previous: Previous<'_>) {
    let Previous {
        fixture,
        binary,
        recovery: recovery2,
        anchor,
        proof,
        candidate,
        envelope,
        configs,
        writer: writer2,
        old_proofs,
    } = previous;
    let cut = anchor.head();
    assert_eq!(cut.record(), &proof.decision.record);
    assert_eq!(anchor.authority().definition().epoch, 2);
    let authority = recovery2.successor();
    let previous_config: serde_json::Value =
        serde_json::from_slice(&fs::read(&configs[0]).unwrap()).unwrap();
    let trust = previous_config["initial_trust"].clone();
    let initial = previous_config["initial_certificate"].clone();
    let old_dirs = [fixture.path.join("epoch2-p"), fixture.path.join("epoch2-r")];
    let (mut pchild, mut paddr) = start(binary, &configs[0]);
    let (mut rchild, mut raddr) = start(binary, &configs[1]);
    let pdir = fixture.path.join("planned-epoch3-p");
    let rdir = fixture.path.join("planned-epoch3-r");
    for dir in [&pdir, &rdir] {
        fs::create_dir(dir).unwrap();
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut successor = authority.definition().clone();
    successor.epoch = 3;
    successor.writer_public_key = public(13);
    successor.predecessor_authority_id = Some(authority.authority_id().unwrap());
    successor.valid_from_position = cut.head().position + 1;
    for (binding, dir, n) in [
        (&mut successor.primary, &pdir, 14),
        (&mut successor.replica, &rdir, 15),
    ] {
        binding.instance.authority_epoch = 3;
        binding.instance.endpoint_id = format!("planned-epoch3-{n}");
        binding.instance.endpoint_public_key = public(n);
        binding.instance.instance_id = h(n + 20);
        binding.instance.storage_id = h(n + 30);
        binding.instance.storage_fingerprint = directory_fingerprint(dir).unwrap();
        binding.fault_domain_id = format!("planned-epoch3-domain-{n}");
    }
    let template = RecoveryCertificateBodyV1 {
        schema_version: 1,
        world: successor.world.clone(),
        operation_id: h(248),
        predecessor_authority_id: authority.authority_id().unwrap(),
        predecessor_epoch: 2,
        sealed_cut: cut.head(),
        tail: RecoveryTailV1::Empty,
        fencing_evidence_digest: h(0),
        successor,
        first_normal_position: Some(cut.head().position + 2),
        retained_context: None,
    };
    let fixed = RecoveryTailWitnessV1::Empty;
    let pending = RecoveryPendingHandoffV1::empty();
    let plan = seam_tests::persist_plan(
        fixture,
        authority,
        cut,
        &fixed,
        &template,
        &pending,
        writer2.plan_authorization(),
    );
    let ack = writer2.persist_plan(&plan, &fixed, None, &pending);
    assert!(
        writer2
            .normal
            .sign_candidate(&candidate.candidate.record, candidate.frozen.as_ref())
            .is_err()
    );
    for (address, role) in [
        (paddr, EndpointRole::Primary),
        (raddr, EndpointRole::Replica),
    ] {
        let mut endpoint = writer2.endpoint(address, role, cut.head());
        let checkpoint = endpoint.checkpoint().unwrap();
        endpoint
            .prepare_recovery_plan(plan.clone(), pending.clone(), ack.clone(), checkpoint)
            .unwrap();
    }
    recovery_credential_configs(fixture, configs);
    pchild.kill().unwrap();
    pchild.wait().unwrap();
    rchild.kill().unwrap();
    rchild.wait().unwrap();
    (pchild, paddr) = start(binary, &configs[0]);
    (rchild, raddr) = start(binary, &configs[1]);
    let mut p = writer2.endpoint(paddr, EndpointRole::Primary, cut.head());
    let mut r = writer2.endpoint(raddr, EndpointRole::Replica, cut.head());
    let drained = p.recovery_drained_proof().unwrap();
    assert_eq!(drained, r.recovery_drained_proof().unwrap());
    assert_eq!(drained.qualified, *proof);
    assert_eq!(drained.fixed, fixed);
    let writer_seal = writer2.persist_seal(&plan, &drained);
    // persist_seal cold-reopens the real writer service; use its current port.
    p = writer2.endpoint(paddr, EndpointRole::Primary, cut.head());
    r = writer2.endpoint(raddr, EndpointRole::Replica, cut.head());
    let checkpoint = p.checkpoint().unwrap();
    let ps = p
        .seal_planned_epoch(writer_seal.clone(), checkpoint)
        .unwrap();
    let checkpoint = r.checkpoint().unwrap();
    let rs = r.seal_planned_epoch(writer_seal, checkpoint).unwrap();
    let manifests = [p.archive_manifest().unwrap(), r.archive_manifest().unwrap()];
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: public(3),
        authorized_resource_bindings: authority
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let pv = verify_epoch_seal(&ps, &policy).unwrap();
    let rv = verify_epoch_seal(&rs, &policy).unwrap();
    let mut body = plan.body.intent.clone();
    body.schema_version = 3;
    body.fencing_evidence_digest = recovery_fencing_digest([&pv, &rv]).unwrap();
    let intent = VerifiedRecoveryIntent::verify(body, authority, cut, [&pv, &rv], &fixed).unwrap();
    let (certificate, witness, issuance) = seam_tests::issue_after_interruptions(fixture, &intent);
    assert_eq!(witness.body.grant_revision, 2);
    let recovery3 = VerifiedRecovery::verify(&certificate, &intent, authority, &issuance).unwrap();
    let transfers = [0, 1].map(|i| {
        verify_formal_archive_transfer(
            &manifests[i],
            authority,
            Arc::new(FileFormalArchiveSourceV1::new(old_dirs[i].clone()).unwrap()),
        )
        .unwrap()
    });
    let initial_hot = |role, archive| {
        super::super::super::protected_signer::initial_hot(
            recovery3.successor(),
            authority,
            &Some(recovery3.clone()),
            &Some(cut.clone()),
            match role {
                EndpointRole::Primary => &recovery3.successor().definition().primary.instance,
                EndpointRole::Replica => &recovery3.successor().definition().replica.instance,
            },
            Some(archive),
        )
        .unwrap()
        .0
    };
    let p3o = Oracle::start_registered(
        recovery3.successor().clone(),
        EndpointRole::Primary,
        key(14),
        policy.authorized_resource_bindings[0].clone(),
        Some(initial_hot(EndpointRole::Primary, &transfers[0])),
    );
    let r3o = Oracle::start_registered(
        recovery3.successor().clone(),
        EndpointRole::Replica,
        key(15),
        policy.authorized_resource_bindings[0].clone(),
        Some(initial_hot(EndpointRole::Replica, &transfers[1])),
    );
    let mut configs3 = vec![];
    let mut permission3 = None;
    for (i, role, dir, oracle) in [
        (0, EndpointRole::Primary, &pdir, &p3o),
        (1, EndpointRole::Replica, &rdir, &r3o),
    ] {
        let permission = serde_json::json!({
            "predecessor_checkpoint": anchor.checkpoint(),
            "predecessor_checkpoint_digest": anchor.checkpoint().digest().unwrap(),
            "certificate": certificate, "fencing": {"Dual":{"primary":ps,"replica":rs}},
            "tail": fixed, "issuer_witness": witness,
            "minimum_issuer_anchor": witness.body.current_anchor,
        });
        permission3 = Some(
            serde_json::from_value::<RecoveryPermissionBootstrapV1>(permission.clone()).unwrap(),
        );
        let mut bootstrap = permission;
        bootstrap["archive_manifest"] = serde_json::to_value(&manifests[i]).unwrap();
        bootstrap["archive_directory"] = serde_json::json!(old_dirs[i]);
        bootstrap["recovery_copy_max_bytes"] =
            serde_json::json!(recovery_copy_budget(&old_dirs[i]));
        let config = fixture.path.join(format!("planned-epoch3-{role:?}.json"));
        fs::write(
            &config,
            serde_json::to_vec(&serde_json::json!({
                "schema_version":2,"initial_trust":trust,"initial_certificate":initial,
                "initial_qualified":null,"role":role,"data_dir":dir,
                "oracle_authentication_key_file":fixture.path.join("oracle-auth"),
                "oracle_address":oracle.address,"listen":"127.0.0.1:0","timeout_ms":3000,
                "recovery_bootstrap":bootstrap,
            }))
            .unwrap(),
        )
        .unwrap();
        configs3.push(config);
    }
    let (mut p3, p3addr) = start(binary, &configs3[0]);
    let (mut r3, r3addr) = start(binary, &configs3[1]);
    // Both new owners have independently materialized the full old closure.
    // Keep E2 unavailable through Control, two cold opens and the next request.
    let _offline = OfflineArchives::hide(&[&old_dirs[0], &old_dirs[1]]);
    let mut writer3 = seam_tests::WriterProcess::start(
        fixture,
        binary,
        recovery3.successor(),
        13,
        Some((permission3.unwrap(), witness.body.current_anchor.clone())),
    );
    let mut coordinator = FormalCoordinator::new(
        writer3.normal_endpoint(p3addr, EndpointRole::Primary, cut.head()),
        writer3.normal_endpoint(r3addr, EndpointRole::Replica, cut.head()),
        recovery3.successor().clone(),
        authority.clone(),
        Some(recovery3.clone()),
        Some(cut.clone()),
    )
    .unwrap();
    let (control, control_envelope) = seam_tests::control_candidate(
        recovery3.successor(),
        &certificate,
        proof,
        candidate,
        envelope,
        &[&pdir],
        &writer3.normal,
    );
    let activation = coordinator.submit(control.clone()).unwrap();
    assert_eq!(coordinator.submit(control).unwrap(), activation);
    let activated =
        verify_formal_qualified(&activation, recovery3.successor(), Some(cut), None).unwrap();
    assert_eq!(activated.record().position, cut.head().position + 1);
    assert_eq!(
        activated.record().before_state_root,
        cut.record().after_state_root
    );
    assert_eq!(
        activated.record().after_state_root,
        cut.record().after_state_root
    );
    assert_eq!(
        control_envelope.execution_result_cbor,
        envelope.execution_result_cbor
    );
    drop(coordinator);
    let snapshot = writer3.normal.query().unwrap();
    writer3.cold_reopen(snapshot.revision);
    p3.kill().unwrap();
    p3.wait().unwrap();
    r3.kill().unwrap();
    r3.wait().unwrap();
    let (mut p3, p3addr) = start(binary, &configs3[0]);
    let (mut r3, r3addr) = start(binary, &configs3[1]);
    let mut coordinator = FormalCoordinator::new(
        writer3.normal_endpoint(p3addr, EndpointRole::Primary, cut.head()),
        writer3.normal_endpoint(r3addr, EndpointRole::Replica, cut.head()),
        recovery3.successor().clone(),
        authority.clone(),
        Some(recovery3.clone()),
        Some(activated.clone()),
    )
    .unwrap();
    let (mut input, material) = fixture.frozen(&activated, 49);
    input.body.authority_id = recovery3.successor().authority_id().unwrap();
    input.body.epoch = 3;
    input.signature_hex = writer3.normal.sign_frozen(&input.body).unwrap();
    let frozen = coordinator.freeze(input, material).unwrap();
    let mut next = fixture.candidate(&frozen);
    next.candidate.record.original_epoch = 3;
    seam_tests::business_candidate(
        fixture,
        &mut next,
        &activation,
        &control_envelope,
        &[&pdir],
        13,
    );
    next.candidate.writer_signature_hex = writer3
        .normal
        .sign_candidate(&next.candidate.record, next.frozen.as_ref())
        .unwrap();
    let completed = coordinator.submit(next.clone()).unwrap();
    assert_eq!(coordinator.submit(next).unwrap(), completed);
    let current =
        verify_formal_qualified(&completed, recovery3.successor(), Some(&activated), None).unwrap();
    assert_eq!(current.record().position, cut.head().position + 2);
    drop(coordinator);
    p3.kill().unwrap();
    p3.wait().unwrap();
    r3.kill().unwrap();
    r3.wait().unwrap();
    for (config, role) in configs3
        .iter()
        .zip([EndpointRole::Primary, EndpointRole::Replica])
    {
        let (mut child, address) = start(binary, config);
        let mut endpoint = writer3.normal_endpoint(address, role, cut.head());
        let restored = endpoint
            .restore_native_current(authority, Some(&recovery3), Some(cut))
            .unwrap();
        assert_eq!(restored.commit().head(), current.head());
        for old in old_proofs
            .iter()
            .copied()
            .chain([proof, &activation, &completed])
        {
            let FormalQueryOutcomeV1::Recorded(entry) =
                endpoint.query(&old.decision.record.request_digest).unwrap()
            else {
                panic!("second recovery lost an original permanent request identity");
            };
            let FormalRequestStateV1::Qualified { proof: actual, .. } = *entry else {
                panic!("second recovery original request is not Qualified");
            };
            assert_eq!(&actual, old);
        }
        child.kill().unwrap();
        child.wait().unwrap();
    }
    pchild.kill().unwrap();
    pchild.wait().unwrap();
    rchild.kill().unwrap();
    rchild.wait().unwrap();
    eprintln!(
        "CA-F2 profile2 consecutive E2 plan/drain/seals -> E3 local closure/Control/cold/next passed"
    );
}
