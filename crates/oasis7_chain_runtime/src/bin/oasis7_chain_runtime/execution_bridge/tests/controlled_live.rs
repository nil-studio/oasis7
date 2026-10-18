//! Real local engineering fixtures only. These directories do not establish
//! independent hardware fault domains, a formal world or production readiness.
use super::super::controlled_live::*;
use super::super::{controlled_bootstrap_anchor, controlled_live_history as history};
use crate::controlled_live_config::{ENGINEERING_SCOPE, GuardedAuthority, GuardedConfiguration};
use ed25519_dalek::SigningKey;
use oasis7::world_service::*;
use oasis7_distfs::controlled_authority::replicated_protocol::{
    DurabilityEvidence, FixedTrust, HeadAnchor, verify_evidence,
};
use oasis7_node::{NodeConfig, NodeRole, NodeRuntime};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    config: PathBuf,
    records: PathBuf,
    replica: PathBuf,
    trust: FixedTrust,
    initial: HeadAnchor,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "oasis7-guarded-live-fixture-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let (config_bytes, evidence_bytes) = controlled_bootstrap_anchor::tests::live_cli_fixture();
        let evidence: DurabilityEvidence = serde_json::from_slice(&evidence_bytes).unwrap();
        let trust = evidence.proposal.body.trust.clone();
        let initial = verify_evidence(&evidence, &trust, &trust.genesis_anchor().unwrap()).unwrap();
        let primary = root.join("primary");
        let replica = root.join("replica");
        for (path, role, n) in [
            (&primary, EndpointRole::Primary, 2),
            (&replica, EndpointRole::Replica, 3),
        ] {
            drop(
                FileEndpoint::restore(
                    path,
                    trust.clone(),
                    role,
                    SigningKey::from_bytes(&[n; 32]),
                    &initial,
                    std::slice::from_ref(&evidence),
                )
                .unwrap(),
            );
        }
        for path in [&primary, &replica] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let trusted = root.join("trusted.json");
        let original = root.join("activation.json");
        fs::write(&trusted, config_bytes).unwrap();
        fs::write(&original, evidence_bytes).unwrap();
        let writer_key_file = root.join("writer.key");
        let primary_key_file = root.join("primary.key");
        let replica_key_file = root.join("replica.key");
        for (path, n) in [
            (&writer_key_file, 1),
            (&primary_key_file, 2),
            (&replica_key_file, 3),
        ] {
            fs::write(path, hex::encode([n; 32])).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let records = root.join("private-records");
        let config = root.join("engineering.json");
        fs::write(
            &config,
            serde_json::to_vec(&GuardedConfiguration {
                schema_version: 1,
                remote_endpoints: None,
                scope: ENGINEERING_SCOPE.into(),
                node_id: "node-a".into(),
                node_keypair_directory: root.join("node-identity"),
                records_directory: records.clone(),
                status_bind: "127.0.0.1:0".parse().unwrap(),
                node_tick_ms: 1000,
                trusted_bootstrap_config: trusted,
                activation_evidence: original,
                primary_directory: primary,
                replica_directory: replica.clone(),
                writer_key_file,
                primary_key_file,
                replica_key_file,
                minimum_runtime_head: initial.clone(),
            })
            .unwrap(),
        )
        .unwrap();
        Self {
            root,
            config,
            records,
            replica,
            trust,
            initial,
        }
    }
    fn driver(&self) -> GuardedExecutionDriver {
        GuardedExecutionDriver::new(
            GuardedAuthority::load(&self.config).unwrap(),
            self.records.clone(),
        )
        .unwrap()
    }
    fn request(&self) -> SubmitIntentRequest<WorldServicePayloadV1> {
        let owner = hex::encode([7; 32]);
        let change = AgentSignerDelegationChangeV1 {
            world_id: self.trust.world_id.clone(),
            branch_id: "main".into(),
            agent_id: "agent-a".into(),
            owner_binding: "owner-a".into(),
            agent_identity_generation: 1,
            generation: 1,
            delegate_public_key: sign_read_request("delegate", (), &hex::encode([8; 32]))
                .unwrap()
                .subject_public_key,
            revoked: false,
            nonce: 1,
        };
        let signed_payload = WorldServicePayloadV1::Delegation(
            sign_read_request("delegation", change, &owner).unwrap(),
        );
        SubmitIntentRequest {
            contract_version: 1,
            correlation: derive_correlation(
                WorldIdentity {
                    world_id: self.trust.world_id.clone(),
                    genesis_digest: self.trust.genesis_digest.clone(),
                },
                &signed_payload,
            )
            .unwrap(),
            deadline_unix_ms: None,
            signed_payload,
        }
    }
    fn node(&self, driver: GuardedExecutionDriver) -> NodeRuntime {
        let bootstrap = driver.bootstrap().unwrap();
        NodeRuntime::new(
            NodeConfig::new("node-a", &self.trust.world_id, NodeRole::Sequencer)
                .unwrap()
                .with_tick_interval(Duration::from_millis(1000))
                .unwrap()
                .with_auto_attest_all_validators(true)
                .with_require_execution_on_commit(true),
        )
        .with_local_execution_bootstrap(bootstrap)
        .with_execution_hook(driver)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn file_image(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn visit(
        base: &Path,
        directory: &Path,
        output: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(base, &path, output);
            } else {
                output.insert(
                    path.strip_prefix(base).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut output = std::collections::BTreeMap::new();
    visit(root, root, &mut output);
    output
}
#[track_caller]
fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "guarded fixture did not reach expected state"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[cfg(all(unix, feature = "wasmtime"))]
fn actual_node_unknown_restart_preserves_original_signed_request_and_private_predecessor() {
    let fixture = Fixture::new();
    let authority = GuardedAuthority::load(&fixture.config).unwrap();
    let before = history::load_head(&authority, &fixture.records, None).unwrap();
    let original_world = fixture.root.join("original-world");
    before
        .world(&authority.release_policy)
        .unwrap()
        .save_to_dir(&original_world)
        .unwrap();
    let original_bytes = file_image(&original_world);
    let driver = fixture.driver();
    let fence = driver.read_authority();
    let displaced = fixture.root.join("fixture-unavailable-replica");
    fs::rename(&fixture.replica, &displaced).unwrap();
    // A real ENOTDIR I/O failure makes status unknown at any process privilege;
    // local missing-record absence alone would instead be a protocol rejection.
    fs::write(&fixture.replica, b"fixture transport unavailable").unwrap();
    let request = fixture.request();
    let action = correlation::encode_consensus_intent(&request).unwrap();
    let mut node = fixture.node(driver);
    node.submit_consensus_action_payload(42, action).unwrap();
    node.start().unwrap();
    wait_until(|| {
        fs::read(pending_path(&fixture.records))
            .ok()
            .and_then(|b| capture::decode_generic::<PendingTransaction>(&b).ok())
            .is_some_and(|p| p.candidate_ref.is_some())
    });
    let pending_bytes = fs::read(pending_path(&fixture.records)).unwrap();
    let original: PendingTransaction = capture::decode_generic(&pending_bytes).unwrap();
    assert_eq!(original.continuation.context.height, 42);
    assert_eq!(node.snapshot().consensus.committed_height, 41);
    assert_eq!(fence.current().unwrap(), fixture.initial);
    assert!(!history::evidence_path(&fixture.records, 2).exists());
    assert_eq!(file_image(&original_world), original_bytes);
    let candidate = capture::load_package(
        &LocalCasStore::new(fixture.records.join("controlled-private-cas")),
        original.candidate_ref.as_deref().unwrap(),
    )
    .unwrap();
    let input = capture::object(
        &candidate,
        &candidate.roots
            [&oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::Input],
    )
    .unwrap();
    assert_eq!(
        input,
        to_cbor(&original.continuation.context.committed_actions).unwrap()
    );
    let candidate_digest = oasis7_runtime::blake3_hex(&to_cbor(&candidate).unwrap());
    node.stop().unwrap();
    drop(node);
    fs::remove_file(&fixture.replica).unwrap();
    fs::rename(displaced, &fixture.replica).unwrap();
    let mut incomplete = original.clone();
    incomplete.candidate_ref = None;
    fs::write(
        pending_path(&fixture.records),
        to_cbor(&incomplete).unwrap(),
    )
    .unwrap();
    let blocked_bytes = file_image(&fixture.records);
    assert!(
        GuardedExecutionDriver::new(
            GuardedAuthority::load(&fixture.config).unwrap(),
            fixture.records.clone()
        )
        .is_err()
    );
    assert_eq!(file_image(&fixture.records), blocked_bytes);
    fs::write(pending_path(&fixture.records), &pending_bytes).unwrap();
    let mut driver = fixture.driver();
    assert_eq!(
        driver.pending_local_continuation().unwrap().unwrap(),
        original.continuation
    );
    let fence = driver.read_authority();
    let mut restarted = fixture.node(driver);
    restarted.start().unwrap();
    let mut previous_error = None;
    wait_until(|| {
        let snapshot = restarted.snapshot();
        if snapshot.last_error != previous_error {
            eprintln!("guarded restart diagnostic: {:?}", snapshot.last_error);
            previous_error = snapshot.last_error;
        }
        snapshot.consensus.committed_height == 42
    });
    restarted.stop().unwrap();
    drop(restarted);
    let evidence_bytes = fs::read(history::evidence_path(&fixture.records, 2)).unwrap();
    let evidence: DurabilityEvidence = serde_json::from_slice(&evidence_bytes).unwrap();
    assert_eq!(
        oasis7_runtime::blake3_hex(&to_cbor(&evidence.proposal.body.record).unwrap()),
        candidate_digest
    );
    let completed: PendingTransaction =
        capture::decode_generic(&fs::read(pending_path(&fixture.records)).unwrap()).unwrap();
    assert_eq!(completed.continuation, original.continuation);
    assert_eq!(completed.candidate_ref, original.candidate_ref);
    assert!(completed.published_head.is_some());
    let qualified = history::load_head(&authority, &fixture.records, None).unwrap();
    assert_eq!(qualified.record.height, 42);
    assert_ne!(qualified.record.height, qualified.snapshot.state.time);
    let key = correlation::key_digest(&request.correlation.key).unwrap();
    assert!(
        qualified
            .snapshot
            .capability_revocation_state
            .world_service_results
            .contains_key(&key)
    );
    assert_eq!(fence.current().unwrap(), qualified.head);
    // Terminal consumption is process-local: reopening revalidates the exact
    // original checkpoint instead of trusting cleared Node counters.
    let mut retry = fixture.driver();
    assert_eq!(
        retry.pending_local_continuation().unwrap().unwrap(),
        original.continuation
    );
    let NodeExecutionCommitOutcome::Applied(result) = retry
        .on_commit_outcome_with_expected(original.continuation.context.clone(), None, None)
        .unwrap()
    else {
        panic!("retained qualified continuation must remain applied")
    };
    assert_eq!(result.execution_height, 42);
    assert_eq!(
        result.execution_block_hash,
        qualified.record.execution_block_hash
    );
    retry
        .complete_local_continuation(&original.continuation.context)
        .unwrap();
    assert!(retry.pending_local_continuation().unwrap().is_none());
    drop(retry);
    let mut reopened = fixture.driver();
    assert!(reopened.pending_local_continuation().unwrap().is_some());
    let evidence_path = history::evidence_path(&fixture.records, 2);
    fs::write(&evidence_path, b"corrupt retained proof").unwrap();
    assert!(reopened.pending_local_continuation().is_err());
    drop(reopened);
    fs::write(&evidence_path, &evidence_bytes).unwrap();
    fs::remove_file(history::evidence_path(&fixture.records, 2)).unwrap();
    assert!(
        super::super::world_service_read::pin_guarded(
            &fixture.records,
            &fixture.config,
            &fence,
            None,
            None
        )
        .is_err()
    );
    // Endpoint heads still retain the irreversible original decision. Prefix
    // truncation is never permission to execute another candidate from height41.
    assert!(
        GuardedExecutionDriver::new(
            GuardedAuthority::load(&fixture.config).unwrap(),
            fixture.records.clone()
        )
        .is_err()
    );
}

#[test]
#[cfg(all(unix, feature = "wasmtime"))]
fn guarded_read_capture_barrier_never_returns_unpublished_successor() {
    let fixture = Fixture::new();
    let authority = GuardedAuthority::load(&fixture.config).unwrap();
    let initial = history::load_head(&authority, &fixture.records, None).unwrap();
    let captured_fence = crate::controlled_live_config::GuardedReadAuthority::from_verified(
        initial.head.clone(),
        authority.configuration_digest.clone(),
    );
    let driver = fixture.driver();
    let request = fixture.request();
    let mut node = fixture.node(driver);
    node.submit_consensus_action_payload(
        42,
        correlation::encode_consensus_intent(&request).unwrap(),
    )
    .unwrap();
    node.start().unwrap();
    wait_until(|| node.snapshot().consensus.committed_height == 42);
    node.stop().unwrap();
    drop(node);
    let successor = history::load_head(&authority, &fixture.records, None).unwrap();
    let successor_commit = successor.commit(&authority).unwrap();
    let path = history::evidence_path(&fixture.records, 2);
    let bytes = fs::read(&path).unwrap();
    for (fixed, height) in [
        (None, None),
        (Some(&successor_commit), None),
        (None, Some(42)),
    ] {
        fs::remove_file(&path).unwrap();
        // Deterministically stage the real Node-qualified successor after the
        // reader captures its predecessor, before advancing publication fence.
        // No fabricated record, wall-clock race or executor replay is involved.
        let result = super::super::world_service_read::pin_guarded_with_capture_barrier(
            &fixture.records,
            &fixture.config,
            &captured_fence,
            fixed,
            height,
            || durable_transaction::write_file_durable(&path, &bytes).unwrap(),
        );
        if fixed.is_none() && height.is_none() {
            assert_eq!(result.unwrap().record.height, 41);
        } else {
            assert!(
                result.is_err(),
                "explicit pin crossed captured publication fence"
            );
        }
    }
}

#[test]
#[cfg(all(unix, feature = "wasmtime"))]
fn guarded_inputs_fail_closed_before_creating_private_records() {
    use crate::controlled_live_config::{read_existing_signing_key, require_private_owned};
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let bytes = fs::read(&fixture.config).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (field, value) in [
        ("schema_version", serde_json::json!(3)),
        ("scope", serde_json::json!("formal")),
        ("status_bind", serde_json::json!("0.0.0.0:0")),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        fs::write(&fixture.config, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(GuardedAuthority::load(&fixture.config).is_err());
        assert!(!fixture.records.exists());
    }
    fs::write(&fixture.config, &bytes).unwrap();
    let key = fixture.root.join("writer.key");
    fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(read_existing_signing_key(&key).is_err());
    assert!(
        GuardedExecutionDriver::new(
            GuardedAuthority::load(&fixture.config).unwrap(),
            fixture.records.clone()
        )
        .is_err()
    );
    assert!(!fixture.records.exists());
    fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
    let alias = fixture.root.join("key-alias");
    symlink(&key, &alias).unwrap();
    assert!(read_existing_signing_key(&alias).is_err());
    fs::set_permissions(&fixture.replica, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(require_private_owned(&fixture.replica, true).is_err());
    assert!(
        GuardedExecutionDriver::new(
            GuardedAuthority::load(&fixture.config).unwrap(),
            fixture.records.clone()
        )
        .is_err()
    );
    assert!(!fixture.records.exists());
}

#[test]
#[cfg(all(unix, feature = "wasmtime"))]
fn signed_http_pure_client_requires_explicit_prerequisite_policy_for_all_operations() {
    use oasis7::world_service::client::{
        RemoteWorldServiceClient, WorldServiceClientConfig, WorldServicePort,
    };
    use oasis7::world_service::verified_view::ReadEvidencePolicy;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{Arc, Mutex},
    };
    let fixture = Fixture::new();
    let driver = fixture.driver();
    let fence = driver.read_authority();
    let request = fixture.request();
    let mut node = fixture.node(driver);
    node.submit_consensus_action_payload(
        42,
        correlation::encode_consensus_intent(&request).unwrap(),
    )
    .unwrap();
    node.start().unwrap();
    wait_until(|| node.snapshot().consensus.committed_height == 42);
    node.stop().unwrap();
    let runtime = Arc::new(Mutex::new(node));
    let context = crate::controlled_live_runtime::GuardedConnectionContext {
        world_id: fixture.trust.world_id.clone(),
        config_path: fixture.config.clone(),
        read_authority: fence,
    };
    let private = hex::encode([9; 32]);
    let signer = crate::FeedbackSubmitSigner {
        public_key_hex: hex::encode(SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes()),
        private_key_hex: private,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let config = WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint: format!("http://{address}"),
        trusted_service_public_key: signer.public_key_hex.clone(),
        expected_world: request.correlation.key.world.clone(),
        scope_id: "agent:agent-a".into(),
        read_private_key_hex: hex::encode([7; 32]),
        timeout: Duration::from_secs(5),
        max_response_bytes: 4 * 1024 * 1024,
    };
    let records = fixture.records.clone();
    let server = thread::spawn(move || {
        for _ in 0..17 {
            let (stream, _) = listener.accept().unwrap();
            crate::status_server_support::handle_guarded_connection(
                stream, &runtime, &context, &records, &signer,
            )
            .unwrap();
        }
    });
    let mut public_config = config.clone();
    public_config.scope_id = "public".into();
    let ordinary_public = RemoteWorldServiceClient::new(public_config.clone()).unwrap();
    let diagnostic_public = RemoteWorldServiceClient::new_with_evidence_policy(
        public_config,
        ReadEvidencePolicy::AllowControlledPrerequisite,
    )
    .unwrap();
    let ordinary = RemoteWorldServiceClient::new(config.clone()).unwrap();
    let engineering = RemoteWorldServiceClient::new_with_evidence_policy(
        config,
        ReadEvidencePolicy::AllowControlledPrerequisite,
    )
    .unwrap();
    assert_ne!(
        ordinary.read_authority_identity().unwrap(),
        engineering.read_authority_identity().unwrap()
    );
    let view_request = ReadWorldViewRequest {
        observation_nonce: None,
        formal_anchor: None,
        contract_version: 1,
        world: request.correlation.key.world.clone(),
        scope_id: "agent:agent-a".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let initial = engineering.read_view(view_request.clone()).unwrap();
    let changes = ReadWorldChangesRequest {
        contract_version: 1,
        cursor: initial.continuation().clone(),
        max_items: 32,
        max_bytes: 1024 * 1024,
    };
    let lookup = LookupIntentRequest {
        contract_version: 1,
        key: request.correlation.key.clone(),
    };
    assert!(ordinary.describe().is_err());
    assert!(ordinary.submit(request.clone()).is_err());
    assert!(
        ordinary
            .lookup(lookup.clone(), request.signed_payload.clone())
            .is_err()
    );
    assert!(ordinary.read_view(view_request.clone()).is_err());
    assert!(ordinary.read_changes(changes.clone()).is_err());
    assert_eq!(
        engineering
            .describe()
            .unwrap()
            .execution_evidence_scope
            .as_deref(),
        Some(oasis7_distfs::controlled_authority::replicated_protocol::EVIDENCE_SCOPE)
    );
    let SubmitObservation::Response(response) = engineering.submit(request.clone()).unwrap() else {
        panic!("qualified fixture should have immutable result")
    };
    assert!(matches!(response.outcome, IntentOutcome::Committed { .. }));
    assert!(matches!(
        engineering
            .lookup(lookup, request.signed_payload)
            .unwrap()
            .outcome,
        IntentOutcome::Committed { .. }
    ));
    assert_eq!(
        engineering
            .read_view(view_request)
            .unwrap()
            .evidence_policy(),
        ReadEvidencePolicy::AllowControlledPrerequisite
    );
    assert_eq!(
        engineering
            .read_changes(changes)
            .unwrap()
            .execution_evidence_scope
            .as_deref(),
        Some(oasis7_distfs::controlled_authority::replicated_protocol::EVIDENCE_SCOPE)
    );
    let membership_request = ReadMembershipRequest {
        contract_version: 1,
        world: request.correlation.key.world.clone(),
        min_commit: Some(initial.version().commit.clone()),
        fixed_commit: None,
    };
    assert!(
        ordinary_public
            .read_membership(membership_request.clone())
            .is_err()
    );
    let membership = diagnostic_public
        .read_membership(membership_request)
        .unwrap();
    assert_eq!(membership.response().commit, initial.version().commit);
    assert_eq!(
        membership.response().execution_evidence_scope.as_deref(),
        Some(verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE)
    );
    assert_eq!(membership.response().applied_consensus_membership, None);
    assert!(!membership.response().supports_safe_runtime_update);
    for (path, expected) in [
        ("/checkpoint", "404"),
        (VIEW_PATH, "503"),
        (MEMBERSHIP_PATH, "503"),
    ] {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(stream, "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with(&format!("HTTP/1.1 {expected}")));
    }
    server.join().unwrap();
}

#[path = "controlled_live_remote.rs"]
mod remote;
