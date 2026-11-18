//! Real endpoint executable recovery with simulated external custody/uniqueness.
//! This verifies bootstrap plumbing, not real host/storage fault domains.
use super::*;
#[path = "endpoint_consecutive_process_tests.rs"]
mod consecutive_tests;
use crate::controlled_authority::formal::{
    coordinator::FormalDurabilityEndpoint, transport::RemoteFormalEndpoint,
};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::process::{Command, Stdio};
fn start(binary: &std::ffi::OsStr, config: &Path) -> (EndpointChild, std::net::SocketAddr) {
    let child = Command::new(binary)
        .arg("--config")
        .arg(config)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut child = EndpointChild(child);
    let address = read_endpoint_ready_address(&mut child)
        .unwrap_or_else(|error| panic!("recovery endpoint readiness failed: {error}"));
    (child, address)
}
fn credential(path: &Path) {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(hex::encode([17u8; 32]).as_bytes()).unwrap();
}
fn remote(
    address: std::net::SocketAddr,
    authority: &VerifiedAuthorityV1,
    role: EndpointRole,
    writer: u8,
    minimum: FormalHeadRefV1,
) -> RemoteFormalEndpoint {
    RemoteFormalEndpoint::new(
        address,
        Duration::from_secs(3),
        authority.clone(),
        role,
        key(writer),
        minimum,
    )
    .unwrap()
}
#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; external providers are simulations"]
fn recovered_endpoint_children_import_ready_complete_original_and_reopen_without_reexecution() {
    run_recovery(false);
}
#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; external providers are simulations"]
fn recovered_endpoint_children_consecutive_epoch_before_first_completion_preserves_original() {
    run_recovery(true);
}
fn run_recovery(consecutive: bool) {
    let binary = std::env::var_os("OASIS7_FORMAL_ENDPOINT_TEST_BIN").unwrap();
    let fixture = Fixture::new();
    let po = Oracle::start(&fixture, EndpointRole::Primary);
    let ro = Oracle::start(&fixture, EndpointRole::Replica);
    let authfile = fixture.path.join("oracle-auth");
    credential(&authfile);
    let body = fixture.authority.initial_body().cloned().unwrap();
    let initial = SignedInitialActivationV1 {
        signature_hex: signed(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let trust = InitialActivationTrustV1 {
        issuer_public_key: public(9),
        world: fixture.authority.definition().world.clone(),
        initial_state_root: initial.body.initial_state_root.clone(),
        governing_manifest_digest: initial.body.governing_manifest_digest.clone(),
        recovery_policy_digest: fixture.authority.policy().digest().unwrap(),
    };
    let mut old_configs = Vec::new();
    for (role, dir, oracle) in [
        (EndpointRole::Primary, &fixture.pdir, &po),
        (EndpointRole::Replica, &fixture.rdir, &ro),
    ] {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
        let config = fixture.path.join(format!("old-{role:?}.json"));
        fs::write(&config,serde_json::to_vec(&serde_json::json!({"schema_version":2,"initial_trust":trust,"initial_certificate":initial,"initial_qualified":null,"role":role,"data_dir":dir,"oracle_authentication_key_file":authfile,"oracle_address":oracle.address,"listen":"127.0.0.1:0","timeout_ms":3000})).unwrap()).unwrap();
        old_configs.push(config);
    }
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
    let (ph, _) = super::super::super::protected_signer::initial_hot(
        recovery.successor(),
        &fixture.authority,
        &Some(recovery.clone()),
        &Some(cut.clone()),
        &recovery.successor().definition().primary.instance,
        Some(&pt),
    )
    .unwrap();
    let (rh, _) = super::super::super::protected_signer::initial_hot(
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
        Some(verified),
    )
    .unwrap();
    assert_eq!(coordinator.submit(candidate).unwrap(), completed);
    drop(coordinator);
    p2.kill().unwrap();
    p2.wait().unwrap();
    r2.kill().unwrap();
    r2.wait().unwrap();
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
    rchild.kill().unwrap();
    rchild.wait().unwrap();
    drop(p2o);
    drop(r2o);
    drop(po);
    drop(ro);
}
