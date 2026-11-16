//! Standalone account selection. No game authority, client, or outbound transport.
mod config;
pub(crate) mod platform;
mod state;
#[cfg(test)]
mod tests;
