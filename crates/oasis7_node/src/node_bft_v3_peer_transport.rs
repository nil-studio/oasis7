//! Public protocol transport only; receipt of data grants no validator authority.
use oasis7_consensus::tendermint_v3::{BftPeerContextV3, BftPublicPeerMessageV3};
pub struct NodeBftReceivedPublicV3 {
    pub peer_id: String,
    pub message: BftPublicPeerMessageV3,
}
pub trait NodeBftPeerTransportV3: Send + Sync {
    fn context(&self) -> &BftPeerContextV3;
    fn broadcast_public(&self, message: &BftPublicPeerMessageV3) -> Result<(), String>;
    fn send_public(&self, peer: &str, message: &BftPublicPeerMessageV3) -> Result<(), String>;
    fn drain_public(&self, max: usize) -> Result<Vec<NodeBftReceivedPublicV3>, String>;
    fn peers(&self) -> Vec<String>;
}
#[cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
#[path = "node_bft_v3_peer_transport/libp2p.rs"]
mod native;
#[cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
pub use native::NodeBftLibp2pPeerTransportV3;
