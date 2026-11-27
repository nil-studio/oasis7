//! Control-only Host. No Executor store, Native SID, business tools or raw signer API.
#[path = "custody_host_registry.rs"]
mod registry;
#[path = "custody_host_state.rs"]
pub(crate) mod state;
