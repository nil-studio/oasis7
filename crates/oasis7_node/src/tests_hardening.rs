use super::gossip_udp::{GossipEndpoint, GossipProposalMessage};
use super::*;
use ed25519_dalek::{Signer as _, SigningKey};
use oasis7_consensus::node_consensus_signature::{
    NodeConsensusMessageSigner as ConsensusMessageSigner, sign_proposal_message,
};
use oasis7_distfs::{
    FeedbackCreateRequest, FeedbackStore, FeedbackStoreConfig, FileReplicationRecord,
    LocalCasStore, blake3_hex, build_replication_record_with_epoch, feedback_announce_topic,
    public_key_hex_from_signing_key_hex, sign_feedback_create_request,
};
use oasis7_proto::distributed::DistributedErrorCode;
use oasis7_proto::distributed_net::NetworkSubscription;
use oasis7_proto::world_error::WorldError;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-node-hardening-{prefix}-{unique}"))
}

fn deterministic_keypair_hex(seed: u8) -> (String, String) {
    let bytes = [seed; 32];
    let signing_key = SigningKey::from_bytes(&bytes);
    (
        hex::encode(signing_key.to_bytes()),
        hex::encode(signing_key.verifying_key().to_bytes()),
    )
}

fn signed_replication_config(root_dir: PathBuf, seed: u8) -> NodeReplicationConfig {
    let (private_hex, public_hex) = deterministic_keypair_hex(seed);
    NodeReplicationConfig::new(root_dir)
        .expect("replication config")
        .with_signing_keypair(private_hex, public_hex)
        .expect("signing keypair")
}

fn signed_pos_config_with_signer_seeds(
    validators: Vec<PosValidator>,
    signer_seeds: &[(&str, u8)],
) -> NodePosConfig {
    let seed_map = signer_seeds
        .iter()
        .map(|(validator_id, seed)| ((*validator_id).to_string(), *seed))
        .collect::<HashMap<_, _>>();
    let mut signer_map = BTreeMap::new();
    for validator in &validators {
        let seed = seed_map
            .get(validator.validator_id.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "missing signer seed for validator {}",
                    validator.validator_id
                )
            });
        let (_, public_key_hex) = deterministic_keypair_hex(*seed);
        signer_map.insert(validator.validator_id.clone(), public_key_hex);
    }
    NodePosConfig::ethereum_like(validators)
        .with_validator_signer_public_keys(signer_map)
        .expect("signed pos config")
}

fn gossip_config(
    bind_addr: std::net::SocketAddr,
    peers: Vec<std::net::SocketAddr>,
) -> NodeGossipConfig {
    NodeGossipConfig {
        bind_addr,
        peers,
        max_dynamic_peers: 1024,
        dynamic_peer_ttl_ms: 10 * 60 * 1000,
    }
}

#[derive(Debug, Serialize)]
struct ReplicationSigningPayload<'a> {
    version: u8,
    world_id: &'a str,
    node_id: &'a str,
    record: &'a FileReplicationRecord,
    payload: &'a [u8],
    public_key_hex: Option<&'a str>,
}

fn sign_replication_message_for_test(
    message: &super::replication::GossipReplicationMessage,
    private_key_hex: &str,
) -> String {
    let private_key: [u8; 32] = hex::decode(private_key_hex)
        .expect("private key decode")
        .try_into()
        .expect("private key length");
    let signing_key = SigningKey::from_bytes(&private_key);
    let payload = ReplicationSigningPayload {
        version: message.version,
        world_id: message.world_id.as_str(),
        node_id: message.node_id.as_str(),
        record: &message.record,
        payload: &message.payload,
        public_key_hex: message.public_key_hex.as_deref(),
    };
    let bytes = serde_json::to_vec(&payload).expect("encode signing payload");
    let signature = signing_key.sign(bytes.as_slice());
    hex::encode(signature.to_bytes())
}

fn signed_replication_message_for_writer(
    world_id: &str,
    node_id: &str,
    private_key_hex: &str,
    public_key_hex: &str,
    sequence: u64,
) -> super::replication::GossipReplicationMessage {
    let payload = format!("payload-{sequence}").into_bytes();
    let path = format!("consensus/commits/{:020}.json", sequence.max(1));
    let record = build_replication_record_with_epoch(
        world_id,
        public_key_hex,
        1,
        sequence.max(1),
        path.as_str(),
        payload.as_slice(),
        1_000,
    )
    .expect("record");
    let mut message = super::replication::GossipReplicationMessage {
        version: 1,
        world_id: world_id.to_string(),
        node_id: node_id.to_string(),
        record,
        payload,
        public_key_hex: Some(public_key_hex.to_string()),
        signature_hex: None,
    };
    message.signature_hex = Some(sign_replication_message_for_test(&message, private_key_hex));
    message
}

fn empty_action_root() -> String {
    compute_consensus_action_root(&[]).expect("empty action root")
}

fn wait_until(deadline: Instant, mut predicate: impl FnMut() -> bool) -> bool {
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    predicate()
}

type TestHandler = Arc<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>;

#[derive(Clone, Default)]
struct TestInMemoryNetwork {
    retained: Arc<Mutex<HashMap<String, Vec<Vec<u8>>>>>,
    subscribers: Arc<Mutex<Vec<TestNetworkInbox>>>,
    handlers: Arc<Mutex<HashMap<String, Vec<TestHandler>>>>,
}

type TestNetworkInbox = Arc<Mutex<HashMap<String, Vec<Vec<u8>>>>>;

impl oasis7_proto::distributed_net::DistributedNetwork<WorldError> for TestInMemoryNetwork {
    fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), WorldError> {
        self.retained
            .lock()
            .expect("lock retained")
            .entry(topic.to_string())
            .or_default()
            .push(payload.to_vec());
        let subscribers = self.subscribers.lock().expect("lock subscribers");
        for inbox in subscribers.iter() {
            let mut topic_inbox = inbox.lock().expect("lock subscriber inbox");
            topic_inbox
                .entry(topic.to_string())
                .or_default()
                .push(payload.to_vec());
        }
        Ok(())
    }

    fn subscribe(&self, topic: &str) -> Result<NetworkSubscription, WorldError> {
        let inbox = Arc::new(Mutex::new(HashMap::<String, Vec<Vec<u8>>>::new()));
        let retained = self.retained.lock().expect("lock retained");
        let seeded = retained.get(topic).cloned().unwrap_or_default();
        drop(retained);
        {
            let mut topic_inbox = inbox.lock().expect("lock subscriber inbox");
            topic_inbox.insert(topic.to_string(), seeded);
        }
        self.subscribers
            .lock()
            .expect("lock subscribers")
            .push(Arc::clone(&inbox));
        Ok(NetworkSubscription::new(topic.to_string(), inbox))
    }

    fn request(&self, protocol: &str, payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        let handlers = self.handlers.lock().expect("lock handlers");
        let Some(protocol_handlers) = handlers.get(protocol) else {
            return Err(WorldError::NetworkProtocolUnavailable {
                protocol: protocol.to_string(),
            });
        };
        let mut last_error = None;
        for handler in protocol_handlers {
            match handler(payload) {
                Ok(response) => return Ok(response),
                Err(err) => last_error = Some(err),
            }
        }
        Err(
            last_error.unwrap_or(WorldError::NetworkProtocolUnavailable {
                protocol: protocol.to_string(),
            }),
        )
    }

    fn register_handler(
        &self,
        protocol: &str,
        handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        self.handlers
            .lock()
            .expect("lock handlers")
            .entry(protocol.to_string())
            .or_default()
            .push(Arc::from(handler));
        Ok(())
    }
}

#[test]
fn config_rejects_duplicate_validator_signer_bindings() {
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let (_, signer_public_key) = deterministic_keypair_hex(41);
    let mut signer_map = BTreeMap::new();
    signer_map.insert("node-a".to_string(), signer_public_key.clone());
    signer_map.insert("node-b".to_string(), signer_public_key);

    let result =
        NodePosConfig::ethereum_like(validators).with_validator_signer_public_keys(signer_map);
    assert!(matches!(result, Err(NodeError::InvalidConfig { .. })));
}

#[test]
fn pos_engine_rejects_signed_proposal_when_signer_binding_mismatches_validator() {
    let socket_a = UdpSocket::bind("127.0.0.1:0").expect("bind a");
    let socket_b = UdpSocket::bind("127.0.0.1:0").expect("bind b");
    let addr_a = socket_a.local_addr().expect("addr a");
    let addr_b = socket_b.local_addr().expect("addr b");
    drop(socket_a);
    drop(socket_b);

    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let (_, node_a_expected_pub) = deterministic_keypair_hex(51);
    let (_, node_b_expected_pub) = deterministic_keypair_hex(52);
    let mut signer_map = BTreeMap::new();
    signer_map.insert("node-a".to_string(), node_a_expected_pub);
    signer_map.insert("node-b".to_string(), node_b_expected_pub);

    let pos_config = NodePosConfig::ethereum_like(validators)
        .with_validator_signer_public_keys(signer_map)
        .expect("pos config");
    let config_b = NodeConfig::new("node-b", "world-signer-binding", NodeRole::Observer)
        .expect("config b")
        .with_pos_config(pos_config)
        .expect("validators")
        .with_replication(signed_replication_config(temp_dir("signer-binding"), 52));
    let mut engine = PosNodeEngine::new(&config_b).expect("engine");

    let (wrong_private_hex, wrong_public_hex) = deterministic_keypair_hex(61);
    let wrong_signing_key = SigningKey::from_bytes(
        &hex::decode(wrong_private_hex)
            .expect("private decode")
            .try_into()
            .expect("private len"),
    );
    let wrong_signer =
        ConsensusMessageSigner::new(wrong_signing_key, wrong_public_hex).expect("wrong signer");

    let endpoint_a =
        GossipEndpoint::bind(&gossip_config(addr_a, vec![addr_b])).expect("endpoint a");
    let endpoint_b =
        GossipEndpoint::bind(&gossip_config(addr_b, vec![addr_a])).expect("endpoint b");

    let mut proposal = GossipProposalMessage {
        version: 1,
        world_id: config_b.world_id.clone(),
        node_id: "node-a".to_string(),
        player_id: "node-a".to_string(),
        proposer_id: "node-a".to_string(),
        height: 1,
        slot: 0,
        epoch: 0,
        block_hash: format!("{}:h1:s0:p{}", config_b.world_id, "node-a"),
        action_root: empty_action_root(),
        actions: Vec::new(),
        proposed_at_ms: 1_000,
        public_key_hex: None,
        signature_hex: None,
    };
    sign_proposal_message(&mut proposal, &wrong_signer).expect("sign proposal");
    endpoint_a
        .broadcast_proposal(&proposal)
        .expect("broadcast proposal");
    std::thread::sleep(Duration::from_millis(20));

    engine
        .ingest_peer_messages(&endpoint_b, &config_b.node_id, &config_b.world_id, None, 0)
        .expect("ingest");
    assert!(engine.pending.is_none());
}

#[test]
fn pos_engine_rejects_signed_mode_without_complete_validator_signer_bindings() {
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let pos_config = NodePosConfig::ethereum_like(validators);
    let config = NodeConfig::new("node-b", "world-signer-complete", NodeRole::Observer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(temp_dir("signer-complete"), 52));

    let err = PosNodeEngine::new(&config).expect_err("engine should reject incomplete signer map");
    assert!(matches!(
        err,
        NodeError::InvalidConfig { reason }
            if reason.contains("requires signer bindings for all validators")
    ));
}

#[test]
fn pos_engine_rejects_signed_mode_when_local_signer_binding_mismatches() {
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let pos_config =
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 51), ("node-b", 52)]);
    let config = NodeConfig::new("node-b", "world-signer-local-mismatch", NodeRole::Observer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(
            temp_dir("signer-local-mismatch"),
            53,
        ));

    let err = PosNodeEngine::new(&config).expect_err("engine should reject local signer mismatch");
    assert!(matches!(
        err,
        NodeError::InvalidConfig { reason }
            if reason.contains("consensus signer binding mismatch for local validator")
    ));
}

#[test]
fn runtime_start_fails_when_pos_state_snapshot_is_corrupted() {
    let dir = temp_dir("pos-state-corrupt");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(dir.join("node_pos_state.json"), b"not-json").expect("write corrupted snapshot");

    let config = NodeConfig::new("node-a", "world-pos-corrupt", NodeRole::Observer)
        .expect("config")
        .with_replication_root(dir.clone())
        .expect("replication");
    let mut runtime = NodeRuntime::new(config);

    let err = runtime.start().expect_err("start should fail");
    assert!(matches!(err, NodeError::Replication { .. }));
    assert!(!runtime.snapshot().running);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn runtime_start_fails_when_pos_state_snapshot_height_overflows() {
    let dir = temp_dir("pos-state-overflow");
    fs::create_dir_all(&dir).expect("create dir");
    let snapshot = super::pos_state_store::PosNodeStateSnapshot {
        next_height: 0,
        next_slot: 0,
        last_observed_slot: 0,
        missed_slot_count: 0,
        last_observed_tick: 0,
        missed_tick_count: 0,
        committed_height: u64::MAX,
        network_committed_height: u64::MAX,
        last_broadcast_proposal_height: 0,
        last_broadcast_local_attestation_height: 0,
        last_broadcast_committed_height: 0,
        last_committed_block_hash: None,
        last_execution_height: 0,
        last_execution_block_hash: None,
        last_execution_state_root: None,
    };
    fs::write(
        dir.join("node_pos_state.json"),
        serde_json::to_vec(&snapshot).expect("encode snapshot"),
    )
    .expect("write overflow snapshot");

    let config = NodeConfig::new("node-a", "world-pos-overflow", NodeRole::Observer)
        .expect("config")
        .with_replication_root(dir.clone())
        .expect("replication");
    let mut runtime = NodeRuntime::new(config);

    let err = runtime.start().expect_err("start should fail");
    assert!(
        matches!(err, NodeError::Replication { reason } if reason.contains("committed_height"))
    );
    let snapshot = runtime.snapshot();
    assert!(!snapshot.running);
    assert_eq!(snapshot.consensus.committed_height, 0);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn runtime_replication_ingest_reports_error_and_does_not_advance_network_height_on_invalid_message()
{
    let world_id = "world-repl-hardening";
    let dir = temp_dir("repl-ingest-hardening");
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let pos_config =
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 82), ("node-b", 81)]);

    let network_impl = Arc::new(TestInMemoryNetwork::default());
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();

    let config = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config")
        .with_tick_interval(Duration::from_millis(50))
        .expect("tick")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 81));
    let mut runtime = NodeRuntime::new(config)
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    runtime.start().expect("start");

    let payload = b"payload-actual".to_vec();
    let bad_message = super::replication::GossipReplicationMessage {
        version: 1,
        world_id: world_id.to_string(),
        node_id: "node-a".to_string(),
        record: FileReplicationRecord {
            world_id: world_id.to_string(),
            writer_id: "writer-a".to_string(),
            writer_epoch: 1,
            sequence: 1,
            path: "consensus/commits/00000000000000000001.json".to_string(),
            content_hash: blake3_hex(b"payload-expected"),
            size_bytes: payload.len() as u64,
            updated_at_ms: 1,
        },
        payload,
        public_key_hex: None,
        signature_hex: None,
    };
    let encoded = serde_json::to_vec(&bad_message).expect("encode message");
    let topic = super::network_bridge::default_replication_topic(world_id);
    network
        .publish(topic.as_str(), encoded.as_slice())
        .expect("publish invalid message");

    let mut last_republish_at = Instant::now();
    let has_error = wait_until(Instant::now() + Duration::from_secs(5), || {
        if runtime
            .snapshot()
            .last_error
            .as_ref()
            .map(|reason| reason.contains("replication ingest rejected"))
            .unwrap_or(false)
        {
            return true;
        }

        if last_republish_at.elapsed() >= Duration::from_millis(10) {
            network
                .publish(topic.as_str(), encoded.as_slice())
                .expect("republish invalid message");
            last_republish_at = Instant::now();
        }
        false
    });
    assert!(
        has_error,
        "runtime did not report replication ingest rejection"
    );

    runtime.stop().expect("stop");
    let snapshot = runtime.snapshot();
    assert_eq!(
        snapshot.consensus.network_committed_height,
        snapshot.consensus.committed_height
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn runtime_replication_ingest_rejects_signed_writer_outside_allowlist() {
    let world_id = "world-repl-allowlist";
    let dir = temp_dir("repl-allowlist");
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let mut pos_config =
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 91), ("node-b", 92)]);
    pos_config.slot_duration_ms = 60_000;
    pos_config.ticks_per_slot = 10;
    pos_config.proposal_tick_phase = 9;

    let network_impl = Arc::new(TestInMemoryNetwork::default());
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();

    let config = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 92));
    let mut runtime = NodeRuntime::new(config)
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    runtime.start().expect("start");

    let (unauthorized_private_hex, unauthorized_public_hex) = deterministic_keypair_hex(99);
    let unauthorized_message = signed_replication_message_for_writer(
        world_id,
        "node-a",
        unauthorized_private_hex.as_str(),
        unauthorized_public_hex.as_str(),
        1,
    );
    let encoded = serde_json::to_vec(&unauthorized_message).expect("encode message");
    let topic = super::network_bridge::default_replication_topic(world_id);
    let mut last_republish_at = Instant::now() - Duration::from_millis(100);
    let unauthorized_rejected = wait_until(Instant::now() + Duration::from_secs(2), || {
        if last_republish_at.elapsed() >= Duration::from_millis(50) {
            network
                .publish(topic.as_str(), encoded.as_slice())
                .expect("publish unauthorized message");
            last_republish_at = Instant::now();
        }
        runtime
            .snapshot()
            .last_error
            .as_ref()
            .map(|reason| reason.contains("not authorized"))
            .unwrap_or(false)
    });
    assert!(
        unauthorized_rejected,
        "runtime did not reject unauthorized remote writer"
    );

    runtime.stop().expect("stop");
    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.consensus.network_committed_height, 0);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn runtime_fetch_handlers_reject_unsigned_fetch_request_in_signed_mode() {
    let world_id = "world-fetch-auth-hardening";
    let dir = temp_dir("fetch-auth-hardening");
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 111)],
    );
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(TestInMemoryNetwork::default());

    let config = NodeConfig::new("node-a", world_id, NodeRole::Storage)
        .expect("config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir.clone(), 111));
    let mut runtime = NodeRuntime::new(config)
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    runtime.start().expect("start");

    let unsigned_request = super::replication::FetchCommitRequest {
        world_id: world_id.to_string(),
        height: 1,
        requester_public_key_hex: None,
        requester_signature_hex: None,
    };
    let payload = serde_json::to_vec(&unsigned_request).expect("encode request");
    let err = network
        .request(
            super::replication::REPLICATION_FETCH_COMMIT_PROTOCOL,
            payload.as_slice(),
        )
        .expect_err("unsigned fetch request should be rejected");
    match err {
        WorldError::NetworkRequestFailed { code, message, .. } => {
            assert_eq!(code, DistributedErrorCode::ErrBadRequest);
            assert!(message.contains("authorization failed"));
        }
        other => panic!("unexpected error: {other:?}"),
    }

    runtime.stop().expect("stop");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn runtime_fetch_handlers_authorize_explicit_fetch_requester_without_writer_access() {
    let world_id = "world-fetch-requester-hardening";
    let dir_validator = temp_dir("fetch-requester-validator");
    let dir_observer = temp_dir("fetch-requester-observer");
    let (_, observer_public_hex) = deterministic_keypair_hex(120);
    let (_, other_public_hex) = deterministic_keypair_hex(121);
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 111)],
    );
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(TestInMemoryNetwork::default());

    let replication_config = signed_replication_config(dir_validator.clone(), 111)
        .with_remote_writer_allowlist(vec![other_public_hex])
        .expect("writer allowlist")
        .with_fetch_requester_allowlist(vec![observer_public_hex.clone()])
        .expect("fetch requester allowlist");
    let config = NodeConfig::new("node-a", world_id, NodeRole::Storage)
        .expect("config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(replication_config);
    let mut runtime = NodeRuntime::new(config)
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    runtime.start().expect("start");

    let observer_replication_config = signed_replication_config(dir_observer.clone(), 120);
    let observer_replication =
        ReplicationRuntime::new(&observer_replication_config, "node-observer")
            .expect("observer replication runtime");
    let authorized_request = observer_replication
        .build_fetch_commit_request(world_id, 1)
        .expect("authorized request");
    assert_eq!(
        authorized_request.requester_public_key_hex.as_deref(),
        Some(observer_public_hex.as_str())
    );
    let payload = serde_json::to_vec(&authorized_request).expect("encode authorized request");
    let response = network
        .request(
            super::replication::REPLICATION_FETCH_COMMIT_PROTOCOL,
            payload.as_slice(),
        )
        .expect("explicit fetch requester should be authorized");
    let response: super::replication::FetchCommitResponse =
        serde_json::from_slice(response.as_slice()).expect("decode response");
    assert!(!response.found);

    let dir_denied = temp_dir("fetch-denied");
    let unauthorized_replication_config = signed_replication_config(dir_denied.clone(), 122);
    let unauthorized_replication =
        ReplicationRuntime::new(&unauthorized_replication_config, "node-denied")
            .expect("unauthorized replication runtime");
    let unauthorized_request = unauthorized_replication
        .build_fetch_commit_request(world_id, 1)
        .expect("unauthorized request");
    let payload = serde_json::to_vec(&unauthorized_request).expect("encode unauthorized request");
    let err = network
        .request(
            super::replication::REPLICATION_FETCH_COMMIT_PROTOCOL,
            payload.as_slice(),
        )
        .expect_err("unlisted fetch requester should be rejected");
    match err {
        WorldError::NetworkRequestFailed { code, message, .. } => {
            assert_eq!(code, DistributedErrorCode::ErrBadRequest);
            assert!(message.contains("not authorized"));
            assert!(message.contains("authorized_fetch_requester_count=1"));
            assert!(message.contains("authorized_fetch_requester_fingerprint="));
        }
        other => panic!("unexpected error: {other:?}"),
    }

    runtime.stop().expect("stop");
    let _ = fs::remove_dir_all(dir_validator);
    let _ = fs::remove_dir_all(dir_observer);
    let _ = fs::remove_dir_all(dir_denied);
}

#[test]
fn runtime_start_rejects_observer_feedback_p2p_without_blob_state_lane_access() {
    let world_id = "world-feedback-observer-gate";
    let dir = temp_dir("feedback-observer-gate");
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 130)],
    );
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(TestInMemoryNetwork::default());
    let config = NodeConfig::new("node-a", world_id, NodeRole::Observer)
        .expect("config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 130))
        .with_feedback_p2p(NodeFeedbackP2pConfig::default())
        .expect("feedback p2p");
    let mut runtime = NodeRuntime::new(config)
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));

    let err = runtime
        .start()
        .expect_err("observer feedback_p2p should fail blob/state lane gate");
    assert!(matches!(err, NodeError::InvalidConfig { .. }));
    assert!(
        err.to_string()
            .contains("feedback_p2p requires blob/state lane publish+subscribe access"),
        "unexpected error: {err}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn runtime_feedback_submit_publishes_and_peer_ingests() {
    let world_id = "world-feedback-runtime-sync";
    let dir_a = temp_dir("feedback-sync-a");
    let dir_b = temp_dir("feedback-sync-b");
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let pos_config =
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 131), ("node-b", 132)]);
    let feedback_config = NodeFeedbackP2pConfig::default()
        .with_max_incoming_announces_per_tick(32)
        .expect("incoming limit")
        .with_max_outgoing_announces_per_tick(32)
        .expect("outgoing limit");

    let network_impl = Arc::new(TestInMemoryNetwork::default());
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();

    let config_a = NodeConfig::new("node-a", world_id, NodeRole::Storage)
        .expect("config a")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_replication(signed_replication_config(dir_a.clone(), 131))
        .with_feedback_p2p(feedback_config.clone())
        .expect("feedback config a");
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Storage)
        .expect("config b")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(signed_replication_config(dir_b.clone(), 132))
        .with_feedback_p2p(feedback_config)
        .expect("feedback config b");

    let mut runtime_a = NodeRuntime::new(config_a)
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)))
        .with_replication_network_consensus_enabled(false);
    let mut runtime_b = NodeRuntime::new(config_b)
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)))
        .with_replication_network_consensus_enabled(false);
    runtime_a.start().expect("start node a");
    runtime_b.start().expect("start node b");

    let feedback_id = "fb-runtime-sync-1";
    let signing_key_hex =
        "3131313131313131313131313131313131313131313131313131313131313131".to_string();
    let author_public_key_hex =
        public_key_hex_from_signing_key_hex(signing_key_hex.as_str()).expect("derive pubkey");
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_millis() as i64;
    let mut request = FeedbackCreateRequest {
        feedback_id: feedback_id.to_string(),
        author_public_key_hex,
        submit_ip: "127.0.0.10".to_string(),
        category: "bug".to_string(),
        platform: "web".to_string(),
        game_version: "0.9.0".to_string(),
        content: "runtime feedback payload".to_string(),
        attachments: vec![],
        nonce: "fb-runtime-sync-nonce-1".to_string(),
        timestamp_ms: now_ms,
        expires_at_ms: now_ms + 120_000,
        signature_hex: String::new(),
    };
    request.signature_hex =
        sign_feedback_create_request(&request, signing_key_hex.as_str()).expect("sign feedback");
    runtime_a
        .submit_feedback(request)
        .expect("submit feedback from node a");

    let follower_store = FeedbackStore::new(
        LocalCasStore::new(dir_b.join("store")),
        FeedbackStoreConfig::default(),
    );
    let replicated = wait_until(Instant::now() + Duration::from_secs(3), || {
        follower_store
            .read_feedback_public(feedback_id)
            .map(|view| view.is_some())
            .unwrap_or(false)
    });
    assert!(replicated, "node b did not ingest feedback announce");

    let view = follower_store
        .read_feedback_public(feedback_id)
        .expect("read feedback")
        .expect("feedback exists");
    assert_eq!(view.content, "runtime feedback payload");
    assert_eq!(view.append_events.len(), 0);
    assert!(!view.tombstoned);

    let announce_topic = feedback_announce_topic(world_id);
    let payloads = network_impl
        .retained
        .lock()
        .expect("lock retained")
        .get(announce_topic.as_str())
        .cloned()
        .unwrap_or_default();
    assert!(
        !payloads.is_empty(),
        "feedback announce topic should have retained payloads"
    );
    network
        .publish(announce_topic.as_str(), payloads[0].as_slice())
        .expect("republish announce");

    let duplicate_ok = wait_until(Instant::now() + Duration::from_secs(2), || {
        follower_store
            .read_feedback_public(feedback_id)
            .map(|view| {
                view.map(|entry| entry.append_events.len())
                    .unwrap_or_default()
                    == 0
            })
            .unwrap_or(false)
    });
    assert!(duplicate_ok, "duplicate announce should remain idempotent");
    assert!(
        runtime_b.snapshot().last_error.is_none(),
        "duplicate announce should not raise runtime error: {:?}",
        runtime_b.snapshot().last_error
    );

    runtime_b.stop().expect("stop node b");
    runtime_a.stop().expect("stop node a");
    let _ = fs::remove_dir_all(dir_a);
    let _ = fs::remove_dir_all(dir_b);
}
