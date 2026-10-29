//! Single-parent incremental execution transactions and complete transitions.
//! Integrity checks here never replace execution legality or original authority.
use crate::incremental_state::{
    MerkleNodeReader, STATE_MAX_KEY_BYTES, StateDeltaV1, StateNamespaceV1, StateNodeWriteV1,
    StateRootV1, StateWriteV1,
};
use oasis7_proto::memory_budget::{
    MemoryBudget, MemoryClass, MemoryLimitExceeded, MemoryReservation, ReservedValue,
};

#[path = "incremental_root_set.rs"]
mod root_set;
pub use root_set::RootSetV1;
#[path = "incremental_transaction.rs"]
mod transaction;
pub use transaction::{ExecutionTransaction, PreparedGuard, TransactionLimits};
#[path = "incremental_transition.rs"]
mod transition;
pub use transition::{PreparedTransition, TransitionRecordV1, TransitionVerifier};
#[path = "incremental_execution_codec.rs"]
mod codec;

pub const EXECUTION_MAX_READS: usize = 4096;
pub const EXECUTION_MAX_WRITES: usize = 1024;
pub const EXECUTION_MAX_FACTS: usize = 4096;
pub const EXECUTION_MAX_RECORD_BYTES: usize = 8 * 1024 * 1024;
const NAMESPACES: [StateNamespaceV1; 6] = [
    StateNamespaceV1::Business,
    StateNamespaceV1::Allocator,
    StateNamespaceV1::Scheduler,
    StateNamespaceV1::Authorization,
    StateNamespaceV1::Effects,
    StateNamespaceV1::Artifacts,
];

#[derive(Debug)]
pub enum ExecutionError {
    Invalid(&'static str),
    ResourceLimited(&'static str),
    Memory(MemoryLimitExceeded),
    Tree(String),
    StaleGuard,
    ParentConflict,
    Aborted,
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(reason) => write!(f, "invalid IWS execution: {reason}"),
            Self::ResourceLimited(reason) => write!(f, "IWS execution resource limited: {reason}"),
            Self::Memory(error) => error.fmt(f),
            Self::Tree(error) => write!(f, "IWS state tree: {error}"),
            Self::StaleGuard => f.write_str("stale IWS prepared guard"),
            Self::ParentConflict => f.write_str("IWS parent/authority fence changed"),
            Self::Aborted => f.write_str("IWS transaction aborted after an execution error"),
        }
    }
}
impl std::error::Error for ExecutionError {}
impl From<MemoryLimitExceeded> for ExecutionError {
    fn from(error: MemoryLimitExceeded) -> Self {
        Self::Memory(error)
    }
}

pub(crate) fn validate_source_budget(
    source: &impl MerkleNodeReader,
    memory: &MemoryBudget,
) -> Result<(), ExecutionError> {
    if source
        .allocation_budget()
        .is_some_and(|source| !source.same_pool(memory))
    {
        return Err(ExecutionError::Invalid("Merkle source memory pool"));
    }
    Ok(())
}

/// An exact binding supplied by the original authority adapter. Possessing
/// these bytes alone does not prove writer qualification or grant a new epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorityBinding {
    pub token: [u8; 32],
    pub epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionFence {
    pub parent_commit: [u8; 32],
    pub parent_roots: [u8; 32],
    pub authority: AuthorityBinding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionParent {
    pub commit: [u8; 32],
    pub roots: RootSetV1,
    pub authority: AuthorityBinding,
}
impl ExecutionParent {
    pub fn fence(&self) -> Result<ExecutionFence, ExecutionError> {
        Ok(ExecutionFence {
            parent_commit: self.commit,
            parent_roots: self.roots.digest()?,
            authority: self.authority,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReadV1 {
    pub namespace: StateNamespaceV1,
    pub key: Vec<u8>,
    pub before: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionWriteV1 {
    pub namespace: StateNamespaceV1,
    pub mutation: StateWriteV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExecutionFactKindV1 {
    Input = 1,
    Result = 2,
    Event = 3,
}

/// Original facts stay in execution order even if final writes cancel out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionFactRefV1 {
    pub kind: ExecutionFactKindV1,
    pub id: [u8; 32],
}

fn namespace_index(namespace: StateNamespaceV1) -> usize {
    namespace as usize - 1
}
fn validate_key(key: &[u8]) -> Result<(), ExecutionError> {
    if key.len() > STATE_MAX_KEY_BYTES {
        return Err(ExecutionError::Invalid("state key bound"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "incremental_execution_tests.rs"]
mod tests;
