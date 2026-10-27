use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub use oasis7_consensus::node_consensus_action::NodeConsensusAction;
use oasis7_consensus::node_consensus_action::{
    compute_consensus_action_root as core_compute_consensus_action_root,
    drain_ordered_consensus_actions as core_drain_ordered_consensus_actions,
    validate_consensus_action_root as core_validate_consensus_action_root,
};
use oasis7_consensus::node_consensus_error::NodeConsensusError;
use oasis7_consensus::node_consensus_signature::{
    NodeConsensusMessageSigner, sign_attestation_message as core_sign_attestation_message,
    sign_commit_message as core_sign_commit_message,
    sign_proposal_message as core_sign_proposal_message,
    verify_attestation_message_signature as core_verify_attestation_message_signature,
    verify_commit_message_signature as core_verify_commit_message_signature,
    verify_proposal_message_signature as core_verify_proposal_message_signature,
};
use oasis7_consensus::node_pos::{
    NodePosDecision, NodePosError, NodePosPendingProposal, NodePosStatusAdapter,
    advance_pending_attestations as core_advance_pending_attestations,
    insert_attestation as core_insert_attestation, propose_next_head as core_propose_next_head,
};
use oasis7_distfs::{
    FeedbackAnnounce, FeedbackAnnounceBridge, FeedbackStore, LocalCasStore, blake3_hex,
};
#[cfg(not(target_arch = "wasm32"))]
use oasis7_net::{
    ReplicaMaintenancePolicy, ReplicaMaintenancePollingPolicy, ReplicaMaintenancePollingState,
    ReplicaTransferExecutor, ReplicaTransferTask, run_replica_maintenance_poll,
};
use oasis7_proto::distributed::DistributedErrorCode;
use oasis7_proto::distributed_dht as proto_dht;
use oasis7_proto::world_error::WorldError as ProtoWorldError;
use serde::Deserialize;

mod consensus_progress_observer;
mod consensus_support;
mod error;
mod execution_hook;
mod feedback_runtime;
mod gossip_udp;
#[cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
mod libp2p_replication_network;
#[cfg(all(feature = "libp2p", target_arch = "wasm32"))]
mod libp2p_replication_network_wasm;
mod network_bridge;
mod network_bridge_gap_sync_budget;
mod network_bridge_gap_sync_observability;
mod network_error_classification;
mod node_bft_exports;
#[cfg(test)]
mod node_bft_integration_tests;
mod node_bft_runtime;
mod node_bft_v2_runtime;
#[cfg(all(test, unix))]
mod node_bft_v2_test_provider;
mod node_bft_v2_transport;
mod node_bft_v3_runtime;
mod node_bft_wal;
mod node_engine_core;
mod node_engine_execution;
mod node_engine_gap_sync_outcome;
mod node_engine_guarded_local;
mod node_engine_local_bootstrap;
mod node_engine_network;
mod node_engine_network_hash;
mod node_engine_network_observations;
mod node_engine_peer_types;
mod node_engine_proposal_reservation;
mod node_engine_replicated_input;
mod node_engine_replication;
mod node_engine_replication_checkpoint;
mod node_engine_replication_checkpoint_fetch;
mod node_engine_replication_lineage;
mod node_engine_replication_local_state_block;
mod node_engine_replication_pending_checkpoint_receipt;
mod node_engine_replication_provider_route;
mod node_engine_slashing;
mod node_engine_storage_challenge;
mod node_engine_transfer_filter;
mod node_runtime_batch_retention;
mod node_runtime_committed_batches;
mod node_runtime_core;
mod node_runtime_lifecycle;
mod node_runtime_local_bootstrap;
mod node_runtime_replicated_input;
mod node_runtime_state;
mod pos_engine_gossip;
mod pos_schedule;
mod pos_state_store;
mod pos_validation;
mod provider_publication_queue;
mod replica_maintenance_support;
mod replication;
mod replication_checkpoint_lineage;
mod replication_fetch_handler_support;
mod replication_probe_gate;
mod replication_state_reconcile;
mod runtime_util;
mod types;
mod types_action_budgets;
mod types_consensus;
pub use consensus_support::compute_consensus_action_root;
use consensus_support::{
    checked_consensus_successor, checked_replication_successor, dequeue_pending_consensus_actions,
    drain_ordered_consensus_actions, merge_pending_consensus_actions_with_budget,
    node_consensus_error, node_pos_error, release_action_payload_bytes,
    reserve_action_payload_bytes, sign_attestation_message, sign_commit_message,
    sign_proposal_message, validate_consensus_action_root, verify_attestation_message_signature,
    verify_commit_message_signature, verify_proposal_message_signature,
};
pub use error::NodeError;
use gossip_udp::{
    GossipAttestationMessage, GossipCommitMessage, GossipEndpoint, GossipMessage,
    GossipProposalMessage,
};
pub use gossip_udp::{
    GossipTrafficDirectionMetricsSnapshot, GossipTrafficLaneMetricsSnapshot,
    GossipTrafficMetricsSnapshot,
};
pub use node_bft_exports::*;
#[cfg(feature = "libp2p")]
mod libp2p_network_exports;
#[cfg(feature = "libp2p")]
pub use libp2p_network_exports::*;
pub use network_bridge::NodeReplicationNetworkHandle;
use node_runtime_batch_retention::{action_payload_bytes, push_committed_action_batch};
#[cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
pub use oasis7_net::{
    Libp2pControlPlaneMetricsSnapshot, Libp2pReachabilitySnapshot, Libp2pTrafficMetricsSnapshot,
    LiveAutoNatStatus, LiveHolePunchState, LivePublicPortReachability, LiveTransportKind,
    LiveTransportTransition, LiveTransportTransitionCounters,
};
pub use replication::NodeReplicationConfig;
pub use types::{
    NodeAutoNatStatus, NodeCommittedActionBatch, NodeConfig, NodeConsensusMode,
    NodeConsensusSnapshot, NodeFeedbackP2pConfig, NodeFinalityLatencySnapshot, NodeGossipConfig,
    NodeHolePunchViability, NodeMainTokenControllerBindingConfig,
    NodeMainTokenControllerSignerPolicy, NodeNetworkPolicy, NodePeerCommittedHead,
    NodePendingConsensusActionsSnapshot, NodePendingProposalSnapshot, NodePosConfig,
    NodePublicPortReachability, NodeReachabilityAutoDetection, NodeReplicaMaintenanceConfig,
    NodeReplicationGapSyncRouteSnapshot, NodeRole, NodeSnapshot, NodeUserMode,
    NodeUserModeRecommendation, PosConsensusStatus, PosValidator,
};
pub use types_consensus::{
    NodeConsensusMisbehaviorEvidenceSnapshot, NodeConsensusSlashingIntentSnapshot,
    NodeConsensusSlashingReceiptSnapshot, NodeValidatorStakeProofSnapshot,
    NodeValidatorStakeProofStepSnapshot,
};

use consensus_progress_observer::{
    ConsensusProgressObserverDispatcher, publish_runtime_progress_snapshot,
};
pub use consensus_progress_observer::{
    NodeConsensusProgressObserver, NodeConsensusProgressObserverError, ObserverLifecycleAuthority,
};
use feedback_runtime::{
    maybe_ingest_runtime_feedback_announces, maybe_publish_runtime_feedback_announces,
};
use network_bridge::{ConsensusNetworkEndpoint, ReplicationNetworkEndpoint};
use node_engine_peer_types::{ConsensusMisbehaviorEvidence, PeerCommittedHead};
use node_runtime_core::RuntimeState;
use node_runtime_replicated_input::bind_pending_replicated_execution_inputs as bind_replicated_inputs;
use pos_state_store::PosNodeStateStore;
use pos_validation::{normalize_consensus_public_key_hex, validated_pos_state};
use provider_publication_queue::{ProviderPublicationEnqueueResult, ProviderPublicationQueue};
use replica_maintenance_support::maybe_run_runtime_replica_maintenance_poll;
use replication::{
    FetchBlobRequest, FetchBlobResponse, FetchCommitRequest, FetchCommitResponse, FetchHeadRequest,
    FetchHeadResponse, REPLICATION_FETCH_BLOB_PROTOCOL, REPLICATION_FETCH_COMMIT_PROTOCOL,
    REPLICATION_GET_HEAD_PROTOCOL, ReplicationHeadSummary, ReplicationRuntime,
    load_commit_message_from_root, load_latest_commit_message_from_root,
};
#[cfg(test)]
use replication_fetch_handler_support::should_export_checkpoint_for_fetch_commit;
use replication_fetch_handler_support::{
    admit_fetch_commit_request, attach_checkpoint_for_fetch_commit_if_boundary,
    register_fetch_blob_handler,
};
#[cfg(test)]
use replication_probe_gate::request_fetch_blob_with_route_fallback_resuming;
use replication_probe_gate::{
    FetchBlobChunkProgress, replication_request_waitable_connection_gap,
    request_fetch_blob_with_route_fallback,
    request_fetch_blob_with_route_fallback_resuming_with_provenance,
    request_fetch_blob_with_storage_challenge_routes,
};
use replication_state_reconcile::{
    NodeEngineTickResult, ReplicationCommitPayloadView, parse_replication_commit_payload,
    parse_replication_commit_payload_view, reconcile_engine_with_persisted_replication,
};
use runtime_util::{lock_state, now_unix_ms};

const STORAGE_GATE_NETWORK_SAMPLES_PER_CHECK: usize = 3;
const STORAGE_GATE_NETWORK_MIN_MATCHES_CAP: usize = 2;
const STORAGE_GATE_NETWORK_WARMUP_HEIGHT: u64 = 32;
const STORAGE_GATE_FALLBACK_SAMPLES_PER_CHECK: usize = 3;
const STORAGE_CHALLENGE_NETWORK_RETRY_COOLDOWN_MS: i64 = 30_000;
const STORAGE_CHALLENGE_SUCCESS_CACHE_MAX_AGE_HEIGHTS: u64 = 2;
const REPLICATION_GAP_SYNC_MAX_RETRIES_PER_HEIGHT: usize = 3;
const REPLICATION_GAP_SYNC_MAX_HEIGHTS_PER_POLL: u64 = 64;
const REPLICATION_FETCH_BLOB_GENERIC_ROUTE_ATTEMPTS: usize = 3;
const REPLICATION_SUCCESSOR_PROBE_COOLDOWN_MS: i64 = 1_000;
const EXECUTION_BINDING_HISTORY_LIMIT: usize = 256;
const FINALITY_LATENCY_HISTORY_LIMIT: usize = 128;
pub const EXECUTION_MISSING_PREDECESSOR_RECORD_SIGNATURE: &str =
    "execution driver missing predecessor record for non-contiguous committed height";

fn required_network_blob_matches(sample_count: usize) -> usize {
    sample_count.clamp(1, STORAGE_GATE_NETWORK_MIN_MATCHES_CAP)
}

impl NodePosStatusAdapter for PosConsensusStatus {
    fn pending() -> Self {
        PosConsensusStatus::Pending
    }

    fn committed() -> Self {
        PosConsensusStatus::Committed
    }

    fn rejected() -> Self {
        PosConsensusStatus::Rejected
    }
}

fn with_execution_hook<T>(
    execution_hook: &mut Option<&mut dyn NodeExecutionHook>,
    f: impl FnOnce(Option<&mut dyn NodeExecutionHook>) -> T,
) -> T {
    match execution_hook {
        Some(hook) => f(Some(&mut **hook)),
        None => f(None),
    }
}

pub struct NodeRuntime {
    config: NodeConfig,
    bft_config: Option<node_bft_runtime::SelectedNodeBftConfigV1>,
    bft_v2_config: Option<node_bft_v2_runtime::SelectedNodeBftConfigV2>,
    bft_v3_config: Option<node_bft_v3_runtime::SelectedNodeBftConfigV3>,
    local_execution_bootstrap: Option<execution_hook::NodeExecutionBootstrap>,
    local_execution_genesis_bootstrap: Option<execution_hook::NodeExecutionGenesisBootstrap>,
    replication_network: Option<NodeReplicationNetworkHandle>,
    replication_network_consensus_enabled: bool,
    gossip_endpoint: Option<Arc<GossipEndpoint>>,
    feedback_store: Option<Arc<FeedbackStore>>,
    pending_feedback_announces: Arc<Mutex<Vec<FeedbackAnnounce>>>,
    execution_hook: Option<std::sync::Arc<std::sync::Mutex<Box<dyn NodeExecutionHook>>>>,
    consensus_progress_observer: Option<Box<dyn NodeConsensusProgressObserver>>,
    consensus_progress_observer_dispatcher: Option<ConsensusProgressObserverDispatcher>,
    replica_maintenance_dht:
        Option<Arc<dyn proto_dht::DistributedDht<ProtoWorldError> + Send + Sync>>,
    pending_consensus_actions: Arc<Mutex<Vec<NodeConsensusAction>>>,
    pending_consensus_action_queue_bytes: Arc<AtomicUsize>,
    committed_action_batches: Arc<(Mutex<Vec<NodeCommittedActionBatch>>, Condvar)>,
    running: Arc<AtomicBool>,
    state: Arc<Mutex<RuntimeState>>,
    stop_tx: Option<mpsc::Sender<()>>,
    worker: Option<JoinHandle<()>>,
    #[cfg(test)]
    fail_next_worker_spawn: bool,
    #[cfg(test)]
    fail_next_observer_worker_spawn: bool,
}

#[derive(Clone)]
pub struct NodeCommittedActionBatchesHandle {
    state: Arc<(Mutex<Vec<NodeCommittedActionBatch>>, Condvar)>,
}

impl NodeRuntime {
    fn start_pos(&mut self) -> Result<(), NodeError> {
        let pending_consensus_actions = self
            .pending_consensus_actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.running.swap(true, Ordering::SeqCst) {
            drop(pending_consensus_actions);
            return Err(NodeError::AlreadyRunning {
                node_id: self.config.node_id.clone(),
            });
        }
        let pending_bytes = action_payload_bytes(pending_consensus_actions.iter());
        self.pending_consensus_action_queue_bytes
            .store(pending_bytes, Ordering::Release);
        drop(pending_consensus_actions);

        {
            let mut state = lock_state(&self.state);
            let lifecycle_generation = state.generation;
            *state = RuntimeState::default();
            state.generation = lifecycle_generation;
        }
        {
            let (committed_lock, committed_signal) = &*self.committed_action_batches;
            let mut committed = committed_lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            committed.clear();
            committed_signal.notify_all();
        }

        let mut engine = match PosNodeEngine::new_with_pending_consensus_action_queue_bytes(
            &self.config,
            Arc::clone(&self.pending_consensus_action_queue_bytes),
        ) {
            Ok(engine) => engine,
            Err(err) => {
                self.running.store(false, Ordering::SeqCst);
                return Err(err);
            }
        };
        self.initialize_local_execution_bootstrap(&mut engine)?;
        let effective_replication_config = self
            .config
            .replication
            .as_ref()
            .map(|config| {
                let validator_signer_public_keys = self
                    .config
                    .pos_config
                    .validator_signer_public_keys
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                config
                    .clone()
                    .with_default_remote_writer_allowlist(
                        validator_signer_public_keys.iter().cloned(),
                    )?
                    .with_default_fetch_requester_allowlist(validator_signer_public_keys)
            })
            .transpose()?;
        let pos_state_store = effective_replication_config
            .as_ref()
            .map(PosNodeStateStore::from_replication);
        if let Some(store) = pos_state_store.as_ref() {
            match store.load() {
                Ok(Some(snapshot)) => {
                    if let Err(err) = engine.restore_state_snapshot(snapshot, Some(now_unix_ms())) {
                        self.running.store(false, Ordering::SeqCst);
                        return Err(err);
                    }
                    self.validate_local_execution_bootstrap_after_restore(&engine)?;
                }
                Ok(None) => {}
                Err(err) => {
                    self.running.store(false, Ordering::SeqCst);
                    return Err(err);
                }
            }
        }
        let gossip = if let Some(config) = &self.config.gossip {
            match GossipEndpoint::bind(config) {
                Ok(endpoint) => Some(Arc::new(endpoint)),
                Err(err) => {
                    self.running.store(false, Ordering::SeqCst);
                    return Err(err);
                }
            }
        } else {
            None
        };
        self.gossip_endpoint = gossip.clone();
        let mut replication = if let Some(config) = effective_replication_config.as_ref() {
            match ReplicationRuntime::new(config, &self.config.node_id) {
                Ok(runtime) => Some(runtime),
                Err(err) => {
                    self.running.store(false, Ordering::SeqCst);
                    return Err(err);
                }
            }
        } else {
            None
        };
        if let Some(replication_runtime) = replication.as_ref() {
            let reconcile_result = if let Some(execution_hook) = self.execution_hook.as_ref() {
                match execution_hook.lock() {
                    Ok(mut hook) => reconcile_engine_with_persisted_replication(
                        &mut engine,
                        replication_runtime,
                        self.config.world_id.as_str(),
                        Some(hook.as_mut()),
                    ),
                    Err(_) => Err(NodeError::Execution {
                        reason: "execution hook lock poisoned".to_string(),
                    }),
                }
            } else {
                reconcile_engine_with_persisted_replication(
                    &mut engine,
                    replication_runtime,
                    self.config.world_id.as_str(),
                    None,
                )
            };
            if let Err(err) = reconcile_result {
                self.running.store(false, Ordering::SeqCst);
                return Err(err);
            }
            if let Some(store) = pos_state_store.as_ref()
                && let Err(err) = store.save_engine_state(&engine)
            {
                self.running.store(false, Ordering::SeqCst);
                return Err(err);
            }
        }
        let reserved_bytes = engine.pending_action_reservation_bytes();
        if let Err(err) = bind_replicated_inputs(self, engine.next_height, reserved_bytes) {
            self.running.store(false, Ordering::SeqCst);
            return Err(err);
        }
        {
            let mut current = lock_state(&self.state);
            current.generation = current.generation.saturating_add(1);
            current.consensus_progress_observer_error = None;
            match engine.restored_consensus_snapshot() {
                Ok(snapshot) => current.consensus = snapshot,
                Err(err) => {
                    self.running.store(false, Ordering::SeqCst);
                    return Err(err);
                }
            }
        }
        if let (Some(network), Some(replication_config)) = (
            &self.replication_network,
            effective_replication_config.as_ref(),
        ) && let Err(err) = register_replication_fetch_handlers_with_checkpoint_export(
            network,
            replication_config,
            self.config.node_id.as_str(),
            self.config.world_id.as_str(),
            &self.config.network_policy,
            self.execution_hook.clone(),
        ) {
            self.running.store(false, Ordering::SeqCst);
            return Err(err);
        }
        let mut replication_network = if let Some(network) = &self.replication_network {
            let subscribe = self.config.replication.is_some();
            match ReplicationNetworkEndpoint::new(
                network,
                &self.config.world_id,
                subscribe,
                &self.config.network_policy,
            ) {
                Ok(endpoint) => Some(endpoint),
                Err(err) => {
                    self.running.store(false, Ordering::SeqCst);
                    return Err(err);
                }
            }
        } else {
            None
        };
        let mut consensus_network = if let Some(network) = &self.replication_network {
            let subscribe = self.replication_network_consensus_enabled
                || (self.config.role == NodeRole::Observer
                    && network.uses_default_topic(self.config.world_id.as_str()));
            if subscribe {
                match ConsensusNetworkEndpoint::new(
                    network,
                    &self.config.world_id,
                    true,
                    &self.config.network_policy,
                ) {
                    Ok(endpoint) => Some(endpoint),
                    Err(err) => {
                        self.running.store(false, Ordering::SeqCst);
                        return Err(err);
                    }
                }
            } else {
                None
            }
        } else {
            None
        };
        let feedback_p2p = self.config.feedback_p2p.clone();
        let feedback_store = if let Some(feedback_config) = feedback_p2p.as_ref() {
            let Some(replication_config) = effective_replication_config.as_ref() else {
                self.running.store(false, Ordering::SeqCst);
                return Err(NodeError::InvalidConfig {
                    reason: "feedback_p2p requires replication config".to_string(),
                });
            };
            Some(FeedbackStore::new(
                LocalCasStore::new(replication_config.root_dir.join("store")),
                feedback_config.store.clone(),
            ))
        } else {
            None
        };
        let feedback_store = feedback_store.map(Arc::new);
        self.feedback_store = feedback_store.clone();
        let feedback_bridge = if feedback_p2p.is_some() {
            let Some(network) = &self.replication_network else {
                self.running.store(false, Ordering::SeqCst);
                return Err(NodeError::InvalidConfig {
                    reason: "feedback_p2p requires replication network".to_string(),
                });
            };
            if !self.config.network_policy.allows_lane_operation(
                oasis7_proto::distributed_net::NetworkLane::BlobState,
                oasis7_proto::distributed_net::NetworkLaneOperation::Publish,
            ) || !self.config.network_policy.allows_lane_operation(
                oasis7_proto::distributed_net::NetworkLane::BlobState,
                oasis7_proto::distributed_net::NetworkLaneOperation::Subscribe,
            ) {
                self.running.store(false, Ordering::SeqCst);
                return Err(NodeError::InvalidConfig {
                    reason: format!(
                        "feedback_p2p requires blob/state lane publish+subscribe access for node_role_claim={}",
                        self.config.network_policy.node_role_claim
                    ),
                });
            }
            match FeedbackAnnounceBridge::new(
                self.config.world_id.as_str(),
                network.clone_network(),
            ) {
                Ok(bridge) => Some(bridge),
                Err(err) => {
                    self.running.store(false, Ordering::SeqCst);
                    return Err(NodeError::Replication {
                        reason: format!(
                            "feedback announce bridge initialization failed: {:?}",
                            err
                        ),
                    });
                }
            }
        } else {
            None
        };
        let tick_interval = self.config.tick_interval;
        let worker_name = format!("aw-node-{}", self.config.node_id);
        let running = Arc::clone(&self.running);
        let state = Arc::clone(&self.state);
        let execution_hook = self.execution_hook.clone();
        self.start_consensus_progress_observer_dispatcher()?;
        let consensus_progress_observer = self
            .consensus_progress_observer_dispatcher
            .as_ref()
            .map(ConsensusProgressObserverDispatcher::submitter);
        let replica_maintenance = self.config.replica_maintenance;
        let replica_maintenance_dht = self.replica_maintenance_dht.clone();
        let pending_consensus_actions = Arc::clone(&self.pending_consensus_actions);
        let pending_feedback_announces = Arc::clone(&self.pending_feedback_announces);
        let committed_action_batches = Arc::clone(&self.committed_action_batches);
        let node_id = self.config.node_id.clone();
        let world_id = self.config.world_id.clone();
        let max_committed_action_batches = self.config.max_committed_action_batches.max(1);
        let max_committed_action_batch_bytes = self.config.max_committed_action_batch_bytes;
        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        let worker = self
            .runtime_worker_spawner(thread::Builder::new().name(worker_name))
            .spawn(move || {
                loop {
                    let wait_duration =
                        engine.next_tick_wait_duration(now_unix_ms(), tick_interval);
                    match stop_rx.recv_timeout(wait_duration) {
                        Ok(()) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            let now_ms = now_unix_ms();
                            let last_polled_at_ms = {
                                let mut current = lock_state(&state);
                                current.tick_count = current.tick_count.saturating_add(1);
                                current.last_tick_unix_ms = Some(now_ms);
                                current.replica_maintenance_last_polled_at_ms
                            };

                            let queued_actions = {
                                let mut pending = pending_consensus_actions
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                                dequeue_pending_consensus_actions(
                                    &mut pending,
                                    engine.pending_consensus_action_capacity(),
                                )
                            };
                            let feedback_publish_result = maybe_publish_runtime_feedback_announces(
                                feedback_p2p.as_ref(),
                                pending_feedback_announces.as_ref(),
                                feedback_bridge.as_ref(),
                            );
                            let feedback_ingest_result = maybe_ingest_runtime_feedback_announces(
                                feedback_p2p.as_ref(),
                                feedback_store.as_deref(),
                                feedback_bridge.as_ref(),
                                replication.as_ref(),
                                replication_network.as_ref(),
                            );

                            let tick_result = if let Some(execution_hook) = execution_hook.as_ref()
                            {
                                match execution_hook.lock() {
                                    Ok(mut hook) => {
                                        let progress_state = Arc::clone(&state);
                                        let mut publish_progress =
                                            |snapshot: NodeConsensusSnapshot| {
                                                publish_runtime_progress_snapshot(
                                                    &progress_state,
                                                    consensus_progress_observer.as_ref(),
                                                    snapshot,
                                                    now_ms,
                                                )
                                            };
                                        engine.tick_with_progress(
                                            &node_id,
                                            &world_id,
                                            now_ms,
                                            gossip.as_deref(),
                                            replication.as_mut(),
                                            replication_network.as_mut(),
                                            consensus_network.as_mut(),
                                            queued_actions,
                                            true,
                                            Some(hook.as_mut()),
                                            Some(&mut publish_progress),
                                        )
                                    }
                                    Err(_) => Err(NodeError::Execution {
                                        reason: "execution hook lock poisoned".to_string(),
                                    }),
                                }
                            } else {
                                let progress_state = Arc::clone(&state);
                                let mut publish_progress = |snapshot: NodeConsensusSnapshot| {
                                    publish_runtime_progress_snapshot(
                                        &progress_state,
                                        consensus_progress_observer.as_ref(),
                                        snapshot,
                                        now_ms,
                                    )
                                };
                                engine.tick_with_progress(
                                    &node_id,
                                    &world_id,
                                    now_ms,
                                    gossip.as_deref(),
                                    replication.as_mut(),
                                    replication_network.as_mut(),
                                    consensus_network.as_mut(),
                                    queued_actions,
                                    true,
                                    None,
                                    Some(&mut publish_progress),
                                )
                            };
                            let maintenance_result = if tick_result.is_ok() {
                                maybe_run_runtime_replica_maintenance_poll(
                                    replica_maintenance,
                                    node_id.as_str(),
                                    world_id.as_str(),
                                    now_ms,
                                    last_polled_at_ms,
                                    replica_maintenance_dht.as_deref(),
                                    replication_network.as_ref(),
                                    replication.as_ref(),
                                )
                            } else {
                                Ok(last_polled_at_ms)
                            };

                            let mut current = lock_state(&state);
                            match tick_result {
                                Ok(tick) => {
                                    current.consensus = tick.consensus_snapshot;
                                    current.last_error = None;
                                    if let Err(err) = feedback_publish_result.as_ref() {
                                        current.last_error = Some(err.to_string());
                                    }
                                    if let Err(err) = feedback_ingest_result.as_ref()
                                        && current.last_error.is_none()
                                    {
                                        current.last_error = Some(err.to_string());
                                    }
                                    match maintenance_result {
                                        Ok(polled_at_ms) => {
                                            current.replica_maintenance_last_polled_at_ms =
                                                polled_at_ms;
                                        }
                                        Err(err) => {
                                            if current.last_error.is_none() {
                                                current.last_error = Some(err.to_string());
                                            }
                                        }
                                    }
                                    if let Some(batch) = tick.committed_action_batch {
                                        push_committed_action_batch(
                                            &committed_action_batches,
                                            batch,
                                            max_committed_action_batches,
                                            max_committed_action_batch_bytes,
                                        );
                                    }
                                    if let Some(store) = pos_state_store.as_ref()
                                        && let Err(err) = store.save_engine_state(&engine)
                                        && current.last_error.is_none()
                                    {
                                        current.last_error = Some(err.to_string());
                                    }
                                }
                                Err(err) => {
                                    current.last_error = Some(err.to_string());
                                }
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                running.store(false, Ordering::SeqCst);
            })
            .map_err(|err| {
                self.restore_consensus_progress_observer_after_start_failure();
                self.running.store(false, Ordering::SeqCst);
                NodeError::ThreadSpawnFailed {
                    reason: err.to_string(),
                }
            })?;

        self.stop_tx = Some(stop_tx);
        self.worker = Some(worker);
        Ok(())
    }

    pub fn snapshot(&self) -> NodeSnapshot {
        let state = lock_state(&self.state);
        let mut snapshot = NodeSnapshot {
            node_id: self.config.node_id.clone(),
            player_id: self.config.player_id.clone(),
            world_id: self.config.world_id.clone(),
            role: self.config.role,
            replication_enabled: self.config.replication.is_some(),
            running: self.running.load(Ordering::SeqCst),
            tick_count: state.tick_count,
            last_tick_unix_ms: state.last_tick_unix_ms,
            consensus: state.consensus.clone(),
            consensus_progress_observer_error: state.consensus_progress_observer_error.clone(),
            last_error: state.last_error.clone(),
        };
        let pending_submit_buffer = self
            .pending_consensus_actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        snapshot
            .consensus
            .pending_consensus_actions
            .submit_buffer_action_count = pending_submit_buffer.len();
        snapshot
            .consensus
            .pending_consensus_actions
            .submit_buffer_payload_bytes = action_payload_bytes(pending_submit_buffer.iter());
        snapshot
            .consensus
            .pending_consensus_actions
            .submit_buffer_max_capacity = self.config.max_pending_consensus_actions;
        snapshot
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn submit_consensus_progress_for_test(
        &self,
        snapshot: NodeConsensusSnapshot,
        observed_at_ms: i64,
    ) -> Result<(), NodeError> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(NodeError::NotRunning {
                node_id: self.config.node_id.clone(),
            });
        }
        let observer = self
            .consensus_progress_observer_dispatcher
            .as_ref()
            .map(ConsensusProgressObserverDispatcher::submitter);
        publish_runtime_progress_snapshot(&self.state, observer.as_ref(), snapshot, observed_at_ms)
    }

    pub fn gossip_traffic_snapshot(&self) -> Option<GossipTrafficMetricsSnapshot> {
        self.gossip_endpoint
            .as_ref()
            .map(|endpoint| endpoint.traffic_metrics_snapshot())
    }
}

#[cfg(test)]
fn register_replication_fetch_handlers(
    handle: &NodeReplicationNetworkHandle,
    replication: &NodeReplicationConfig,
    world_id: &str,
    network_policy: &NodeNetworkPolicy,
) -> Result<(), NodeError> {
    register_replication_fetch_handlers_with_checkpoint_export(
        handle,
        replication,
        "replication-fetch-handler",
        world_id,
        network_policy,
        None,
    )
}

fn register_replication_fetch_handlers_with_checkpoint_export(
    handle: &NodeReplicationNetworkHandle,
    replication: &NodeReplicationConfig,
    node_id: &str,
    world_id: &str,
    network_policy: &NodeNetworkPolicy,
    execution_hook: Option<Arc<Mutex<Box<dyn NodeExecutionHook>>>>,
) -> Result<(), NodeError> {
    let network = handle.clone_network();
    if network_policy.allows_lane_operation(
        oasis7_proto::distributed_net::NetworkLane::Sync,
        oasis7_proto::distributed_net::NetworkLaneOperation::Serve,
    ) {
        let commit_root_dir = replication.root_dir.clone();
        let commit_node_id = node_id.to_string();
        let commit_world_id = world_id.to_string();
        let commit_replication_config = replication.clone();
        let commit_execution_hook = execution_hook.clone();
        let commit_handle = handle.clone();
        let commit_network_policy = network_policy.clone();
        let commit_admission_world_id = commit_world_id.clone();
        let commit_admission_config = commit_replication_config.clone();
        let provider_publications = ProviderPublicationQueue::new();
        network
            .register_context_handler_with_admission(
                REPLICATION_FETCH_COMMIT_PROTOCOL,
                Box::new(move |payload| {
                    admit_fetch_commit_request(
                        payload,
                        commit_admission_world_id.as_str(),
                        &commit_admission_config,
                    )
                    .map(|_| ())
                }),
                Box::new(move |context, payload| {
                    context.check_cancelled(|| network_timeout_error("fetch-commit cancelled"))?;
                    // Keep the same check at execution time: admission protects scarce
                    // workers, while this closes any adapter or scheduling gap.
                    let request = admit_fetch_commit_request(
                        payload,
                        commit_world_id.as_str(),
                        &commit_replication_config,
                    )?;
                    let message = load_commit_message_from_root(
                        commit_root_dir.as_path(),
                        commit_world_id.as_str(),
                        request.height,
                    )
                    .map_err(network_internal_error)?;
                    context.check_cancelled(|| network_timeout_error("fetch-commit cancelled"))?;
                    let message = attach_checkpoint_for_fetch_commit_if_boundary(
                        message,
                        commit_execution_hook.as_ref(),
                        commit_root_dir.as_path(),
                        commit_world_id.as_str(),
                        commit_node_id.as_str(),
                        &commit_replication_config,
                        request.height,
                    )
                    .map_err(network_internal_error)?;
                    context.check_cancelled(|| network_timeout_error("fetch-commit cancelled"))?;
                    if let Some(message) = message.as_ref() {
                        let payload_content_hash = message.record.content_hash.clone();
                        if let Some(payload) =
                            parse_replication_commit_payload(message.payload.as_slice())
                        {
                            let descriptor = payload.execution_checkpoint;
                            let publish_handle = commit_handle.clone();
                            let publish_network_policy = commit_network_policy.clone();
                            let publish_root_dir = commit_root_dir.clone();
                            let publish_world_id = commit_world_id.clone();
                            let publication_key = format!(
                                "{}:{}:{}",
                                publish_world_id, payload_content_hash, request.height
                            );
                            let enqueue_result =
                                provider_publications.enqueue(publication_key, move || {
                                    publish_handle
                                        .publish_commit_and_checkpoint_descriptor_providers_from_root(
                                            &publish_network_policy,
                                            publish_root_dir.as_path(),
                                            publish_world_id.as_str(),
                                            payload_content_hash.as_str(),
                                            descriptor.as_ref(),
                                        )
                                        .map_err(|err| err.to_string())?;
                                    Ok(())
                                });
                            if matches!(
                                enqueue_result,
                                ProviderPublicationEnqueueResult::Saturated
                                    | ProviderPublicationEnqueueResult::Disconnected
                            ) {
                                let snapshot = provider_publications.snapshot();
                                eprintln!(
                                    concat!(
                                        "provider publication enqueue loss ",
                                        "result={:?} depth={} coalesced={} ",
                                        "dropped={} completed={} failed={} retries={}"
                                    ),
                                    enqueue_result,
                                    snapshot.depth,
                                    snapshot.coalesced,
                                    snapshot.dropped,
                                    snapshot.completed,
                                    snapshot.failed,
                                    snapshot.retries
                                );
                            }
                        }
                    }
                    let lineage_envelope = message
                        .as_ref()
                        .map(|message| {
                            replication_checkpoint_lineage::lineage_envelope_from_payload(
                                message.payload.as_slice(),
                            )
                        })
                        .transpose()
                        .map_err(network_internal_error)?
                        .flatten();
                    let response = FetchCommitResponse {
                        found: message.is_some(),
                        message,
                        lineage_envelope,
                    };
                    serde_json::to_vec(&response).map_err(|err| {
                        network_internal_error(NodeError::Replication {
                            reason: format!("encode fetch-commit response failed: {}", err),
                        })
                    })
                }),
            )
            .map_err(network_replication_error)?;
    }

    if network_policy.allows_lane_operation(
        oasis7_proto::distributed_net::NetworkLane::Sync,
        oasis7_proto::distributed_net::NetworkLaneOperation::Serve,
    ) {
        let head_root_dir = replication.root_dir.clone();
        let head_world_id = world_id.to_string();
        let max_hot_commit_messages = replication.max_hot_commit_messages();
        network
            .register_handler(
                REPLICATION_GET_HEAD_PROTOCOL,
                Box::new(move |payload| {
                    let request =
                        serde_json::from_slice::<FetchHeadRequest>(payload).map_err(|err| {
                            network_bad_request(format!(
                                "decode fetch-head request failed: {}",
                                err
                            ))
                        })?;
                    if request.world_id != head_world_id {
                        return Err(network_bad_request(format!(
                            "fetch-head world mismatch: expected={}, got={}",
                            head_world_id, request.world_id
                        )));
                    }
                    let Some(message) = load_latest_commit_message_from_root(
                        head_root_dir.as_path(),
                        head_world_id.as_str(),
                        max_hot_commit_messages,
                    )
                    .map_err(network_internal_error)?
                    else {
                        let response = FetchHeadResponse {
                            found: false,
                            head: None,
                        };
                        return serde_json::to_vec(&response).map_err(|err| {
                            network_internal_error(NodeError::Replication {
                                reason: format!("encode fetch-head response failed: {}", err),
                            })
                        });
                    };
                    let payload = parse_replication_commit_payload(message.payload.as_slice())
                        .ok_or_else(|| {
                            network_internal_error(NodeError::Replication {
                                reason: format!(
                                    "decode latest replication commit payload failed world={}",
                                    head_world_id
                                ),
                            })
                        })?;
                    let response = FetchHeadResponse {
                        found: true,
                        head: Some(ReplicationHeadSummary {
                            world_id: payload.world_id,
                            height: payload.height,
                            block_hash: payload.block_hash,
                            state_root: payload.execution_state_root.unwrap_or_default(),
                            timestamp_ms: payload.committed_at_ms,
                        }),
                    };
                    serde_json::to_vec(&response).map_err(|err| {
                        network_internal_error(NodeError::Replication {
                            reason: format!("encode fetch-head response failed: {}", err),
                        })
                    })
                }),
            )
            .map_err(network_replication_error)?;
    }

    if network_policy.allows_lane_operation(
        oasis7_proto::distributed_net::NetworkLane::BlobState,
        oasis7_proto::distributed_net::NetworkLaneOperation::Serve,
    ) {
        register_fetch_blob_handler(network, replication).map_err(network_replication_error)?;
    }

    Ok(())
}

#[cfg(test)]
fn slice_fetch_blob_response(
    bytes: Vec<u8>,
    offset_bytes: Option<u64>,
    limit_bytes: Option<u64>,
) -> (Vec<u8>, Option<bool>) {
    let Some(offset_bytes) = offset_bytes else {
        return (bytes, None);
    };
    let offset = usize::try_from(offset_bytes).unwrap_or(usize::MAX);
    if offset >= bytes.len() {
        return (Vec::new(), Some(true));
    }
    let limit = limit_bytes
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| bytes.len().saturating_sub(offset));
    let end = offset.saturating_add(limit).min(bytes.len());
    (bytes[offset..end].to_vec(), Some(end >= bytes.len()))
}

fn network_bad_request(message: impl Into<String>) -> ProtoWorldError {
    ProtoWorldError::NetworkRequestFailed {
        code: DistributedErrorCode::ErrBadRequest,
        message: message.into(),
        retryable: false,
    }
}

fn network_internal_error(err: NodeError) -> ProtoWorldError {
    ProtoWorldError::NetworkRequestFailed {
        code: DistributedErrorCode::ErrNotAvailable,
        message: err.to_string(),
        retryable: true,
    }
}

fn network_timeout_error(message: impl Into<String>) -> ProtoWorldError {
    ProtoWorldError::NetworkRequestFailed {
        code: DistributedErrorCode::ErrTimeout,
        message: message.into(),
        retryable: true,
    }
}

fn network_replication_error(err: ProtoWorldError) -> NodeError {
    NodeError::Replication {
        reason: format!("replication network error: {err:?}"),
    }
}

#[derive(Debug, Clone)]
struct PosNodeEngine {
    validators: BTreeMap<String, u64>,
    validator_players: BTreeMap<String, String>,
    validator_by_player: BTreeMap<String, String>,
    validator_signers: BTreeMap<String, String>,
    total_stake: u64,
    required_stake: u64,
    validator_set_hash: String,
    validator_stake_root: String,
    validator_stake_proofs: Vec<NodeValidatorStakeProofSnapshot>,
    epoch_length_slots: u64,
    slot_duration_ms: u64,
    ticks_per_slot: u64,
    proposal_tick_phase: u64,
    adaptive_tick_scheduler_enabled: bool,
    slot_clock_genesis_unix_ms: Option<i64>,
    max_past_slot_lag: u64,
    last_observed_tick: u64,
    missed_tick_count: u64,
    last_observed_slot: u64,
    missed_slot_count: u64,
    local_validator_id: String,
    node_player_id: String,
    gossip_reverse_path_seeding_enabled: bool,
    checkpoint_bootstrap_enabled: bool,
    fresh_observer_checkpoint_preflight_unavailable: bool,
    fresh_observer_checkpoint_bootstrap_retry_pending: bool,
    fresh_observer_checkpoint_low_head_confirmation: Option<(u64, String, String)>,
    last_gossip_reverse_path_seed_at_ms: Option<i64>,
    allow_local_proposals: bool,
    require_execution_on_commit: bool,
    next_height: u64,
    next_slot: u64,
    committed_height: u64,
    network_committed_height: u64,
    replication_enabled: bool,
    replication_persisted_height: u64,
    checkpoint_blob_fetch_progress: BTreeMap<String, FetchBlobChunkProgress>,
    pending_checkpoint_receipt:
        Option<node_engine_replication_pending_checkpoint_receipt::PendingCheckpointReceipt>,
    last_replication_gap_sync_blocked_height: Option<u64>,
    last_replication_gap_sync_blocked_reason: Option<String>,
    last_replication_gap_sync_blocked_at_ms: Option<i64>,
    last_replication_gap_sync_repair_attempt_height: Option<u64>,
    last_replication_gap_sync_repair_attempt_summary: Option<String>,
    last_replication_gap_sync_repair_attempt_route_snapshot:
        Option<NodeReplicationGapSyncRouteSnapshot>,
    last_replication_successor_probe_height: Option<u64>,
    last_replication_successor_probe_at_ms: Option<i64>,
    last_replication_successor_probe_hold: Option<bool>,
    storage_challenge_network_degraded_height: Option<u64>,
    storage_challenge_network_degraded_reason: Option<String>,
    storage_challenge_network_last_probe_at_ms: Option<i64>,
    storage_challenge_network_next_probe_after_ms: Option<i64>,
    storage_challenge_fallback_height: u64,
    recent_storage_challenge_successes: BTreeMap<String, u64>,
    pending: Option<PendingProposal>,
    auto_attest_all_validators: bool,
    last_broadcast_proposal_height: u64,
    last_broadcast_proposal_at_ms: Option<i64>,
    last_broadcast_local_attestation_height: u64,
    last_broadcast_local_attestation_at_ms: Option<i64>,
    last_broadcast_gossip_committed_height: u64,
    last_broadcast_gossip_committed_at_ms: Option<i64>,
    last_broadcast_network_committed_height: u64,
    last_broadcast_network_committed_at_ms: Option<i64>,
    last_broadcast_gossip_replicated_committed_height: u64,
    last_broadcast_gossip_replicated_committed_at_ms: Option<i64>,
    last_broadcast_network_replicated_committed_height: u64,
    last_broadcast_network_replicated_committed_at_ms: Option<i64>,
    replicate_local_commits: bool,
    require_peer_execution_hashes: bool,
    consensus_signer: Option<NodeConsensusMessageSigner>,
    consensus_signer_public_key: Option<String>,
    enforce_consensus_signature: bool,
    peer_heads: BTreeMap<String, PeerCommittedHead>,
    latest_validated_peer_commit: Option<GossipCommitMessage>,
    lineage_state: replication_checkpoint_lineage::CheckpointLineageState,
    misbehavior_evidence: BTreeMap<String, ConsensusMisbehaviorEvidence>,
    quarantined_validators: BTreeSet<String>,
    last_committed_at_ms: Option<i64>,
    last_committed_block_hash: Option<String>,
    inbound_rejected_proposal_future_slot: u64,
    inbound_rejected_proposal_stale_slot: u64,
    inbound_rejected_attestation_future_slot: u64,
    inbound_rejected_attestation_stale_slot: u64,
    inbound_rejected_attestation_epoch_mismatch: u64,
    last_inbound_timing_reject_reason: Option<String>,
    last_execution_height: u64,
    last_execution_block_hash: Option<String>,
    last_execution_state_root: Option<String>,
    recent_finality_latency_ms: VecDeque<i64>,
    execution_bindings: BTreeMap<u64, (String, String)>,
    pending_consensus_actions: BTreeMap<u64, NodeConsensusAction>,
    max_pending_consensus_actions: usize,
    max_consensus_action_payload_bytes: usize,
    pending_consensus_action_queue_bytes: Arc<AtomicUsize>,
    max_pending_consensus_action_queue_bytes: usize,
    pending_consensus_action_reservation_bytes: usize,
    pending_consensus_action_reservation_owned: bool,
}

type PendingProposal = NodePosPendingProposal<NodeConsensusAction, PosConsensusStatus>;
type PosDecision = NodePosDecision<NodeConsensusAction, PosConsensusStatus>;

#[cfg(test)]
include!("tests_modules.rs");

#[cfg(all(test, unix))]
mod node_bft_protected_test_provider;

mod network_error_legacy;
