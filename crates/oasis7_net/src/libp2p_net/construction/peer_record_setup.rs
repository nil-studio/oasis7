use crate::error::WorldError;

use super::super::durable_peer_record_publisher::DurablePeerRecordPublisher;
use super::super::receiver_floor::PeerRecordReceiver;
use super::super::{Libp2pNetwork, Libp2pNetworkConfig};

impl Libp2pNetwork {
    /// Initialize V2 local peer-record publication once, without starting the network.
    pub fn initialize_peer_record_publication(
        config: &Libp2pNetworkConfig,
    ) -> Result<(), WorldError> {
        let directory = config
            .peer_record_publication_directory
            .as_deref()
            .ok_or_else(|| WorldError::NetworkProtocolUnavailable {
                protocol: "peer publication initialization requires a configured directory".into(),
            })?;
        let keypair =
            config
                .keypair
                .as_ref()
                .ok_or_else(|| WorldError::NetworkProtocolUnavailable {
                    protocol:
                        "peer publication initialization requires a stable configured keypair"
                            .into(),
                })?;
        let template =
            config
                .peer_record
                .as_ref()
                .ok_or_else(|| WorldError::NetworkProtocolUnavailable {
                    protocol:
                        "peer publication initialization requires a configured peer-record template"
                            .into(),
                })?;
        DurablePeerRecordPublisher::initialize(directory, keypair, template)
    }

    /// Initialize the durable inbound V2 replay floor without starting networking.
    pub fn initialize_peer_record_receiver(config: &Libp2pNetworkConfig) -> Result<(), WorldError> {
        let keypair =
            config
                .keypair
                .as_ref()
                .ok_or_else(|| WorldError::NetworkProtocolUnavailable {
                    protocol: "receiver initialization requires a stable configured keypair".into(),
                })?;
        let template =
            config
                .peer_record
                .as_ref()
                .ok_or_else(|| WorldError::NetworkProtocolUnavailable {
                    protocol: "receiver initialization requires a configured peer-record template"
                        .into(),
                })?;
        PeerRecordReceiver::initialize(&config.peer_record_receiver, keypair, template)
    }
}
