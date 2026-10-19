//! Actual native closure copy/install/readback/activation and first successor
//! commit. The signer fixture simulates protected custody; no physical claim.
use super::*;
#[path = "endpoint_durable_set_test_provider.rs"]
mod install_provider;
use install_provider::InstallProvider;
#[test]
fn native_healthy_install_awaiting_reopen_dual_activate_and_first_normal() {
    exercise_native_install(false);
}
#[path = "endpoint_durable_set_recovery_tests.rs"]
mod recovery;
#[test]
fn native_healthy_management_recovers_uncertain_activation_before_and_after_provider_commit() {
    exercise_native_install(true);
}
fn exercise_native_install(recover_uncertain_activation: bool) {
    let fixture = Fixture::new();
    let po = Oracle::start(&fixture, EndpointRole::Primary);
    let ro = Oracle::start(&fixture, EndpointRole::Replica);
    let resource = fixture
        .authority
        .policy()
        .authorized_fencing_resource_bindings[0]
        .clone();
    let pp = InstallProvider::new(
        fixture.pdir.clone(),
        5,
        resource.clone(),
        Some(po.provider(&fixture)),
    );
    let next_dir = fixture.path.join("new-replica");
    fs::create_dir(&next_dir).unwrap();
    let rp = InstallProvider::new(next_dir.clone(), 12, resource, None);
    let open = |role, provider: Arc<dyn MonotonicEndpointProvider>| {
        FileFormalEndpoint::open_protected(
            if role == EndpointRole::Primary {
                &fixture.pdir
            } else {
                &fixture.rdir
            },
            FormalEndpointAuthorizationV1 {
                authority: fixture.authority.clone(),
                original: fixture.authority.clone(),
                recovery: None,
                parent: None,
            },
            role,
            provider,
            None,
        )
        .unwrap()
    };
    let mut coordinator = FormalCoordinator::new(
        open(EndpointRole::Primary, pp.clone()),
        open(EndpointRole::Replica, ro.provider(&fixture)),
        fixture.authority.clone(),
        fixture.authority.clone(),
        None,
        None,
    )
    .unwrap();
    let initial = coordinator.submit(fixture.initial.clone()).unwrap();
    let parent = verify_formal_qualified(&initial, &fixture.authority, None, None).unwrap();
    let mut definition = fixture.authority.definition().clone();
    definition.epoch = 2;
    definition.valid_from_position = 3;
    definition.predecessor_authority_id = Some(fixture.authority.authority_id().unwrap());
    definition.primary.instance.authority_epoch = 2;
    definition.replica.instance.authority_epoch = 2;
    definition.replica.instance.endpoint_public_key = public(12);
    definition.replica.instance.instance_id = h(120);
    definition.replica.instance.storage_id = h(121);
    definition.replica.instance.storage_fingerprint = directory_fingerprint(&next_dir).unwrap();
    definition.replica.instance.endpoint_id = "new-replica".into();
    definition.replica.fault_domain_id = "new-r".into();
    let mut body = NormalDurableSetTransitionBodyV1 {
        schema_version: 1,
        world: definition.world.clone(),
        predecessor_authority_id: fixture.authority.authority_id().unwrap(),
        predecessor: parent.head(),
        boundary_position: 2,
        operation_id: h(123),
        successor: definition,
        independent_closure_root: h(124),
    };
    body.independent_closure_root = body.copy_contract_digest().unwrap();
    let signed_transition = SignedNormalDurableSetTransitionV1 {
        writer_signature_hex: signed(
            normal_durable_set_transition_signing_bytes(&body).unwrap(),
            4,
        ),
        body,
    };
    let (frozen, material) = fixture.frozen(&parent, 61);
    let executable = coordinator.freeze(frozen, material).unwrap();
    let mut candidate = fixture.candidate(&executable);
    candidate.candidate.record.schema_version = 2;
    candidate.candidate.record.durable_set_transition_digest =
        Some(signed_transition.body.digest().unwrap());
    candidate.candidate.writer_signature_hex = signed(
        canonical_candidate_signing_bytes(&candidate.candidate.record).unwrap(),
        4,
    );
    let proof = coordinator.submit(candidate).unwrap();
    let boundary =
        verify_formal_qualified(&proof, &fixture.authority, Some(&parent), None).unwrap();
    let transition = verify_normal_durable_set_transition(
        &signed_transition,
        &fixture.authority,
        &parent,
        &boundary,
    )
    .unwrap();
    let (p, r) = coordinator.endpoints_mut();
    let pcp = p.checkpoint().unwrap();
    let rcp = r.checkpoint().unwrap();
    let primary_seal = p.seal_durable_set(&transition, &pcp).unwrap();
    let replica_seal = r.seal_durable_set(&transition, &rcp).unwrap();
    let manifest = r.archive_manifest().unwrap();
    let sealed = Arc::new(
        verify_sealed_durable_set_transition(
            &DurableSetSealedEvidenceV1 {
                transition: signed_transition,
                primary_seal,
                replica_seal,
            },
            &fixture.authority,
            &parent,
            &boundary,
        )
        .unwrap(),
    );
    drop(coordinator);
    let (source, source_owner) = remote_sealed_source(
        open(EndpointRole::Replica, ro.provider(&fixture)),
        &fixture,
        &boundary,
        &sealed,
    );
    let oldp = open(EndpointRole::Primary, pp.clone());
    let mut pc = oldp
        .into_durable_set_primary_candidate(sealed.clone(), 64 * 1024 * 1024)
        .unwrap();
    let mut rc = DurableSetCandidateV1::stage_replica(
        &next_dir,
        sealed.clone(),
        manifest,
        source,
        rp.clone(),
        64 * 1024 * 1024,
    )
    .unwrap();
    drop(source_owner); // independent replica copy no longer depends on old R
    let pi = pc.install().unwrap();
    // The provider independently reads its registered local directory. A root
    // assertion cannot hide damaged/missing copied content or mint an install.
    let artifact = &fixture.initial.material.objects[0];
    let artifact_path = next_dir
        .join("formal-objects-v1")
        .join(&artifact.content_hash);
    fs::write(&artifact_path, b"corrupt").unwrap();
    assert!(rc.install().is_err());
    assert!(rc.query_install().is_err());
    fs::write(&artifact_path, &artifact.bytes).unwrap();
    fs::remove_file(&artifact_path).unwrap();
    assert!(rc.install().is_err());
    fs::write(&artifact_path, &artifact.bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&artifact_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let ri = rc.install().unwrap();
    assert_eq!(
        pi.body.independent_inventory_digest,
        ri.body.independent_inventory_digest
    );
    // Installed alone cannot append or export a formal receipt. Old P can never
    // be re-acquired after its CAS consumes the sealed floor.
    assert!(
        pp.acquire(&fixture.authority.definition().primary.instance, &pcp)
            .is_err()
    );
    let mut awaiting = pp
        .query_durable_set_install(
            &sealed.transition().body().successor.primary.instance,
            &sealed,
        )
        .unwrap();
    let signing = ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: sealed
            .transition()
            .body()
            .successor
            .primary
            .instance
            .digest()
            .unwrap(),
        checkpoint: awaiting.checkpoint().clone(),
        challenge: h(130),
        purpose: ProtectedEndpointSignaturePurposeV1::NativeCheckpoint,
        preimage: Store::checkpoint_signing_bytes(
            &Store::verify_public(awaiting.protected_native_checkpoint().unwrap(), &public(5))
                .unwrap(),
        )
        .unwrap(),
    };
    assert!(awaiting.sign_endpoint(signing).is_err());
    let current = awaiting.checkpoint().clone();
    assert!(
        awaiting
            .begin(CheckpointIntentV1 {
                previous: current.clone(),
                next: current,
                checkpoint_bytes: vec![],
                seal: None
            })
            .is_err()
    );
    drop(awaiting);
    // Reply-loss/restart recovers the same immutable install, not a new epoch.
    drop(pc);
    drop(rc);
    let mut pc = DurableSetCandidateV1::reopen(
        &fixture.pdir,
        sealed.clone(),
        EndpointRole::Primary,
        pp.clone(),
        64 * 1024 * 1024,
    )
    .unwrap();
    let mut rc = DurableSetCandidateV1::reopen(
        &next_dir,
        sealed.clone(),
        EndpointRole::Replica,
        rp.clone(),
        64 * 1024 * 1024,
    )
    .unwrap();
    assert_eq!(pc.query_install().unwrap(), Some(pi.clone()));
    assert_eq!(rc.query_install().unwrap(), Some(ri.clone()));
    let activation = verify_durable_set_activation(
        sealed.transition(),
        [
            &sealed.evidence().primary_seal,
            &sealed.evidence().replica_seal,
        ],
        [&pi, &ri],
    )
    .unwrap();
    let p = if recover_uncertain_activation {
        recovery::verify_management_recovery(
            pc,
            &fixture.pdir,
            pp.clone(),
            sealed.clone(),
            pi.clone(),
            ri.clone(),
        )
    } else {
        verify_candidate_management_rpc(pc, sealed.clone(), pi.clone(), ri.clone())
    };
    let r = rc.activate(&activation, pi.clone(), ri.clone()).unwrap();
    assert_eq!(p.parent.as_ref().unwrap().head(), boundary.head());
    verify_remote_activated_boundary(p, &fixture, &activation, false);
    drop(r);
    // Protected activation can have completed while local publication/reply was
    // lost. Reopen exact transition obtains the same active checkpoint.
    let pc = DurableSetCandidateV1::reopen(
        &fixture.pdir,
        sealed.clone(),
        EndpointRole::Primary,
        pp.clone(),
        64 * 1024 * 1024,
    )
    .unwrap();
    let rc = DurableSetCandidateV1::reopen(
        &next_dir,
        sealed.clone(),
        EndpointRole::Replica,
        rp.clone(),
        64 * 1024 * 1024,
    )
    .unwrap();
    let p = pc.activate(&activation, pi.clone(), ri.clone()).unwrap();
    let r = rc.activate(&activation, pi, ri).unwrap();
    let mut next = FormalCoordinator::new(
        p,
        r,
        activation.successor().clone(),
        activation.successor().clone(),
        None,
        Some(boundary.clone()),
    )
    .unwrap();
    let (mut freeze, material) = fixture.frozen(&boundary, 62);
    freeze.body.authority_id = activation.successor().authority_id().unwrap();
    freeze.body.epoch = 2;
    freeze.signature_hex = signed(frozen_signing_bytes(&freeze.body).unwrap(), 4);
    let executable = next.freeze(freeze, material).unwrap();
    let mut candidate = fixture.candidate(&executable);
    candidate.candidate.record.original_epoch = 2;
    candidate.candidate.writer_signature_hex = signed(
        canonical_candidate_signing_bytes(&candidate.candidate.record).unwrap(),
        4,
    );
    let child = next.submit(candidate.clone()).unwrap();
    assert_eq!(
        verify_formal_qualified(&child, activation.successor(), Some(&boundary), None)
            .unwrap()
            .head()
            .position,
        3
    );
    assert_eq!(next.submit(candidate).unwrap(), child);
    let first =
        verify_formal_qualified(&child, activation.successor(), Some(&boundary), None).unwrap();
    let (mut freeze, material) = fixture.frozen(&first, 63);
    freeze.body.authority_id = activation.successor().authority_id().unwrap();
    freeze.body.epoch = 2;
    freeze.signature_hex = signed(frozen_signing_bytes(&freeze.body).unwrap(), 4);
    let executable = next.freeze(freeze, material).unwrap();
    let mut candidate = fixture.candidate(&executable);
    candidate.candidate.record.original_epoch = 2;
    candidate.candidate.writer_signature_hex = signed(
        canonical_candidate_signing_bytes(&candidate.candidate.record).unwrap(),
        4,
    );
    next.submit(candidate).unwrap();
    drop(next);
    let mut reopened = FileFormalEndpoint::open_durable_set_active(
        &fixture.pdir,
        sealed.clone(),
        &activation,
        EndpointRole::Primary,
        pp.clone(),
        64 * 1024 * 1024,
    )
    .unwrap();
    assert_eq!(reopened.head().unwrap().position, 4);
    let FormalQueryOutcomeV1::Recorded(old) = reopened
        .query(&fixture.initial.candidate.record.request_digest)
        .unwrap()
    else {
        panic!("permanent old result missing");
    };
    assert!(matches!(*old, FormalRequestStateV1::Qualified { .. }));
    verify_remote_activated_boundary(reopened, &fixture, &activation, true);
}

fn verify_remote_activated_boundary(
    endpoint: FileFormalEndpoint,
    fixture: &Fixture,
    activation: &VerifiedDurableSetActivationV1,
    first_normal: bool,
) {
    use crate::controlled_authority::formal::transport::{
        FormalEndpointServer, RemoteFormalEndpoint,
    };
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let server = std::thread::spawn(move || {
        let mut server = FormalEndpointServer::new(endpoint, Duration::from_secs(3)).unwrap();
        while !server_stop.load(Ordering::Acquire) {
            let result = server.serve_one(&listener);
            if server_stop.load(Ordering::Acquire) {
                break;
            }
            result.unwrap();
        }
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut client = RemoteFormalEndpoint::new(
            address,
            Duration::from_secs(3),
            activation.successor().clone(),
            EndpointRole::Primary,
            key(4),
            activation.boundary().head(),
        )
        .unwrap();
        if first_normal {
            let first = client
                .restore_first_normal_with_durable_set(&fixture.authority, activation, None)
                .unwrap();
            assert_eq!(first.reference().position, 3);
            assert_eq!(first.current_head().position, 4);
            assert_eq!(
                first.proof().decision.branch,
                FormalCompletionBranchV1::Normal
            );
            assert_eq!(first.commit().record().kind, FormalRecordKindV1::Execution);
            let exact = client
                .restore_first_normal_with_durable_set(
                    &fixture.authority,
                    activation,
                    Some(&first.reference()),
                )
                .unwrap();
            assert_eq!(exact.proof(), first.proof());
            let mut wrong = first.reference();
            wrong.record_id = "ff".repeat(32);
            assert!(
                client
                    .restore_first_normal_with_durable_set(
                        &fixture.authority,
                        activation,
                        Some(&wrong)
                    )
                    .is_err()
            );
            wrong.position = 4;
            assert!(
                client
                    .restore_first_normal_with_durable_set(
                        &fixture.authority,
                        activation,
                        Some(&wrong)
                    )
                    .is_err()
            );
            return;
        }
        assert!(
            client
                .restore_native_current(&fixture.authority, None, None)
                .is_err()
        );
        let restored = client
            .restore_native_current_with_durable_set(&fixture.authority, activation, None)
            .unwrap();
        assert_eq!(restored.commit().record(), activation.boundary().record());
        assert_eq!(
            restored.active_authority().context_digest().unwrap(),
            activation.successor().context_digest().unwrap()
        );
    }));
    stop.store(true, Ordering::Release);
    let _ = TcpStream::connect(address);
    server.join().unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

fn verify_candidate_management_rpc(
    candidate: DurableSetCandidateV1,
    sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
    primary: DurableSetInstallReceiptV1,
    replica: DurableSetInstallReceiptV1,
) -> FileFormalEndpoint {
    use crate::controlled_authority::formal::transport::{
        DurableSetCandidateServerV1, RemoteDurableSetCandidateV1,
    };
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let server_sealed = sealed.clone();
    let server = std::thread::spawn(move || {
        let mut server = DurableSetCandidateServerV1::new(
            candidate,
            server_sealed,
            EndpointRole::Primary,
            Duration::from_secs(3),
        )
        .unwrap();
        let mut active = None;
        while !server_stop.load(Ordering::Acquire) {
            let result = server.serve_one(&listener);
            if server_stop.load(Ordering::Acquire) {
                break;
            }
            if let Some(endpoint) = result.unwrap() {
                assert!(active.is_none());
                active = Some(endpoint);
            }
        }
        active
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(
            RemoteDurableSetCandidateV1::new(
                address,
                Duration::from_secs(3),
                sealed.clone(),
                EndpointRole::Primary,
                key(19)
            )
            .is_err()
        );
        let mut client = RemoteDurableSetCandidateV1::new(
            address,
            Duration::from_secs(3),
            sealed,
            EndpointRole::Primary,
            key(4),
        )
        .unwrap();
        assert_eq!(client.query_install().unwrap(), primary);
        assert_eq!(client.install().unwrap(), primary);
        assert_eq!(
            client.activate(primary.clone(), replica.clone()).unwrap(),
            primary
        );
        assert_eq!(client.activate(primary.clone(), replica).unwrap(), primary);
        assert_eq!(client.query_install().unwrap(), primary);
    }));
    stop.store(true, Ordering::Release);
    let _ = TcpStream::connect(address);
    let active = server.join().unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    active.expect("protected dual activation returned owner")
}

struct TestSourceOwner {
    stop: Arc<std::sync::atomic::AtomicBool>,
    address: std::net::SocketAddr,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Drop for TestSourceOwner {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        let _ = std::net::TcpStream::connect(self.address);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn remote_sealed_source(
    endpoint: FileFormalEndpoint,
    fixture: &Fixture,
    boundary: &VerifiedFormalCommitV1,
    sealed: &VerifiedSealedDurableSetTransitionV1,
) -> (Arc<dyn FormalArchiveSourceV1>, TestSourceOwner) {
    use crate::controlled_authority::formal::transport::{
        FormalEndpointServer, RemoteFormalEndpoint,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let denials = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let server_denials = denials.clone();
    let worker = std::thread::spawn(move || {
        let mut server = FormalEndpointServer::new(endpoint, Duration::from_secs(3)).unwrap();
        while !server_stop.load(Ordering::Acquire) {
            let result = server.serve_one(&listener);
            if server_stop.load(Ordering::Acquire) {
                break;
            }
            if let Err(error) = result {
                assert!(matches!(error, FormalError::Invalid(ref reason)
                    if reason == "sealed archive delegation cannot mutate"
                        || reason == "archive delegation exact protected sealed scope"));
                server_denials.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    use crate::controlled_authority::formal::transport::{
        ArchiveReadGrantBodyV1, SignedArchiveReadGrantV1, archive_read_grant_signing_bytes,
    };
    let body = ArchiveReadGrantBodyV1 {
        schema_version: 1,
        world: fixture.authority.definition().world.clone(),
        old_authority_id: fixture.authority.authority_id().unwrap(),
        source_instance_digest: fixture
            .authority
            .definition()
            .replica
            .instance
            .digest()
            .unwrap(),
        transition_digest: sealed.transition().digest().into(),
        sealed_cut: boundary.head(),
        next_epoch_floor: sealed.transition().body().successor.epoch,
        target_instance_digest: sealed
            .transition()
            .body()
            .successor
            .replica
            .instance
            .digest()
            .unwrap(),
        reader_public_key: public(17),
    };
    let grant = SignedArchiveReadGrantV1 {
        writer_signature_hex: signed(archive_read_grant_signing_bytes(&body).unwrap(), 4),
        body,
    };
    let target = grant.body.target_instance_digest.clone();
    assert!(
        RemoteFormalEndpoint::new_archive_reader(
            address,
            Duration::from_secs(3),
            fixture.authority.clone(),
            EndpointRole::Replica,
            key(17),
            grant.clone(),
            &h(199)
        )
        .is_err()
    );
    assert!(
        RemoteFormalEndpoint::new_archive_reader(
            address,
            Duration::from_secs(3),
            fixture.authority.clone(),
            EndpointRole::Replica,
            key(4),
            grant.clone(),
            &target
        )
        .is_err()
    );
    // A writer signature alone cannot widen the endpoint's actual sealed cut.
    let mut wrong_scope = grant.clone();
    wrong_scope.body.sealed_cut.record_id = h(198);
    wrong_scope.writer_signature_hex = signed(
        archive_read_grant_signing_bytes(&wrong_scope.body).unwrap(),
        4,
    );
    let mut wrong_scope_client = RemoteFormalEndpoint::new_archive_reader(
        address,
        Duration::from_secs(3),
        fixture.authority.clone(),
        EndpointRole::Replica,
        key(17),
        wrong_scope,
        &target,
    )
    .unwrap();
    let empty_root =
        crate::controlled_authority::formal::business_index::BusinessIndexRootV1::empty().unwrap();
    assert!(
        wrong_scope_client
            .business_index_path(&empty_root, &h(197))
            .is_err()
    );
    assert_eq!(denials.load(Ordering::SeqCst), 1);

    let mut client = RemoteFormalEndpoint::new_archive_reader(
        address,
        Duration::from_secs(3),
        fixture.authority.clone(),
        EndpointRole::Replica,
        key(17),
        grant,
        &target,
    )
    .unwrap();
    let delegated_path = client.business_index_path(&empty_root, &h(197)).unwrap();
    assert!(
        delegated_path
            .verify(&empty_root, &h(197))
            .unwrap()
            .is_none()
    );
    let checkpoint = client.checkpoint().unwrap();
    assert!(client.seal_epoch(h(200), 3, checkpoint).is_err());
    assert_eq!(denials.load(Ordering::SeqCst), 2);
    (
        client.into_archive_source(),
        TestSourceOwner {
            stop,
            address,
            worker: Some(worker),
        },
    )
}
