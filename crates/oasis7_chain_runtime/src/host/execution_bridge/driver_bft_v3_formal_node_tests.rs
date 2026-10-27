//! Object-level cold restart of real Node/World/ledger and native provider TCP.
//! The protected backend is an explicit test-only memory double; no hardware
//! persistence, independent failure domain, or peer-consensus TCP is claimed.
use super::super::super::formal_header::{
    FormalBftHeaderLedgerExecutionV3, FormalBftHeaderNodeDriverV3,
};
use super::ledger_tests::cas;
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_consensus::tendermint_v3::{
    BftHeaderHeightScheduleV3, BftHeaderValueV3, FormalBftCertifiedHeaderV3,
    VerifiedBftHeaderCertificateV3,
};
use oasis7_distfs::protected_bft_v3::*;
use oasis7_node::{
    NodeBftBuildContextV3, NodeBftExecutionCommitResultV3, NodeBftExecutionContextV3,
    NodeBftExecutionHeadV3, NodeBftPrivateCandidateV3, NodeConfig, NodeExecutionHook,
    NodeReplicationConfig, NodeRole, NodeRuntime,
};
use oasis7_runtime::LocalCasStore;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[path = "driver_bft_v3_node_identity_tests.rs"]
mod node_identity_tests;
#[path = "driver_bft_v3_node_cli_tests.rs"]
mod node_cli_tests;
#[path = "driver_bft_v3_node_publication_failure_tests.rs"]
mod node_publication_failure_tests;
#[path = "driver_bft_v3_provider_retry_tests.rs"]
mod provider_retry_tests;

struct MemoryTestSignerKey(SigningKey);

impl BftSigningKeyHandleV3 for MemoryTestSignerKey {
    fn public_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }

    fn sign_preimage(&self, preimage: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        Ok(self.0.sign(preimage).to_bytes())
    }
}

struct MemoryTestSignerBackend {
    binding_digest: String,
    snapshot: Arc<Mutex<BftSignerBackendSnapshotV3>>,
}

impl MemoryTestSignerBackend {
    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, BftSignerBackendSnapshotV3>, BftSignerErrorV3> {
        self.snapshot
            .lock()
            .map_err(|_| BftSignerErrorV3("Node test signer backend lock poisoned".into()))
    }

    fn check_binding(&self, binding_digest: &str) -> Result<(), BftSignerErrorV3> {
        if binding_digest != self.binding_digest {
            return Err(BftSignerErrorV3("Node test signer binding mismatch".into()));
        }
        Ok(())
    }

    fn check_lease(
        snapshot: &BftSignerBackendSnapshotV3,
        generation: u64,
        token_digest: &str,
    ) -> Result<(), BftSignerErrorV3> {
        if generation != snapshot.lease_generation
            || token_digest.is_empty()
            || token_digest != snapshot.lease_token_digest
        {
            return Err(BftSignerErrorV3("Node test signer lease fenced".into()));
        }
        Ok(())
    }
}

impl BftSignerBackendV3 for MemoryTestSignerBackend {
    fn load_provisioned(
        &mut self,
        binding_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.check_binding(binding_digest)?;
        Ok(self.lock()?.clone())
    }

    fn acquire_lease(
        &mut self,
        binding_digest: &str,
        expected_generation: u64,
        new_lease_token_digest: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        self.check_binding(binding_digest)?;
        let mut snapshot = self.lock()?;
        if snapshot.lease_generation != expected_generation
            || !snapshot.lease_token_digest.is_empty()
        {
            return Ok(BftSignerAcquireOutcomeV3::Conflict(snapshot.clone()));
        }
        snapshot.lease_generation = snapshot
            .lease_generation
            .checked_add(1)
            .ok_or_else(|| BftSignerErrorV3("Node test signer generation overflow".into()))?;
        snapshot.lease_token_digest = new_lease_token_digest.to_string();
        Ok(BftSignerAcquireOutcomeV3::Acquired(snapshot.clone()))
    }

    fn release_lease(
        &mut self,
        expected_generation: u64,
        expected_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let mut snapshot = self.lock()?;
        if snapshot.lease_generation == expected_generation
            && snapshot.lease_token_digest == expected_lease_token_digest
        {
            snapshot.lease_generation = snapshot
                .lease_generation
                .checked_add(1)
                .ok_or_else(|| BftSignerErrorV3("Node test signer generation overflow".into()))?;
            snapshot.lease_token_digest.clear();
        }
        Ok(snapshot.clone())
    }

    fn read_for_lease(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let snapshot = self.lock()?;
        Self::check_lease(&snapshot, lease_generation, lease_token_digest)?;
        Ok(snapshot.clone())
    }

    fn protected_time_ms(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<i64, BftSignerErrorV3> {
        let mut snapshot = self.lock()?;
        Self::check_lease(&snapshot, lease_generation, lease_token_digest)?;
        snapshot.protected_time_floor_ms = snapshot
            .protected_time_floor_ms
            .checked_add(1)
            .ok_or_else(|| BftSignerErrorV3("Node test signer clock overflow".into()))?;
        Ok(snapshot.protected_time_floor_ms)
    }

    fn compare_and_swap(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: Option<i64>,
        next_state: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let mut snapshot = self.lock()?;
        Self::check_lease(&snapshot, lease_generation, lease_token_digest)?;
        if snapshot.state.revision != expected_revision
            || snapshot.state.state_digest != expected_state_digest
            || next_state.revision
                != expected_revision
                    .checked_add(1)
                    .ok_or_else(|| BftSignerErrorV3("Node test signer revision overflow".into()))?
        {
            return Err(BftSignerErrorV3("Node test signer CAS mismatch".into()));
        }
        if let Some(event_time) = minimum_event_time_ms {
            snapshot.protected_time_floor_ms = snapshot.protected_time_floor_ms.max(event_time);
        }
        snapshot.state = next_state;
        Ok(snapshot.clone())
    }
}

struct ActualHook(Arc<Mutex<FormalBftHeaderNodeDriverV3>>);
impl NodeExecutionHook for ActualHook {
    fn on_commit(
        &mut self,
        _: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        Err("test actual header hook refuses legacy".into())
    }
    fn configure_bft_v3_execution(
        &mut self,
        b: &BftSignerBindingV3,
    ) -> Result<Option<NodeBftExecutionHeadV3>, String> {
        self.0.lock().unwrap().configure_bft_v3_execution(b)
    }
    fn current_bft_v3_execution_context(
        &mut self,
        s: &BftHeaderHeightScheduleV3,
    ) -> Result<NodeBftExecutionContextV3, String> {
        self.0.lock().unwrap().current_bft_v3_execution_context(s)
    }
    fn build_bft_v3_candidate(
        &mut self,
        c: NodeBftBuildContextV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.0.lock().unwrap().build_bft_v3_candidate(c)
    }
    fn preview_bft_v3_candidate(
        &mut self,
        c: &NodeBftExecutionContextV3,
        p: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        self.0.lock().unwrap().preview_bft_v3_candidate(c, p)
    }
    fn retain_bft_v3_private_candidate(
        &mut self,
        c: &NodeBftExecutionContextV3,
        p: &NodeBftPrivateCandidateV3,
    ) -> Result<String, String> {
        self.0.lock().unwrap().retain_bft_v3_private_candidate(c, p)
    }
    fn pending_bft_v3_private_candidate(
        &mut self,
        c: &NodeBftExecutionContextV3,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        self.0.lock().unwrap().pending_bft_v3_private_candidate(c)
    }
    fn load_bft_v3_private_candidate(
        &mut self,
        v: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.0.lock().unwrap().load_bft_v3_private_candidate(v)
    }
    fn validate_bft_v3_action_payload(&mut self, p: &[u8]) -> Result<(), String> {
        self.0.lock().unwrap().validate_bft_v3_action_payload(p)
    }
    fn validate_bft_v3_action_batch(
        &mut self,
        s: &BftHeaderHeightScheduleV3,
        a: &[oasis7_node::NodeConsensusAction],
    ) -> Result<(), String> {
        self.0.lock().unwrap().validate_bft_v3_action_batch(s, a)
    }
    fn load_bft_v3_commit_history_page(
        &mut self,
        b: &BftSignerBindingV3,
        a: u64,
        m: usize,
    ) -> Result<Vec<FormalBftCertifiedHeaderV3>, String> {
        self.0
            .lock()
            .unwrap()
            .load_bft_v3_commit_history_page(b, a, m)
    }
    fn commit_bft_v3_certificate(
        &mut self,
        t: VerifiedBftHeaderCertificateV3,
    ) -> Result<NodeBftExecutionCommitResultV3, String> {
        self.0.lock().unwrap().commit_bft_v3_certificate(t)
    }
}
fn service(
    binding: &BftSignerBindingV3,
    snapshot: Arc<Mutex<BftSignerBackendSnapshotV3>>,
) -> Arc<BftSignerServiceV3> {
    Arc::new(
        BftSignerServiceV3::open(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(116))),
            vec![Box::new(MemoryTestSignerKey(fixtures::key(120)))],
            Box::new(MemoryTestSignerBackend {
                binding_digest: binding.digest().unwrap(),
                snapshot,
            }),
        )
        .unwrap(),
    )
}
fn tcp(service: Arc<BftSignerServiceV3>) -> (SocketAddr, thread::JoinHandle<Result<(), String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let join = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut stream = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err("Node did not connect to native V3 provider".into());
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(e.to_string()),
            }
        };
        stream.set_nonblocking(false).unwrap();
        service
            .serve_connection(&mut stream)
            .map_err(|e| e.to_string())
    });
    (address, join)
}
fn driver(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Path,
    store: &LocalCasStore,
) -> Arc<Mutex<FormalBftHeaderNodeDriverV3>> {
    driver_with_source(h, root, store, h.reader.clone())
}
fn driver_with_source(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Path,
    store: &LocalCasStore,
    source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
) -> Arc<Mutex<FormalBftHeaderNodeDriverV3>> {
    Arc::new(Mutex::new(
        FormalBftHeaderNodeDriverV3::open_from_handoff(
            &root.join("ledger"),
            store.clone(),
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            source,
        )
        .unwrap(),
    ))
}
fn signer_binding(driver: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>) -> BftSignerBindingV3 {
    let d = driver.lock().unwrap();
    BftSignerBindingV3::new(
        d.handoff().clone(),
        d.initial_schedule().clone(),
        d.material_identity().into(),
        validator_id(0),
        vec![BftSignerKeyIdentityV3 {
            validator_id: validator_id(0),
            public_key_hex: hex::encode(fixtures::key(120).verifying_key().to_bytes()),
        }],
        oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
            propose_ms: 2000,
            prevote_ms: 2000,
            precommit_ms: 2000,
            round_delta_ms: 100,
        },
        "actual-node-object-restart-4975".into(),
        hex::encode(fixtures::key(117).verifying_key().to_bytes()),
    )
    .unwrap()
}
fn runtime(
    root: &Path,
    driver: Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    address: SocketAddr,
) -> NodeRuntime {
    let provider = Arc::new(
        RemoteBftSignerProviderV3::new(
            address,
            Duration::from_secs(10),
            binding.clone(),
            fixtures::key(117),
            BftSignerTrustV3 {
                schema_version: 3,
                provider_public_key: hex::encode(fixtures::key(116).verifying_key().to_bytes()),
                binding_digest: binding.digest().unwrap(),
            },
        )
        .unwrap(),
    );
    let config = NodeConfig::new(
        validator_id(0),
        binding.scope.world_id.clone(),
        NodeRole::Sequencer,
    )
    .unwrap()
    .with_tick_interval(Duration::from_millis(200))
    .unwrap()
    .with_replication(NodeReplicationConfig::new(root.join("replication")).unwrap());
    NodeRuntime::new(config)
        .with_replication_network_consensus_enabled(false)
        .with_execution_hook(ActualHook(driver))
        .with_tendermint_v3(binding.clone(), provider)
        .unwrap()
}
fn payload(id: &str) -> Vec<u8> {
    oasis7::consensus_action_payload::encode_consensus_action_payload(
        &oasis7::consensus_action_payload::ConsensusActionPayloadEnvelope::from_runtime_action(
            Action::RegisterAgent {
                agent_id: id.into(),
                pos: oasis7::geometry::GeoPos::new(4, 5, 6),
            },
        ),
    )
    .unwrap()
}
fn wait_batch(runtime: &NodeRuntime, id: u64) -> oasis7_node::NodeCommittedActionBatch {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        for batch in runtime.drain_committed_action_batches() {
            if batch.actions.iter().any(|a| a.action_id == id) {
                return batch;
            }
        }
        let status = runtime.bft_v3_status().unwrap();
        assert!(status.running, "actual Node stopped: {status:?}");
        assert!(
            Instant::now() < deadline,
            "actual Node did not commit action {id}: {status:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn wire3_formal_read_actual_node_tcp_provider_cold_object_restart_continues() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(node_case)
        .unwrap()
        .join()
        .unwrap();
}
fn node_case() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let initial_root = actual
        .lock()
        .unwrap()
        .world()
        .execution_state_identity()
        .unwrap()
        .state_root_hash;
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let (address, server) = tcp(service(&binding, snapshot.clone()));
    let mut node = runtime(root.path(), actual.clone(), &binding, address);
    let first_payload = payload("node-wire3-first-agent");
    node.start().unwrap();
    node.submit_consensus_action_payload(9001, first_payload.clone())
        .unwrap();
    let first = wait_batch(&node, 9001);
    node.stop().unwrap();
    drop(node);
    server.join().unwrap().unwrap();
    assert!(first.height >= binding.initial_schedule().height());
    assert!(
        actual
            .lock()
            .unwrap()
            .world()
            .state()
            .agents
            .contains_key("node-wire3-first-agent")
    );
    assert_ne!(
        actual
            .lock()
            .unwrap()
            .world()
            .execution_state_identity()
            .unwrap()
            .state_root_hash,
        initial_root
    );
    let protected = snapshot.lock().unwrap().clone();
    assert!(protected.state.pending_driver_apply.is_none());
    let receipt = protected
        .state
        .applied_installs
        .iter()
        .find(|r| r.height == first.height)
        .unwrap();
    assert_eq!(receipt.header_id, first.block_hash);
    assert_eq!(receipt.driver_install_digest.len(), 64);
    let first_certificate = protected
        .state
        .history
        .iter()
        .find(|c| c.execution_commitment.height == first.height)
        .unwrap()
        .clone();
    assert_eq!(
        first_certificate.header.block_id().unwrap(),
        first.block_hash
    );
    let public = serde_json::to_string(&protected.state).unwrap();
    assert!(!public.contains(r#""actions_cbor":"#));
    assert!(!public.contains(r#""payload_cbor":"#));
    assert!(!public.contains("node-wire3-first-agent"));
    let public_bytes = serde_cbor::to_vec(&protected.state).unwrap();
    let exact_private = serde_cbor::to_vec(&first.actions).unwrap();
    assert!(
        !public_bytes
            .windows(exact_private.len())
            .any(|part| part == exact_private)
    );
    let record_path = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1")
        .join(format!("record-{:020}.cbor", first.height));
    let record_bytes = fs::read(&record_path).unwrap();
    let first_history = protected.state.history.clone();
    provider_retry_tests::assert_completed_signature_retries(&binding, snapshot.clone());
    drop(actual);
    let garbage = store
        .put_bytes(b"actual-node-unretained-gc-canary")
        .unwrap();
    assert!(store.prune_orphan_blobs().unwrap() > 0);
    assert!(store.get_verified(&garbage).is_err());
    // New driver, service, remote client and Node objects reuse only the
    // retained software ledger and explicit test backend snapshot. An empty
    // external source proves the original private H closure is sufficient.
    let actual = driver_with_source(&h, root.path(), &store, Arc::new(EmptyIndexReader));
    assert_eq!(
        signer_binding(&actual).digest().unwrap(),
        binding.digest().unwrap()
    );
    let (address, server) = tcp(service(&binding, snapshot.clone()));
    let mut node = runtime(root.path(), actual.clone(), &binding, address);
    let second_payload = payload("node-wire3-second-agent");
    node.start().unwrap();
    node.submit_consensus_action_payload(9002, second_payload)
        .unwrap();
    let second = wait_batch(&node, 9002);
    node.stop().unwrap();
    drop(node);
    server.join().unwrap().unwrap();
    assert!(second.height > first.height);
    let world = actual.lock().unwrap().world().clone();
    assert!(world.state().agents.contains_key("node-wire3-first-agent"));
    assert!(world.state().agents.contains_key("node-wire3-second-agent"));
    assert_eq!(fs::read(record_path).unwrap(), record_bytes);
    let protected = snapshot.lock().unwrap().clone();
    assert_eq!(
        &protected.state.history[..first_history.len()],
        first_history.as_slice()
    );
    assert!(protected.state.pending_driver_apply.is_none());
    assert_eq!(
        protected.state.applied_installs.len(),
        protected.state.history.len()
    );
    drop(actual);
    read_generated_cut(&h, root.path(), &store, second.height, &second.block_hash);
}

fn read_generated_cut(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Path,
    store: &LocalCasStore,
    height: u64,
    id: &str,
) {
    use oasis7_client_api::world_service::FormalBftHeaderReadTargetV4;
    use oasis7_game_client::{FormalBftHeaderReadClientConfigV4, FormalBftHeaderReadClientV4};
    let mut ledger = FormalBftHeaderLedgerExecutionV3::open_from_handoff(
        &root.join("ledger"),
        store.clone(),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        Arc::new(EmptyIndexReader),
        false,
    )
    .unwrap();
    let cut = ledger.verified_cut(height, id).unwrap();
    let publication=oasis7::world_service::formal_bft_header_read_service::FormalBftHeaderReadPublicationV4::new(
        oasis7::world_service::formal_bft_header_read_service::FormalBftHeaderReadPublicationInputV4{
            handoff_anchor:h.anchor.clone(),target_config:h.target.clone(),exact_target:FormalBftHeaderReadTargetV4{height,header_id:id.into()},
            certified_prefix:cut.certified_prefix(),committed_world:cut.world(),result_tree:cut.result_tree(),
        }).unwrap();
    let key = fixtures::key(115);
    let public = hex::encode(key.verifying_key().to_bytes());
    let port = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = port.local_addr().unwrap();
    drop(port);
    let mut server = Arc::new(
        crate::formal_bft_header_http::FormalBftHeaderReadHttpServiceV4::new(
            publication,
            key,
            None,
        ),
    )
    .serve_loopback(address)
    .unwrap();
    let client = FormalBftHeaderReadClientV4::new(
        FormalBftHeaderReadClientConfigV4 {
            endpoint: format!("http://{address}"),
            trusted_service_public_key: public,
            read_private_key_hex: hex::encode(fixtures::key(114).to_bytes()),
            timeout: Duration::from_secs(10),
            scope_id: "public".into(),
            exact_target: FormalBftHeaderReadTargetV4 {
                height,
                header_id: id.into(),
            },
        },
        h.anchor.clone(),
        h.target.clone(),
    )
    .unwrap();
    let page = client.read::<serde_json::Value>().unwrap();
    assert_eq!(page.response().head.header_id, id);
    assert_eq!(page.view()["state"]["time"], serde_json::json!(height));
    assert!(
        page.view()["state"]["agents"]
            .get("node-wire3-first-agent")
            .is_some()
    );
    assert!(
        page.view()["state"]["agents"]
            .get("node-wire3-second-agent")
            .is_some()
    );
    server.stop().unwrap();
}
