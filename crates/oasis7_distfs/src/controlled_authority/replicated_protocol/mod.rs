//! Fixed-epoch, two-file-endpoint durability protocol. No runtime activation,
//! formal CommitRef, automatic takeover or independent-fault-domain assertion.
//! Endpoint signatures are trusted assertions of the configured fsync contract;
//! offline verification cannot establish actual hardware durability/topology.
//! Paths are operator controlled, not protected against malicious same-permission
//! processes replacing directories or lock inodes. Unix durability only.

mod coordinator;
pub use oasis7_formal_protocol::controlled_authority::replicated_protocol::{
    crypto, frozen, package,
};
#[cfg(test)]
mod frozen_tests;
pub use frozen::{
    FrozenCertificate, FrozenInputBody, FrozenOutcome, FrozenReceipt, FrozenReservation,
    FrozenStatus, SignedFrozenInput,
};
mod archive;
#[cfg(test)]
mod archive_tests;
mod archive_trie;
mod endpoint;
#[cfg(test)]
mod fencing_tests;
mod journal;
#[cfg(test)]
mod journal_tests;
mod storage;
#[cfg(test)]
mod tests;
pub mod transport;

pub use coordinator::ReplicatedCoordinator;
pub use endpoint::FileEndpoint;
pub use oasis7_formal_protocol::controlled_authority::replicated_protocol::*;

use super::LocalRequestIdentity;
use serde::{Deserialize, Serialize};
