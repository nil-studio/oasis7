//! Local, exact-grant signing worker and strict IPC protocol.

pub use oasis7_signer_protocol::{detached, error, identity, protocol, rollback, types};
pub mod runtime_gate;

pub mod admin;
pub mod authorization;
pub mod backup;
pub mod control;
pub mod installation;
pub mod job;
pub mod key_envelope;
pub mod key_management;
pub mod store;
pub mod worker;

mod local_fs;

pub use local_fs::read_candidate_bytes;

pub mod manager_cli;

pub mod file_cli;

#[cfg(test)]
mod key_management_tests;
