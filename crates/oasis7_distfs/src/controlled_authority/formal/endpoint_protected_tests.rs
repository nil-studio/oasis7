//! Loopback oracle simulation. Its in-memory floor and test-only exportable keys
//! verify protocol plumbing; they provide NO real physical fencing evidence.
use super::*;
use fencing::*;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicBool;
use std::time::Duration;
#[derive(Clone)]
struct Ledger {
    checkpoint: EndpointCheckpointBindingV1,
    native: Vec<u8>,
    pending: Option<CheckpointIntentV1>,
    seal: Option<EpochSealBodyV1>,
}
#[path = "endpoint_test_oracle_frame.rs"]
mod oracle_frame;
use oracle_frame::recv;

#[test]
fn oracle_idle_is_retryable_but_started_frames_are_bounded() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    server
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    assert_eq!(recv::<u8>(&mut server), Ok(None));
    client.write_all(&[0]).unwrap();
    assert!(recv::<u8>(&mut server).is_err());
}

fn send(stream: &mut TcpStream, reply: &SignedMonotonicOracleReplyV1) -> Result<(), ()> {
    let bytes = serde_cbor::to_vec(reply).unwrap();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| stream.write_all(&bytes))
        .map_err(|_| ())
}
#[path = "endpoint_test_oracle.rs"]
mod oracle;
use oracle::Oracle;
#[cfg(unix)]
#[path = "endpoint_recovery_process_tests.rs"]
mod recovery_process_tests;
#[path = "endpoint_rpc_read_tests.rs"]
mod rpc_read_tests;

#[test]
fn simulated_floor_only_oracle_cannot_open_protected_endpoint() {
    let fixture = Fixture::new();
    let oracle = Oracle::start_with_custody(&fixture, EndpointRole::Primary, false);
    let result = FileFormalEndpoint::open_protected(
        &fixture.pdir,
        FormalEndpointAuthorizationV1 {
            authority: fixture.authority.clone(),
            original: fixture.authority.clone(),
            recovery: None,
            parent: None,
        },
        EndpointRole::Primary,
        oracle.provider(&fixture),
        None,
    );
    assert!(matches!(result, Err(FormalError::FencingUnproven)));
    assert!(!fixture.pdir.join("formal-checkpoint-v1.cbor").exists());
}
#[test]
fn simulated_remote_protected_signer_qualifies_reopens_and_seals_without_local_role_key() {
    let fixture = Fixture::new();
    let primary_oracle = Oracle::start(&fixture, EndpointRole::Primary);
    let replica_oracle = Oracle::start(&fixture, EndpointRole::Replica);
    let open = |role, oracle: &Oracle| {
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
            oracle.provider(&fixture),
            None,
        )
        .unwrap()
    };
    let mut coordinator = FormalCoordinator::new(
        open(EndpointRole::Primary, &primary_oracle),
        open(EndpointRole::Replica, &replica_oracle),
        fixture.authority.clone(),
        fixture.authority.clone(),
        None,
        None,
    )
    .unwrap();
    let result = coordinator.submit(fixture.initial.clone()).unwrap();
    assert_eq!(result.decision.record.position, 1);
    let first = verify_formal_qualified(&result, &fixture.authority, None, None).unwrap();
    let (frozen, material) = fixture.frozen(&first, 33);
    let executable = coordinator.freeze(frozen, material).unwrap();
    let second = coordinator.submit(fixture.candidate(&executable)).unwrap();
    assert_eq!(second.decision.record.position, 2);

    drop(coordinator);
    let mut primary = open(EndpointRole::Primary, &primary_oracle);
    assert!(matches!(
        primary
            .query(&fixture.initial.candidate.record.request_digest)
            .unwrap(),
        FormalQueryOutcomeV1::Recorded(entry) if matches!(*entry, FormalRequestStateV1::Qualified {..})
    ));
    let second_verified =
        verify_formal_qualified(&second, &fixture.authority, Some(&first), None).unwrap();
    let (pending_input, pending_material) = fixture.frozen(&second_verified, 34);
    primary.store.fault = Some(1);
    assert!(
        primary
            .freeze(pending_input.clone(), pending_material.clone())
            .is_err()
    );
    drop(primary);
    let mut primary = open(EndpointRole::Primary, &primary_oracle);
    assert!(primary.freeze(pending_input, pending_material).is_ok());
    let response = primary
        .sign_formal_rpc_response(
            h(233),
            h(234),
            first.head(),
            Ok(super::super::super::transport::Value::Complete),
        )
        .unwrap();
    super::super::native_rpc::verify_formal_rpc_checkpoint(
        &response.body,
        &fixture.authority,
        EndpointRole::Primary,
    )
    .unwrap();
    let mut wrong = response.body.clone();
    wrong.minimum.record_id = h(235);
    assert!(
        super::super::native_rpc::verify_formal_rpc_checkpoint(
            &wrong,
            &fixture.authority,
            EndpointRole::Primary
        )
        .is_err()
    );
    wrong = response.body.clone();
    wrong.ancestry[0][0] ^= 1;
    assert!(
        super::super::native_rpc::verify_formal_rpc_checkpoint(
            &wrong,
            &fixture.authority,
            EndpointRole::Primary
        )
        .is_err()
    );
    wrong = response.body.clone();
    wrong.ancestry.clear();
    assert!(
        super::super::native_rpc::verify_formal_rpc_checkpoint(
            &wrong,
            &fixture.authority,
            EndpointRole::Primary
        )
        .is_err()
    );
    let checkpoint = primary.checkpoint().unwrap();
    let seal = primary.seal_epoch(&h(231), 2, &checkpoint).unwrap();
    assert!(
        verify_epoch_seal(
            &seal,
            &TrustedFencingPolicyV1 {
                provider_public_key: public(3),
                authorized_resource_bindings: fixture
                    .authority
                    .policy()
                    .authorized_fencing_resource_bindings
                    .clone()
            }
        )
        .is_ok()
    );
    let mut unsigned = seal.clone();
    unsigned.protected_signature = None;
    assert!(
        verify_epoch_seal(
            &unsigned,
            &TrustedFencingPolicyV1 {
                provider_public_key: public(3),
                authorized_resource_bindings: fixture
                    .authority
                    .policy()
                    .authorized_fencing_resource_bindings
                    .clone()
            }
        )
        .is_err()
    );
    let index_path = fixture
        .pdir
        .join("formal-objects-v1")
        .join(primary.store.body.index.as_ref().unwrap());
    let original_index = fs::read(&index_path).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_stop = stop.clone();
    let expected_corruption = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_expected_corruption = expected_corruption.clone();
    let server_thread = std::thread::spawn(move || {
        let mut server = super::super::super::transport::FormalEndpointServer::new(
            primary,
            Duration::from_secs(3),
        )
        .unwrap();
        while !server_stop.load(std::sync::atomic::Ordering::Acquire) {
            if let Err(error) = server.serve_one(&listener) {
                assert!(
                    server_stop.load(std::sync::atomic::Ordering::Acquire)
                        || server_expected_corruption.load(std::sync::atomic::Ordering::Acquire),
                    "{error:?}"
                );
                break;
            }
        }
    });
    let mut client = super::super::super::transport::RemoteFormalEndpoint::new(
        address,
        Duration::from_secs(3),
        fixture.authority.clone(),
        EndpointRole::Primary,
        key(4),
        first.head(),
    )
    .unwrap();
    assert_eq!(
        super::super::super::coordinator::FormalDurabilityEndpoint::head(&mut client)
            .unwrap()
            .position,
        2
    );
    let restored = client
        .restore_native_current(&fixture.authority, None, None)
        .unwrap();
    assert_eq!(restored.commit().head(), second_verified.head());
    assert_eq!(restored.proof(), &second);
    assert!(matches!(
        restored.pending(),
        Some(FormalRequestStateV1::Frozen { .. })
    ));
    // A second fresh restore advances this RPC client's minimum to the head.
    // Its opaque origin bridge must remain composable without rewinding RPC.
    let mut bridge_client = super::super::super::transport::RemoteFormalEndpoint::new(
        address,
        Duration::from_secs(3),
        fixture.authority.clone(),
        EndpointRole::Primary,
        key(4),
        first.head(),
    )
    .unwrap();
    let origin_bridge = bridge_client
        .restore_native_current(&fixture.authority, None, None)
        .unwrap();
    assert_eq!(origin_bridge.minimum(), &first.head());
    let fresh_bridge = bridge_client
        .restore_native_current(&fixture.authority, None, None)
        .unwrap();
    super::super::native_restore::assert_bridge_negatives(&origin_bridge, &fresh_bridge);
    let historical_client = |minimum| {
        super::super::super::transport::RemoteFormalEndpoint::new(
            address,
            Duration::from_secs(3),
            fixture.authority.clone(),
            EndpointRole::Primary,
            key(4),
            minimum,
        )
        .unwrap()
    };
    let historical = historical_client(first.head())
        .restore_native_at(&first.head(), &fixture.authority, &fixture.authority, None)
        .unwrap();
    assert_eq!(historical.commit().head(), first.head());
    assert_eq!(historical.proof(), &result);
    assert_eq!(historical.current_head(), &second_verified.head());
    let located = historical_client(first.head())
        .locate_native_position(1, &fixture.authority, &fixture.authority, None)
        .unwrap();
    assert_eq!(located.reference(), first.head());
    let initial_body = fixture.authority.initial_body().cloned().unwrap();
    let initial_certificate = SignedInitialActivationV1 {
        signature_hex: signed(initial_activation_signing_bytes(&initial_body).unwrap(), 9),
        body: initial_body,
    };
    let trusted = VerifiedFormalReadAnchorV1::from_initial(
        &initial_certificate,
        &InitialActivationTrustV1 {
            issuer_public_key: public(9),
            world: fixture.authority.definition().world.clone(),
            initial_state_root: initial_certificate.body.initial_state_root.clone(),
            governing_manifest_digest: initial_certificate.body.governing_manifest_digest.clone(),
            recovery_policy_digest: fixture.authority.policy().digest().unwrap(),
        },
        &result,
    )
    .unwrap();
    assert_eq!(
        historical
            .service_read_anchor(&trusted)
            .unwrap()
            .head()
            .head(),
        first.head()
    );
    let publication = historical.service_read_anchor(&trusted).unwrap();
    assert_eq!(publication.checkpoint(), trusted.checkpoint());
    assert_eq!(
        publication.authority().context_digest().unwrap(),
        historical.authority().context_digest().unwrap()
    );
    assert_eq!(publication.head().record(), historical.commit().record());
    assert_eq!(
        publication.head().finalizing_authority_id(),
        historical.commit().finalizing_authority_id()
    );
    let later_trusted = trusted
        .after_verified_commit(fixture.authority.clone(), second_verified.clone())
        .unwrap();
    let before_pin = historical_client(second_verified.head())
        .locate_native_position(1, &fixture.authority, &fixture.authority, None)
        .unwrap();
    assert!(before_pin.service_read_anchor(&later_trusted).is_err());
    assert!(
        historical_client(first.head())
            .restore_native_at(
                &FormalHeadRefV1 {
                    position: 1,
                    record_id: h(250)
                },
                &fixture.authority,
                &fixture.authority,
                None
            )
            .is_err()
    );
    expected_corruption.store(true, std::sync::atomic::Ordering::Release);
    fs::write(&index_path, b"corrupt-authenticated-index").unwrap();
    assert!(
        client
            .restore_native_current(&fixture.authority, None, None)
            .is_err()
    );
    fs::write(&index_path, original_index).unwrap();
    stop.store(true, std::sync::atomic::Ordering::Release);
    let _ = TcpStream::connect(address);
    server_thread.join().unwrap();
    drop(primary_oracle);
    drop(replica_oracle);
}

#[cfg(unix)]
struct EndpointChild(std::process::Child);
#[cfg(unix)]
impl std::ops::Deref for EndpointChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
#[cfg(unix)]
impl std::ops::DerefMut for EndpointChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
#[cfg(unix)]
impl Drop for EndpointChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(unix)]
const ENDPOINT_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[cfg(unix)]
fn read_endpoint_ready_address(child: &mut EndpointChild) -> Result<std::net::SocketAddr, String> {
    use std::io::BufRead;
    use std::sync::mpsc::{self, RecvTimeoutError};

    let Some(stdout) = child.stdout.take() else {
        let kill = child.kill();
        let status = child.wait();
        return Err(format!(
            "endpoint child stdout was not piped; kill={kill:?}; exit={status:?}"
        ));
    };
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let reader = match std::thread::Builder::new()
        .name("formal-endpoint-ready-reader".into())
        .spawn(move || {
            let mut line = String::new();
            let result = match std::io::BufReader::new(stdout).read_line(&mut line) {
                Ok(bytes) => Ok((bytes, line)),
                Err(error) => Err(format!("{error}; partial stdout={line:?}")),
            };
            let _ = ready_tx.send(result);
        }) {
        Ok(reader) => reader,
        Err(error) => {
            let kill = child.kill();
            let status = child.wait();
            return Err(format!(
                "could not start endpoint readiness reader: {error}; kill={kill:?}; exit={status:?}"
            ));
        }
    };

    let (bytes, line) = match ready_rx.recv_timeout(ENDPOINT_READY_TIMEOUT) {
        Ok(Ok((bytes, line))) if bytes > 0 => (bytes, line),
        Ok(Ok((_, line))) => {
            let kill = child.kill();
            let status = child.wait();
            let _ = reader.join();
            return Err(format!(
                "endpoint child closed stdout before readiness; output={line:?}; kill={kill:?}; exit={status:?}"
            ));
        }
        Ok(Err(error)) => {
            let kill = child.kill();
            let status = child.wait();
            let _ = reader.join();
            return Err(format!(
                "could not read endpoint readiness line: {error}; kill={kill:?}; exit={status:?}"
            ));
        }
        Err(RecvTimeoutError::Timeout) => {
            let kill = child.kill();
            let status = child.wait();
            let partial = ready_rx.recv_timeout(std::time::Duration::from_secs(1));
            let (diagnostic, reader_finished) = match partial {
                Ok(Ok((_, line))) => (format!("stdout={line:?}"), true),
                Ok(Err(error)) => (format!("stdout read error={error}"), true),
                Err(RecvTimeoutError::Timeout) => (
                    "stdout reader did not finish after child termination".into(),
                    false,
                ),
                Err(RecvTimeoutError::Disconnected) => (
                    "stdout reader disconnected after child termination".into(),
                    true,
                ),
            };
            if reader_finished {
                let _ = reader.join();
            }
            return Err(format!(
                "endpoint readiness timed out after {:?}; kill={kill:?}; exit={status:?}; {diagnostic}",
                ENDPOINT_READY_TIMEOUT
            ));
        }
        Err(RecvTimeoutError::Disconnected) => {
            let kill = child.kill();
            let status = child.wait();
            let reader_failed = reader.join().is_err();
            return Err(format!(
                "endpoint readiness reader disconnected (panicked={reader_failed}); kill={kill:?}; exit={status:?}"
            ));
        }
    };
    if reader.join().is_err() {
        let kill = child.kill();
        let status = child.wait();
        return Err(format!(
            "endpoint readiness reader panicked; output={line:?}; kill={kill:?}; exit={status:?}"
        ));
    }
    if !line.ends_with('\n') {
        let kill = child.kill();
        let status = child.wait();
        return Err(format!(
            "endpoint readiness output was not newline terminated (bytes={bytes}); output={line:?}; kill={kill:?}; exit={status:?}"
        ));
    }
    let address: Result<std::net::SocketAddr, String> = line
        .trim()
        .strip_prefix("formal_endpoint_listening=")
        .ok_or_else(|| format!("unexpected endpoint readiness output: {line:?}"))
        .and_then(|address| {
            address
                .parse()
                .map_err(|error| format!("invalid endpoint readiness address in {line:?}: {error}"))
        });
    match address {
        Ok(address) => Ok(address),
        Err(error) => {
            let kill = child.kill();
            let status = child.wait();
            Err(format!("{error}; kill={kill:?}; exit={status:?}"))
        }
    }
}

#[test]
#[ignore = "requires real endpoint binary path in OASIS7_FORMAL_ENDPOINT_TEST_BIN; simulated oracle only"]
#[cfg(unix)]
fn formal_endpoint_child_process_dual_qualification_sigkill_reopen_original() {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::process::{Command, Stdio};
    let binary = std::env::var_os("OASIS7_FORMAL_ENDPOINT_TEST_BIN")
        .expect("explicit built endpoint executable");
    let (fixture, _) = super::business_tests::fixture();
    let primary_oracle = Oracle::start(&fixture, EndpointRole::Primary);
    let replica_oracle = Oracle::start(&fixture, EndpointRole::Replica);
    let credential = fixture.path.join("oracle-auth");
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&credential)
        .unwrap();
    file.write_all(hex::encode([17u8; 32]).as_bytes()).unwrap();
    drop(file);
    let body = fixture.authority.initial_body().cloned().unwrap();
    let certificate = SignedInitialActivationV1 {
        signature_hex: signed(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let trust = InitialActivationTrustV1 {
        issuer_public_key: public(9),
        world: fixture.authority.definition().world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: fixture.authority.policy().digest().unwrap(),
    };
    let mut configs = Vec::new();
    for (role, dir, oracle) in [
        (EndpointRole::Primary, &fixture.pdir, &primary_oracle),
        (EndpointRole::Replica, &fixture.rdir, &replica_oracle),
    ] {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
        let config = fixture.path.join(format!("{role:?}.json"));
        fs::write(&config,serde_json::to_vec(&serde_json::json!({"schema_version":2,"initial_trust":trust,"initial_certificate":certificate,"initial_qualified":null,"role":role,"data_dir":dir,"oracle_authentication_key_file":credential,"oracle_address":oracle.address,"listen":"127.0.0.1:0","timeout_ms":3000})).unwrap()).unwrap();
        configs.push(config);
    }
    let start = |config: &Path| {
        let child = Command::new(&binary)
            .arg("--config")
            .arg(config)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut child = EndpointChild(child);
        let address = read_endpoint_ready_address(&mut child)
            .unwrap_or_else(|error| panic!("strict endpoint readiness failed: {error}"));
        (child, address)
    };
    let (mut pchild, paddr) = start(&configs[0]);
    let (mut rchild, raddr) = start(&configs[1]);
    let remote = |address, role| {
        super::super::super::transport::RemoteFormalEndpoint::new(
            address,
            Duration::from_secs(3),
            fixture.authority.clone(),
            role,
            key(4),
            fixture.authority.definition().world.genesis_head().unwrap(),
        )
        .unwrap()
    };
    let mut coordinator = FormalCoordinator::new(
        remote(paddr, EndpointRole::Primary),
        remote(raddr, EndpointRole::Replica),
        fixture.authority.clone(),
        fixture.authority.clone(),
        None,
        None,
    )
    .unwrap();
    let original = coordinator.submit(fixture.initial.clone()).unwrap();
    drop(coordinator);
    pchild.kill().unwrap();
    pchild.wait().unwrap();
    rchild.kill().unwrap();
    rchild.wait().unwrap();
    let (mut pchild, paddr) = start(&configs[0]);
    let (mut rchild, raddr) = start(&configs[1]);
    let parent = verify_formal_qualified(&original, &fixture.authority, None, None).unwrap();
    let mut coordinator = FormalCoordinator::new(
        remote(paddr, EndpointRole::Primary),
        remote(raddr, EndpointRole::Replica),
        fixture.authority.clone(),
        fixture.authority.clone(),
        None,
        Some(parent),
    )
    .unwrap();
    assert_eq!(
        coordinator.submit(fixture.initial.clone()).unwrap(),
        original
    );
    drop(coordinator);
    pchild.kill().unwrap();
    pchild.wait().unwrap();
    rchild.kill().unwrap();
    rchild.wait().unwrap();
    drop(primary_oracle);
    drop(replica_oracle);
    let failed_fixture = Fixture::new();
    let weak_oracle = Oracle::start_with_custody(&failed_fixture, EndpointRole::Primary, false);
    fs::set_permissions(&failed_fixture.pdir, fs::Permissions::from_mode(0o700)).unwrap();
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&configs[0]).unwrap()).unwrap();
    let weak_body = failed_fixture.authority.initial_body().cloned().unwrap();
    let weak_certificate = SignedInitialActivationV1 {
        signature_hex: signed(initial_activation_signing_bytes(&weak_body).unwrap(), 9),
        body: weak_body,
    };
    let weak_trust = InitialActivationTrustV1 {
        issuer_public_key: public(9),
        world: failed_fixture.authority.definition().world.clone(),
        initial_state_root: weak_certificate.body.initial_state_root.clone(),
        governing_manifest_digest: weak_certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: failed_fixture.authority.policy().digest().unwrap(),
    };
    config["initial_certificate"] = serde_json::to_value(weak_certificate).unwrap();
    config["initial_trust"] = serde_json::to_value(weak_trust).unwrap();
    config["data_dir"] = serde_json::to_value(&failed_fixture.pdir).unwrap();
    config["oracle_address"] = serde_json::to_value(weak_oracle.address).unwrap();
    let failed_config = failed_fixture.path.join("weak.json");
    fs::write(&failed_config, serde_json::to_vec(&config).unwrap()).unwrap();
    let failed = Command::new(&binary)
        .arg("--config")
        .arg(failed_config)
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(!String::from_utf8_lossy(&failed.stdout).contains("formal_endpoint_listening="));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("FENCING_UNPROVEN"));
    assert!(
        !failed_fixture
            .pdir
            .join("formal-checkpoint-v1.cbor")
            .exists()
    );
    drop(weak_oracle);
}
#[path = "endpoint_committed_material_rpc_tests.rs"]
mod committed_material_rpc_tests;
#[cfg(unix)]
#[path = "endpoint_pack_partial_crash_tests.rs"]
mod pack_partial_crash_tests;

#[path = "endpoint_durable_set_seal_tests.rs"]
mod durable_set_seal_tests;

#[path = "endpoint_durable_set_install_tests.rs"]
mod durable_set_install_tests;

/// Test-only oracle custody; this supplies no real physical-domain evidence.
pub(super) fn native_rpc_endpoint(fixture: &Fixture) -> (FileFormalEndpoint, impl Drop) {
    let oracle = Oracle::start(fixture, EndpointRole::Primary);
    let endpoint = FileFormalEndpoint::open_protected(
        &fixture.pdir,
        FormalEndpointAuthorizationV1 {
            authority: fixture.authority.clone(),
            original: fixture.authority.clone(),
            recovery: None,
            parent: None,
        },
        EndpointRole::Primary,
        oracle.provider(fixture),
        None,
    )
    .unwrap();
    (endpoint, oracle)
}
