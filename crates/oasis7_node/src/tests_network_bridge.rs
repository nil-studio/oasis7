use super::*;
use oasis7_distfs::FileReplicationRecord;
use oasis7_proto::distributed_dht::{
    MembershipDirectorySnapshot, ProviderRecord, SignedPeerRecord,
};
use std::sync::mpsc;
use std::time::Duration;

use crate::{NodeConfig, NodeRole, compute_consensus_action_root};

struct NoopDistributedNetwork;

impl DistributedNetwork<WorldError> for NoopDistributedNetwork {
    fn publish(&self, _topic: &str, _payload: &[u8]) -> Result<(), WorldError> {
        Ok(())
    }

    fn subscribe(&self, topic: &str) -> Result<NetworkSubscription, WorldError> {
        Ok(NetworkSubscription::new(
            topic.to_string(),
            Arc::new(Mutex::new(HashMap::new())),
        ))
    }

    fn request(&self, protocol: &str, _payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        Err(WorldError::NetworkProtocolUnavailable {
            protocol: protocol.to_string(),
        })
    }

    fn register_handler(
        &self,
        _protocol: &str,
        _handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        Ok(())
    }
}

#[derive(Default)]
struct BestEffortOnlyNetwork {
    best_effort_topics: Mutex<Vec<String>>,
}

impl DistributedNetwork<WorldError> for BestEffortOnlyNetwork {
    fn publish(&self, topic: &str, _payload: &[u8]) -> Result<(), WorldError> {
        Err(WorldError::NetworkProtocolUnavailable {
            protocol: format!("sync publish should not be used for topic {topic}"),
        })
    }

    fn publish_best_effort(&self, topic: &str, _payload: &[u8]) -> Result<(), WorldError> {
        self.best_effort_topics
            .lock()
            .expect("lock best-effort topics")
            .push(topic.to_string());
        Ok(())
    }

    fn subscribe(&self, topic: &str) -> Result<NetworkSubscription, WorldError> {
        Ok(NetworkSubscription::new(
            topic.to_string(),
            Arc::new(Mutex::new(HashMap::new())),
        ))
    }

    fn request(&self, protocol: &str, _payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        Err(WorldError::NetworkProtocolUnavailable {
            protocol: protocol.to_string(),
        })
    }

    fn register_handler(
        &self,
        _protocol: &str,
        _handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        Ok(())
    }
}

struct BlockingProviderDht {
    entered: Mutex<mpsc::Sender<()>>,
    release: Mutex<mpsc::Receiver<()>>,
    fail_best_effort: bool,
}

impl proto_dht::DistributedDht<WorldError> for BlockingProviderDht {
    fn publish_provider(
        &self,
        _world_id: &str,
        _content_hash: &str,
        _provider_id: &str,
    ) -> Result<(), WorldError> {
        let _ = self.entered.lock().expect("lock entered").send(());
        let _ = self
            .release
            .lock()
            .expect("lock release")
            .recv_timeout(Duration::from_secs(2));
        Ok(())
    }

    fn publish_provider_best_effort(
        &self,
        _world_id: &str,
        _content_hash: &str,
        _provider_id: &str,
    ) -> Result<(), WorldError> {
        let _ = self.entered.lock().expect("lock entered").send(());
        if self.fail_best_effort {
            Err(WorldError::NetworkProtocolUnavailable {
                protocol: "controlled provider publication failure".to_string(),
            })
        } else {
            Ok(())
        }
    }

    fn get_providers(
        &self,
        _world_id: &str,
        _content_hash: &str,
    ) -> Result<Vec<ProviderRecord>, WorldError> {
        Ok(Vec::new())
    }

    fn put_world_head(&self, _world_id: &str, _head: &WorldHeadAnnounce) -> Result<(), WorldError> {
        Ok(())
    }

    fn get_world_head(&self, _world_id: &str) -> Result<Option<WorldHeadAnnounce>, WorldError> {
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
fn consensus_gossip_publish_uses_best_effort_network_path() {
    let network = Arc::new(BestEffortOnlyNetwork::default());
    let handle = NodeReplicationNetworkHandle::new(network.clone());
    let config =
        NodeConfig::new("node-a", "world-gossip-best-effort", NodeRole::Sequencer).expect("config");
    let endpoint = ConsensusNetworkEndpoint::new(
        &handle,
        "world-gossip-best-effort",
        false,
        &config.network_policy,
    )
    .expect("endpoint");
    let proposal = GossipProposalMessage {
        version: 1,
        world_id: "world-gossip-best-effort".to_string(),
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        proposer_id: "node-a".to_string(),
        height: 1,
        slot: 1,
        epoch: 0,
        block_hash: "block-hash".to_string(),
        action_root: compute_consensus_action_root(&[]).expect("empty action root"),
        actions: Vec::new(),
        proposed_at_ms: 1,
        public_key_hex: None,
        signature_hex: None,
    };

    endpoint
        .publish_proposal(&proposal)
        .expect("best-effort consensus publish should not surface sync publish failure");

    let topics = network
        .best_effort_topics
        .lock()
        .expect("lock best-effort topics");
    assert_eq!(
        topics.as_slice(),
        &["aw.world-gossip-best-effort.consensus.proposal"]
    );
}

#[test]
fn replication_gossip_publish_uses_best_effort_network_path() {
    let network = Arc::new(BestEffortOnlyNetwork::default());
    let handle = NodeReplicationNetworkHandle::new(network.clone());
    let config = NodeConfig::new(
        "node-a",
        "world-replication-best-effort",
        NodeRole::Sequencer,
    )
    .expect("config");
    let endpoint = ReplicationNetworkEndpoint::new(
        &handle,
        "world-replication-best-effort",
        false,
        &config.network_policy,
    )
    .expect("endpoint");
    let message = GossipReplicationMessage {
        version: 1,
        world_id: "world-replication-best-effort".to_string(),
        node_id: "node-a".to_string(),
        record: FileReplicationRecord {
            world_id: "world-replication-best-effort".to_string(),
            writer_id: "node-a".to_string(),
            writer_epoch: 1,
            sequence: 1,
            path: "consensus/commits/00000000000000000001.json".to_string(),
            content_hash: "hash-1".to_string(),
            size_bytes: 7,
            updated_at_ms: 1,
        },
        payload: b"payload".to_vec(),
        public_key_hex: None,
        signature_hex: None,
    };

    endpoint
        .publish_replication(&message)
        .expect("best-effort replication publish should not surface sync publish failure");

    let topics = network
        .best_effort_topics
        .lock()
        .expect("lock best-effort topics");
    assert_eq!(
        topics.as_slice(),
        &["aw.world-replication-best-effort.replication"]
    );
}

#[test]
fn publish_failure_stays_generic_replication_error() {
    let err = network_err(WorldError::NetworkProtocolUnavailable {
        protocol: "libp2p publish failed topic=aw.publish.fail: InsufficientPeers".to_string(),
    });
    assert_eq!(
        err.replication_reason(),
        Some(
            "replication network error: NetworkProtocolUnavailable { protocol: \"libp2p publish failed topic=aw.publish.fail: InsufficientPeers\" }"
        )
    );
    assert_eq!(
        crate::network_error_classification::replication_network_failure(&err).origin,
        oasis7_proto::network_failure::FailureOrigin::Publish
    );
}

#[test]
fn cacheable_fetch_commit_success_response_drops_payload_allocation() {
    let mut payload = Vec::with_capacity(4096);
    payload.extend_from_slice(b"replicated-commit-payload");
    let response = FetchCommitResponse {
        found: true,
        message: Some(GossipReplicationMessage {
            version: 1,
            world_id: "world-cache".to_string(),
            node_id: "node-a".to_string(),
            record: FileReplicationRecord {
                world_id: "world-cache".to_string(),
                writer_id: "writer-a".to_string(),
                writer_epoch: 1,
                sequence: 1,
                path: "consensus/commits/00000000000000000001.json".to_string(),
                content_hash: "hash-1".to_string(),
                size_bytes: payload.len() as u64,
                updated_at_ms: 1,
            },
            payload,
            public_key_hex: None,
            signature_hex: None,
        }),
        lineage_envelope: None,
    };

    let cached = cacheable_fetch_commit_success_response(&response).expect("cached response");
    let payload = &cached.message.expect("cached message").payload;
    assert!(payload.is_empty());
    assert_eq!(payload.capacity(), 0);
}

#[test]
fn best_effort_provider_publish_does_not_block_on_slow_dht() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let dht = Arc::new(BlockingProviderDht {
        entered: Mutex::new(entered_tx),
        release: Mutex::new(release_rx),
        fail_best_effort: false,
    });
    let handle = NodeReplicationNetworkHandle::new(Arc::new(NoopDistributedNetwork))
        .with_dht(dht)
        .with_local_provider_id("storage-provider");
    let config =
        NodeConfig::new("storage-provider", "world-provider", NodeRole::Storage).expect("config");
    let endpoint =
        ReplicationNetworkEndpoint::new(&handle, "world-provider", false, &config.network_policy)
            .expect("endpoint");

    endpoint.publish_local_content_provider_best_effort("world-provider", "hash-1");

    entered_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("best-effort provider publish should start promptly");
    drop(release_tx);
}

#[test]
fn best_effort_provider_publish_result_exposes_dht_failure() {
    let (entered_tx, _entered_rx) = mpsc::channel();
    let (_release_tx, release_rx) = mpsc::channel();
    let dht = Arc::new(BlockingProviderDht {
        entered: Mutex::new(entered_tx),
        release: Mutex::new(release_rx),
        fail_best_effort: true,
    });
    let handle = NodeReplicationNetworkHandle::new(Arc::new(NoopDistributedNetwork))
        .with_dht(dht)
        .with_local_provider_id("storage-provider");
    let config = NodeConfig::new("storage-provider", "world-provider", NodeRole::Storage).unwrap();

    let error = handle
        .publish_local_content_provider_best_effort_result(
            &config.network_policy,
            "world-provider",
            "hash-1",
        )
        .expect_err("DHT failure must remain observable to queue worker");
    assert!(
        error
            .to_string()
            .contains("controlled provider publication failure")
    );
}

#[test]
fn result_bearing_provider_publish_waits_for_dht_completion() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let dht = Arc::new(BlockingProviderDht {
        entered: Mutex::new(entered_tx),
        release: Mutex::new(release_rx),
        fail_best_effort: false,
    });
    let handle = NodeReplicationNetworkHandle::new(Arc::new(NoopDistributedNetwork))
        .with_dht(dht)
        .with_local_provider_id("storage-provider");
    let network_policy = NodeConfig::new("storage-provider", "world-provider", NodeRole::Storage)
        .expect("config")
        .network_policy;
    let (result_tx, result_rx) = mpsc::channel();

    std::thread::spawn(move || {
        let result = handle.publish_local_content_provider(
            &network_policy,
            "world-provider",
            "hash-actual-completion",
        );
        result_tx.send(result).expect("send publish result");
    });

    entered_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("result-bearing publish should enter DHT");
    assert!(
        matches!(
            result_rx.recv_timeout(Duration::from_millis(20)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "result-bearing publication must not report success before DHT completion"
    );
    release_tx.send(()).expect("release DHT completion");
    result_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("publication result")
        .expect("completed DHT publication");
}
