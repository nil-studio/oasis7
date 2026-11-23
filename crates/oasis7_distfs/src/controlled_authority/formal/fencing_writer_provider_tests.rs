//! Actual TCP clients and the production file-backed service, with isolated
//! cryptographic fixtures. No physical-custody or canonical World claim.
use super::*;
use crate::controlled_authority::formal::issuer::{IssuerAnchorV1, RecoveryIssuer};
use crate::controlled_authority::formal::transport::{self, Command, RequestBody};
use crate::controlled_authority::replicated_protocol::{
    ArtifactObject, ArtifactRole, ClosedRecord,
};
use oasis7_formal_protocol::controlled_authority::formal::test_fixtures as fixture;
use std::collections::BTreeMap;
use std::io::Write;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[path = "fencing_writer_activation_tests.rs"]
mod activation;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-writer-custody-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Server {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Server {
    fn start(provider: ControlledFileWriterProvider) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = thread::spawn(move || {
            let mut server =
                WriterCustodyServerV1::new(provider, Duration::from_millis(500)).unwrap();
            while !stopping.load(Ordering::SeqCst) {
                let _ = server.serve_one(&listener);
                thread::sleep(Duration::from_millis(2));
            }
        });
        Self {
            address,
            stop,
            thread: Some(thread),
        }
    }
    fn client(
        &self,
        authority: &VerifiedAuthorityV1,
        credential: u8,
        revision: u64,
    ) -> RemoteWriterCustodyClientV1 {
        RemoteWriterCustodyClientV1::new(
            self.address,
            Duration::from_secs(3),
            authority.authority_id().unwrap(),
            authority.definition().writer_public_key.clone(),
            fixture::key(credential),
            revision,
        )
        .unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

fn initialize(path: &Path, authority: &VerifiedAuthorityV1) -> ControlledFileWriterProvider {
    ControlledFileWriterProvider::initialize(
        path,
        authority.clone(),
        fixture::key(4),
        fixture::public(17),
        fixture::public(18),
        None,
    )
    .unwrap()
}
fn reopen(
    path: &Path,
    authority: &VerifiedAuthorityV1,
) -> Result<ControlledFileWriterProvider, FormalError> {
    ControlledFileWriterProvider::open_existing(
        path,
        authority.clone(),
        fixture::key(4),
        fixture::public(17),
        fixture::public(18),
        None,
    )
}
fn request(
    authority: &VerifiedAuthorityV1,
    credential: u8,
    revision: u64,
    command: WriterCustodyCommandV1,
) -> SignedWriterCustodyRequestV1 {
    let body = WriterCustodyRequestBodyV1 {
        schema_version: 1,
        authority_id: authority.authority_id().unwrap(),
        challenge: fixture::h(221),
        expected_revision: Some(revision),
        credential_public_key: fixture::public(credential),
        command,
    };
    SignedWriterCustodyRequestV1 {
        signature_hex: fixture::sign(
            writer_custody_request_signing_bytes(&body).unwrap(),
            credential,
        ),
        body,
    }
}
fn lose_reply(address: SocketAddr, request: &SignedWriterCustodyRequestV1) {
    let bytes = serde_cbor::to_vec(request).unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&bytes).unwrap();
    stream.shutdown(Shutdown::Both).unwrap();
    drop(stream);
}
fn wait_revision(client: &RemoteWriterCustodyClientV1, expected: u64) -> WriterCustodySnapshotV1 {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(state) = client.query()
            && state.revision == expected
        {
            return state;
        }
        assert!(
            Instant::now() < until,
            "writer did not durably reach revision {expected}"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn closing(
    authority: &VerifiedAuthorityV1,
    cut: &VerifiedFormalCommitV1,
) -> (FormalFrozenCertificateV1, CanonicalRecordV1) {
    let frozen = fixture::frozen(
        authority,
        cut,
        Some(ProfileHandoffV1 {
            next_profile: "bft".into(),
            next_profile_version: 1,
            next_authority_digest: fixture::h(224),
            valid_from_position: cut.head().position + 2,
        }),
    );
    let record = fixture::execution(authority, &frozen);
    (frozen, record)
}

#[test]
fn writer_tcp_lost_handoff_reply_closes_fresh_signing_and_strict_cold_open_preserves_original() {
    let directory = Directory::new();
    let path = directory.0.join("provider");
    let (_, authority, _, cut) = fixture::initial_with_profile(2);
    let provider = initialize(&path, &authority);
    provider.require_minimum_revision(0).unwrap();
    assert!(provider.require_minimum_revision(1).is_err());
    assert!(
        reopen(&path, &authority).is_err(),
        "the same original lease is exclusive"
    );
    let server = Server::start(provider);
    let normal = server.client(&authority, 17, 0);
    let recovery = server.client(&authority, 18, 0);
    let (frozen, record) = closing(&authority, &cut);
    let original_signature = normal.sign_frozen(&frozen.reservation.body).unwrap();
    assert_eq!(original_signature, frozen.reservation.signature_hex);
    assert!(recovery.sign_frozen(&frozen.reservation.body).is_err());
    assert!(server.client(&authority, 19, 0).query().is_err());

    let rpc = WriterEndpointRequestV1::new(RequestBody {
        schema_version: 1,
        world: authority.definition().world.clone(),
        authority_id: authority.authority_id().unwrap(),
        instance_digest: authority.definition().primary.instance.digest().unwrap(),
        challenge: fixture::h(222),
        minimum: cut.head(),
        command: Command::Head,
        archive_read_grant: None,
    });
    let signature = normal.sign_endpoint_request(&rpc).unwrap();
    crypto::verify(
        transport::REQUEST_DOMAIN,
        &rpc.body,
        &signature,
        &authority.definition().writer_public_key,
    )
    .unwrap();
    let mut wrong_rpc = rpc.clone();
    wrong_rpc.body.instance_digest = fixture::h(223);
    assert!(normal.sign_endpoint_request(&wrong_rpc).is_err());
    assert_eq!(normal.query().unwrap().revision, 0);

    // The complete request reaches the real service. The caller receives no
    // candidate signature, but the service must first durably close this H.
    lose_reply(
        server.address,
        &request(
            &authority,
            17,
            0,
            WriterCustodyCommandV1::Candidate {
                record: Box::new(record.clone()),
                frozen: Some(Box::new(frozen.clone())),
            },
        ),
    );
    let state = wait_revision(&normal, 1);
    assert!(state.plan_digest.is_none() && state.seal_digest.is_none());
    let original = normal.sign_candidate(&record, Some(&frozen)).unwrap();
    assert_eq!(
        original,
        fixture::sign(canonical_candidate_signing_bytes(&record).unwrap(), 4)
    );
    assert!(normal.sign_frozen(&frozen.reservation.body).is_err());
    let mut different = record.clone();
    different.event_root = fixture::h(225);
    assert!(normal.sign_candidate(&different, Some(&frozen)).is_err());
    normal.sign_endpoint_request(&rpc).unwrap();
    assert!(server.client(&authority, 17, 2).query().is_err());
    drop(server);

    let retained = fs::read(path.join(LEDGER_FILE)).unwrap();
    assert!(
        ControlledFileWriterProvider::initialize(
            &path,
            authority.clone(),
            fixture::key(4),
            fixture::public(17),
            fixture::public(18),
            None
        )
        .is_err()
    );
    assert!(
        ControlledFileWriterProvider::open_existing(
            &path,
            authority.clone(),
            fixture::key(4),
            fixture::public(19),
            fixture::public(18),
            None
        )
        .is_err()
    );
    fs::rename(path.join(LEDGER_FILE), path.join("original-ledger.cbor")).unwrap();
    assert!(
        reopen(&path, &authority).is_err(),
        "missing original may not become an empty epoch"
    );
    assert!(
        ControlledFileWriterProvider::initialize(
            &path,
            authority.clone(),
            fixture::key(4),
            fixture::public(17),
            fixture::public(18),
            None
        )
        .is_err()
    );
    fs::rename(path.join("original-ledger.cbor"), path.join(LEDGER_FILE)).unwrap();
    let mut damaged = retained.clone();
    damaged[0] ^= 1;
    fs::write(path.join(LEDGER_FILE), damaged).unwrap();
    assert!(reopen(&path, &authority).is_err());
    fs::write(path.join(LEDGER_FILE), &retained).unwrap();
    let cold = reopen(&path, &authority).unwrap();
    cold.require_minimum_revision(1).unwrap();
    let server = Server::start(cold);
    let normal = server.client(&authority, 17, 1);
    assert_eq!(
        normal.sign_candidate(&record, Some(&frozen)).unwrap(),
        original
    );
    assert!(normal.sign_frozen(&frozen.reservation.body).is_err());
    assert_eq!(fs::read(path.join(LEDGER_FILE)).unwrap(), retained);
    drop(server);
}

#[test]
fn writer_tcp_missing_original_storage_poison_requires_strict_cold_reconciliation() {
    let directory = Directory::new();
    let path = directory.0.join("provider");
    let moved = directory.0.join("original-provider-moved");
    let (_, authority, _, cut) = fixture::initial_with_profile(2);
    let server = Server::start(initialize(&path, &authority));
    let normal = server.client(&authority, 17, 0);
    let (frozen, record) = closing(&authority, &cut);
    let original = fs::read(path.join(LEDGER_FILE)).unwrap();
    // Keep the actual lease descriptor alive while its pathname becomes
    // unavailable. The live provider must refuse before it can sign or
    // recreate storage, and remain poisoned until strict original reopen.
    fs::rename(&path, &moved).unwrap();
    assert!(normal.sign_candidate(&record, Some(&frozen)).is_err());
    fs::rename(&moved, &path).unwrap();
    assert!(
        normal.query().is_err(),
        "an uncertain live provider must not silently resume"
    );
    assert!(normal.sign_frozen(&frozen.reservation.body).is_err());
    assert_eq!(fs::read(path.join(LEDGER_FILE)).unwrap(), original);
    drop(server);
    let server = Server::start(reopen(&path, &authority).unwrap());
    let normal = server.client(&authority, 17, 0);
    assert_eq!(normal.query().unwrap().revision, 0);
    normal.sign_candidate(&record, Some(&frozen)).unwrap();
    assert_eq!(normal.query().unwrap().revision, 1);
    assert!(normal.sign_frozen(&frozen.reservation.body).is_err());
    drop(server);
}

fn checked_plan(
    authority: &VerifiedAuthorityV1,
    cut: &VerifiedFormalCommitV1,
    fixed: &RecoveryTailWitnessV1,
    pending: &RecoveryPendingHandoffV1,
) -> VerifiedRecoveryPlanV1 {
    let mut successor = authority.definition().clone();
    successor.epoch += 1;
    successor.writer_public_key = fixture::public(10);
    successor.predecessor_authority_id = Some(authority.authority_id().unwrap());
    for (binding, seed) in [(&mut successor.primary, 11), (&mut successor.replica, 12)] {
        binding.instance.authority_epoch = successor.epoch;
        binding.instance.endpoint_id = format!("writer-recovery-endpoint-{seed}");
        binding.instance.endpoint_public_key = fixture::public(seed);
        binding.instance.instance_id = fixture::h(seed + 10);
        binding.instance.storage_id = fixture::h(seed + 20);
        binding.instance.storage_fingerprint = fixture::h(seed + 30);
        binding.fault_domain_id = format!("recovery-domain-{seed}");
    }
    let body = RecoveryPlanBodyV1::new(
        RecoveryCertificateBodyV1 {
            schema_version: 1,
            retained_context: None,
            world: authority.definition().world.clone(),
            operation_id: fixture::h(230),
            predecessor_authority_id: authority.authority_id().unwrap(),
            predecessor_epoch: authority.definition().epoch,
            sealed_cut: cut.head(),
            tail: recovery_tail_from_witness(fixed).unwrap(),
            fencing_evidence_digest: fixture::h(231),
            successor,
            first_normal_position: None,
        },
        authority,
        cut,
        fixed,
        pending,
        RecoveryPlanAuthorityV1 {
            recovery_credential_public_key: fixture::public(18),
            isolation_evidence_digest: fixture::h(232),
        },
    )
    .unwrap();
    VerifiedRecoveryPlanV1::verify(body, authority, cut, fixed, pending).unwrap()
}

fn material_fixture() -> (
    VerifiedAuthorityV1,
    VerifiedFormalCommitV1,
    FormalFrozenCertificateV1,
    ClosedRecord,
) {
    let (mut certificate, _, old_proof, _) = fixture::initial_with_profile(2);
    let mut objects = BTreeMap::new();
    let mut roots = BTreeMap::new();
    for role in [
        ArtifactRole::Input,
        ArtifactRole::Result,
        ArtifactRole::Snapshot,
        ArtifactRole::Journal,
        ArtifactRole::NonceIndex,
        ArtifactRole::EffectOutbox,
        ArtifactRole::ExecutionManifest,
        ArtifactRole::Wasm,
        ArtifactRole::Rules,
    ] {
        let bytes = format!("writer-service-original-{role:?}").into_bytes();
        let digest = blake3::hash(&bytes).to_hex().to_string();
        roots.insert(role, digest.clone());
        objects.insert(
            digest.clone(),
            ArtifactObject {
                content_hash: digest,
                bytes,
                references: vec![],
            },
        );
    }
    certificate.body.artifact_roots = roots.clone();
    certificate.body.artifact_roots.remove(&ArtifactRole::Input);
    certificate.body.initial_state_root = roots[&ArtifactRole::Snapshot].clone();
    certificate.body.runtime_manifest_digest = roots[&ArtifactRole::ExecutionManifest].clone();
    certificate.signature_hex = fixture::sign(
        initial_activation_signing_bytes(&certificate.body).unwrap(),
        9,
    );
    let trust = InitialActivationTrustV1 {
        issuer_public_key: fixture::public(9),
        world: certificate.body.authority.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
    };
    let authority = verify_initial_activation(&certificate, &trust).unwrap();
    let mut record = old_proof.decision.record;
    record.activation_ref = authority.activation_ref().into();
    record.before_state_root = roots[&ArtifactRole::Snapshot].clone();
    record.after_state_root = record.before_state_root.clone();
    record.runtime_manifest_digest = roots[&ArtifactRole::ExecutionManifest].clone();
    record.result_root = roots[&ArtifactRole::Result].clone();
    record.payload_digest = blake3::hash(&initial_activation_input_bytes(&certificate).unwrap())
        .to_hex()
        .to_string();
    record.artifact_roots = roots.clone();
    record
        .artifact_roots
        .insert(ArtifactRole::Input, record.payload_digest.clone());
    let proof = fixture::proof(
        record,
        &authority,
        FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    let cut = verify_formal_qualified(&proof, &authority, None, None).unwrap();
    let material = ClosedRecord {
        payload_digest: roots[&ArtifactRole::Input].clone(),
        before_state_root: roots[&ArtifactRole::Snapshot].clone(),
        after_state_root: roots[&ArtifactRole::Snapshot].clone(),
        roots,
        objects: objects.into_values().collect(),
    };
    let mut frozen = fixture::frozen(&authority, &cut, None);
    frozen.reservation.body.payload_digest = material.payload_digest.clone();
    frozen.reservation.body.runtime_manifest_digest =
        material.roots[&ArtifactRole::ExecutionManifest].clone();
    frozen.reservation.body.frozen_material_digest = frozen_material_digest(&material).unwrap();
    frozen.reservation.signature_hex =
        fixture::sign(frozen_signing_bytes(&frozen.reservation.body).unwrap(), 4);
    for (receipt, seed) in [(&mut frozen.primary, 5), (&mut frozen.replica, 6)] {
        receipt.body.reservation_id = frozen.reservation.body.reservation_id().unwrap();
        receipt.signature_hex =
            fixture::sign(frozen_receipt_signing_bytes(&receipt.body).unwrap(), seed);
    }
    frozen
        .verify(authority.definition(), authority.activation_ref())
        .unwrap();
    verify_frozen_material(&frozen.reservation.body, &material).unwrap();
    (authority, cut, frozen, material)
}

fn exchange(
    address: SocketAddr,
    request: &SignedWriterCustodyRequestV1,
    authority: &VerifiedAuthorityV1,
) -> Result<WriterCustodyValueV1, FormalError> {
    let response: SignedWriterCustodyResponseV1 = transport::writer_custody_exchange(
        &address,
        request,
        Instant::now() + Duration::from_secs(3),
    )
    .unwrap();
    verify_writer_custody_response(
        &response,
        request,
        &authority.definition().writer_public_key,
    )
    .unwrap();
    response.body.result
}

#[test]
fn writer_tcp_original_plan_drains_once_and_cold_seal_reconciles_lost_replies() {
    // Both a sole Frozen input and an already signed Prepared candidate must
    // remain in the old epoch. Protocol proof fixtures are independently signed
    // here; the separate endpoint process matrix supplies actual P/R stores.
    for prepared in [false, true] {
        let directory = Directory::new();
        let path = directory.0.join("provider");
        let (authority, cut, frozen, fixed_material) = material_fixture();
        let record = fixture::execution(&authority, &frozen);
        let server = Server::start(initialize(&path, &authority));
        let normal = server.client(&authority, 17, 0);
        let signed = SignedCanonicalCandidateV1 {
            record: record.clone(),
            writer_signature_hex: normal.sign_candidate(&record, Some(&frozen)).unwrap(),
        };
        let fixed = if prepared {
            RecoveryTailWitnessV1::Prepared {
                frozen: frozen.clone(),
                candidate: signed.clone(),
            }
        } else {
            RecoveryTailWitnessV1::Frozen(frozen.clone())
        };
        let pending = RecoveryPendingHandoffV1::empty();
        let verified = checked_plan(&authority, &cut, &fixed, &pending);
        let mut issuer = RecoveryIssuer::open(
            &directory.0.join("issuer"),
            authority.policy().clone(),
            fixture::key(1),
            IssuerAnchorV1::initial(authority.policy()).unwrap(),
        )
        .unwrap();
        let plan = issuer.prepare_plan(&verified).unwrap();
        let install = request(
            &authority,
            18,
            0,
            WriterCustodyCommandV1::InstallPlan {
                plan: Box::new(plan.clone()),
                fixed: Box::new(fixed.clone()),
                fixed_material: Some(Box::new(fixed_material.clone())),
                pending: Box::new(pending.clone()),
            },
        );
        lose_reply(server.address, &install);
        let state = wait_revision(&normal, 1);
        assert_eq!(
            state.plan_digest.as_deref(),
            Some(plan.digest().unwrap().as_str())
        );
        assert!(state.drained_digest.is_none() && state.seal_digest.is_none());
        let after_plan = fs::read(path.join(LEDGER_FILE)).unwrap();
        let recovery = server.client(&authority, 18, 0);
        let ack = recovery
            .prepare_recovery_plan(&plan, &fixed, Some(&fixed_material), &pending, &authority)
            .unwrap();
        ack.verify(&plan, &authority).unwrap();
        assert_eq!(ack.body.revision, 1);
        assert_eq!(fs::read(path.join(LEDGER_FILE)).unwrap(), after_plan);

        let mut replacement = verified.body().clone();
        replacement.intent.successor.writer_public_key = fixture::public(13);
        let replacement =
            VerifiedRecoveryPlanV1::verify(replacement, &authority, &cut, &fixed, &pending)
                .unwrap();
        assert!(issuer.prepare_plan(&replacement).is_err());
        // An independently valid signature from the authorized issuer still
        // cannot replace the plan already held by the final writer service.
        let replacement = SignedRecoveryPlanV1::sign(&replacement, &fixture::key(1)).unwrap();
        assert!(
            exchange(
                server.address,
                &request(
                    &authority,
                    18,
                    1,
                    WriterCustodyCommandV1::InstallPlan {
                        plan: Box::new(replacement),
                        fixed: Box::new(fixed.clone()),
                        fixed_material: Some(Box::new(fixed_material.clone())),
                        pending: Box::new(pending.clone()),
                    }
                ),
                &authority
            )
            .is_err()
        );
        assert_eq!(fs::read(path.join(LEDGER_FILE)).unwrap(), after_plan);

        assert!(normal.sign_frozen(&frozen.reservation.body).is_err());
        assert!(normal.sign_candidate(&record, Some(&frozen)).is_err());
        let original_rpc = WriterEndpointRequestV1::new(RequestBody {
            schema_version: 1,
            world: authority.definition().world.clone(),
            authority_id: authority.authority_id().unwrap(),
            instance_digest: authority.definition().primary.instance.digest().unwrap(),
            challenge: fixture::h(234),
            minimum: cut.head(),
            command: Command::CompleteFrozen(Box::new(frozen.clone())),
            archive_read_grant: None,
        });
        assert!(normal.sign_endpoint_request(&original_rpc).is_err());
        recovery.sign_endpoint_request(&original_rpc).unwrap();
        assert_eq!(
            recovery.sign_frozen(&frozen.reservation.body).unwrap(),
            frozen.reservation.signature_hex
        );
        let mut other_frozen = frozen.reservation.body.clone();
        other_frozen.request_digest = fixture::h(235);
        assert!(recovery.sign_frozen(&other_frozen).is_err());
        assert_eq!(fs::read(path.join(LEDGER_FILE)).unwrap(), after_plan);

        lose_reply(
            server.address,
            &request(
                &authority,
                18,
                1,
                WriterCustodyCommandV1::Candidate {
                    record: Box::new(record.clone()),
                    frozen: Some(Box::new(frozen.clone())),
                },
            ),
        );
        let candidate_revision = if prepared { 1 } else { 2 };
        wait_revision(&normal, candidate_revision);
        assert_eq!(
            recovery.sign_candidate(&record, Some(&frozen)).unwrap(),
            signed.writer_signature_hex
        );
        let mut fork = record.clone();
        fork.event_root = fixture::h(236);
        assert!(recovery.sign_candidate(&fork, Some(&frozen)).is_err());
        assert_eq!(recovery.query().unwrap().revision, candidate_revision);
        let qualified = fixture::proof(
            record.clone(),
            &authority,
            FormalCompletionBranchV1::Normal,
            None,
            Some(frozen.clone()),
            4,
            5,
            6,
        );
        let drained = RecoveryDrainedProofV1 {
            schema_version: 1,
            plan_digest: plan.digest().unwrap(),
            fixed: fixed.clone(),
            qualified,
        };
        let drained_cut = drained.verify(&plan, &authority).unwrap();
        assert_eq!(drained_cut.position, cut.head().position + 1);
        assert!(recovery.seal_epoch(&plan, &drained, &authority).is_err());
        let before_drained = fs::read(path.join(LEDGER_FILE)).unwrap();
        let mut bad = drained.clone();
        bad.qualified.replica_complete.signature_hex = fixture::h(237);
        assert!(
            exchange(
                server.address,
                &request(
                    &authority,
                    18,
                    candidate_revision,
                    WriterCustodyCommandV1::DrainComplete(Box::new(bad))
                ),
                &authority
            )
            .is_err()
        );
        assert_eq!(fs::read(path.join(LEDGER_FILE)).unwrap(), before_drained);
        lose_reply(
            server.address,
            &request(
                &authority,
                18,
                candidate_revision,
                WriterCustodyCommandV1::DrainComplete(Box::new(drained.clone())),
            ),
        );
        let drained_revision = candidate_revision + 1;
        let state = wait_revision(&normal, drained_revision);
        assert_eq!(state.drained_digest, Some(drained.digest().unwrap()));
        let stale = server.client(&authority, 18, 0);
        assert_eq!(
            stale.complete_drain(&plan, &drained, &authority).unwrap(),
            drained
        );
        assert!(recovery.sign_candidate(&record, Some(&frozen)).is_err());
        assert!(recovery.sign_frozen(&frozen.reservation.body).is_err());

        lose_reply(
            server.address,
            &request(
                &authority,
                18,
                drained_revision,
                WriterCustodyCommandV1::Seal {
                    plan_digest: plan.digest().unwrap(),
                    cut: drained_cut,
                },
            ),
        );
        let sealed_revision = drained_revision + 1;
        let state = wait_revision(&normal, sealed_revision);
        let stale = server.client(&authority, 18, 0);
        let seal = stale.seal_epoch(&plan, &drained, &authority).unwrap();
        seal.verify_with_ack(&plan, &authority, &ack).unwrap();
        assert_eq!(state.seal_digest, Some(seal.digest().unwrap()));
        assert_eq!(seal.drained, drained);
        assert!(normal.sign_candidate(&record, Some(&frozen)).is_err());
        assert!(stale.sign_endpoint_request(&original_rpc).is_err());
        let sealed_bytes = fs::read(path.join(LEDGER_FILE)).unwrap();
        drop(server);

        let cold = Server::start(reopen(&path, &authority).unwrap());
        let recovery = cold.client(&authority, 18, 0);
        assert_eq!(
            recovery
                .prepare_recovery_plan(&plan, &fixed, Some(&fixed_material), &pending, &authority)
                .unwrap(),
            ack
        );
        assert_eq!(
            recovery
                .complete_drain(&plan, &drained, &authority)
                .unwrap(),
            drained
        );
        assert_eq!(
            recovery.seal_epoch(&plan, &drained, &authority).unwrap(),
            seal
        );
        assert_eq!(recovery.query().unwrap().revision, sealed_revision);
        assert!(
            cold.client(&authority, 17, 0)
                .sign_frozen(&frozen.reservation.body)
                .is_err()
        );
        assert!(recovery.sign_candidate(&record, Some(&frozen)).is_err());
        assert_eq!(fs::read(path.join(LEDGER_FILE)).unwrap(), sealed_bytes);
        drop(cold);
    }
}

#[test]
fn writer_private_original_and_disjoint_credentials_are_checked_by_library_open() {
    let directory = Directory::new();
    let (_, authority, _, _) = fixture::initial_with_profile(2);
    // The service library enforces these roles independently of CLI validation.
    for seed in [1, 2, 3, 4, 5, 6, 17] {
        let path = directory.0.join(format!("bad-credential-{seed}"));
        assert!(
            ControlledFileWriterProvider::initialize(
                &path,
                authority.clone(),
                fixture::key(4),
                fixture::public(17),
                fixture::public(seed),
                None
            )
            .is_err()
        );
    }
    let path = directory.0.join("provider");
    drop(initialize(&path, &authority));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for (original, bad_mode, good_mode) in [
            (path.clone(), 0o777, 0o700),
            (path.join(LOCK_FILE), 0o644, 0o600),
            (path.join(LEDGER_FILE), 0o644, 0o600),
        ] {
            fs::set_permissions(&original, fs::Permissions::from_mode(bad_mode)).unwrap();
            assert!(reopen(&path, &authority).is_err());
            fs::set_permissions(&original, fs::Permissions::from_mode(good_mode)).unwrap();
            drop(reopen(&path, &authority).unwrap());
        }
        let original = path.join("retained-ledger.cbor");
        fs::rename(path.join(LEDGER_FILE), &original).unwrap();
        std::os::unix::fs::symlink(&original, path.join(LEDGER_FILE)).unwrap();
        assert!(reopen(&path, &authority).is_err());
        fs::remove_file(path.join(LEDGER_FILE)).unwrap();
        fs::rename(&original, path.join(LEDGER_FILE)).unwrap();
    }
    let server = Server::start(reopen(&path, &authority).unwrap());
    assert_eq!(
        server.client(&authority, 17, 0).query().unwrap().revision,
        0
    );
}
