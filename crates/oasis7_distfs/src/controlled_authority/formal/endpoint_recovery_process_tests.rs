//! Real endpoint executable recovery with simulated external custody/uniqueness.
//! This verifies bootstrap plumbing, not real host/storage fault domains.
use super::*;
use crate::controlled_authority::formal::ProfileHandoffV1;
use crate::controlled_authority::formal::business_index::*;
#[path = "endpoint_recovery_boundary_tests.rs"]
mod boundary_tests;
#[path = "endpoint_consecutive_process_tests.rs"]
mod consecutive_tests;
#[path = "endpoint_recovery_fencing_tests.rs"]
mod final_fencing_tests;
#[path = "endpoint_legacy_plan_reopen_tests.rs"]
mod legacy_plan_reopen_tests;
#[path = "endpoint_legacy_recovery_process_tests.rs"]
mod legacy_tests;
#[path = "endpoint_planned_consecutive_tests.rs"]
mod planned_consecutive_tests;
#[path = "endpoint_recovery_seam_tests.rs"]
mod seam_tests;
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
fn recovery_credential_configs(fixture: &Fixture, configs: &[PathBuf]) {
    let file = fixture.path.join("provider-recovery-auth");
    if !file.exists() {
        let mut key_file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&file)
            .unwrap();
        key_file
            .write_all(hex::encode([28u8; 32]).as_bytes())
            .unwrap();
        key_file.sync_all().unwrap();
    }
    for config in configs {
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(config).unwrap()).unwrap();
        value["oracle_authentication_key_file"] = serde_json::json!(file);
        fs::write(config, serde_json::to_vec(&value).unwrap()).unwrap();
    }
}
fn recovery_copy_budget(directory: &Path) -> u64 {
    let source = FileFormalArchiveSourceV1::new(directory.to_path_buf()).unwrap();
    let mut objects = std::collections::BTreeMap::<String, u64>::new();
    for name in source.logical_ids().unwrap() {
        let bytes = source.read_object(&name).unwrap().unwrap();
        assert_eq!(blake3::hash(&bytes).to_hex().as_str(), name);
        objects.insert(name, bytes.len() as u64);
        if let Ok(FormalRequestStateV1::Qualified { candidate, .. }) =
            serde_cbor::from_slice::<FormalRequestStateV1>(&bytes)
        {
            for material in
                std::iter::once(&candidate.material).chain(candidate.frozen_material.iter())
            {
                for object in &material.objects {
                    assert_eq!(
                        blake3::hash(&object.bytes).to_hex().as_str(),
                        object.content_hash
                    );
                    let old =
                        objects.insert(object.content_hash.clone(), object.bytes.len() as u64);
                    assert!(old.is_none_or(|length| length == object.bytes.len() as u64));
                }
            }
        }
    }
    objects
        .values()
        .try_fold(64 * 1024_u64, |total, bytes| total.checked_add(*bytes))
        .unwrap()
}
/// Make the configured predecessor paths unavailable without changing their
/// original inode/fingerprint when they are restored for later fencing checks.
struct OfflineArchives(Vec<(PathBuf, PathBuf, String)>);
impl OfflineArchives {
    fn hide(directories: &[&Path]) -> Self {
        let mut moved = Self(Vec::new());
        for directory in directories {
            let fingerprint = directory_fingerprint(directory).unwrap();
            let offline = directory.with_extension("offline-source");
            assert!(!offline.exists());
            fs::rename(directory, &offline).unwrap();
            assert!(
                !directory.exists(),
                "the original archive path must be offline"
            );
            moved
                .0
                .push((directory.to_path_buf(), offline, fingerprint));
        }
        moved
    }
}
impl Drop for OfflineArchives {
    fn drop(&mut self) {
        for (original, offline, fingerprint) in self.0.iter().rev() {
            if let Err(error) = fs::rename(offline, original) {
                if std::thread::panicking() {
                    eprintln!("cannot restore test archive {original:?}: {error}");
                    continue;
                }
                panic!("cannot restore test archive {original:?}: {error}");
            }
            assert_eq!(&directory_fingerprint(original).unwrap(), fingerprint);
        }
    }
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
fn original_configs(
    fixture: &Fixture,
    po: &Oracle,
    ro: &Oracle,
    authfile: &Path,
) -> (
    SignedInitialActivationV1,
    InitialActivationTrustV1,
    Vec<PathBuf>,
) {
    credential(authfile);
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
    let mut configs = Vec::new();
    for (role, dir, oracle) in [
        (EndpointRole::Primary, &fixture.pdir, &po),
        (EndpointRole::Replica, &fixture.rdir, &ro),
    ] {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
        let config = fixture.path.join(format!("old-{role:?}.json"));
        fs::write(&config,serde_json::to_vec(&serde_json::json!({"schema_version":2,"initial_trust":trust,"initial_certificate":initial,"initial_qualified":null,"role":role,"data_dir":dir,"oracle_authentication_key_file":authfile,"oracle_address":oracle.address,"listen":"127.0.0.1:0","timeout_ms":3000})).unwrap()).unwrap();
        configs.push(config);
    }
    (initial, trust, configs)
}

#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; external providers are simulations"]
fn recovered_endpoint_children_import_ready_complete_original_and_reopen_without_reexecution() {
    seam_tests::first_seal_binds_original_successor();
    run_recovery(false);
    boundary_tests::run();
}
#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; external providers are simulations"]
fn recovered_endpoint_children_consecutive_epoch_before_first_completion_preserves_original() {
    legacy_tests::run(true);
    run_recovery(true);
}
// CA-R2 developer-software negative matrix at the *real successor process*
// boundary. Custody/floor witnesses are simulations here; they cannot assert
// that a physical provider or signer is rollback-resistant.
fn assert_corrupt_recovery_bootstraps_reject_before_listen(
    binary: &std::ffi::OsStr,
    fixture: &Fixture,
    original_configs: &[std::path::PathBuf],
) {
    use std::io::Read;
    use std::time::{Duration, Instant};

    // Regular files avoid a blocked pipe reader if a failed endpoint leaves
    // grandchildren holding stdout/stderr open. Keep both streams bounded and
    // private; never echo untrusted config paths or secret material into CI.
    const MAX_CAPTURE_BYTES: u64 = 64 * 1024;
    fn read_bounded(path: &Path, limit: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .unwrap()
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(
            bytes.len() as u64 <= limit,
            "CA-R2 child output exceeded limit"
        );
        bytes
    }
    assert_eq!(original_configs.len(), 2, "CA-R2 needs both P/R roles");
    for (role_index, config) in original_configs.iter().enumerate() {
        let role = ["primary", "replica"][role_index];
        let source: serde_json::Value = serde_json::from_slice(&fs::read(config).unwrap()).unwrap();
        let mut variants = Vec::new();

        // A missing immutable predecessor CAS cannot be substituted with the
        // current/live directory, even when all signed certificates are intact.
        let mut absent_archive = source.clone();
        absent_archive["recovery_bootstrap"]["archive_directory"] = serde_json::Value::String(
            fixture
                .path
                .join("unprovisioned-original-archive")
                .to_string_lossy()
                .into_owned(),
        );
        variants.push(("missing-original-archive", absent_archive));

        // The signed issuer witness fixes the one-use successor. A signature
        // that has the right length but no attestation must not authorize it.
        let mut forged_witness = source.clone();
        forged_witness["recovery_bootstrap"]["issuer_witness"]["signature_hex"] =
            serde_json::Value::String("00".repeat(64));
        variants.push(("forged-issuer-witness", forged_witness));

        // An otherwise valid witness for a different minimum is not a
        // license to rewind the anti-rollback anchor of this exact request.
        let mut changed_floor = source.clone();
        changed_floor["recovery_bootstrap"]["minimum_issuer_anchor"]["digest"] =
            serde_json::Value::String(h(219));
        variants.push(("wrong-minimum-anchor", changed_floor));

        // Missing exact authoritative inventory must not be reconstructed
        // from live files or accepted merely because the snapshot exists.
        let mut missing_manifest = source.clone();
        missing_manifest["recovery_bootstrap"]["archive_manifest"] = serde_json::Value::Null;
        variants.push(("missing-original-manifest", missing_manifest));

        for (label, variant) in variants {
            let file = fixture
                .path
                .join(format!("reject-{role_index}-{label}.json"));
            fs::write(&file, serde_json::to_vec(&variant).unwrap()).unwrap();
            let stdout_path = fixture
                .path
                .join(format!("reject-{role_index}-{label}.stdout"));
            let stderr_path = fixture
                .path
                .join(format!("reject-{role_index}-{label}.stderr"));
            let capture = |path: &Path| {
                fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .mode(0o600)
                    .open(path)
                    .unwrap()
            };
            let mut child = Command::new(binary)
                .arg("--config")
                .arg(&file)
                .stdout(Stdio::from(capture(&stdout_path)))
                .stderr(Stdio::from(capture(&stderr_path)))
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(6);
            loop {
                let oversized = [&stdout_path, &stderr_path]
                    .iter()
                    .any(|path| fs::metadata(path).unwrap().len() > MAX_CAPTURE_BYTES);
                if oversized {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("CA-R2 {role}/{label} exceeded bounded process output");
                }
                if let Some(status) = child.try_wait().unwrap() {
                    let stdout = read_bounded(&stdout_path, MAX_CAPTURE_BYTES);
                    let stderr = read_bounded(&stderr_path, MAX_CAPTURE_BYTES);
                    assert!(
                        !status.success(),
                        "CA-R2 {role}/{label} accepted an untrusted bootstrap: {status}"
                    );
                    assert!(
                        !String::from_utf8_lossy(&stdout).contains("formal_endpoint_listening="),
                        "CA-R2 {role}/{label} announced a writable successor"
                    );
                    let diagnostic = String::from_utf8_lossy(&stderr);
                    // Assert the actual startup error matches the corrupted
                    // input, not an unrelated missing binary or early failure.
                    // Log only a reviewed category, not raw stderr/credentials.
                    let reason = match label {
                        "missing-original-archive"
                            if diagnostic.contains("No such file")
                                || diagnostic.contains("formal archive") =>
                        {
                            "archive-unavailable"
                        }
                        "forged-issuer-witness"
                            if diagnostic.contains("signature")
                                || diagnostic.contains("issuer/witness") =>
                        {
                            "issuer-signature-untrusted"
                        }
                        "wrong-minimum-anchor"
                            if diagnostic.contains("ISSUER_UNPROVEN")
                                || diagnostic.contains("independently retained floor") =>
                        {
                            "minimum-floor-untrusted"
                        }
                        "missing-original-manifest"
                            if diagnostic.contains("invalid type")
                                || diagnostic.contains("archive_manifest")
                                || diagnostic.contains("missing field") =>
                        {
                            "archive-manifest-invalid"
                        }
                        _ => panic!(
                            "CA-R2 {role}/{label} returned an unrelated or empty startup error (bytes={})",
                            stderr.len()
                        ),
                    };
                    eprintln!(
                        "CA-R2 child role={role} mutation={label} exit={status} listening=false rejection={reason} stdout_bytes={} stderr_bytes={}",
                        stdout.len(),
                        stderr.len()
                    );
                    break;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("CA-R2 {role}/{label} timed out before startup rejection");
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn run_recovery(consecutive: bool) {
    let binary = std::env::var_os("OASIS7_FORMAL_ENDPOINT_TEST_BIN").unwrap();
    let (fixture, initial_envelope) = super::super::business_tests::fixture();
    assert_eq!(fixture.authority.definition().profile_version, 2);
    let po = Oracle::start(&fixture, EndpointRole::Primary);
    let ro = Oracle::start(&fixture, EndpointRole::Replica);
    let authfile = fixture.path.join("oracle-auth");
    let (initial, trust, old_configs) = original_configs(&fixture, &po, &ro, &authfile);
    let (mut pchild, mut paddr) = start(&binary, &old_configs[0]);
    let (mut rchild, mut raddr) = start(&binary, &old_configs[1]);
    let mut writer =
        seam_tests::WriterProcess::start(&fixture, &binary, &fixture.authority, 4, None);
    let genesis = fixture.authority.definition().world.genesis_head().unwrap();
    let mut coordinator = FormalCoordinator::new(
        writer.normal_endpoint(paddr, EndpointRole::Primary, genesis.clone()),
        writer.normal_endpoint(raddr, EndpointRole::Replica, genesis),
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
    let (mut input, material) = fixture.frozen(&cut, 44);
    input.signature_hex = writer.normal.sign_frozen(&input.body).unwrap();
    let executable = coordinator.freeze(input, material).unwrap();
    let mut candidate = fixture.candidate(&executable);
    let candidate_envelope = seam_tests::business_candidate(
        &fixture,
        &mut candidate,
        &first,
        &initial_envelope,
        &[&fixture.pdir],
        4,
    );
    candidate.candidate.writer_signature_hex = writer
        .normal
        .sign_candidate(&candidate.candidate.record, candidate.frozen.as_ref())
        .unwrap();
    let (p, r) = coordinator.endpoints_mut();
    let pp = p.prepare(candidate.clone()).unwrap();
    let rp = r.prepare(candidate.clone()).unwrap();
    let request = &candidate.candidate.record.request_digest;
    let pf = p.decide(request, pp.clone(), rp.clone()).unwrap();
    let rf = r.decide(request, pp, rp).unwrap();
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
    drop(coordinator);
    let original_ready = final_fencing_tests::capture_ready(&fixture, &cut);
    let operation = h(240);
    let pdir = fixture.path.join("epoch2-p");
    let rdir = fixture.path.join("epoch2-r");
    for dir in [&pdir, &rdir] {
        fs::create_dir(dir).unwrap();
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
    }
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
    let template = RecoveryCertificateBodyV1 {
        retained_context: None,
        schema_version: 1,
        world: successor.world.clone(),
        operation_id: operation.clone(),
        predecessor_authority_id: fixture.authority.authority_id().unwrap(),
        predecessor_epoch: 1,
        sealed_cut: cut.head(),
        tail: RecoveryTailV1::Decision {
            position: 2,
            reservation_id: frozen.reservation.body.reservation_id().unwrap(),
            record_id: candidate.candidate.record.record_id().unwrap(),
            handoff: None,
        },
        fencing_evidence_digest: h(0),
        successor,
        first_normal_position: Some(3),
    };
    let fixed = RecoveryTailWitnessV1::Decision {
        frozen,
        candidate: candidate.candidate.clone(),
    };
    let pending = RecoveryPendingHandoffV1::empty();
    let plan = seam_tests::persist_plan(
        &fixture,
        &fixture.authority,
        &cut,
        &fixed,
        &template,
        &pending,
        writer.plan_authorization(),
    );
    let ack = writer.persist_plan(&plan, &fixed, candidate.frozen_material.as_ref(), &pending);
    assert!(
        writer
            .normal
            .sign_candidate(&candidate.candidate.record, candidate.frozen.as_ref())
            .is_err()
    );
    assert!(
        writer
            .normal
            .sign_frozen(&candidate.frozen.as_ref().unwrap().reservation.body)
            .is_err()
    );
    assert_eq!(
        writer
            .recovery
            .sign_candidate(&candidate.candidate.record, candidate.frozen.as_ref())
            .unwrap(),
        candidate.candidate.writer_signature_hex
    );
    let mut p = writer.endpoint(paddr, EndpointRole::Primary, cut.head());
    let mut r = writer.endpoint(raddr, EndpointRole::Replica, cut.head());
    let pc = p.checkpoint().unwrap();
    p.prepare_recovery_plan(plan.clone(), pending.clone(), ack.clone(), pc)
        .unwrap();
    recovery_credential_configs(&fixture, &old_configs[..1]);
    pchild.kill().unwrap();
    pchild.wait().unwrap();
    (pchild, paddr) = start(&binary, &old_configs[0]);
    p = writer.endpoint(paddr, EndpointRole::Primary, cut.head());
    assert_eq!(p.recovery_pending_handoff().unwrap(), pending);
    let pc = p.checkpoint().unwrap();
    p.prepare_recovery_plan(plan.clone(), pending.clone(), ack.clone(), pc)
        .unwrap();
    let rc = r.checkpoint().unwrap();
    r.prepare_recovery_plan(plan.clone(), pending.clone(), ack, rc)
        .unwrap();
    recovery_credential_configs(&fixture, &old_configs[1..]);
    rchild.kill().unwrap();
    rchild.wait().unwrap();
    (rchild, raddr) = start(&binary, &old_configs[1]);
    r = writer.endpoint(raddr, EndpointRole::Replica, cut.head());
    assert_eq!(r.recovery_pending_handoff().unwrap(), pending);
    final_fencing_tests::final_entries(&fixture, [&po, &ro], &original_ready);
    let mut drain = FormalCoordinator::new(
        p,
        r,
        fixture.authority.clone(),
        fixture.authority.clone(),
        None,
        Some(cut.clone()),
    )
    .unwrap();
    let completed = drain.submit(candidate.clone()).unwrap();
    assert_eq!(completed.decision.record, candidate.candidate.record);
    assert_eq!(
        completed.decision.writer_signature_hex,
        candidate.candidate.writer_signature_hex
    );
    assert_eq!(completed.decision.branch, FormalCompletionBranchV1::Normal);
    let drained_cut =
        verify_formal_qualified(&completed, &fixture.authority, Some(&cut), None).unwrap();
    let drained_anchor = anchor
        .after_verified_commit(fixture.authority.clone(), drained_cut.clone())
        .unwrap();
    let (p, r) = drain.endpoints_mut();
    let drained = p.recovery_drained_proof().unwrap();
    assert_eq!(drained, r.recovery_drained_proof().unwrap());
    assert_eq!(drained.fixed, fixed);
    assert_eq!(drained.qualified, completed);
    drop(drain);
    pchild.kill().unwrap();
    pchild.wait().unwrap();
    rchild.kill().unwrap();
    rchild.wait().unwrap();
    (pchild, paddr) = start(&binary, &old_configs[0]);
    (rchild, raddr) = start(&binary, &old_configs[1]);
    let writer_seal = writer.persist_seal(&plan, &drained);
    assert!(
        writer
            .recovery
            .sign_candidate(&candidate.candidate.record, candidate.frozen.as_ref())
            .is_err()
    );
    let mut p = writer.endpoint(paddr, EndpointRole::Primary, drained_cut.head());
    let mut r = writer.endpoint(raddr, EndpointRole::Replica, drained_cut.head());
    assert_eq!(p.recovery_drained_proof().unwrap(), drained);
    let pc = p.checkpoint().unwrap();
    let ps = p.seal_planned_epoch(writer_seal.clone(), pc).unwrap();
    pchild.kill().unwrap();
    pchild.wait().unwrap();
    (pchild, paddr) = start(&binary, &old_configs[0]);
    p = writer.endpoint(paddr, EndpointRole::Primary, drained_cut.head());
    let pc = p.checkpoint().unwrap();
    assert_eq!(
        p.seal_planned_epoch(writer_seal.clone(), pc).unwrap().body,
        ps.body
    );
    let rc = r.checkpoint().unwrap();
    let rs = r.seal_planned_epoch(writer_seal.clone(), rc).unwrap();
    rchild.kill().unwrap();
    rchild.wait().unwrap();
    (rchild, raddr) = start(&binary, &old_configs[1]);
    r = writer.endpoint(raddr, EndpointRole::Replica, drained_cut.head());
    let rc = r.checkpoint().unwrap();
    assert_eq!(r.seal_planned_epoch(writer_seal, rc).unwrap().body, rs.body);
    let pm = p.archive_manifest().unwrap();
    let rm = r.archive_manifest().unwrap();
    drop((p, r));
    final_fencing_tests::final_entries(&fixture, [&po, &ro], &original_ready);
    let original_request = candidate.candidate.record.request_digest.clone();
    let assert_old_writers_fenced = || {
        for (address, role) in [
            (paddr, EndpointRole::Primary),
            (raddr, EndpointRole::Replica),
        ] {
            let mut old = remote(address, &fixture.authority, role, 4, drained_cut.head());
            let before = old.query(&original_request).unwrap();
            assert_eq!(old.head().unwrap(), drained_cut.head());
            let (fresh, material) = fixture.frozen(&drained_cut, 45);
            let fresh_request = fresh.body.request_digest.clone();
            assert!(matches!(
                old.query(&fresh_request).unwrap(),
                FormalQueryOutcomeV1::NotRecorded
            ));
            assert!(old.freeze(fresh, material).is_err());
            assert_eq!(old.head().unwrap(), drained_cut.head());
            assert_eq!(old.query(&original_request).unwrap(), before);
            assert!(matches!(
                old.query(&fresh_request).unwrap(),
                FormalQueryOutcomeV1::NotRecorded
            ));
        }
    };
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
    let mut body = plan.body.intent.clone();
    body.schema_version = 3;
    body.sealed_cut = drained_cut.head();
    body.tail = RecoveryTailV1::Empty;
    body.retained_context = None;
    body.fencing_evidence_digest = recovery_fencing_digest([&pv, &rv]).unwrap();
    let tail = RecoveryTailWitnessV1::Empty;
    let intent =
        VerifiedRecoveryIntent::verify(body, &fixture.authority, &drained_cut, [&pv, &rv], &tail)
            .unwrap();
    use crate::controlled_authority::formal::issuer::*;
    let minimum = IssuerAnchorV1::initial(fixture.authority.policy()).unwrap();
    let (certificate, witness, issuance) = seam_tests::issue_after_interruptions(&fixture, &intent);
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
    let initial_hot = |role, archive| {
        super::super::super::protected_signer::initial_hot(
            recovery.successor(),
            &fixture.authority,
            &Some(recovery.clone()),
            &Some(drained_cut.clone()),
            match role {
                EndpointRole::Primary => &recovery.successor().definition().primary.instance,
                EndpointRole::Replica => &recovery.successor().definition().replica.instance,
            },
            Some(archive),
        )
        .unwrap()
        .0
    };
    let p2o = Oracle::start_registered(
        recovery.successor().clone(),
        EndpointRole::Primary,
        key(11),
        policy.authorized_resource_bindings[0].clone(),
        Some(initial_hot(EndpointRole::Primary, &pt)),
    );
    let r2o = Oracle::start_registered(
        recovery.successor().clone(),
        EndpointRole::Replica,
        key(12),
        policy.authorized_resource_bindings[0].clone(),
        Some(initial_hot(EndpointRole::Replica, &rt)),
    );
    let mut new_configs = Vec::new();
    for (role, dir, oracle, manifest, archive) in [
        (EndpointRole::Primary, &pdir, &p2o, &pm, &fixture.pdir),
        (EndpointRole::Replica, &rdir, &r2o, &rm, &fixture.rdir),
    ] {
        let config = fixture.path.join(format!("new-{role:?}.json"));
        fs::write(&config,serde_json::to_vec(&serde_json::json!({"schema_version":2,"initial_trust":trust,"initial_certificate":initial,
            "initial_qualified":null,"role":role,"data_dir":dir,"oracle_authentication_key_file":authfile,"oracle_address":oracle.address,
            "listen":"127.0.0.1:0","timeout_ms":3000,"recovery_bootstrap":{"predecessor_checkpoint":drained_anchor.checkpoint(),
            "predecessor_checkpoint_digest":drained_anchor.checkpoint().digest().unwrap(),"certificate":certificate,
            "fencing":{"Dual":{"primary":ps,"replica":rs}},"tail":tail,"issuer_witness":witness,"minimum_issuer_anchor":minimum,
            "archive_manifest":manifest,"archive_directory":archive,
            "recovery_copy_max_bytes":recovery_copy_budget(archive)}})).unwrap()).unwrap();
        new_configs.push(config);
    }
    assert_corrupt_recovery_bootstraps_reject_before_listen(&binary, &fixture, &new_configs);
    let (mut p2, p2addr) = start(&binary, &new_configs[0]);
    let (mut r2, r2addr) = start(&binary, &new_configs[1]);
    let restored_cut = remote(
        p2addr,
        recovery.successor(),
        EndpointRole::Primary,
        10,
        drained_cut.head(),
    )
    .restore_native_current(&fixture.authority, Some(&recovery), Some(&drained_cut))
    .unwrap();
    assert_eq!(restored_cut.commit().head(), drained_cut.head());
    assert!(restored_cut.pending().is_none());
    let mut permission: serde_json::Value =
        serde_json::from_slice(&fs::read(&new_configs[0]).unwrap()).unwrap();
    permission = permission["recovery_bootstrap"].clone();
    permission
        .as_object_mut()
        .unwrap()
        .remove("archive_manifest");
    permission
        .as_object_mut()
        .unwrap()
        .remove("archive_directory");
    permission
        .as_object_mut()
        .unwrap()
        .remove("recovery_copy_max_bytes");
    let mut writer2 = seam_tests::WriterProcess::start(
        &fixture,
        &binary,
        recovery.successor(),
        10,
        Some((
            serde_json::from_value(permission).unwrap(),
            witness.body.current_anchor.clone(),
        )),
    );
    let (mut premature, premature_material) = fixture.frozen(&drained_cut, 46);
    premature.body.authority_id = recovery.successor().authority_id().unwrap();
    premature.body.epoch = 2;
    premature.signature_hex = signed(frozen_signing_bytes(&premature.body).unwrap(), 10);
    assert!(
        writer2.normal.sign_frozen(&premature.body).is_err(),
        "business must wait for unique successor Control"
    );
    let mut control_coordinator = FormalCoordinator::new(
        writer2.normal_endpoint(p2addr, EndpointRole::Primary, drained_cut.head()),
        writer2.normal_endpoint(r2addr, EndpointRole::Replica, drained_cut.head()),
        recovery.successor().clone(),
        fixture.authority.clone(),
        Some(recovery.clone()),
        Some(drained_cut.clone()),
    )
    .unwrap();
    assert!(
        control_coordinator
            .freeze(premature, premature_material)
            .is_err()
    );
    let (control, control_envelope) = seam_tests::control_candidate(
        recovery.successor(),
        &certificate,
        &completed,
        &candidate,
        &candidate_envelope,
        &[&fixture.pdir, &pdir],
        &writer2.normal,
    );
    let expected_control = if consecutive {
        seam_tests::publish_one(&mut control_coordinator, &control)
    } else {
        control_coordinator.submit(control.clone()).unwrap()
    };
    drop(control_coordinator);
    p2.kill().unwrap();
    p2.wait().unwrap();
    r2.kill().unwrap();
    r2.wait().unwrap();
    let offline = OfflineArchives::hide(&[&fixture.pdir, &fixture.rdir]);
    let (mut p2, p2addr) = start(&binary, &new_configs[0]);
    let (mut r2, r2addr) = start(&binary, &new_configs[1]);
    let mut control_coordinator = FormalCoordinator::new(
        writer2.normal_endpoint(p2addr, EndpointRole::Primary, drained_cut.head()),
        writer2.normal_endpoint(r2addr, EndpointRole::Replica, drained_cut.head()),
        recovery.successor().clone(),
        fixture.authority.clone(),
        Some(recovery.clone()),
        Some(drained_cut.clone()),
    )
    .unwrap();
    let activation = control_coordinator.submit(control.clone()).unwrap();
    assert_eq!(activation, expected_control);
    assert_eq!(
        control_coordinator.submit(control.clone()).unwrap(),
        activation
    );
    let activated =
        verify_formal_qualified(&activation, recovery.successor(), Some(&drained_cut), None)
            .unwrap();
    assert_eq!(activated.record().position, drained_cut.head().position + 1);
    assert_eq!(
        activated.record().before_state_root,
        drained_cut.record().after_state_root
    );
    assert_eq!(
        activated.record().after_state_root,
        drained_cut.record().after_state_root
    );
    assert_eq!(
        control_envelope.execution_result_cbor,
        candidate_envelope.execution_result_cbor
    );
    drop(control_coordinator);
    drop(offline);
    let snapshot = writer2.normal.query().unwrap();
    writer2.cold_reopen(snapshot.revision);
    let mut coordinator = FormalCoordinator::new(
        writer2.normal_endpoint(p2addr, EndpointRole::Primary, drained_cut.head()),
        writer2.normal_endpoint(r2addr, EndpointRole::Replica, drained_cut.head()),
        recovery.successor().clone(),
        fixture.authority.clone(),
        Some(recovery.clone()),
        Some(activated.clone()),
    )
    .unwrap();
    let (mut next_input, next_material) = fixture.frozen(&activated, 47);
    next_input.body.authority_id = recovery.successor().authority_id().unwrap();
    next_input.body.epoch = 2;
    next_input.signature_hex = writer2.normal.sign_frozen(&next_input.body).unwrap();
    let next_executable = coordinator.freeze(next_input, next_material).unwrap();
    let mut next_candidate = fixture.candidate(&next_executable);
    next_candidate.candidate.record.original_epoch = 2;
    let next_envelope = seam_tests::business_candidate(
        &fixture,
        &mut next_candidate,
        &activation,
        &control_envelope,
        &[&pdir, &fixture.pdir],
        10,
    );
    next_candidate.candidate.writer_signature_hex = writer2
        .normal
        .sign_candidate(
            &next_candidate.candidate.record,
            next_candidate.frozen.as_ref(),
        )
        .unwrap();
    let next = coordinator.submit(next_candidate.clone()).unwrap();
    let next_verified =
        verify_formal_qualified(&next, recovery.successor(), Some(&activated), None).unwrap();
    assert_eq!(
        next_verified.record().position,
        drained_cut.head().position + 2
    );
    assert_eq!(coordinator.submit(next_candidate.clone()).unwrap(), next);
    drop(coordinator);
    assert_old_writers_fenced();
    p2.kill().unwrap();
    p2.wait().unwrap();
    r2.kill().unwrap();
    r2.wait().unwrap();
    let offline = OfflineArchives::hide(&[&fixture.pdir, &fixture.rdir]);
    for (config, role) in new_configs
        .iter()
        .zip([EndpointRole::Primary, EndpointRole::Replica])
    {
        let (mut child, address) = start(&binary, config);
        let mut endpoint = writer2.normal_endpoint(address, role, drained_cut.head());
        let restored = endpoint
            .restore_native_current(&fixture.authority, Some(&recovery), Some(&drained_cut))
            .unwrap();
        assert_eq!(restored.commit().head(), next_verified.head());
        assert_eq!(restored.proof(), &next);
        for proof in [&completed, &activation, &next] {
            let FormalQueryOutcomeV1::Recorded(state) = endpoint
                .query(&proof.decision.record.request_digest)
                .unwrap()
            else {
                panic!("original/control/next identity missing after cold open");
            };
            let FormalRequestStateV1::Qualified {
                proof: retained, ..
            } = *state
            else {
                panic!("original/control/next not Qualified");
            };
            assert_eq!(&retained, proof);
        }
        child.kill().unwrap();
        child.wait().unwrap();
    }
    drop(offline);
    if consecutive {
        let _offline = OfflineArchives::hide(&[&fixture.pdir, &fixture.rdir]);
        let next_anchor = drained_anchor
            .after_verified_commit(recovery.successor().clone(), next_verified.clone())
            .unwrap();
        planned_consecutive_tests::run(planned_consecutive_tests::Previous {
            fixture: &fixture,
            binary: &binary,
            recovery: &recovery,
            anchor: &next_anchor,
            proof: &next,
            candidate: &next_candidate,
            envelope: &next_envelope,
            configs: &new_configs,
            writer: &mut writer2,
            old_proofs: &[&first, &completed, &activation],
        });
    }
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
    final_fencing_tests::remount_sealed(
        &binary,
        &fixture,
        &old_configs,
        &drained_cut,
        &original_ready,
    );
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
    fs::write(&primary_checkpoint, &sealed_checkpoint).unwrap();
    let (mut sealed, address) = start(&binary, &old_configs[0]);
    let mut old = remote(
        address,
        &fixture.authority,
        EndpointRole::Primary,
        4,
        drained_cut.head(),
    );
    assert_eq!(old.head().unwrap(), drained_cut.head());
    let (fresh, material) = fixture.frozen(&drained_cut, 45);
    assert!(old.freeze(fresh, material).is_err());
    assert_eq!(fs::read(&primary_checkpoint).unwrap(), sealed_checkpoint);
    sealed.kill().unwrap();
    sealed.wait().unwrap();
    eprintln!(
        "CA-F2 profile2 actual writer-plan/P-plan/R-plan/old-E-drain/writer-seal/P-seal/R-seal/issuer/control/next cold seams operation={operation}"
    );
}
