use super::gossip_udp::{GossipCommitMessage, GossipEndpoint, GossipMessage};
use super::*;
use ed25519_dalek::{Signer as _, SigningKey};
use oasis7_consensus::node_consensus_signature::{
    sign_attestation_message, sign_commit_message, sign_proposal_message,
    verify_commit_message_signature, NodeConsensusMessageSigner as ConsensusMessageSigner,
};
use oasis7_distfs::{FileStore as _, LocalCasStore, SingleWriterReplicationGuard};
use oasis7_proto::distributed::WorldHeadAnnounce;
use oasis7_proto::distributed_dht::{
    self as proto_dht, MembershipDirectorySnapshot, ProviderRecord, SignedPeerRecord,
};
use oasis7_proto::distributed_net::NetworkSubscription;
use oasis7_proto::world_error::WorldError;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "tests_commit_execution_hashes.rs"]
mod commit_execution_hashes_tests;
#[path = "tests_pos_engine_guardrails.rs"]
mod pos_engine_guardrails_tests;
#[path = "tests_peer_head_binding.rs"]
mod peer_head_binding_tests;
#[path = "tests_pos_signature_and_runtime.rs"]
mod pos_signature_and_runtime_tests;
#[path = "tests_runtime_replica_maintenance.rs"]
mod runtime_replica_maintenance_tests;
#[path = "tests_synced_execution_rollback.rs"]
mod synced_execution_rollback_tests;

fn multi_validators() -> Vec<PosValidator> {
    vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 40,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 35,
        },
        PosValidator {
            validator_id: "node-c".to_string(),
            stake: 25,
        },
    ]
}

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-node-tests-{prefix}-{unique}"))
}

fn deterministic_keypair_hex(seed: u8) -> (String, String) {
    let bytes = [seed; 32];
    let signing_key = SigningKey::from_bytes(&bytes);
    (
        hex::encode(signing_key.to_bytes()),
        hex::encode(signing_key.verifying_key().to_bytes()),
    )
}

fn empty_action_root() -> String {
    compute_consensus_action_root(&[]).expect("empty action root")
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

#[derive(Debug, Serialize)]
struct FetchCommitRequestSigningPayload<'a> {
    version: u8,
    world_id: &'a str,
    height: u64,
    requester_public_key_hex: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct FetchBlobRequestSigningPayload<'a> {
    version: u8,
    content_hash: &'a str,
    requester_public_key_hex: Option<&'a str>,
}

fn signed_fetch_commit_request_for_test(
    world_id: &str,
    height: u64,
    signer_seed: u8,
) -> super::replication::FetchCommitRequest {
    let (private_hex, public_hex) = deterministic_keypair_hex(signer_seed);
    let signing_key_bytes: [u8; 32] = hex::decode(private_hex)
        .expect("private key decode")
        .try_into()
        .expect("private key len");
    let signing_key = SigningKey::from_bytes(&signing_key_bytes);
    let mut request = super::replication::FetchCommitRequest {
        world_id: world_id.to_string(),
        height,
        requester_public_key_hex: Some(public_hex),
        requester_signature_hex: None,
    };
    let payload = FetchCommitRequestSigningPayload {
        version: 1,
        world_id: request.world_id.as_str(),
        height: request.height,
        requester_public_key_hex: request.requester_public_key_hex.as_deref(),
    };
    let payload_bytes = serde_json::to_vec(&payload).expect("encode fetch-commit signing payload");
    let signature = signing_key.sign(payload_bytes.as_slice());
    request.requester_signature_hex = Some(hex::encode(signature.to_bytes()));
    request
}

fn signed_fetch_blob_request_for_test(
    content_hash: &str,
    signer_seed: u8,
) -> super::replication::FetchBlobRequest {
    let (private_hex, public_hex) = deterministic_keypair_hex(signer_seed);
    let signing_key_bytes: [u8; 32] = hex::decode(private_hex)
        .expect("private key decode")
        .try_into()
        .expect("private key len");
    let signing_key = SigningKey::from_bytes(&signing_key_bytes);
    let mut request = super::replication::FetchBlobRequest {
        content_hash: content_hash.to_string(),
        offset_bytes: None,
        limit_bytes: None,
        requester_public_key_hex: Some(public_hex),
        requester_signature_hex: None,
    };
    let payload = FetchBlobRequestSigningPayload {
        version: 1,
        content_hash: request.content_hash.as_str(),
        requester_public_key_hex: request.requester_public_key_hex.as_deref(),
    };
    let payload_bytes = serde_json::to_vec(&payload).expect("encode fetch-blob signing payload");
    let signature = signing_key.sign(payload_bytes.as_slice());
    request.requester_signature_hex = Some(hex::encode(signature.to_bytes()));
    request
}

#[derive(Clone)]
struct RecordingExecutionHook {
    calls: Arc<Mutex<Vec<NodeExecutionCommitContext>>>,
}

impl RecordingExecutionHook {
    fn new(calls: Arc<Mutex<Vec<NodeExecutionCommitContext>>>) -> Self {
        Self { calls }
    }
}

impl NodeExecutionHook for RecordingExecutionHook {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        self.calls
            .lock()
            .expect("lock execution calls")
            .push(context.clone());
        Ok(NodeExecutionCommitResult {
            execution_height: context.height,
            execution_block_hash: format!("exec-block-{:020}", context.height),
            execution_state_root: format!("exec-state-{:020}", context.height),
        })
    }
}

fn with_noop_execution_hook(runtime: NodeRuntime) -> NodeRuntime {
    let calls: Arc<Mutex<Vec<NodeExecutionCommitContext>>> = Arc::new(Mutex::new(Vec::new()));
    runtime.with_execution_hook(RecordingExecutionHook::new(calls))
}

fn wait_until(deadline: Instant, mut predicate: impl FnMut() -> bool) -> bool {
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    predicate()
}

type TestHandler = Arc<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>;

#[derive(Clone, Default)]
struct TestInMemoryNetwork {
    retained: Arc<Mutex<HashMap<String, Vec<Vec<u8>>>>>,
    subscribers: Arc<Mutex<Vec<TestNetworkInbox>>>,
    handlers: Arc<Mutex<HashMap<String, TestHandler>>>,
}

type TestNetworkInbox = Arc<Mutex<HashMap<String, Vec<Vec<u8>>>>>;

impl TestInMemoryNetwork {
    fn clear_topic(&self, topic: &str) {
        self.retained
            .lock()
            .expect("lock retained")
            .insert(topic.to_string(), Vec::new());
        let subscribers = self.subscribers.lock().expect("lock subscribers");
        for inbox in subscribers.iter() {
            inbox
                .lock()
                .expect("lock subscriber inbox")
                .insert(topic.to_string(), Vec::new());
        }
    }
}

#[derive(Clone)]
struct ProviderAwareTestNetwork {
    inner: TestInMemoryNetwork,
    storage_root: PathBuf,
    required_provider_id: String,
    provider_attempts: Arc<Mutex<Vec<Vec<String>>>>,
}

impl ProviderAwareTestNetwork {
    fn new(storage_root: PathBuf, required_provider_id: impl Into<String>) -> Self {
        Self {
            inner: TestInMemoryNetwork::default(),
            storage_root,
            required_provider_id: required_provider_id.into(),
            provider_attempts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn provider_attempts(&self) -> Vec<Vec<String>> {
        self.provider_attempts
            .lock()
            .expect("lock provider attempts")
            .clone()
    }

    fn clear_topic(&self, topic: &str) {
        self.inner.clear_topic(topic);
    }
}

#[derive(Clone)]
struct ProviderFallbackTestNetwork {
    inner: TestInMemoryNetwork,
    storage_root: PathBuf,
    provider_attempts: Arc<Mutex<Vec<Vec<String>>>>,
    generic_attempts: Arc<Mutex<usize>>,
}

impl ProviderFallbackTestNetwork {
    fn new(storage_root: PathBuf) -> Self {
        Self {
            inner: TestInMemoryNetwork::default(),
            storage_root,
            provider_attempts: Arc::new(Mutex::new(Vec::new())),
            generic_attempts: Arc::new(Mutex::new(0)),
        }
    }

    fn provider_attempts(&self) -> Vec<Vec<String>> {
        self.provider_attempts
            .lock()
            .expect("lock provider attempts")
            .clone()
    }

    fn generic_attempts(&self) -> usize {
        *self.generic_attempts.lock().expect("lock generic attempts")
    }

    fn clear_topic(&self, topic: &str) {
        self.inner.clear_topic(topic);
    }
}

#[derive(Clone)]
struct ProviderNotFoundFallbackTestNetwork {
    inner: TestInMemoryNetwork,
    storage_root: PathBuf,
    provider_attempts: Arc<Mutex<Vec<Vec<String>>>>,
    generic_attempts: Arc<Mutex<usize>>,
}

impl ProviderNotFoundFallbackTestNetwork {
    fn new(storage_root: PathBuf) -> Self {
        Self {
            inner: TestInMemoryNetwork::default(),
            storage_root,
            provider_attempts: Arc::new(Mutex::new(Vec::new())),
            generic_attempts: Arc::new(Mutex::new(0)),
        }
    }

    fn provider_attempts(&self) -> Vec<Vec<String>> {
        self.provider_attempts
            .lock()
            .expect("lock provider attempts")
            .clone()
    }

    fn generic_attempts(&self) -> usize {
        *self.generic_attempts.lock().expect("lock generic attempts")
    }

    fn clear_topic(&self, topic: &str) {
        self.inner.clear_topic(topic);
    }
}

#[derive(Clone)]
struct TestReplicaMaintenanceDht {
    source_provider_id: String,
    local_provider_id: String,
    always_source_provider: bool,
    providers_by_hash: Arc<Mutex<HashMap<String, Vec<ProviderRecord>>>>,
    provider_seed_count: Arc<Mutex<usize>>,
    published: Arc<Mutex<Vec<(String, String, String)>>>,
    heads: Arc<Mutex<HashMap<String, WorldHeadAnnounce>>>,
    memberships: Arc<Mutex<HashMap<String, MembershipDirectorySnapshot>>>,
}

impl TestReplicaMaintenanceDht {
    fn new(source_provider_id: impl Into<String>, local_provider_id: impl Into<String>) -> Self {
        Self {
            source_provider_id: source_provider_id.into(),
            local_provider_id: local_provider_id.into(),
            always_source_provider: false,
            providers_by_hash: Arc::new(Mutex::new(HashMap::new())),
            provider_seed_count: Arc::new(Mutex::new(0)),
            published: Arc::new(Mutex::new(Vec::new())),
            heads: Arc::new(Mutex::new(HashMap::new())),
            memberships: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn with_source_provider_for_all_lookups(mut self) -> Self {
        self.always_source_provider = true;
        self
    }

    fn published_records(&self) -> Vec<(String, String, String)> {
        self.published.lock().expect("lock published").clone()
    }

    fn seed_provider(&self, content_hash: &str, provider_id: &str) {
        self.providers_by_hash
            .lock()
            .expect("lock providers_by_hash")
            .insert(
                content_hash.to_string(),
                vec![ProviderRecord {
                    provider_id: provider_id.to_string(),
                    last_seen_ms: 1_000,
                    storage_total_bytes: None,
                    storage_available_bytes: None,
                    uptime_ratio_per_mille: None,
                    challenge_pass_ratio_per_mille: None,
                    load_ratio_per_mille: None,
                    p50_read_latency_ms: None,
                }],
            );
    }
}

impl proto_dht::DistributedDht<WorldError> for TestReplicaMaintenanceDht {
    fn publish_provider(
        &self,
        world_id: &str,
        content_hash: &str,
        provider_id: &str,
    ) -> Result<(), WorldError> {
        self.published.lock().expect("lock published").push((
            world_id.to_string(),
            content_hash.to_string(),
            provider_id.to_string(),
        ));
        Ok(())
    }

    fn get_providers(
        &self,
        _world_id: &str,
        content_hash: &str,
    ) -> Result<Vec<ProviderRecord>, WorldError> {
        let mut providers_by_hash = self
            .providers_by_hash
            .lock()
            .expect("lock providers_by_hash");
        if let Some(cached) = providers_by_hash.get(content_hash) {
            return Ok(cached.clone());
        }

        let mut seed_count = self
            .provider_seed_count
            .lock()
            .expect("lock provider_seed_count");
        let providers = if self.always_source_provider || *seed_count == 0 {
            vec![ProviderRecord {
                provider_id: self.source_provider_id.clone(),
                last_seen_ms: 1_000,
                storage_total_bytes: None,
                storage_available_bytes: None,
                uptime_ratio_per_mille: None,
                challenge_pass_ratio_per_mille: None,
                load_ratio_per_mille: None,
                p50_read_latency_ms: None,
            }]
        } else {
            vec![ProviderRecord {
                provider_id: self.local_provider_id.clone(),
                last_seen_ms: 1_100,
                storage_total_bytes: None,
                storage_available_bytes: None,
                uptime_ratio_per_mille: None,
                challenge_pass_ratio_per_mille: None,
                load_ratio_per_mille: None,
                p50_read_latency_ms: None,
            }]
        };
        *seed_count = seed_count.saturating_add(1);
        providers_by_hash.insert(content_hash.to_string(), providers.clone());
        Ok(providers)
    }

    fn put_world_head(&self, world_id: &str, head: &WorldHeadAnnounce) -> Result<(), WorldError> {
        self.heads
            .lock()
            .expect("lock heads")
            .insert(world_id.to_string(), head.clone());
        Ok(())
    }

    fn get_world_head(&self, world_id: &str) -> Result<Option<WorldHeadAnnounce>, WorldError> {
        Ok(self
            .heads
            .lock()
            .expect("lock heads")
            .get(world_id)
            .cloned())
    }

    fn put_membership_directory(
        &self,
        world_id: &str,
        snapshot: &MembershipDirectorySnapshot,
    ) -> Result<(), WorldError> {
        self.memberships
            .lock()
            .expect("lock memberships")
            .insert(world_id.to_string(), snapshot.clone());
        Ok(())
    }

    fn get_membership_directory(
        &self,
        world_id: &str,
    ) -> Result<Option<MembershipDirectorySnapshot>, WorldError> {
        Ok(self
            .memberships
            .lock()
            .expect("lock memberships")
            .get(world_id)
            .cloned())
    }

    fn put_peer_record(
        &self,
        _world_id: &str,
        _record: &SignedPeerRecord,
    ) -> Result<(), WorldError> {
        Ok(())
    }

    fn get_peer_record(
        &self,
        _world_id: &str,
        _peer_id: &str,
    ) -> Result<Option<SignedPeerRecord>, WorldError> {
        Ok(None)
    }
}

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
        let handler = {
            let handlers = self.handlers.lock().expect("lock handlers");
            let Some(handler) = handlers.get(protocol) else {
                return Err(WorldError::NetworkProtocolUnavailable {
                    protocol: protocol.to_string(),
                });
            };
            Arc::clone(handler)
        };
        handler(payload)
    }

    fn register_handler(
        &self,
        protocol: &str,
        handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        self.handlers
            .lock()
            .expect("lock handlers")
            .insert(protocol.to_string(), Arc::from(handler));
        Ok(())
    }
}

impl oasis7_proto::distributed_net::DistributedNetwork<WorldError> for ProviderAwareTestNetwork {
    fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), WorldError> {
        self.inner.publish(topic, payload)
    }

    fn subscribe(&self, topic: &str) -> Result<NetworkSubscription, WorldError> {
        self.inner.subscribe(topic)
    }

    fn request(&self, protocol: &str, payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        self.request_with_providers(protocol, payload, &[])
    }

    fn request_with_providers(
        &self,
        protocol: &str,
        payload: &[u8],
        providers: &[String],
    ) -> Result<Vec<u8>, WorldError> {
        self.provider_attempts
            .lock()
            .expect("lock provider attempts")
            .push(providers.to_vec());
        if protocol != super::replication::REPLICATION_FETCH_BLOB_PROTOCOL {
            return self
                .inner
                .request_with_providers(protocol, payload, providers);
        }
        if !providers
            .iter()
            .any(|provider_id| provider_id == &self.required_provider_id)
        {
            return Err(WorldError::NetworkProtocolUnavailable {
                protocol: format!(
                    "provider selection missing required provider {}",
                    self.required_provider_id
                ),
            });
        }
        let request = serde_json::from_slice::<super::replication::FetchBlobRequest>(payload)
            .map_err(|err| WorldError::DistributedValidationFailed {
                reason: format!("decode fetch blob request failed: {err}"),
            })?;
        let blob = super::replication::load_blob_from_root(
            self.storage_root.as_path(),
            request.content_hash.as_str(),
        )
        .map_err(|err| WorldError::DistributedValidationFailed {
            reason: format!("load local blob failed: {err}"),
        })?;
        let response = super::replication::FetchBlobResponse {
            found: blob.is_some(),
            range_offset_bytes: None,
            range_complete: None,
            blob,
        };
        serde_json::to_vec(&response).map_err(|err| WorldError::DistributedValidationFailed {
            reason: format!("encode fetch blob response failed: {err}"),
        })
    }

    fn register_handler(
        &self,
        protocol: &str,
        handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        self.inner.register_handler(protocol, handler)
    }
}

impl oasis7_proto::distributed_net::DistributedNetwork<WorldError> for ProviderFallbackTestNetwork {
    fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), WorldError> {
        self.inner.publish(topic, payload)
    }

    fn subscribe(&self, topic: &str) -> Result<NetworkSubscription, WorldError> {
        self.inner.subscribe(topic)
    }

    fn request(&self, protocol: &str, payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        self.request_with_providers(protocol, payload, &[])
    }

    fn request_with_providers(
        &self,
        protocol: &str,
        payload: &[u8],
        providers: &[String],
    ) -> Result<Vec<u8>, WorldError> {
        if protocol != super::replication::REPLICATION_FETCH_BLOB_PROTOCOL {
            return self
                .inner
                .request_with_providers(protocol, payload, providers);
        }
        if !providers.is_empty() {
            self.provider_attempts
                .lock()
                .expect("lock provider attempts")
                .push(providers.to_vec());
            return Err(WorldError::NetworkProtocolUnavailable {
                protocol: "simulated provider route unavailable".to_string(),
            });
        }
        *self.generic_attempts.lock().expect("lock generic attempts") += 1;
        let request = serde_json::from_slice::<super::replication::FetchBlobRequest>(payload)
            .map_err(|err| WorldError::DistributedValidationFailed {
                reason: format!("decode fetch blob request failed: {err}"),
            })?;
        let blob = super::replication::load_blob_from_root(
            self.storage_root.as_path(),
            request.content_hash.as_str(),
        )
        .map_err(|err| WorldError::DistributedValidationFailed {
            reason: format!("load local blob failed: {err}"),
        })?;
        let response = super::replication::FetchBlobResponse {
            found: blob.is_some(),
            range_offset_bytes: None,
            range_complete: None,
            blob,
        };
        serde_json::to_vec(&response).map_err(|err| WorldError::DistributedValidationFailed {
            reason: format!("encode fetch blob response failed: {err}"),
        })
    }

    fn register_handler(
        &self,
        protocol: &str,
        handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        self.inner.register_handler(protocol, handler)
    }
}

impl oasis7_proto::distributed_net::DistributedNetwork<WorldError>
    for ProviderNotFoundFallbackTestNetwork
{
    fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), WorldError> {
        self.inner.publish(topic, payload)
    }

    fn subscribe(&self, topic: &str) -> Result<NetworkSubscription, WorldError> {
        self.inner.subscribe(topic)
    }

    fn request(&self, protocol: &str, payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        self.request_with_providers(protocol, payload, &[])
    }

    fn request_with_providers(
        &self,
        protocol: &str,
        payload: &[u8],
        providers: &[String],
    ) -> Result<Vec<u8>, WorldError> {
        if protocol != super::replication::REPLICATION_FETCH_BLOB_PROTOCOL {
            return self
                .inner
                .request_with_providers(protocol, payload, providers);
        }
        if !providers.is_empty() {
            self.provider_attempts
                .lock()
                .expect("lock provider attempts")
                .push(providers.to_vec());
            let response = super::replication::FetchBlobResponse {
                found: false,
            range_offset_bytes: None,
            range_complete: None,
                blob: None,
            };
            return serde_json::to_vec(&response).map_err(|err| {
                WorldError::DistributedValidationFailed {
                    reason: format!("encode fetch blob response failed: {err}"),
                }
            });
        }
        *self.generic_attempts.lock().expect("lock generic attempts") += 1;
        let request = serde_json::from_slice::<super::replication::FetchBlobRequest>(payload)
            .map_err(|err| WorldError::DistributedValidationFailed {
                reason: format!("decode fetch blob request failed: {err}"),
            })?;
        let blob = super::replication::load_blob_from_root(
            self.storage_root.as_path(),
            request.content_hash.as_str(),
        )
        .map_err(|err| WorldError::DistributedValidationFailed {
            reason: format!("load local blob failed: {err}"),
        })?;
        let response = super::replication::FetchBlobResponse {
            found: blob.is_some(),
            range_offset_bytes: None,
            range_complete: None,
            blob,
        };
        serde_json::to_vec(&response).map_err(|err| WorldError::DistributedValidationFailed {
            reason: format!("encode fetch blob response failed: {err}"),
        })
    }

    fn register_handler(
        &self,
        protocol: &str,
        handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        self.inner.register_handler(protocol, handler)
    }
}
