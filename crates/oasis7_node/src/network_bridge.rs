use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod network_bridge_world_head;

use oasis7_proto::distributed::WorldHeadAnnounce;
use oasis7_proto::distributed_dht as proto_dht;
use oasis7_proto::distributed_net::{
    DistributedNetwork, NetworkLane, NetworkLaneOperation, NetworkSubscription,
    classify_network_protocol,
};
use oasis7_proto::world_error::WorldError;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::gossip_udp::{
    GossipAttestationMessage, GossipCommitMessage, GossipMessage, GossipProposalMessage,
};
use crate::network_bridge_gap_sync_budget::{
    gap_sync_fetch_commit_probe_route_budget, gap_sync_fetch_commit_route_budget,
    gap_sync_fetch_commit_route_budget_exhausted, short_node_error,
};
use crate::network_bridge_gap_sync_observability::{
    CachedFetchCommitSuccess, FetchCommitSuccessCacheKey, GapSyncFetchCommitResponse,
    GapSyncFetchCommitRouteKind, GapSyncFetchCommitRouteObserver, fetch_commit_success_cache_key,
    split_provider_route_timeout_ms, summarize_gap_sync_fetch_commit_routes,
};
pub(crate) use crate::network_error_classification::{
    network_world_error_is_publish_failure, network_world_error_is_retryable_connection_gap,
    replication_network_error_is_availability_gap, replication_network_error_is_not_found,
    replication_network_error_is_protocol_unavailable,
    replication_network_error_is_rate_limited_protocol,
    replication_network_error_is_route_unavailable, replication_network_error_is_timeout_protocol,
    replication_network_error_is_unsupported_protocol, replication_network_error_kind_label,
    replication_network_error_mentions_protocol,
    replication_network_error_should_keep_timeout_over_provider_gap,
};
use crate::replication::{
    FetchCommitRequest, FetchCommitResponse, GossipReplicationMessage,
    REPLICATION_FETCH_COMMIT_PROTOCOL, REPLICATION_GET_HEAD_PROTOCOL, load_blob_from_root,
};
use crate::{
    NodeError, NodeExecutionCheckpointDescriptor, NodeNetworkPolicy,
    NodeReplicationGapSyncRouteSnapshot,
};

pub(crate) const DEFAULT_REPLICATION_TOPIC_PREFIX: &str = "aw";
pub(crate) const DEFAULT_CONSENSUS_PROPOSAL_TOPIC_SUFFIX: &str = "consensus.proposal";
pub(crate) const DEFAULT_CONSENSUS_ATTESTATION_TOPIC_SUFFIX: &str = "consensus.attestation";
pub(crate) const DEFAULT_CONSENSUS_COMMIT_TOPIC_SUFFIX: &str = "consensus.commit";
pub(crate) const DEFAULT_CONSENSUS_LINEAGE_TOPIC_SUFFIX: &str = "consensus.checkpoint_lineage";
const FETCH_COMMIT_SUCCESS_CACHE_AFTER_MS: u64 = 5_000;
const FETCH_COMMIT_SUCCESS_CACHE_MAX_ENTRIES: usize = 64;
pub(crate) const REPLICATION_NETWORK_AVAILABILITY_GAP_PREFIX: &str =
    "replication network availability gap: ";
pub(crate) const REPLICATION_NETWORK_ROUTE_UNAVAILABLE_PREFIX: &str =
    "replication network route unavailable: ";
const FETCH_COMMIT_GENERIC_ROUTE_ATTEMPTS: usize = 4;
const GAP_SYNC_FETCH_COMMIT_MAX_PROVIDER_ROUTES_PER_POLL: usize = 8;
const GAP_SYNC_FETCH_COMMIT_MIN_PROVIDER_ROUTE_TIMEOUT_MS: u64 = 1_500;
const GAP_SYNC_FETCH_HEAD_REQUEST_TIMEOUT_MS: u64 = 1_500;
const GAP_SYNC_FETCH_HEAD_RETRY_BUDGET_MS: u64 = 3_000;
const GAP_SYNC_FETCH_HEAD_MAX_PROVIDER_ROUTES_PER_POLL: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GapSyncFetchCommitRetryPolicy {
    FullGapSync,
    SingleProbe,
}

pub(crate) fn default_replication_topic(world_id: &str) -> String {
    format!("{DEFAULT_REPLICATION_TOPIC_PREFIX}.{world_id}.replication")
}

pub(crate) fn default_consensus_proposal_topic(world_id: &str) -> String {
    format!(
        "{DEFAULT_REPLICATION_TOPIC_PREFIX}.{world_id}.{}",
        DEFAULT_CONSENSUS_PROPOSAL_TOPIC_SUFFIX
    )
}

pub(crate) fn default_consensus_attestation_topic(world_id: &str) -> String {
    format!(
        "{DEFAULT_REPLICATION_TOPIC_PREFIX}.{world_id}.{}",
        DEFAULT_CONSENSUS_ATTESTATION_TOPIC_SUFFIX
    )
}

pub(crate) fn default_consensus_commit_topic(world_id: &str) -> String {
    format!(
        "{DEFAULT_REPLICATION_TOPIC_PREFIX}.{world_id}.{}",
        DEFAULT_CONSENSUS_COMMIT_TOPIC_SUFFIX
    )
}

pub(crate) fn default_consensus_lineage_topic(world_id: &str) -> String {
    format!(
        "{DEFAULT_REPLICATION_TOPIC_PREFIX}.{world_id}.{}",
        DEFAULT_CONSENSUS_LINEAGE_TOPIC_SUFFIX
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrafficLaneRegistry {
    pub replication_topic: String,
    pub consensus_proposal_topic: String,
    pub consensus_attestation_topic: String,
    pub consensus_commit_topic: String,
}

impl TrafficLaneRegistry {
    fn for_handle(handle: &NodeReplicationNetworkHandle, world_id: &str) -> Self {
        Self {
            replication_topic: handle.resolved_topic(world_id),
            consensus_proposal_topic: default_consensus_proposal_topic(world_id),
            consensus_attestation_topic: default_consensus_attestation_topic(world_id),
            consensus_commit_topic: default_consensus_commit_topic(world_id),
        }
    }
}

fn validate_lane_access(
    network_policy: &NodeNetworkPolicy,
    lane: NetworkLane,
    operation: NetworkLaneOperation,
    label: &str,
) -> Result<(), NodeError> {
    if network_policy.allows_lane_operation(lane, operation) {
        return Ok(());
    }
    Err(NodeError::InvalidConfig {
        reason: format!(
            "node_role_claim={} cannot {:?} {} on lane={}",
            network_policy.node_role_claim, operation, label, lane
        ),
    })
}

#[derive(Clone)]
pub struct NodeReplicationNetworkHandle {
    network: Arc<dyn DistributedNetwork<WorldError> + Send + Sync>,
    dht: Option<Arc<dyn proto_dht::DistributedDht<WorldError> + Send + Sync>>,
    local_provider_id: Option<String>,
    topic: Option<String>,
}

impl fmt::Debug for NodeReplicationNetworkHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeReplicationNetworkHandle")
            .field("topic", &self.topic)
            .finish()
    }
}

impl NodeReplicationNetworkHandle {
    pub fn new(network: Arc<dyn DistributedNetwork<WorldError> + Send + Sync>) -> Self {
        Self {
            network,
            dht: None,
            local_provider_id: None,
            topic: None,
        }
    }

    pub fn with_dht(
        mut self,
        dht: Arc<dyn proto_dht::DistributedDht<WorldError> + Send + Sync>,
    ) -> Self {
        self.dht = Some(dht);
        self
    }

    pub fn with_local_provider_id(mut self, provider_id: impl Into<String>) -> Self {
        let provider_id = provider_id.into();
        let provider_id = provider_id.trim();
        if !provider_id.is_empty() {
            self.local_provider_id = Some(provider_id.to_string());
        }
        self
    }

    pub fn with_topic(mut self, topic: impl Into<String>) -> Result<Self, NodeError> {
        let topic = topic.into();
        if topic.trim().is_empty() {
            return Err(NodeError::InvalidConfig {
                reason: "replication network topic cannot be empty".to_string(),
            });
        }
        self.topic = Some(topic);
        Ok(self)
    }

    pub fn clone_network(&self) -> Arc<dyn DistributedNetwork<WorldError> + Send + Sync> {
        Arc::clone(&self.network)
    }

    pub(crate) fn publish_local_content_provider(
        &self,
        network_policy: &NodeNetworkPolicy,
        world_id: &str,
        content_hash: &str,
    ) -> Result<(), NodeError> {
        let Some(dht) = self.dht.as_ref() else {
            return Ok(());
        };
        let Some(local_provider_id) = self.local_provider_id.as_deref() else {
            return Ok(());
        };
        if !network_policy
            .allows_lane_operation(NetworkLane::BlobState, NetworkLaneOperation::Serve)
        {
            return Ok(());
        }
        dht.publish_provider(world_id, content_hash, local_provider_id)
            .map_err(network_err)
    }

    #[allow(dead_code)]
    pub(crate) fn publish_checkpoint_descriptor_providers_from_root_best_effort(
        &self,
        network_policy: &NodeNetworkPolicy,
        root_dir: &Path,
        world_id: &str,
        descriptor: Option<&NodeExecutionCheckpointDescriptor>,
    ) -> Result<(), NodeError> {
        let Some(descriptor) = descriptor else {
            return Ok(());
        };
        self.publish_checkpoint_blob_provider_from_root_best_effort(
            network_policy,
            root_dir,
            world_id,
            descriptor.manifest_ref.as_str(),
            descriptor.manifest_size_bytes,
        )?;
        for blob_ref in &descriptor.blobs {
            self.publish_checkpoint_blob_provider_from_root_best_effort(
                network_policy,
                root_dir,
                world_id,
                blob_ref.content_hash.as_str(),
                blob_ref.size_bytes,
            )?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn publish_checkpoint_descriptor_providers_from_root(
        &self,
        network_policy: &NodeNetworkPolicy,
        root_dir: &Path,
        world_id: &str,
        descriptor: Option<&NodeExecutionCheckpointDescriptor>,
    ) -> Result<(), NodeError> {
        let Some(descriptor) = descriptor else {
            return Ok(());
        };
        let validated_hashes =
            self.validate_checkpoint_descriptor_from_root(root_dir, descriptor)?;
        self.publish_validated_checkpoint_descriptor_providers(
            network_policy,
            world_id,
            validated_hashes.as_slice(),
        )
    }

    pub(crate) fn publish_commit_and_checkpoint_descriptor_providers_from_root(
        &self,
        network_policy: &NodeNetworkPolicy,
        root_dir: &Path,
        world_id: &str,
        commit_hash: &str,
        descriptor: Option<&NodeExecutionCheckpointDescriptor>,
    ) -> Result<(), NodeError> {
        // Validate the complete checkpoint closure before publishing the
        // commit payload. A descriptor member can be present at its expected
        // CAS path while containing same-size corrupt bytes; publishing the
        // commit first would advertise a partial/invalid closure.
        let validated_hashes = descriptor
            .map(|descriptor| self.validate_checkpoint_descriptor_from_root(root_dir, descriptor))
            .transpose()?;
        self.publish_local_content_provider(network_policy, world_id, commit_hash)?;
        if let Some(validated_hashes) = validated_hashes {
            self.publish_validated_checkpoint_descriptor_providers(
                network_policy,
                world_id,
                validated_hashes.as_slice(),
            )?;
        }
        Ok(())
    }

    fn publish_validated_checkpoint_descriptor_providers(
        &self,
        network_policy: &NodeNetworkPolicy,
        world_id: &str,
        validated_hashes: &[String],
    ) -> Result<(), NodeError> {
        for content_hash in validated_hashes {
            self.publish_local_content_provider(network_policy, world_id, content_hash)?;
        }
        Ok(())
    }

    fn validate_checkpoint_blob_from_root(
        &self,
        root_dir: &Path,
        content_hash: &str,
        expected_size_bytes: u64,
    ) -> Result<(), NodeError> {
        let Some(bytes) = load_blob_from_root(root_dir, content_hash)? else {
            return Err(NodeError::Replication {
                reason: format!(
                    "checkpoint provider publication closure incomplete hash={content_hash}"
                ),
            });
        };
        if bytes.len() as u64 != expected_size_bytes {
            return Err(NodeError::Replication {
                reason: format!(
                    concat!(
                        "checkpoint provider publish local blob size mismatch ",
                        "hash={} expected={} actual={}"
                    ),
                    content_hash,
                    expected_size_bytes,
                    bytes.len()
                ),
            });
        }
        let actual_hash = oasis7_distfs::blake3_hex(bytes.as_slice());
        if actual_hash != content_hash {
            return Err(NodeError::Replication {
                reason: format!(
                    "checkpoint provider publish local blob hash mismatch expected={} actual={}",
                    content_hash, actual_hash
                ),
            });
        }
        Ok(())
    }

    fn validate_checkpoint_descriptor_from_root(
        &self,
        root_dir: &Path,
        descriptor: &NodeExecutionCheckpointDescriptor,
    ) -> Result<Vec<String>, NodeError> {
        let mut validated_hashes = Vec::with_capacity(1 + descriptor.blobs.len());
        self.validate_checkpoint_blob_from_root(
            root_dir,
            descriptor.manifest_ref.as_str(),
            descriptor.manifest_size_bytes,
        )?;
        validated_hashes.push(descriptor.manifest_ref.clone());
        for blob_ref in &descriptor.blobs {
            self.validate_checkpoint_blob_from_root(
                root_dir,
                blob_ref.content_hash.as_str(),
                blob_ref.size_bytes,
            )?;
            validated_hashes.push(blob_ref.content_hash.clone());
        }
        Ok(validated_hashes)
    }

    #[allow(dead_code)]
    fn publish_checkpoint_blob_provider_from_root_best_effort(
        &self,
        network_policy: &NodeNetworkPolicy,
        root_dir: &Path,
        world_id: &str,
        content_hash: &str,
        expected_size_bytes: u64,
    ) -> Result<(), NodeError> {
        let Some(bytes) = load_blob_from_root(root_dir, content_hash)? else {
            return Ok(());
        };
        if bytes.len() as u64 != expected_size_bytes {
            return Err(NodeError::Replication {
                reason: format!(
                    "checkpoint provider publish local blob size mismatch hash={} expected={} actual={}",
                    content_hash,
                    expected_size_bytes,
                    bytes.len()
                ),
            });
        }
        self.publish_local_content_provider_best_effort_result(
            network_policy,
            world_id,
            content_hash,
        )
    }

    pub(crate) fn publish_local_content_provider_best_effort_result(
        &self,
        network_policy: &NodeNetworkPolicy,
        world_id: &str,
        content_hash: &str,
    ) -> Result<(), NodeError> {
        let Some(dht) = self.dht.as_ref() else {
            return Ok(());
        };
        let Some(local_provider_id) = self.local_provider_id.as_deref() else {
            return Ok(());
        };
        if !network_policy
            .allows_lane_operation(NetworkLane::BlobState, NetworkLaneOperation::Serve)
        {
            return Ok(());
        }
        dht.publish_provider_best_effort(world_id, content_hash, local_provider_id)
            .map_err(network_err)
    }

    fn resolved_topic(&self, world_id: &str) -> String {
        self.topic
            .clone()
            .unwrap_or_else(|| default_replication_topic(world_id))
    }

    pub(crate) fn uses_default_topic(&self, world_id: &str) -> bool {
        self.resolved_topic(world_id) == default_replication_topic(world_id)
    }

    fn resolved_lane_registry(&self, world_id: &str) -> TrafficLaneRegistry {
        TrafficLaneRegistry::for_handle(self, world_id)
    }
}

pub(crate) struct ReplicationNetworkEndpoint {
    network: Arc<dyn DistributedNetwork<WorldError> + Send + Sync>,
    dht: Option<Arc<dyn proto_dht::DistributedDht<WorldError> + Send + Sync>>,
    local_provider_id: Option<String>,
    network_policy: NodeNetworkPolicy,
    topic: String,
    subscription: Option<NetworkSubscription>,
    fetch_commit_success_cache_after: Duration,
    recent_fetch_commit_successes:
        Mutex<HashMap<FetchCommitSuccessCacheKey, CachedFetchCommitSuccess>>,
    last_gap_sync_fetch_commit_failure_route_snapshot:
        Mutex<Option<NodeReplicationGapSyncRouteSnapshot>>,
}

impl ReplicationNetworkEndpoint {
    pub(crate) fn new(
        handle: &NodeReplicationNetworkHandle,
        world_id: &str,
        subscribe: bool,
        network_policy: &NodeNetworkPolicy,
    ) -> Result<Self, NodeError> {
        let registry = handle.resolved_lane_registry(world_id);
        let topic = registry.replication_topic;
        let subscription = if subscribe {
            validate_lane_access(
                network_policy,
                NetworkLane::Sync,
                NetworkLaneOperation::Subscribe,
                topic.as_str(),
            )?;
            Some(
                handle
                    .network
                    .subscribe(topic.as_str())
                    .map_err(network_err)?,
            )
        } else {
            None
        };
        Ok(Self {
            network: Arc::clone(&handle.network),
            dht: handle.dht.clone(),
            local_provider_id: handle.local_provider_id.clone(),
            network_policy: network_policy.clone(),
            topic,
            subscription,
            fetch_commit_success_cache_after: Duration::from_millis(
                FETCH_COMMIT_SUCCESS_CACHE_AFTER_MS,
            ),
            recent_fetch_commit_successes: Mutex::new(HashMap::new()),
            last_gap_sync_fetch_commit_failure_route_snapshot: Mutex::new(None),
        })
    }

    #[cfg(test)]
    pub(crate) fn set_fetch_commit_success_cache_after_for_testing(&mut self, duration: Duration) {
        self.fetch_commit_success_cache_after = duration;
    }

    pub(crate) fn publish_replication(
        &self,
        message: &GossipReplicationMessage,
    ) -> Result<(), NodeError> {
        validate_lane_access(
            &self.network_policy,
            NetworkLane::Sync,
            NetworkLaneOperation::Publish,
            self.topic.as_str(),
        )?;
        let payload = serde_json::to_vec(message).map_err(|err| NodeError::Replication {
            reason: format!("serialize replication network message failed: {}", err),
        })?;
        self.network
            .publish_best_effort(self.topic.as_str(), payload.as_slice())
            .map_err(network_publish_err)
    }

    pub(crate) fn drain_replications(&self) -> Result<Vec<GossipReplicationMessage>, NodeError> {
        let Some(subscription) = &self.subscription else {
            return Ok(Vec::new());
        };

        let mut messages = Vec::new();
        for payload in subscription.drain() {
            if let Ok(message) = serde_json::from_slice::<GossipReplicationMessage>(&payload) {
                messages.push(message);
            }
        }
        Ok(messages)
    }

    pub(crate) fn lookup_world_head(
        &self,
        world_id: &str,
    ) -> Result<Option<WorldHeadAnnounce>, NodeError> {
        let mut best_head: Option<WorldHeadAnnounce> = None;
        if let Some(dht) = self.dht.as_ref() {
            match dht.get_world_head(world_id).map_err(network_err) {
                Ok(Some(head)) => {
                    validate_world_head_world_id(world_id, &head)?;
                    if best_head
                        .as_ref()
                        .map(|current| head.height > current.height)
                        .unwrap_or(true)
                    {
                        best_head = Some(head);
                    }
                }
                Ok(None) => {}
                Err(err) if world_head_lookup_can_fallback(&err) => {}
                Err(err) => return Err(err),
            }
        }
        self.request_world_head_from_peers(world_id, best_head)
    }

    pub(crate) fn request_json<Req, Resp>(
        &self,
        protocol: &str,
        request: &Req,
    ) -> Result<Resp, NodeError>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        if let Some(lane) = classify_network_protocol(protocol) {
            validate_lane_access(
                &self.network_policy,
                lane,
                NetworkLaneOperation::Request,
                protocol,
            )?;
        }
        let payload = serde_json::to_vec(request).map_err(|err| NodeError::Replication {
            reason: format!("serialize replication request {} failed: {}", protocol, err),
        })?;
        let response_bytes = self
            .network
            .request(protocol, payload.as_slice())
            .map_err(|err| network_err_for_protocol(protocol, err))?;
        serde_json::from_slice::<Resp>(&response_bytes).map_err(|err| {
            NodeError::ReplicationNetwork {
                reason: format!("decode replication response {} failed: {}", protocol, err),
                failure: Box::new(oasis7_proto::network_failure::NetworkFailure::observed(
                    oasis7_proto::network_failure::FailureOrigin::DecodeValidation,
                    oasis7_proto::network_failure::FailureCause::Validation,
                    Some(protocol),
                )),
            }
        })
    }

    pub(crate) fn request_fetch_commit_for_gap_sync(
        &self,
        request: &FetchCommitRequest,
    ) -> Result<GapSyncFetchCommitResponse, NodeError> {
        self.request_fetch_commit_for_gap_sync_with_policy(
            request,
            GapSyncFetchCommitRetryPolicy::FullGapSync,
            false,
        )
    }

    pub(crate) fn request_fetch_commit_for_gap_sync_single_probe(
        &self,
        request: &FetchCommitRequest,
    ) -> Result<GapSyncFetchCommitResponse, NodeError> {
        self.request_fetch_commit_for_gap_sync_with_policy(
            request,
            GapSyncFetchCommitRetryPolicy::SingleProbe,
            false,
        )
    }

    pub(crate) fn request_fetch_commit_for_high_checkpoint_probe(
        &self,
        request: &FetchCommitRequest,
    ) -> Result<GapSyncFetchCommitResponse, NodeError> {
        self.request_fetch_commit_for_gap_sync_with_policy(
            request,
            GapSyncFetchCommitRetryPolicy::SingleProbe,
            true,
        )
    }

    pub(crate) fn take_last_gap_sync_fetch_commit_failure_route_snapshot(
        &self,
    ) -> Option<NodeReplicationGapSyncRouteSnapshot> {
        self.last_gap_sync_fetch_commit_failure_route_snapshot
            .lock()
            .ok()
            .and_then(|mut snapshot| snapshot.take())
    }

    fn record_gap_sync_fetch_commit_failure_route_snapshot(
        &self,
        snapshot: NodeReplicationGapSyncRouteSnapshot,
    ) {
        if let Ok(mut last_snapshot) = self
            .last_gap_sync_fetch_commit_failure_route_snapshot
            .lock()
        {
            *last_snapshot = Some(snapshot);
        }
    }

    #[cfg(test)]
    pub(crate) fn seed_gap_sync_fetch_commit_failure_route_snapshot_for_test(
        &self,
        snapshot: NodeReplicationGapSyncRouteSnapshot,
    ) {
        self.record_gap_sync_fetch_commit_failure_route_snapshot(snapshot);
    }

    fn request_fetch_commit_for_gap_sync_with_policy(
        &self,
        request: &FetchCommitRequest,
        retry_policy: GapSyncFetchCommitRetryPolicy,
        require_lineage_sidecar: bool,
    ) -> Result<GapSyncFetchCommitResponse, NodeError> {
        // A failure snapshot describes exactly one request sweep.  Discard an
        // unconsumed prior snapshot before any early validation return can
        // otherwise cause it to be reported for this attempt.
        let _ = self.take_last_gap_sync_fetch_commit_failure_route_snapshot();
        if let Some(lane) = classify_network_protocol(REPLICATION_FETCH_COMMIT_PROTOCOL) {
            validate_lane_access(
                &self.network_policy,
                lane,
                NetworkLaneOperation::Request,
                REPLICATION_FETCH_COMMIT_PROTOCOL,
            )?;
        }
        if let Some(response) = self.cached_fetch_commit_success_response(request)
            && (!require_lineage_sidecar || response.lineage_envelope.is_some())
        {
            return Ok(GapSyncFetchCommitResponse {
                response,
                repair_summary: "cache=hit".to_string(),
                route_snapshot: NodeReplicationGapSyncRouteSnapshot::default(),
            });
        }
        let mut last_err = None;
        let mut route_events = Vec::new();
        let route_sweep_started_at = Instant::now();
        let mut route_observer = GapSyncFetchCommitRouteObserver::new(route_sweep_started_at);
        let route_budget = || match retry_policy {
            GapSyncFetchCommitRetryPolicy::FullGapSync => {
                gap_sync_fetch_commit_route_budget(route_sweep_started_at)
            }
            GapSyncFetchCommitRetryPolicy::SingleProbe => {
                gap_sync_fetch_commit_probe_route_budget(route_sweep_started_at)
            }
        };
        let Some((request_timeout_ms, retry_budget_ms)) = route_budget() else {
            let err = gap_sync_fetch_commit_route_budget_exhausted();
            route_observer.observe_budget_exhausted(&err);
            self.record_gap_sync_fetch_commit_failure_route_snapshot(route_observer.finish());
            return Err(err);
        };
        let mut response = match self
            .request_json_budget::<FetchCommitRequest, FetchCommitResponse>(
                REPLICATION_FETCH_COMMIT_PROTOCOL,
                request,
                request_timeout_ms,
                retry_budget_ms,
            ) {
            Ok(response) => {
                route_observer.observe_found(GapSyncFetchCommitRouteKind::Generic, response.found);
                route_events.push(format!("generic:found={}", response.found));
                response
            }
            Err(err) => {
                route_observer.observe_error(GapSyncFetchCommitRouteKind::Generic, &err);
                route_events.push(format!("generic:error={}", short_node_error(&err)));
                last_err = Some(err);
                FetchCommitResponse {
                    found: false,
                    message: None,
                    lineage_envelope: None,
                }
            }
        };
        if !response.found
            || (require_lineage_sidecar
                && (response.message.is_none() || response.lineage_envelope.is_none()))
        {
            // A fresh observer can have enough stale discovery candidates to
            // exhaust this bounded sweep before reaching the validators that
            // supplied its signed head.  Prefer live connected peers, then
            // configured bootstrap peers, before using remaining discovery
            // candidates.  The request remains provider-directed and bounded;
            // this only determines which existing provenance-backed routes
            // receive the budget.
            let mut peer_ids = self.network.connected_peer_ids();
            peer_ids.sort();
            peer_ids.dedup();
            let mut static_bootstrap_peer_ids = self.network.configured_static_bootstrap_peer_ids();
            static_bootstrap_peer_ids.sort();
            static_bootstrap_peer_ids.dedup();
            static_bootstrap_peer_ids.retain(|peer_id| !peer_ids.contains(peer_id));
            peer_ids.extend(static_bootstrap_peer_ids);
            let mut known_peer_ids = self.network.known_peer_ids();
            known_peer_ids.sort();
            known_peer_ids.dedup();
            known_peer_ids.retain(|peer_id| !peer_ids.contains(peer_id));
            peer_ids.extend(known_peer_ids);
            peer_ids.retain(|peer_id| !peer_id.trim().is_empty());
            peer_ids.truncate(GAP_SYNC_FETCH_COMMIT_MAX_PROVIDER_ROUTES_PER_POLL);
            let peer_count = peer_ids.len();
            for (peer_index, peer_id) in peer_ids.into_iter().enumerate() {
                let provider_route = [peer_id.clone()];
                let Some((request_timeout_ms, retry_budget_ms)) = route_budget() else {
                    let err = gap_sync_fetch_commit_route_budget_exhausted();
                    route_observer.observe_budget_exhausted(&err);
                    if last_err.is_none() {
                        last_err = Some(err);
                    }
                    break;
                };
                let remaining_provider_routes = peer_count.saturating_sub(peer_index).max(1);
                let retry_budget_ms = split_provider_route_timeout_ms(
                    retry_budget_ms,
                    remaining_provider_routes,
                    GAP_SYNC_FETCH_COMMIT_MIN_PROVIDER_ROUTE_TIMEOUT_MS,
                );
                let request_timeout_ms = request_timeout_ms.min(retry_budget_ms);
                match self
                    .request_json_with_providers_budget::<FetchCommitRequest, FetchCommitResponse>(
                        REPLICATION_FETCH_COMMIT_PROTOCOL,
                        request,
                        provider_route.as_slice(),
                        request_timeout_ms,
                        retry_budget_ms,
                    ) {
                    Ok(candidate) => {
                        route_observer
                            .observe_found(GapSyncFetchCommitRouteKind::Provider, candidate.found);
                        route_events.push(format!("peer:{}:found={}", peer_id, candidate.found));
                        let candidate_satisfies_request = candidate.found
                            && (!require_lineage_sidecar
                                || (candidate.message.is_some()
                                    && candidate.lineage_envelope.is_some()));
                        if candidate_satisfies_request {
                            response = candidate;
                            last_err = None;
                            break;
                        }
                        response = candidate;
                        last_err = None;
                    }
                    Err(err) => {
                        route_observer.observe_error(GapSyncFetchCommitRouteKind::Provider, &err);
                        route_events.push(format!(
                            "peer:{}:error={}",
                            peer_id,
                            short_node_error(&err)
                        ));
                        if !replication_network_error_should_keep_timeout_over_provider_gap(
                            last_err.as_ref(),
                            &err,
                            REPLICATION_FETCH_COMMIT_PROTOCOL,
                        ) {
                            last_err = Some(err);
                        }
                    }
                }
            }
        }
        if retry_policy == GapSyncFetchCommitRetryPolicy::FullGapSync {
            for _ in 1..FETCH_COMMIT_GENERIC_ROUTE_ATTEMPTS {
                if response.found
                    && (!require_lineage_sidecar
                        || (response.message.is_some() && response.lineage_envelope.is_some()))
                {
                    break;
                }
                let Some((request_timeout_ms, retry_budget_ms)) = route_budget() else {
                    let err = gap_sync_fetch_commit_route_budget_exhausted();
                    route_observer.observe_budget_exhausted(&err);
                    if last_err.is_none() {
                        last_err = Some(err);
                    }
                    break;
                };
                match self.request_json_budget::<FetchCommitRequest, FetchCommitResponse>(
                    REPLICATION_FETCH_COMMIT_PROTOCOL,
                    request,
                    request_timeout_ms,
                    retry_budget_ms,
                ) {
                    Ok(candidate) => {
                        route_observer.observe_found(
                            GapSyncFetchCommitRouteKind::GenericRetry,
                            candidate.found,
                        );
                        route_events.push(format!("generic_retry:found={}", candidate.found));
                        response = candidate;
                        last_err = None;
                    }
                    Err(err) => {
                        route_observer
                            .observe_error(GapSyncFetchCommitRouteKind::GenericRetry, &err);
                        route_events
                            .push(format!("generic_retry:error={}", short_node_error(&err)));
                        last_err = Some(err);
                    }
                }
            }
        }
        if response.found || last_err.is_none() {
            let route_snapshot = route_observer.finish();
            return Ok(GapSyncFetchCommitResponse {
                response,
                repair_summary: summarize_gap_sync_fetch_commit_routes(
                    &route_events,
                    &route_snapshot,
                ),
                route_snapshot,
            });
        }
        let route_snapshot = route_observer.finish();
        self.record_gap_sync_fetch_commit_failure_route_snapshot(route_snapshot);
        Err(last_err.expect("gap-sync fetch-commit last_err should exist"))
    }

    pub(crate) fn request_json_budget<Req, Resp>(
        &self,
        protocol: &str,
        request: &Req,
        request_timeout_ms: u64,
        retry_budget_ms: u64,
    ) -> Result<Resp, NodeError>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        if let Some(lane) = classify_network_protocol(protocol) {
            validate_lane_access(
                &self.network_policy,
                lane,
                NetworkLaneOperation::Request,
                protocol,
            )?;
        }
        let payload = serde_json::to_vec(request).map_err(|err| NodeError::Replication {
            reason: format!("serialize replication request {} failed: {}", protocol, err),
        })?;
        let response_bytes = self
            .network
            .request_with_providers_budget(
                protocol,
                payload.as_slice(),
                &[],
                request_timeout_ms,
                retry_budget_ms,
            )
            .map_err(|err| network_err_for_protocol(protocol, err))?;
        serde_json::from_slice::<Resp>(&response_bytes).map_err(|err| {
            NodeError::ReplicationNetwork {
                reason: format!("decode replication response {} failed: {}", protocol, err),
                failure: Box::new(oasis7_proto::network_failure::NetworkFailure::observed(
                    oasis7_proto::network_failure::FailureOrigin::DecodeValidation,
                    oasis7_proto::network_failure::FailureCause::Validation,
                    Some(protocol),
                )),
            }
        })
    }

    pub(crate) fn remember_validated_fetch_commit_success(
        &self,
        request: &FetchCommitRequest,
        response: &FetchCommitResponse,
    ) {
        let Some(response) = cacheable_fetch_commit_success_response(response) else {
            return;
        };
        let now = Instant::now();
        let mut cache = self
            .recent_fetch_commit_successes
            .lock()
            .expect("lock fetch-commit success cache");
        cache.retain(|_, entry| entry.valid_until > now);
        if cache.len() >= FETCH_COMMIT_SUCCESS_CACHE_MAX_ENTRIES
            && let Some(oldest_key) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.cached_at)
                .map(|(key, _)| key.clone())
        {
            cache.remove(&oldest_key);
        }
        cache.insert(
            fetch_commit_success_cache_key(request),
            CachedFetchCommitSuccess {
                response,
                cached_at: now,
                valid_until: now + self.fetch_commit_success_cache_after,
            },
        );
    }

    pub(crate) fn request_json_with_providers<Req, Resp>(
        &self,
        protocol: &str,
        request: &Req,
        providers: &[String],
    ) -> Result<Resp, NodeError>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        if let Some(lane) = classify_network_protocol(protocol) {
            validate_lane_access(
                &self.network_policy,
                lane,
                NetworkLaneOperation::Request,
                protocol,
            )?;
        }
        let payload = serde_json::to_vec(request).map_err(|err| NodeError::Replication {
            reason: format!("serialize replication request {} failed: {}", protocol, err),
        })?;
        let response_bytes = self
            .network
            .request_with_providers(protocol, payload.as_slice(), providers)
            .map_err(|err| network_err_for_protocol(protocol, err))?;
        serde_json::from_slice::<Resp>(&response_bytes).map_err(|err| {
            NodeError::ReplicationNetwork {
                reason: format!("decode replication response {} failed: {}", protocol, err),
                failure: Box::new(oasis7_proto::network_failure::NetworkFailure::observed(
                    oasis7_proto::network_failure::FailureOrigin::DecodeValidation,
                    oasis7_proto::network_failure::FailureCause::Validation,
                    Some(protocol),
                )),
            }
        })
    }

    pub(crate) fn request_json_with_providers_budget<Req, Resp>(
        &self,
        protocol: &str,
        request: &Req,
        providers: &[String],
        request_timeout_ms: u64,
        retry_budget_ms: u64,
    ) -> Result<Resp, NodeError>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        if let Some(lane) = classify_network_protocol(protocol) {
            validate_lane_access(
                &self.network_policy,
                lane,
                NetworkLaneOperation::Request,
                protocol,
            )?;
        }
        let payload = serde_json::to_vec(request).map_err(|err| NodeError::Replication {
            reason: format!("serialize replication request {} failed: {}", protocol, err),
        })?;
        let response_bytes = self
            .network
            .request_with_providers_budget(
                protocol,
                payload.as_slice(),
                providers,
                request_timeout_ms,
                retry_budget_ms,
            )
            .map_err(|err| network_err_for_protocol(protocol, err))?;
        serde_json::from_slice::<Resp>(&response_bytes).map_err(|err| {
            NodeError::ReplicationNetwork {
                reason: format!("decode replication response {} failed: {}", protocol, err),
                failure: Box::new(oasis7_proto::network_failure::NetworkFailure::observed(
                    oasis7_proto::network_failure::FailureOrigin::DecodeValidation,
                    oasis7_proto::network_failure::FailureCause::Validation,
                    Some(protocol),
                )),
            }
        })
    }

    pub(crate) fn connected_peer_ids(&self) -> Vec<String> {
        self.network.connected_peer_ids()
    }

    pub(crate) fn lookup_provider_ids_for_content_hash(
        &self,
        world_id: &str,
        content_hash: &str,
    ) -> Result<Option<Vec<String>>, NodeError> {
        let Some(dht) = self.dht.as_ref() else {
            return Ok(None);
        };
        let mut providers = dht
            .get_providers(world_id, content_hash)
            .map_err(network_err)?;
        providers.sort_by(|left, right| {
            right
                .last_seen_ms
                .cmp(&left.last_seen_ms)
                .then_with(|| left.provider_id.cmp(&right.provider_id))
        });
        let mut provider_ids = Vec::with_capacity(providers.len());
        for provider in providers {
            let provider_id = provider.provider_id.trim();
            if provider_id.is_empty() {
                continue;
            }
            if self.local_provider_id.as_deref() == Some(provider_id) {
                continue;
            }
            if provider_ids.iter().any(|existing| existing == provider_id) {
                continue;
            }
            provider_ids.push(provider_id.to_string());
        }
        Ok(Some(provider_ids))
    }

    #[allow(dead_code)]
    pub(crate) fn publish_local_content_provider(
        &self,
        world_id: &str,
        content_hash: &str,
    ) -> Result<(), NodeError> {
        let handle = NodeReplicationNetworkHandle {
            network: Arc::clone(&self.network),
            dht: self.dht.clone(),
            local_provider_id: self.local_provider_id.clone(),
            topic: Some(self.topic.clone()),
        };
        handle.publish_local_content_provider(&self.network_policy, world_id, content_hash)
    }

    pub(crate) fn publish_local_content_provider_best_effort(
        &self,
        world_id: &str,
        content_hash: &str,
    ) {
        let Some(dht) = self.dht.as_ref() else {
            return;
        };
        let Some(local_provider_id) = self.local_provider_id.as_deref() else {
            return;
        };
        if !self
            .network_policy
            .allows_lane_operation(NetworkLane::BlobState, NetworkLaneOperation::Serve)
        {
            return;
        }
        let _ = dht.publish_provider_best_effort(world_id, content_hash, local_provider_id);
    }

    fn cached_fetch_commit_success_response(
        &self,
        request: &FetchCommitRequest,
    ) -> Option<FetchCommitResponse> {
        let now = Instant::now();
        let mut cache = self
            .recent_fetch_commit_successes
            .lock()
            .expect("lock fetch-commit success cache");
        cache.retain(|_, entry| entry.valid_until > now);
        cache
            .get(&fetch_commit_success_cache_key(request))
            .map(|entry| entry.response.clone())
    }
}

fn cacheable_fetch_commit_success_response(
    response: &FetchCommitResponse,
) -> Option<FetchCommitResponse> {
    if !response.found {
        return None;
    }
    let mut cached = response.clone();
    let message = cached.message.as_mut()?;
    message.payload = Vec::new();
    Some(cached)
}

#[cfg(test)]
#[path = "tests_network_bridge.rs"]
mod tests;

#[path = "network_bridge_consensus.rs"]
mod network_bridge_consensus;
pub(crate) use network_bridge_consensus::ConsensusNetworkEndpoint;

#[path = "network_bridge_error.rs"]
mod error_mapping;
use error_mapping::{
    network_err, network_err_for_protocol, network_publish_err, validate_world_head_world_id,
    world_head_lookup_can_fallback,
};
#[cfg(test)]
#[path = "network_bridge_typed_failure_tests.rs"]
mod typed_failure_contract_tests;
