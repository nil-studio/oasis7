//! Version-independent compatibility protocol; libp2p types stay in each adapter.
mod driver;
mod protocol;

pub use driver::main;
pub use protocol::{Adapter, Event, IDENTITY_MESSAGE, RPC_PROTOCOL, TOPIC};
