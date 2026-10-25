#[cfg(not(target_arch = "wasm32"))]
pub use crate::libp2p_replication_network::{
    Libp2pReplicationNetwork, Libp2pReplicationNetworkConfig, PeerRecordPublicationTransition,
    PeerRecordReceiverConfig, PeerRecordReceiverMode, ReplicationNetworkDebugSnapshot,
    derive_libp2p_identity_keypair,
};

#[cfg(target_arch = "wasm32")]
pub use crate::libp2p_replication_network_wasm::{
    Libp2pReplicationNetwork, Libp2pReplicationNetworkConfig, derive_libp2p_identity_keypair,
};
