use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
#[cfg(feature = "libp2p")]
use std::time::{Duration, Instant};
#[cfg(feature = "libp2p")]
#[path = "tests_libp2p_discovery.rs"]
mod libp2p_discovery_tests;
#[cfg(feature = "libp2p")]
#[path = "tests_libp2p_helpers.rs"]
mod libp2p_test_helpers;
#[cfg(feature = "libp2p")]
#[path = "tests_libp2p_traffic.rs"]
mod libp2p_traffic_tests;
#[path = "tests_module_fetch.rs"]
mod module_fetch_tests;
use super::*;
use crate::util::to_canonical_cbor;
use oasis7_proto::distributed as proto_distributed;
use oasis7_proto::distributed_dht::{DistributedDht as _, SignedPeerRecord};
use oasis7_proto::distributed_net as proto_net;
use oasis7_proto::distributed_net::DistributedNetwork as _;
use oasis7_proto::distributed_storage as proto_storage;

#[cfg(feature = "libp2p")]
fn wait_until(what: &str, deadline: Instant, mut condition: impl FnMut() -> bool) {
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn net_exports_are_available() {
    let _ = std::any::type_name::<NetworkMessage>();
    let _ = std::any::type_name::<DistributedClient>();
    let _ = std::any::type_name::<CachedDht>();
    let _ = std::any::type_name::<DhtCacheConfig>();
    let _ = std::any::type_name::<dyn DistributedIndexStore>();
    let _ = std::any::type_name::<HeadIndexRecord>();
    let _ = std::any::type_name::<InMemoryIndexStore>();
    let _ = std::any::type_name::<ProviderCache>();
    let _ = std::any::type_name::<ProviderCacheConfig>();
    let _ = std::any::type_name::<ProviderSelectionPolicy>();
    let _ = std::any::type_name::<IndexPublishResult>();
    let _ = std::any::type_name::<SubmitActionReceipt>();
}

fn sample_action() -> proto_distributed::ActionEnvelope {
    proto_distributed::ActionEnvelope {
        world_id: "w1".to_string(),
        action_id: "a1".to_string(),
        actor_id: "actor-1".to_string(),
        action_kind: "test".to_string(),
        payload_cbor: vec![1, 2, 3],
        payload_hash: "hash".to_string(),
        nonce: 1,
        timestamp_ms: 10,
        intent_batch_hash: String::new(),
        idempotency_key: String::new(),
        zone_id: String::new(),
        signature: "sig".to_string(),
    }
}

#[test]
fn in_memory_publish_delivers_to_subscribers() {
    let network = InMemoryNetwork::new();
    let subscription = network.subscribe("aw.w1.action").expect("subscribe");

    network
        .publish("aw.w1.action", b"payload")
        .expect("publish");

    let messages = subscription.drain();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0], b"payload".to_vec());
}

#[test]
fn in_memory_publish_bounded_subscription_inbox_evicts_oldest_messages() {
    let network = InMemoryNetwork::new();
    let subscription = network.subscribe("aw.w1.action").expect("subscribe");
    let max_messages = proto_net::DEFAULT_SUBSCRIPTION_INBOX_MAX_MESSAGES;
    let total_messages = max_messages + 8;

    for idx in 0..total_messages {
        let payload = format!("payload-{idx}");
        network
            .publish("aw.w1.action", payload.as_bytes())
            .expect("publish");
    }

    let messages = subscription.drain();
    assert_eq!(messages.len(), max_messages);
    assert_eq!(messages.first(), Some(&b"payload-8".to_vec()));
    assert_eq!(
        messages.last(),
        Some(&format!("payload-{}", total_messages - 1).into_bytes())
    );
}

#[test]
fn gateway_publishes_action() {
    let network: Arc<dyn DistributedNetwork + Send + Sync> = Arc::new(InMemoryNetwork::new());
    let subscription = network.subscribe("aw.w1.action").expect("subscribe");
    let gateway = NetworkGateway::new_with_clock(Arc::clone(&network), Arc::new(|| 1234));

    let receipt = gateway.submit_action(sample_action()).expect("submit");
    assert_eq!(receipt.action_id, "a1");
    assert_eq!(receipt.accepted_at_ms, 1234);

    let messages = subscription.drain();
    assert_eq!(messages.len(), 1);
    let decoded: proto_distributed::ActionEnvelope =
        serde_cbor::from_slice(&messages[0]).expect("decode");
    assert_eq!(decoded.action_id, "a1");
}

#[test]
fn in_memory_request_invokes_handler() {
    let network = InMemoryNetwork::new();
    network
        .register_handler(
            "/aw/rr/1.0.0/get_world_head",
            Box::new(|payload| {
                let mut out = payload.to_vec();
                out.extend_from_slice(b"-ok");
                Ok(out)
            }),
        )
        .expect("register handler");

    let response = network
        .request("/aw/rr/1.0.0/get_world_head", b"ping")
        .expect("request");
    assert_eq!(response, b"ping-ok".to_vec());
}

#[cfg(feature = "libp2p")]
#[test]
fn libp2p_smoke_request_response_and_pubsub_work_between_peers() {
    use std::time::{Duration, Instant};

    use libp2p::Multiaddr;
    use oasis7_proto::distributed_dht::{
        PeerDeploymentMode, PeerDiscoverySource, PeerNodeRole, PeerReachabilityClass, PeerRecord,
    };

    let peer_manager_policy = PeerManagerPolicy {
        min_active_discovery_sources: 0,
        min_peer_discovery_sources: 0,
        min_active_peers_for_share_limits: 4,
        max_ipv4_subnet_active_peers: None,
        max_ipv4_subnet_share_per_mille: 1_000,
        block_ipv4_subnet_share_per_mille: 1_000,
        max_relay_domain_share_per_mille: 1_000,
        block_relay_domain_share_per_mille: 1_000,
        max_operator_share_per_mille: 1_000,
        block_operator_share_per_mille: 1_000,
        max_asn_share_per_mille: 1_000,
        block_asn_share_per_mille: 1_000,
        max_relayed_active_peer_share_per_mille: 1_000,
    };
    let peer_record = PeerRecord {
        peer_id: String::new(),
        node_id: "smoke-node".to_string(),
        world_id: "smoke-world".to_string(),
        network_id: "smoke-network".to_string(),
        node_role: PeerNodeRole::FullStorage.as_str().to_string(),
        deployment_mode: PeerDeploymentMode::Private,
        reachability_class: PeerReachabilityClass::Private,
        direct_addrs: Vec::new(),
        hole_punch_addrs: Vec::new(),
        relay_addrs: Vec::new(),
        discovery_sources: vec![PeerDiscoverySource::Dht],
        capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
        source_operator: None,
        source_asn: None,
        published_at_ms: 1,
        ttl_ms: 60_000,
    };

    fn wait_until(what: &str, deadline: Instant, mut condition: impl FnMut() -> bool) {
        while Instant::now() < deadline {
            if condition() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("timed out waiting for condition: {what}");
    }

    let listen_addr: Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().expect("multiaddr");
    let net1 = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec![listen_addr],
        peer_record: Some(peer_record.clone()),
        peer_manager_policy: peer_manager_policy.clone(),
        ..Libp2pNetworkConfig::default()
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    wait_until("net1 listening addrs", deadline, || {
        !net1.listening_addrs().is_empty()
    });
    let dial_addr = net1
        .listening_addrs()
        .into_iter()
        .find(|addr| addr.to_string().contains("127.0.0.1"))
        .expect("listening addr")
        .with(libp2p::multiaddr::Protocol::P2p(net1.peer_id()));

    net1.register_handler(
        "/aw/rr/1.0.0/ping",
        Box::new(|payload| {
            let mut out = payload.to_vec();
            out.extend_from_slice(b"-ok");
            Ok(out)
        }),
    )
    .expect("register handler");

    let net2 = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen")],
        bootstrap_peers: vec![dial_addr],
        peer_record: Some(peer_record),
        peer_manager_policy,
        ..Libp2pNetworkConfig::default()
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if !net2.connected_peers().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if net2.connected_peers().is_empty() {
        panic!(
            "timed out waiting for net2 connected peers; net2_errors={:?}; net1_errors={:?}; net1_addrs={:?}",
            net2.debug_errors(),
            net1.debug_errors(),
            net1.listening_addrs(),
        );
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_reply = None;
    let mut last_error = None;
    while Instant::now() < deadline {
        match net2.request("/aw/rr/1.0.0/ping", b"ping") {
            Ok(reply) => {
                if reply == b"ping-ok".to_vec() {
                    last_reply = Some(reply);
                    break;
                }
                last_reply = Some(reply);
                last_error = None;
            }
            Err(WorldError::NetworkProtocolUnavailable { .. }) => {
                last_error = Some("NetworkProtocolUnavailable".to_string());
            }
            Err(err) => panic!("unexpected request error: {err:?}"),
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        last_reply,
        Some(b"ping-ok".to_vec()),
        "request/response did not converge; last_error={last_error:?}; net2_errors={:?}; net1_errors={:?}",
        net2.debug_errors(),
        net1.debug_errors(),
    );

    let sub2 = net2.subscribe("aw.smoke").expect("sub2");
    let _sub1 = net1.subscribe("aw.smoke").expect("sub1");
    std::thread::sleep(Duration::from_millis(200));

    net1.publish("aw.smoke", b"hello").expect("publish");

    let deadline = Instant::now() + Duration::from_secs(10);
    wait_until("gossipsub deliver", deadline, || {
        sub2.drain().iter().any(|msg| msg == b"hello")
    });
}

#[cfg(feature = "libp2p")]
#[test]
fn libp2p_publish_surfaces_gossipsub_failures() {
    let net = Libp2pNetwork::new(Libp2pNetworkConfig::default());
    let _sub = net.subscribe("aw.publish.fail").expect("subscribe");

    let err = net
        .publish("aw.publish.fail", b"payload")
        .expect_err("publish without peers should fail");
    match err.diagnostic_source() {
        WorldError::NetworkProtocolUnavailable { protocol } => {
            assert!(protocol.contains("libp2p publish failed topic=aw.publish.fail"));
        }
        other => panic!("unexpected publish error: {other:?}"),
    }
    assert!(
        net.published().is_empty(),
        "failed publish should not be recorded"
    );
}

#[cfg(feature = "libp2p")]
#[test]
fn libp2p_best_effort_publish_does_not_surface_gossipsub_failures() {
    let net = Libp2pNetwork::new(Libp2pNetworkConfig::default());
    let _sub = net
        .subscribe("aw.publish.best_effort_fail")
        .expect("subscribe");

    net.publish_best_effort("aw.publish.best_effort_fail", b"payload")
        .expect("best-effort publish should enqueue without waiting for gossipsub result");

    let deadline = Instant::now() + Duration::from_secs(2);
    wait_until("best-effort publish failure recorded", deadline, || {
        net.debug_errors()
            .iter()
            .any(|err| err.contains("libp2p publish failed topic=aw.publish.best_effort_fail"))
    });
    assert!(
        net.published().is_empty(),
        "failed best-effort publish should not be recorded as delivered"
    );
}

#[test]
fn in_memory_dht_stores_providers() {
    let dht = InMemoryDht::new();
    dht.publish_provider("w1", "hash", "peer-1")
        .expect("publish provider");
    dht.publish_provider("w1", "hash", "peer-2")
        .expect("publish provider");

    let providers = dht.get_providers("w1", "hash").expect("get providers");
    assert_eq!(providers.len(), 2);
}

#[test]
fn in_memory_dht_tracks_world_head() {
    let dht = InMemoryDht::new();
    let head = proto_distributed::WorldHeadAnnounce {
        world_id: "w1".to_string(),
        height: 1,
        block_hash: "b1".to_string(),
        state_root: "s1".to_string(),
        timestamp_ms: 1,
        signature: "sig".to_string(),
    };
    dht.put_world_head("w1", &head).expect("put head");

    let loaded = dht.get_world_head("w1").expect("get head");
    assert_eq!(loaded, Some(head));
}

#[test]
fn in_memory_dht_tracks_membership_directory_snapshot() {
    let dht = InMemoryDht::new();
    let snapshot = MembershipDirectorySnapshot {
        world_id: "w1".to_string(),
        requester_id: "seq-1".to_string(),
        requested_at_ms: 1,
        reason: Some("bootstrap".to_string()),
        validators: vec![
            "seq-1".to_string(),
            "seq-2".to_string(),
            "seq-3".to_string(),
        ],
        quorum_threshold: 2,
        signature_key_id: Some("k1".to_string()),
        signature: Some("deadbeef".to_string()),
    };
    dht.put_membership_directory("w1", &snapshot)
        .expect("put membership");

    let loaded = dht.get_membership_directory("w1").expect("get membership");
    assert_eq!(loaded, Some(snapshot));
}

fn sample_blob_ref(content_hash: &str) -> proto_distributed::BlobRef {
    proto_distributed::BlobRef {
        content_hash: content_hash.to_string(),
        size_bytes: 1,
        codec: "dag-cbor".to_string(),
        links: Vec::new(),
    }
}

fn sample_segment_ref(
    from_event_id: u64,
    to_event_id: u64,
    content_hash: &str,
) -> proto_storage::JournalSegmentRef {
    proto_storage::JournalSegmentRef {
        from_event_id,
        to_event_id,
        content_hash: content_hash.to_string(),
        size_bytes: 1,
    }
}

fn sample_write_result() -> ExecutionWriteResult {
    ExecutionWriteResult {
        block: proto_distributed::WorldBlock {
            world_id: "w1".to_string(),
            height: 1,
            prev_block_hash: "genesis".to_string(),
            action_root: "action-root".to_string(),
            event_root: "event-root".to_string(),
            state_root: "state-root".to_string(),
            journal_ref: "journal-index".to_string(),
            snapshot_ref: "snapshot-manifest".to_string(),
            receipts_root: "receipts-root".to_string(),
            proposer_id: "node-1".to_string(),
            timestamp_ms: 1,
            signature: "sig".to_string(),
        },
        block_hash: "block-hash".to_string(),
        block_ref: sample_blob_ref("block-hash"),
        block_announce: proto_distributed::BlockAnnounce {
            world_id: "w1".to_string(),
            height: 1,
            block_hash: "block-hash".to_string(),
            prev_block_hash: "genesis".to_string(),
            state_root: "state-root".to_string(),
            event_root: "event-root".to_string(),
            timestamp_ms: 1,
            signature: "sig".to_string(),
        },
        head_announce: proto_distributed::WorldHeadAnnounce {
            world_id: "w1".to_string(),
            height: 1,
            block_hash: "block-hash".to_string(),
            state_root: "state-root".to_string(),
            timestamp_ms: 1,
            signature: "sig".to_string(),
        },
        snapshot_manifest: proto_distributed::SnapshotManifest {
            world_id: "w1".to_string(),
            epoch: 1,
            chunks: vec![
                proto_distributed::StateChunkRef {
                    chunk_id: "chunk-1".to_string(),
                    content_hash: "chunk-hash-1".to_string(),
                    size_bytes: 1,
                },
                proto_distributed::StateChunkRef {
                    chunk_id: "chunk-2".to_string(),
                    content_hash: "chunk-hash-2".to_string(),
                    size_bytes: 1,
                },
            ],
            state_root: "state-root".to_string(),
        },
        snapshot_manifest_ref: sample_blob_ref("snapshot-manifest"),
        journal_segments: vec![
            sample_segment_ref(1, 1, "journal-seg-1"),
            sample_segment_ref(2, 2, "journal-seg-2"),
        ],
        journal_segments_ref: sample_blob_ref("journal-index"),
    }
}

#[test]
fn publish_execution_providers_indexes_all_hashes() {
    let write = sample_write_result();
    let dht = InMemoryDht::new();
    let result =
        publish_execution_providers(&dht, "w1", "store-1", &write).expect("publish providers");
    assert!(result.published > 0);

    let mut expected = HashSet::new();
    expected.insert(write.block_ref.content_hash.clone());
    expected.insert(write.snapshot_manifest_ref.content_hash.clone());
    expected.insert(write.journal_segments_ref.content_hash.clone());
    for chunk in &write.snapshot_manifest.chunks {
        expected.insert(chunk.content_hash.clone());
    }
    for segment in &write.journal_segments {
        expected.insert(segment.content_hash.clone());
    }

    for hash in expected {
        let providers = query_providers(&dht, "w1", &hash).expect("get providers");
        assert!(!providers.is_empty());
        assert_eq!(providers[0].provider_id, "store-1");
    }
}

#[test]
fn publish_execution_providers_cached_indexes_all_hashes() {
    let write = sample_write_result();
    let dht = InMemoryDht::new();
    let index_store = InMemoryIndexStore::new();
    let cache = ProviderCache::new(
        Arc::new(dht.clone()),
        Arc::new(index_store),
        "store-1",
        ProviderCacheConfig::default(),
    );

    let result =
        publish_execution_providers_cached(&cache, "w1", &write).expect("publish providers");
    assert!(result.published > 0);

    for hash in [
        "block-hash",
        "snapshot-manifest",
        "journal-index",
        "chunk-hash-1",
        "chunk-hash-2",
        "journal-seg-1",
        "journal-seg-2",
    ] {
        let providers = query_providers(&dht, "w1", hash).expect("get providers");
        assert!(!providers.is_empty(), "missing providers for {hash}");
    }
}

#[derive(Default)]
struct SpyNetwork {
    providers: Arc<Mutex<Vec<String>>>,
    provider_attempts: Arc<Mutex<Vec<Vec<String>>>>,
    provider_failures_remaining: Arc<Mutex<HashMap<String, usize>>>,
    blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl SpyNetwork {
    fn providers(&self) -> Vec<String> {
        self.providers.lock().expect("lock providers").clone()
    }

    fn provider_attempts(&self) -> Vec<Vec<String>> {
        self.provider_attempts
            .lock()
            .expect("lock provider attempts")
            .clone()
    }

    fn set_blob(&self, content_hash: &str, bytes: Vec<u8>) {
        let mut blobs = self.blobs.lock().expect("lock blobs");
        blobs.insert(content_hash.to_string(), bytes);
    }

    fn fail_provider_requests(&self, provider_id: &str, failures: usize) {
        let mut failures_remaining = self
            .provider_failures_remaining
            .lock()
            .expect("lock provider failures");
        failures_remaining.insert(provider_id.to_string(), failures);
    }
}

impl proto_net::DistributedNetwork<WorldError> for SpyNetwork {
    fn publish(&self, _topic: &str, _payload: &[u8]) -> Result<(), WorldError> {
        Ok(())
    }

    fn subscribe(&self, _topic: &str) -> Result<NetworkSubscription, WorldError> {
        Err(WorldError::NetworkProtocolUnavailable {
            protocol: "spy".to_string(),
        })
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
        let mut captured = self.providers.lock().expect("lock providers");
        *captured = providers.to_vec();

        match protocol {
            proto_distributed::RR_FETCH_BLOB => {
                self.provider_attempts
                    .lock()
                    .expect("lock provider attempts")
                    .push(providers.to_vec());
                if let Some(provider_id) = providers.first() {
                    let mut failures_remaining = self
                        .provider_failures_remaining
                        .lock()
                        .expect("lock provider failures");
                    if let Some(remaining) = failures_remaining.get_mut(provider_id)
                        && *remaining > 0
                    {
                        *remaining -= 1;
                        return Err(WorldError::NetworkProtocolUnavailable {
                            protocol: format!("provider {provider_id} unavailable"),
                        });
                    }
                }

                let request: proto_distributed::FetchBlobRequest = serde_cbor::from_slice(payload)?;
                let blob = self
                    .blobs
                    .lock()
                    .expect("lock blobs")
                    .get(&request.content_hash)
                    .cloned()
                    .unwrap_or_else(|| b"data".to_vec());
                let response = proto_distributed::FetchBlobResponse {
                    blob,
                    content_hash: request.content_hash,
                };
                Ok(to_canonical_cbor(&response)?)
            }
            proto_distributed::RR_GET_MODULE_MANIFEST => {
                let request: proto_distributed::GetModuleManifestRequest =
                    serde_cbor::from_slice(payload)?;
                let response = proto_distributed::GetModuleManifestResponse {
                    manifest_ref: proto_distributed::BlobRef {
                        content_hash: request.manifest_hash,
                        size_bytes: 0,
                        codec: "raw".to_string(),
                        links: Vec::new(),
                    },
                };
                Ok(to_canonical_cbor(&response)?)
            }
            proto_distributed::RR_GET_MODULE_ARTIFACT => {
                let request: proto_distributed::GetModuleArtifactRequest =
                    serde_cbor::from_slice(payload)?;
                let response = proto_distributed::GetModuleArtifactResponse {
                    artifact_ref: proto_distributed::BlobRef {
                        content_hash: request.wasm_hash,
                        size_bytes: 0,
                        codec: "raw".to_string(),
                        links: Vec::new(),
                    },
                };
                Ok(to_canonical_cbor(&response)?)
            }
            _ => Err(WorldError::NetworkProtocolUnavailable {
                protocol: protocol.to_string(),
            }),
        }
    }

    fn register_handler(
        &self,
        _protocol: &str,
        _handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct StaticProvidersDht {
    default_providers: Vec<ProviderRecord>,
    providers_by_hash: HashMap<String, Vec<ProviderRecord>>,
}

impl StaticProvidersDht {
    fn new(providers: Vec<ProviderRecord>) -> Self {
        Self {
            default_providers: providers,
            providers_by_hash: HashMap::new(),
        }
    }

    fn with_providers_by_hash(providers_by_hash: HashMap<String, Vec<ProviderRecord>>) -> Self {
        Self {
            default_providers: Vec::new(),
            providers_by_hash,
        }
    }
}

impl proto_dht::DistributedDht<WorldError> for StaticProvidersDht {
    fn publish_provider(
        &self,
        _world_id: &str,
        _content_hash: &str,
        _provider_id: &str,
    ) -> Result<(), WorldError> {
        Ok(())
    }

    fn get_providers(
        &self,
        _world_id: &str,
        content_hash: &str,
    ) -> Result<Vec<ProviderRecord>, WorldError> {
        if let Some(providers) = self.providers_by_hash.get(content_hash) {
            return Ok(providers.clone());
        }
        Ok(self.default_providers.clone())
    }

    fn put_world_head(
        &self,
        _world_id: &str,
        _head: &proto_distributed::WorldHeadAnnounce,
    ) -> Result<(), WorldError> {
        Ok(())
    }

    fn get_world_head(
        &self,
        _world_id: &str,
    ) -> Result<Option<proto_distributed::WorldHeadAnnounce>, WorldError> {
        Ok(None)
    }

    fn put_membership_directory(
        &self,
        _world_id: &str,
        _snapshot: &MembershipDirectorySnapshot,
    ) -> Result<(), WorldError> {
        Ok(())
    }

    fn get_membership_directory(
        &self,
        _world_id: &str,
    ) -> Result<Option<MembershipDirectorySnapshot>, WorldError> {
        Ok(None)
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

#[test]
fn client_get_world_head_round_trip() {
    let network = InMemoryNetwork::new();
    network
        .register_handler(
            proto_distributed::RR_GET_WORLD_HEAD,
            Box::new(|payload| {
                let request: proto_distributed::GetWorldHeadRequest =
                    serde_cbor::from_slice(payload).expect("decode request");
                assert_eq!(request.world_id, "w1");
                let response = proto_distributed::GetWorldHeadResponse {
                    head: proto_distributed::WorldHeadAnnounce {
                        world_id: "w1".to_string(),
                        height: 7,
                        block_hash: "b1".to_string(),
                        state_root: "s1".to_string(),
                        timestamp_ms: 123,
                        signature: "sig".to_string(),
                    },
                };
                Ok(to_canonical_cbor(&response).expect("encode response"))
            }),
        )
        .expect("register handler");

    let client = DistributedClient::new(Arc::new(network));
    let head = client.get_world_head("w1").expect("get world head");
    assert_eq!(head.height, 7);
}

#[test]
fn client_maps_error_response() {
    let network = InMemoryNetwork::new();
    network
        .register_handler(
            proto_distributed::RR_FETCH_BLOB,
            Box::new(|_payload| {
                let response = proto_distributed::ErrorResponse {
                    code: proto_distributed::DistributedErrorCode::ErrNotFound,
                    message: "missing".to_string(),
                    retryable: false,
                };
                Ok(to_canonical_cbor(&response).expect("encode"))
            }),
        )
        .expect("register handler");

    let client = DistributedClient::new(Arc::new(network));
    let err = client.fetch_blob("missing").expect_err("expect error");
    assert!(matches!(err, WorldError::NetworkRequestFailed { .. }));
}

#[test]
fn client_fetch_blob_with_providers_passes_list() {
    let spy = Arc::new(SpyNetwork::default());
    let network: Arc<dyn DistributedNetwork + Send + Sync> = spy.clone();
    let client = DistributedClient::new(network);
    let providers = vec!["p1".to_string(), "p2".to_string()];
    let blob = client
        .fetch_blob_with_providers("hash", &providers)
        .expect("fetch");
    assert_eq!(blob, b"data".to_vec());

    let seen = spy.providers();
    assert_eq!(seen, providers);
}

fn provider_record(provider_id: &str, last_seen_ms: i64) -> ProviderRecord {
    ProviderRecord {
        provider_id: provider_id.to_string(),
        last_seen_ms,
        storage_total_bytes: None,
        storage_available_bytes: None,
        uptime_ratio_per_mille: None,
        challenge_pass_ratio_per_mille: None,
        load_ratio_per_mille: None,
        p50_read_latency_ms: None,
    }
}

fn map_dht(entries: &[(&str, &[&str])]) -> StaticProvidersDht {
    let mut providers_by_hash = HashMap::new();
    for (content_hash, providers) in entries {
        let records = providers
            .iter()
            .map(|provider_id| provider_record(provider_id, 1_000))
            .collect();
        providers_by_hash.insert((*content_hash).to_string(), records);
    }
    StaticProvidersDht::with_providers_by_hash(providers_by_hash)
}

#[test]
fn client_fetch_blob_from_dht_uses_provider_list() {
    let spy = Arc::new(SpyNetwork::default());
    let network: Arc<dyn DistributedNetwork + Send + Sync> = spy.clone();
    let client = DistributedClient::new(network);
    let dht = InMemoryDht::new();
    dht.publish_provider("w1", "hash", "peer-1")
        .expect("publish provider");

    let blob = client
        .fetch_blob_from_dht("w1", "hash", &dht)
        .expect("fetch");
    assert_eq!(blob, b"data".to_vec());

    let seen = spy.providers();
    assert_eq!(seen, vec!["peer-1".to_string()]);
}

#[test]
fn client_fetch_blob_from_dht_retries_ranked_providers_until_success() {
    let spy = Arc::new(SpyNetwork::default());
    spy.set_blob("hash-rank", b"ranked-success".to_vec());
    spy.fail_provider_requests("peer-1", 1);

    let network: Arc<dyn DistributedNetwork + Send + Sync> = spy.clone();
    let client = DistributedClient::new(network);
    let mut preferred = provider_record("peer-1", 1_000);
    preferred.storage_total_bytes = Some(100);
    preferred.storage_available_bytes = Some(90);
    preferred.uptime_ratio_per_mille = Some(990);
    preferred.challenge_pass_ratio_per_mille = Some(980);
    preferred.load_ratio_per_mille = Some(80);
    preferred.p50_read_latency_ms = Some(20);

    let mut fallback = provider_record("peer-2", 1_000);
    fallback.storage_total_bytes = Some(100);
    fallback.storage_available_bytes = Some(10);
    fallback.uptime_ratio_per_mille = Some(600);
    fallback.challenge_pass_ratio_per_mille = Some(600);
    fallback.load_ratio_per_mille = Some(900);
    fallback.p50_read_latency_ms = Some(900);

    let dht = StaticProvidersDht::new(vec![fallback, preferred]);
    let blob = client
        .fetch_blob_from_dht("w1", "hash-rank", &dht)
        .expect("fetch");
    assert_eq!(blob, b"ranked-success".to_vec());
    assert_eq!(
        spy.provider_attempts(),
        vec![vec!["peer-1".to_string()], vec!["peer-2".to_string()]]
    );
}

#[test]
fn client_fetch_blob_from_dht_fails_when_no_providers() {
    let spy = Arc::new(SpyNetwork::default());
    let network: Arc<dyn DistributedNetwork + Send + Sync> = spy.clone();
    let client = DistributedClient::new(network);
    let dht = InMemoryDht::new();

    let err = client
        .fetch_blob_from_dht("w1", "hash-missing", &dht)
        .expect_err("must fail when dht has no provider");
    assert!(matches!(
        err,
        WorldError::DistributedValidationFailed { .. }
    ));
    assert!(spy.provider_attempts().is_empty());
}

#[test]
fn client_fetch_blob_from_dht_fails_after_ranked_provider_failures() {
    let spy = Arc::new(SpyNetwork::default());
    spy.set_blob("hash-fallback", b"fallback-success".to_vec());
    spy.fail_provider_requests("peer-1", 1);
    spy.fail_provider_requests("peer-2", 1);

    let network: Arc<dyn DistributedNetwork + Send + Sync> = spy.clone();
    let client = DistributedClient::new(network);
    let mut preferred = provider_record("peer-1", 1_000);
    preferred.uptime_ratio_per_mille = Some(980);
    preferred.challenge_pass_ratio_per_mille = Some(980);

    let mut fallback = provider_record("peer-2", 1_000);
    fallback.uptime_ratio_per_mille = Some(800);
    fallback.challenge_pass_ratio_per_mille = Some(800);

    let dht = StaticProvidersDht::new(vec![fallback, preferred]);
    let err = client
        .fetch_blob_from_dht("w1", "hash-fallback", &dht)
        .expect_err("all providers should fail");
    assert!(matches!(err, WorldError::NetworkProtocolUnavailable { .. }));
    assert_eq!(
        spy.provider_attempts(),
        vec![vec!["peer-1".to_string()], vec!["peer-2".to_string()]]
    );
}

#[test]
fn provider_distribution_rejects_insufficient_replicas() {
    let dht = map_dht(&[("hash-a", &["peer-1"]), ("hash-b", &["peer-1", "peer-2"])]);
    let hashes = vec!["hash-a".to_string(), "hash-b".to_string()];

    let err =
        audit_provider_distribution(&dht, "w1", &hashes, ProviderDistributionPolicy::default())
            .expect_err("insufficient replicas must fail");
    assert!(matches!(
        err,
        WorldError::DistributedValidationFailed { .. }
    ));
}

#[test]
fn provider_distribution_dedupes_duplicate_providers_per_hash() {
    let dht = map_dht(&[("hash-a", &["peer-1", "peer-1"])]);
    let hashes = vec!["hash-a".to_string()];

    let err =
        audit_provider_distribution(&dht, "w1", &hashes, ProviderDistributionPolicy::default())
            .expect_err("duplicate provider ids must not count as separate replicas");
    assert!(matches!(
        err,
        WorldError::DistributedValidationFailed { .. }
    ));
}

#[test]
fn provider_distribution_rejects_single_provider_full_coverage() {
    let dht = map_dht(&[
        ("hash-a", &["peer-1", "peer-2"]),
        ("hash-b", &["peer-1", "peer-3"]),
    ]);
    let hashes = vec!["hash-a".to_string(), "hash-b".to_string()];

    let err =
        audit_provider_distribution(&dht, "w1", &hashes, ProviderDistributionPolicy::default())
            .expect_err("single provider full coverage must fail");
    assert!(matches!(
        err,
        WorldError::DistributedValidationFailed { .. }
    ));
}

#[test]
fn provider_distribution_accepts_distributed_coverage() {
    let dht = map_dht(&[
        ("hash-a", &["peer-1", "peer-2"]),
        ("hash-b", &["peer-2", "peer-3"]),
        ("hash-c", &["peer-1", "peer-3"]),
    ]);
    let hashes = vec![
        "hash-a".to_string(),
        "hash-b".to_string(),
        "hash-c".to_string(),
    ];

    let audit =
        audit_provider_distribution(&dht, "w1", &hashes, ProviderDistributionPolicy::default())
            .expect("distribution audit");
    assert_eq!(audit.required_blob_count, 3);
    assert_eq!(audit.distinct_provider_count, 3);
}

#[test]
fn client_fetch_blobs_from_dht_with_distribution_prevents_single_provider_full_coverage() {
    let spy = Arc::new(SpyNetwork::default());
    let network: Arc<dyn DistributedNetwork + Send + Sync> = spy.clone();
    let client = DistributedClient::new(network);
    let dht = map_dht(&[
        ("hash-a", &["peer-1", "peer-2"]),
        ("hash-b", &["peer-1", "peer-3"]),
    ]);
    let hashes = vec!["hash-a".to_string(), "hash-b".to_string()];

    let err = client
        .fetch_blobs_from_dht_with_distribution(
            "w1",
            &hashes,
            &dht,
            ProviderDistributionPolicy::default(),
        )
        .expect_err("distribution preflight must fail");
    assert!(matches!(
        err,
        WorldError::DistributedValidationFailed { .. }
    ));
    assert!(spy.provider_attempts().is_empty());
}

#[test]
fn client_fetch_blobs_from_dht_with_distribution_fetches_on_valid_distribution() {
    let spy = Arc::new(SpyNetwork::default());
    spy.set_blob("hash-a", b"blob-a".to_vec());
    spy.set_blob("hash-b", b"blob-b".to_vec());

    let network: Arc<dyn DistributedNetwork + Send + Sync> = spy.clone();
    let client = DistributedClient::new(network);
    let dht = map_dht(&[
        ("hash-a", &["peer-1", "peer-2"]),
        ("hash-b", &["peer-3", "peer-4"]),
    ]);
    let hashes = vec!["hash-a".to_string(), "hash-b".to_string()];

    let blobs = client
        .fetch_blobs_from_dht_with_distribution(
            "w1",
            &hashes,
            &dht,
            ProviderDistributionPolicy::default(),
        )
        .expect("fetch distributed blobs");
    assert_eq!(blobs.get("hash-a"), Some(&b"blob-a".to_vec()));
    assert_eq!(blobs.get("hash-b"), Some(&b"blob-b".to_vec()));
    assert_eq!(spy.provider_attempts().len(), 2);
}

#[test]
fn client_fetch_blob_from_dht_supports_legacy_provider_records_without_capabilities() {
    let spy = Arc::new(SpyNetwork::default());
    spy.set_blob("hash-legacy", b"legacy-success".to_vec());
    spy.fail_provider_requests("peer-fresh", 1);

    let network: Arc<dyn DistributedNetwork + Send + Sync> = spy.clone();
    let client = DistributedClient::new(network);
    let fresh_legacy = provider_record("peer-fresh", i64::MAX);
    let stale_legacy = provider_record("peer-stale", 0);
    let dht = StaticProvidersDht::new(vec![stale_legacy, fresh_legacy]);

    let blob = client
        .fetch_blob_from_dht("w1", "hash-legacy", &dht)
        .expect("fetch");
    assert_eq!(blob, b"legacy-success".to_vec());
    assert_eq!(
        spy.provider_attempts(),
        vec![
            vec!["peer-fresh".to_string()],
            vec!["peer-stale".to_string()]
        ]
    );
}
