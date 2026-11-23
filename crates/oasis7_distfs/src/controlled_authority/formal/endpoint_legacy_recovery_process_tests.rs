//! Original profile1 E1/E2/E3 executable acceptance retained without changing
//! its original candidate, permission chain, next request or cold assertions.
use super::*;
pub(super) fn run(consecutive: bool) {
    let binary = std::env::var_os("OASIS7_FORMAL_ENDPOINT_TEST_BIN").unwrap();
    let fixture = Fixture::new();
    let po = Oracle::start(&fixture, EndpointRole::Primary);
    let ro = Oracle::start(&fixture, EndpointRole::Replica);
    let authfile = fixture.path.join("oracle-auth");
    let (initial, trust, old_configs) = original_configs(&fixture, &po, &ro, &authfile);
    let (mut pchild, paddr) = start(&binary, &old_configs[0]);
    let (mut rchild, raddr) = start(&binary, &old_configs[1]);
    let genesis = fixture.authority.definition().world.genesis_head().unwrap();
    let mut coordinator = FormalCoordinator::new(
        remote(
            paddr,
            &fixture.authority,
            EndpointRole::Primary,
            4,
            genesis.clone(),
        ),
        remote(raddr, &fixture.authority, EndpointRole::Replica, 4, genesis),
        fixture.authority.clone(),
        fixture.authority.clone(),
        None,
        None,
    )
    .unwrap();
    let first = coordinator.submit(fixture.initial.clone()).unwrap();
    let cut = verify_formal_qualified(&first, &fixture.authority, None, None).unwrap();
    let anchor = VerifiedFormalReadAnchorV1::from_initial(&initial, &trust, &first).unwrap();
    let primary_checkpoint = fixture.pdir.join("formal-checkpoint-v1.cbor");
    let original_checkpoint = fs::read(&primary_checkpoint).unwrap();
    let (input, material) = fixture.frozen(&cut, 44);
    let executable = coordinator.freeze(input, material).unwrap();
    let mut candidate = fixture.candidate(&executable);
    let (p, r) = coordinator.endpoints_mut();
    let pp = p.prepare(candidate.clone()).unwrap();
    let rp = r.prepare(candidate.clone()).unwrap();
    let pf = p
        .decide(
            &candidate.candidate.record.request_digest,
            pp.clone(),
            rp.clone(),
        )
        .unwrap();
    let rf = r
        .decide(&candidate.candidate.record.request_digest, pp, rp)
        .unwrap();
    let decision = FormalDecisionProofV1 {
        schema_version: 1,
        record: candidate.candidate.record.clone(),
        branch: FormalCompletionBranchV1::Normal,
        writer_signature_hex: candidate.candidate.writer_signature_hex.clone(),
        frozen: candidate.frozen.clone(),
        primary: pf,
        replica: rf,
    };
    p.install_proof(decision.clone()).unwrap();
    r.install_proof(decision).unwrap();
    // Ready on both, no Qualified publication: recover along original candidate.
    let operation = h(240);
    let pc = p.checkpoint().unwrap();
    let rc = r.checkpoint().unwrap();
    let ps = p.seal_epoch(operation.clone(), 2, pc).unwrap();
    let rs = r.seal_epoch(operation.clone(), 2, rc).unwrap();
    let pm = p.archive_manifest().unwrap();
    let rm = r.archive_manifest().unwrap();
    drop(coordinator);
    let original_request = candidate.candidate.record.request_digest.clone();
    let assert_old_writers_fenced = || {
        for (address, role) in [
            (paddr, EndpointRole::Primary),
            (raddr, EndpointRole::Replica),
        ] {
            let mut old = remote(address, &fixture.authority, role, 4, cut.head());
            let before = old.query(&original_request).unwrap();
            assert_eq!(old.head().unwrap(), cut.head());
            let (fresh_input, fresh_material) = fixture.frozen(&cut, 45);
            let fresh_request = fresh_input.body.request_digest.clone();
            assert!(matches!(
                old.query(&fresh_request).unwrap(),
                FormalQueryOutcomeV1::NotRecorded
            ));
            let error = old.freeze(fresh_input, fresh_material).unwrap_err();
            assert!(error.to_string().contains("formal epoch sealed"), "{error}");
            assert_eq!(old.head().unwrap(), cut.head());
            assert_eq!(old.query(&original_request).unwrap(), before);
            assert!(matches!(
                old.query(&fresh_request).unwrap(),
                FormalQueryOutcomeV1::NotRecorded
            ));
        }
    };
    // The old authenticated endpoint processes remain alive throughout takeover.
    // A fresh request must fail at the seal, not merely at a stale credential.
    assert_old_writers_fenced();
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: public(3),
        authorized_resource_bindings: fixture
            .authority
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let pv = verify_epoch_seal(&ps, &policy).unwrap();
    let rv = verify_epoch_seal(&rs, &policy).unwrap();
    let pdir = fixture.path.join("epoch2-p");
    let rdir = fixture.path.join("epoch2-r");
    fs::create_dir(&pdir).unwrap();
    fs::create_dir(&rdir).unwrap();
    fs::set_permissions(&pdir, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&rdir, fs::Permissions::from_mode(0o700)).unwrap();
    let mut successor = fixture.authority.definition().clone();
    successor.epoch = 2;
    successor.writer_public_key = public(10);
    successor.predecessor_authority_id = Some(fixture.authority.authority_id().unwrap());
    successor.valid_from_position = 2;
    for (binding, dir, n) in [
        (&mut successor.primary, &pdir, 11),
        (&mut successor.replica, &rdir, 12),
    ] {
        binding.instance.authority_epoch = 2;
        binding.instance.endpoint_id = format!("successor-{n}");
        binding.instance.endpoint_public_key = public(n);
        binding.instance.instance_id = h(n + 20);
        binding.instance.storage_id = h(n + 30);
        binding.instance.storage_fingerprint = directory_fingerprint(dir).unwrap();
        binding.fault_domain_id = format!("successor-domain-{n}");
    }
    let frozen = candidate.frozen.clone().unwrap();
    let body = RecoveryCertificateBodyV1 {
        retained_context: None,
        schema_version: 1,
        world: successor.world.clone(),
        operation_id: operation,
        predecessor_authority_id: fixture.authority.authority_id().unwrap(),
        predecessor_epoch: 1,
        sealed_cut: cut.head(),
        tail: RecoveryTailV1::Decision {
            position: 2,
            reservation_id: frozen.reservation.body.reservation_id().unwrap(),
            record_id: candidate.candidate.record.record_id().unwrap(),
            handoff: None,
        },
        fencing_evidence_digest: recovery_fencing_digest([&pv, &rv]).unwrap(),
        successor,
        first_normal_position: Some(3),
    };
    let tail = RecoveryTailWitnessV1::Decision {
        frozen,
        candidate: candidate.candidate.clone(),
    };
    let intent =
        VerifiedRecoveryIntent::verify(body.clone(), &fixture.authority, &cut, [&pv, &rv], &tail)
            .unwrap();
    let certificate = SignedRecoveryCertificateV1 {
        signature_hex: signed(recovery_signing_bytes(&body).unwrap(), 1),
        body,
    };
    use crate::controlled_authority::formal::issuer::*;
    let minimum = IssuerAnchorV1::initial(fixture.authority.policy()).unwrap();
    let witness = SignedIssuanceWitnessV1::sign(
        IssuanceWitnessBodyV1 {
            schema_version: 1,
            capability: IssuerWitnessCapabilityV1::NonrollbackUniqueSuccessorV1,
            request: IssuanceRequestBodyV1 {
                schema_version: 1,
                issuer_public_key: public(1),
                policy_binding: minimum.digest.clone(),
                predecessor_authority_id: fixture.authority.authority_id().unwrap(),
                certificate_digest: recovery_certificate_digest(&certificate).unwrap(),
                observation_nonce: h(241),
                minimum_anchor: minimum.clone(),
            },
            grant_revision: 1,
            current_anchor: IssuerAnchorV1 {
                revision: 1,
                digest: h(242),
            },
        },
        &key(2),
    )
    .unwrap();
    let issuance =
        verify_issuance_witness(&witness, &certificate, fixture.authority.policy()).unwrap();
    let recovery =
        VerifiedRecovery::verify(&certificate, &intent, &fixture.authority, &issuance).unwrap();
    let pt = verify_formal_archive_transfer(
        &pm,
        &fixture.authority,
        Arc::new(FileFormalArchiveSourceV1::new(fixture.pdir.clone()).unwrap()),
    )
    .unwrap();
    let rt = verify_formal_archive_transfer(
        &rm,
        &fixture.authority,
        Arc::new(FileFormalArchiveSourceV1::new(fixture.rdir.clone()).unwrap()),
    )
    .unwrap();
    let (ph, _) = super::super::super::super::protected_signer::initial_hot(
        recovery.successor(),
        &fixture.authority,
        &Some(recovery.clone()),
        &Some(cut.clone()),
        &recovery.successor().definition().primary.instance,
        Some(&pt),
    )
    .unwrap();
    let (rh, _) = super::super::super::super::protected_signer::initial_hot(
        recovery.successor(),
        &fixture.authority,
        &Some(recovery.clone()),
        &Some(cut.clone()),
        &recovery.successor().definition().replica.instance,
        Some(&rt),
    )
    .unwrap();
    let p2o = Oracle::start_registered(
        recovery.successor().clone(),
        EndpointRole::Primary,
        key(11),
        policy.authorized_resource_bindings[0].clone(),
        Some(ph),
    );
    let r2o = Oracle::start_registered(
        recovery.successor().clone(),
        EndpointRole::Replica,
        key(12),
        policy.authorized_resource_bindings[0].clone(),
        Some(rh),
    );
    let mut new_configs = Vec::new();
    for (role, dir, oracle, manifest, archive) in [
        (EndpointRole::Primary, &pdir, &p2o, &pm, &fixture.pdir),
        (EndpointRole::Replica, &rdir, &r2o, &rm, &fixture.rdir),
    ] {
        let config = fixture.path.join(format!("new-{role:?}.json"));
        fs::write(&config,serde_json::to_vec(&serde_json::json!({"schema_version":2,"initial_trust":trust,"initial_certificate":initial,"initial_qualified":null,"role":role,"data_dir":dir,"oracle_authentication_key_file":authfile,"oracle_address":oracle.address,"listen":"127.0.0.1:0","timeout_ms":3000,"recovery_bootstrap":{"predecessor_checkpoint":anchor.checkpoint(),"predecessor_checkpoint_digest":anchor.checkpoint().digest().unwrap(),"certificate":certificate,"fencing":{"Dual":{"primary":ps,"replica":rs}},"tail":tail,"issuer_witness":witness,"minimum_issuer_anchor":minimum,"archive_manifest":manifest,"archive_directory":archive}})).unwrap()).unwrap();
        new_configs.push(config);
    }
    // Refused bootstraps must not poison the original Ready recovery. The
    // correct P/R children start immediately afterwards and recover the exact
    // original candidate before the next legitimate request.
    assert_corrupt_recovery_bootstraps_reject_before_listen(&binary, &fixture, &new_configs);
    let (mut p2, p2addr) = start(&binary, &new_configs[0]);
    let (mut r2, r2addr) = start(&binary, &new_configs[1]);
    let restored_cut = remote(
        p2addr,
        recovery.successor(),
        EndpointRole::Primary,
        10,
        cut.head(),
    )
    .restore_native_current(&fixture.authority, Some(&recovery), Some(&cut))
    .unwrap();
    assert_eq!(restored_cut.commit().head(), cut.head());
    assert!(matches!(
        restored_cut.pending(),
        Some(FormalRequestStateV1::Ready { .. })
    ));
    if consecutive {
        consecutive_tests::consecutive(
            &fixture,
            &binary,
            &recovery,
            &cut,
            &anchor,
            &candidate,
            &tail,
            p2addr,
            r2addr,
            &new_configs,
        );
        assert_old_writers_fenced();
        return;
    }
    let mut coordinator = FormalCoordinator::new(
        remote(
            p2addr,
            recovery.successor(),
            EndpointRole::Primary,
            10,
            cut.head(),
        ),
        remote(
            r2addr,
            recovery.successor(),
            EndpointRole::Replica,
            10,
            cut.head(),
        ),
        recovery.successor().clone(),
        fixture.authority.clone(),
        Some(recovery.clone()),
        Some(cut.clone()),
    )
    .unwrap();
    assert_eq!(coordinator.submit(fixture.initial.clone()).unwrap(), first);
    candidate.branch = FormalCompletionBranchV1::CompletePrepared;
    let completed = coordinator.submit(candidate.clone()).unwrap();
    assert_eq!(
        completed.decision.record.record_id().unwrap(),
        candidate.candidate.record.record_id().unwrap()
    );
    let verified =
        verify_formal_qualified(&completed, &fixture.authority, Some(&cut), Some(&recovery))
            .unwrap();
    assert_old_writers_fenced();
    drop(coordinator);
    p2.kill().unwrap();
    p2.wait().unwrap();
    r2.kill().unwrap();
    r2.wait().unwrap();
    let (mut p2, p2addr) = start(&binary, &new_configs[0]);
    let (mut r2, r2addr) = start(&binary, &new_configs[1]);
    let restored = remote(
        p2addr,
        recovery.successor(),
        EndpointRole::Primary,
        10,
        cut.head(),
    )
    .restore_native_current(&fixture.authority, Some(&recovery), Some(&cut))
    .unwrap();
    assert_eq!(restored.commit().head(), verified.head());
    assert_eq!(restored.proof(), &completed);
    assert_old_writers_fenced();
    let mut coordinator = FormalCoordinator::new(
        remote(
            p2addr,
            recovery.successor(),
            EndpointRole::Primary,
            10,
            cut.head(),
        ),
        remote(
            r2addr,
            recovery.successor(),
            EndpointRole::Replica,
            10,
            cut.head(),
        ),
        recovery.successor().clone(),
        fixture.authority.clone(),
        Some(recovery.clone()),
        Some(verified.clone()),
    )
    .unwrap();
    // Replaying the retained request proves idempotence, but alone cannot
    // demonstrate that the recovered owner can accept the *next* business
    // request after the cold restart. Use the real successor writer and an
    // independent new request ID on the same native P/R endpoint pair.
    assert_eq!(coordinator.submit(candidate).unwrap(), completed);
    let (mut next_input, next_material) = fixture.frozen(&verified, 46);
    next_input.body.authority_id = recovery.successor().authority_id().unwrap();
    next_input.body.epoch = 2;
    next_input.body.activation_ref = recovery.successor().activation_ref().into();
    next_input.signature_hex = signed(frozen_signing_bytes(&next_input.body).unwrap(), 10);
    let next_executable = coordinator.freeze(next_input, next_material).unwrap();
    let mut next_candidate = fixture.candidate(&next_executable);
    next_candidate.candidate.record.original_epoch = 2;
    next_candidate.candidate.writer_signature_hex = signed(
        canonical_candidate_signing_bytes(&next_candidate.candidate.record).unwrap(),
        10,
    );
    assert_ne!(
        next_candidate.candidate.record.request_digest,
        completed.decision.record.request_digest
    );
    let next = coordinator.submit(next_candidate.clone()).unwrap();
    assert_eq!(next.decision.record.position, verified.head().position + 1);
    // Fresh Normal writes belong to the successor authority, never the
    // predecessor's one-time CompletePrepared recovery authorization.
    let next_verified =
        verify_formal_qualified(&next, recovery.successor(), Some(&verified), None).unwrap();
    assert_eq!(coordinator.submit(next_candidate).unwrap(), next);
    drop(coordinator);
    p2.kill().unwrap();
    p2.wait().unwrap();
    r2.kill().unwrap();
    r2.wait().unwrap();
    // A second cold open must retain the *new* result, not merely the
    // restored original request or the predecessor epoch's pinned cut.
    // Both original P and R must independently reopen the same new proof.
    // Checking only P permits an apparently successful resumed writer while
    // R has lost its newer qualified result (or cannot reconstruct its CAS).
    for (config, role) in new_configs
        .iter()
        .zip([EndpointRole::Primary, EndpointRole::Replica])
    {
        let (mut child, address) = start(&binary, config);
        let mut endpoint = remote(address, recovery.successor(), role, 10, cut.head());
        let next_restored = endpoint
            .restore_native_current(&fixture.authority, Some(&recovery), Some(&cut))
            .unwrap();
        assert_eq!(next_restored.commit().head(), next_verified.head());
        assert_eq!(next_restored.proof(), &next);
        // Independently query the exact original new request on the *cold*
        // endpoint. Matching head alone does not prove its receipt was kept.
        let FormalQueryOutcomeV1::Recorded(state) = endpoint
            .query(&next.decision.record.request_digest)
            .unwrap()
        else {
            panic!("new successor request missing after the second cold open");
        };
        let FormalRequestStateV1::Qualified { proof, .. } = *state else {
            panic!("new successor request not qualified after the second cold open");
        };
        assert_eq!(proof, next);
        child.kill().unwrap();
        child.wait().unwrap();
    }
    // External fixed-cut mismatch cannot publish a ready endpoint.
    let mut wrong: serde_json::Value =
        serde_json::from_slice(&fs::read(&new_configs[0]).unwrap()).unwrap();
    wrong["recovery_bootstrap"]["predecessor_checkpoint_digest"] =
        serde_json::Value::String(h(243));
    let wrong_file = fixture.path.join("wrong-cut.json");
    fs::write(&wrong_file, serde_json::to_vec(&wrong).unwrap()).unwrap();
    let failed = Command::new(&binary)
        .arg("--config")
        .arg(wrong_file)
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(!String::from_utf8_lossy(&failed.stdout).contains("formal_endpoint_listening="));
    pchild.kill().unwrap();
    pchild.wait().unwrap();
    // Restore the earlier signed checkpoint in the original directory, preserving
    // its storage identity and oracle authentication. Startup must reach the same
    // oracle, and the oracle must reject the superseded checkpoint.
    let sealed_checkpoint = fs::read(&primary_checkpoint).unwrap();
    assert_ne!(sealed_checkpoint, original_checkpoint);
    let before = po.stale_checkpoint_opens();
    assert_eq!(before, 0);
    fs::write(&primary_checkpoint, &original_checkpoint).unwrap();
    let mut rolled_back = EndpointChild(
        Command::new(&binary)
            .arg("--config")
            .arg(&old_configs[0])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let error = read_endpoint_ready_address(&mut rolled_back).unwrap_err();
    assert!(
        error.starts_with("endpoint child closed stdout before readiness"),
        "{error}"
    );
    assert!(!rolled_back.wait().unwrap().success());
    assert_eq!(po.stale_checkpoint_opens(), before + 1);
    assert_eq!(fs::read(&primary_checkpoint).unwrap(), original_checkpoint);

    // The rejected attempt leaves the provider usable for the current sealed
    // snapshot, with the original head and mutation gate still intact.
    fs::write(&primary_checkpoint, &sealed_checkpoint).unwrap();
    let (mut sealed, address) = start(&binary, &old_configs[0]);
    let mut old = remote(
        address,
        &fixture.authority,
        EndpointRole::Primary,
        4,
        cut.head(),
    );
    assert_eq!(old.head().unwrap(), cut.head());
    let (fresh_input, fresh_material) = fixture.frozen(&cut, 45);
    let error = old.freeze(fresh_input, fresh_material).unwrap_err();
    assert!(error.to_string().contains("formal epoch sealed"), "{error}");
    assert_eq!(fs::read(&primary_checkpoint).unwrap(), sealed_checkpoint);
    sealed.kill().unwrap();
    sealed.wait().unwrap();
    rchild.kill().unwrap();
    rchild.wait().unwrap();
    drop(p2o);
    drop(r2o);
    drop(po);
    drop(ro);
}
