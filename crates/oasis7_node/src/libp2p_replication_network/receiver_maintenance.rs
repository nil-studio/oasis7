//! Explicit receiver ledger maintenance; these operations never start networking.
use oasis7_net::{Libp2pNetwork, Libp2pNetworkConfig};
use oasis7_proto::world_error::WorldError;

use super::{Libp2pReplicationNetwork, Libp2pReplicationNetworkConfig};

impl Libp2pReplicationNetwork {
    /// Creates the durable receiver floor without starting a network.
    /// Ordinary construction only opens the existing receiver state.
    pub fn initialize_peer_record_receiver(
        config: &Libp2pReplicationNetworkConfig,
    ) -> Result<(), WorldError> {
        Libp2pNetwork::initialize_peer_record_receiver(&Libp2pNetworkConfig {
            keypair: config.keypair.clone(),
            peer_record: config.peer_record.clone(),
            peer_record_receiver: config.peer_record_receiver.clone(),
            ..Libp2pNetworkConfig::default()
        })
    }

    /// Explicitly upgrade/recover the existing receiver ledger without networking.
    pub fn upgrade_peer_record_receiver(
        config: &Libp2pReplicationNetworkConfig,
    ) -> Result<(), WorldError> {
        Libp2pNetwork::upgrade_peer_record_receiver(&Libp2pNetworkConfig {
            keypair: config.keypair.clone(),
            peer_record: config.peer_record.clone(),
            peer_record_receiver: config.peer_record_receiver.clone(),
            ..Libp2pNetworkConfig::default()
        })
    }
}
