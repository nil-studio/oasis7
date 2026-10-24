//! Network-focused facade for distributed runtime capabilities.

mod client;
mod dht;
mod dht_cache;
mod gateway;
mod head_sync;
mod head_tracking;
mod index;
mod index_store;
mod network;
mod provider_cache;
mod provider_distribution;
mod provider_selection;
mod provider_trim;
mod replay_flow;
mod replica_maintenance;
mod util;

#[cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
mod libp2p_net;
#[cfg(feature = "libp2p")]
mod peer_manager_policy;

pub mod distributed_net {
    pub use super::network::*;
}

pub mod distributed {
    pub use oasis7_proto::distributed::*;
}

pub mod distributed_dht {
    pub use super::dht::*;
}

pub mod distributed_client {
    pub use super::client::*;
}

pub mod distributed_index_store {
    pub use super::index_store::*;
}

pub mod distributed_provider_cache {
    pub use super::provider_cache::*;
}

pub mod distributed_provider_distribution {
    pub use super::provider_distribution::*;
}

pub mod distributed_replica_maintenance {
    pub use super::replica_maintenance::*;
}

pub mod observer_flow {
    pub use super::head_sync::{
        HeadFollowReport, HeadSyncReport, HeadSyncResult, compose_head_sync_report,
        follow_head_sync,
    };
}

pub mod observer_replay_flow {
    pub use super::replay_flow::load_manifest_and_segments;
}

pub mod distributed_storage {
    pub use oasis7_proto::distributed_storage::{ExecutionWriteConfig, ExecutionWriteResult};
}

pub mod error {
    pub use oasis7_proto::world_error::WorldError;
}

pub mod modules {
    pub use oasis7_wasm_abi::{ModuleArtifact, ModuleManifest};
}

pub use client::DistributedClient;
pub use dht::{DistributedDht, InMemoryDht};
pub use dht_cache::{CachedDht, DhtCacheConfig};
pub use distributed_storage::{ExecutionWriteConfig, ExecutionWriteResult};
pub use error::WorldError;
pub use gateway::{ActionGateway, NetworkGateway, SubmitActionReceipt};
pub use head_tracking::{HeadTracker, HeadUpdateDecision};
pub use index::{
    IndexPublishResult, publish_execution_providers, publish_execution_providers_cached,
    publish_world_head, query_providers,
};
pub use index_store::{DistributedIndexStore, HeadIndexRecord, InMemoryIndexStore};
pub use modules::{ModuleArtifact, ModuleManifest};
pub use network::{DistributedNetwork, InMemoryNetwork};
pub use oasis7_proto::distributed_dht as proto_dht;
pub use oasis7_proto::distributed_net as proto_net;
pub use proto_dht::{MembershipDirectorySnapshot, ProviderRecord};
pub use proto_net::{NetworkMessage, NetworkRequest, NetworkResponse, NetworkSubscription};
pub use provider_cache::{ProviderCache, ProviderCacheConfig};
pub use provider_distribution::{
    ProviderDistributionAudit, ProviderDistributionPolicy, audit_provider_distribution,
};
pub use provider_selection::ProviderSelectionPolicy;
pub use replica_maintenance::{
    ReplicaMaintenanceFailedTask, ReplicaMaintenancePlan, ReplicaMaintenancePolicy,
    ReplicaMaintenancePollingPolicy, ReplicaMaintenancePollingState, ReplicaMaintenanceReport,
    ReplicaMaintenanceRoundResult, ReplicaTransferExecutor, ReplicaTransferKind,
    ReplicaTransferTask, execute_replica_maintenance_plan, plan_replica_maintenance,
    run_replica_maintenance_poll,
};

#[cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
pub use libp2p_net::error_mapping::{
    Libp2pAvailabilityClass, classify_world_error_availability, world_error_is_missing_handler,
    world_error_is_publish_failure, world_error_is_retryable_connection_gap,
};

#[cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
pub use libp2p_net::{
    DiscoveryWorkMetricsSnapshot, Libp2pControlPlaneMetricsSnapshot, Libp2pNetwork,
    Libp2pNetworkConfig, Libp2pReachabilitySnapshot, Libp2pTrafficMetricsSnapshot,
    LiveAutoNatStatus, LiveHolePunchState, LivePublicPortReachability, LiveTransportKind,
    LiveTransportTransition, LiveTransportTransitionCounters, PeerManagerBlockArtifact,
    PeerManagerHealthIssue, PeerManagerHealthStatus, PeerManagerPeerHealth, PeerManagerPolicy,
    PeerRecordReceiverConfig, PeerRecordReceiverMode, TrafficDirectionMetricsSnapshot,
    TrafficLaneMetricsSnapshot, WireByteDirectionMetricsSnapshot, WireByteLaneMetricsSnapshot,
};

#[cfg(test)]
mod tests;

#[cfg(all(feature = "libp2p", target_arch = "wasm32"))]
pub use peer_manager_policy::PeerManagerPolicy;
