//! Strict signer IPC, canonical payloads and pure request validation.
//!
//! This crate performs no key custody, filesystem access or process execution.

pub mod detached;
pub mod error;
pub mod identity;
pub mod protocol;
pub mod rollback;
pub mod types;
