//! Local durability prerequisite, deliberately disconnected from runtime activation.
//!
//! This module provides no cross-host fencing, independent-fault-domain durability,
//! signer rotation or formal committed receipt. Its proof is offline local evidence.
//! Callers must retain a head anchor outside this directory to detect rollback.
//! Storage must be a stable, operator-controlled local filesystem path supporting
//! exclusive file locks, atomic replacement and directory fsync. The lock excludes
//! cooperative writers on its retained inode; it does not protect against malicious
//! processes with the same filesystem permissions replacing that inode or directory.

pub mod activation;
#[path = "formal/fencing.rs"]
pub mod fencing;
pub mod formal;
mod journal;
pub mod proof;
pub mod replicated_protocol;
mod storage;
mod writer_lock;
pub use journal::ControlledAuthorityLocalJournal;
pub use oasis7_formal_protocol::controlled_authority::*;
#[cfg(test)]
mod tests;
