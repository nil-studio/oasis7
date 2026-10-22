//! Version-independent messages and the adapter contract.
use std::fmt::Debug;

pub const RPC_PROTOCOL: &str = "/oasis7/compat/1";
pub const TOPIC: &str = "oasis7-compatibility";
pub const IDENTITY_MESSAGE: &[u8] = b"oasis7-identity-compatibility";

pub enum Event<P, C> {
    Listening(String),
    Connected(P),
    Request(Vec<u8>, C),
    Response(Vec<u8>),
    Subscribed(P),
    Gossip(Vec<u8>),
    OutboundFailure(String),
}

#[allow(async_fn_in_trait)]
pub trait Adapter: Sized {
    type PeerId: Copy + Eq + Debug;
    type ResponseChannel;
    type PublishError: Debug;
    fn identity();
    fn addresses(addresses: &[String]);
    fn new(server: bool, address: &str) -> Self;
    async fn next(&mut self) -> Event<Self::PeerId, Self::ResponseChannel>;
    fn request(&mut self, peer: &Self::PeerId, bytes: Vec<u8>);
    fn respond(&mut self, channel: Self::ResponseChannel, bytes: Vec<u8>);
    fn publish(&mut self, bytes: Vec<u8>) -> Result<(), Self::PublishError>;
}
