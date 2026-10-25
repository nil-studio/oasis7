use super::*;
use crate::node_bft_protected_test_provider::*;
use oasis7_distfs::protected_bft::*;

struct ProtectedRuntimeFixture {
    bft: NodeBftConfigV1,
    binding: BftSignerBindingV1,
    backend: MemoryBackend,
    provider: Arc<RemoteBftSignerProviderV1>,
    _server: TcpServiceServer,
}

impl ProtectedRuntimeFixture {
    fn new() -> Self {
        let validator = SigningKey::from_bytes(&[81; 32]);
        let requester = SigningKey::from_bytes(&[82; 32]);
        let provider_key = SigningKey::from_bytes(&[83; 32]);
        let bft = NodeBftConfigV1 {
            scope: scope(),
            authority_epoch: 1,
            validators: vec![BftValidatorV1 {
                validator_id: "protected-v0".into(),
                public_key_hex: hex::encode(validator.verifying_key().to_bytes()),
                voting_power: 1,
            }],
            local_validator_id: "protected-v0".into(),
            timeouts: TEST_TIMEOUTS,
        };
        let binding = BftSignerBindingV1 {
            schema_version: 1,
            scope: bft.scope.clone(),
            authority_epoch: 1,
            validators: bft.validators.clone(),
            local_validator_id: bft.local_validator_id.clone(),
            local_signing_public_key: hex::encode(validator.verifying_key().to_bytes()),
            timeouts: bft.timeouts,
            operator_instance_id: "runtime-tcp-provider-fixture".into(),
            requester_auth_public_key: hex::encode(requester.verifying_key().to_bytes()),
        };
        let backend = MemoryBackend::provision(&binding).unwrap();
        let trust = BftSignerTrustV1 {
            schema_version: 1,
            provider_public_key: hex::encode(provider_key.verifying_key().to_bytes()),
            binding_digest: binding.digest().unwrap(),
        };
        let service = Arc::new(
            BftSignerServiceV1::open(
                binding.clone(),
                Box::new(SoftwareKey(provider_key)),
                Box::new(SoftwareKey(validator)),
                Box::new(backend.clone()),
            )
            .unwrap(),
        );
        let server = TcpServiceServer::start(service);
        let provider = Arc::new(
            RemoteBftSignerProviderV1::new(
                server.address,
                Duration::from_secs(2),
                binding.clone(),
                requester,
                trust,
            )
            .unwrap(),
        );
        Self {
            bft,
            binding,
            backend,
            provider,
            _server: server,
        }
    }

    fn runtime(&self, root: &Path, applied: Arc<Mutex<Vec<u64>>>) -> NodeRuntime {
        let replication = NodeReplicationConfig::new(root.join("replication")).unwrap();
        let config = NodeConfig::new("protected-v0", WORLD_ID, NodeRole::Sequencer)
            .unwrap()
            .with_tick_interval(Duration::from_millis(10))
            .unwrap()
            .with_replication(replication);
        NodeRuntime::new(config)
            .with_execution_hook(DurableTestHook::open(root.join("driver"), applied))
            .with_replication_network_consensus_enabled(false)
            .with_tendermint_v1(self.bft.clone())
            .unwrap()
            .with_protected_bft_signer(self.binding.clone(), self.provider.clone())
            .unwrap()
    }
}

fn wait_protected_height(node: &NodeRuntime, height: u64) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let snapshot = node.snapshot();
        assert!(
            snapshot.last_error.is_none(),
            "protected runtime stopped: {:?}",
            snapshot.last_error
        );
        if snapshot.consensus.committed_height >= height {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "protected runtime stalled at {:?}",
            snapshot.consensus
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn protected_node_runtime_commits_reopens_and_continues_without_validator_key() {
    let root = TempRoot::new("protected-runtime");
    let fixture = ProtectedRuntimeFixture::new();
    let applied = Arc::new(Mutex::new(Vec::new()));
    let mut node = fixture.runtime(root.path(), applied.clone());
    node.start().expect("start protected runtime");
    wait_protected_height(&node, 2);
    let generation = fixture.backend.0.lock().unwrap().lease_generation;
    let before_duplicate = node.snapshot().consensus.committed_height;
    let mut duplicate = fixture.runtime(root.path(), applied.clone());
    assert!(
        duplicate.start().is_err(),
        "same directory already owns the lifecycle lock"
    );
    assert_eq!(
        fixture.backend.0.lock().unwrap().lease_generation,
        generation,
        "failed duplicate start must not fence the live provider lease"
    );
    wait_protected_height(&node, before_duplicate + 1);
    drop(duplicate);
    node.stop().unwrap();
    let first: DurableTestEvidence = serde_cbor::from_slice(
        &fs::read(root.path().join("driver/bft-test-evidence.cbor")).unwrap(),
    )
    .unwrap();
    let old_height = first.certificates.len() as u64;
    assert!(old_height >= 2);
    drop(node);
    let mut reopened = fixture.runtime(root.path(), applied.clone());
    reopened
        .start()
        .expect("replay complete history and provider state");
    wait_protected_height(&reopened, old_height + 1);
    reopened.stop().unwrap();
    let after: DurableTestEvidence = serde_cbor::from_slice(
        &fs::read(root.path().join("driver/bft-test-evidence.cbor")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        &after.certificates[..first.certificates.len()],
        &first.certificates
    );
    let heights = applied.lock().unwrap();
    assert!(heights.windows(2).all(|pair| pair[1] == pair[0] + 1));
    assert!(heights.last().copied().unwrap() > old_height);
    let mut schedule = oasis7_consensus::tendermint_v1::BftHeightProposerScheduleV1::genesis(
        &fixture.binding.validate().unwrap(),
        1,
    )
    .unwrap();
    for qc in &after.certificates {
        let token = oasis7_consensus::tendermint_v1::verify_commit_certificate_token_v1(
            &fixture.binding.validate().unwrap(),
            &schedule,
            qc,
        )
        .unwrap();
        schedule = token.next_height_schedule().clone();
    }
    assert_eq!(schedule.height(), after.certificates.len() as u64 + 1);
    assert!(fixture.backend.0.lock().unwrap().state.history.len() >= old_height as usize);
    drop(reopened);
}

#[test]
fn protected_node_runtime_refuses_driver_history_rollback() {
    let root = TempRoot::new("protected-history-rollback");
    let fixture = ProtectedRuntimeFixture::new();
    let applied = Arc::new(Mutex::new(Vec::new()));
    let mut node = fixture.runtime(root.path(), applied.clone());
    node.start().unwrap();
    wait_protected_height(&node, 2);
    node.stop().unwrap();
    drop(node);
    fs::remove_file(root.path().join("driver/bft-test-evidence.cbor")).unwrap();
    let before = fixture.backend.0.lock().unwrap().state.clone();
    let mut rolled_back = fixture.runtime(root.path(), applied);
    assert!(
        rolled_back.start().is_err(),
        "missing complete driver history must stop signing"
    );
    assert!(!rolled_back.snapshot().running);
    assert_eq!(fixture.backend.0.lock().unwrap().state, before);
}
