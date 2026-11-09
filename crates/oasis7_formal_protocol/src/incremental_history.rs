//! Immutable IWS history/commit objects and authenticated catalog structure.
//! Content proofs never confer finality. The original authority must accept
//! the exact body before an extension can be selected as a serving head.
use crate::incremental_execution::{
    AuthorityBinding, ExecutionError, ExecutionFactKindV1, ExecutionFactRefV1, RootSetV1,
    TransitionRecordV1,
};
use crate::incremental_state::MerkleNodeReader;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

#[path = "incremental_history_objects.rs"]
mod objects;
#[path = "incremental_history_wire.rs"]
pub(crate) mod wire;
pub use objects::*;
#[path = "incremental_catalog.rs"]
mod catalog;
pub use catalog::*;
#[path = "incremental_catalog_codec.rs"]
mod catalog_codec;
#[path = "incremental_history_preparation.rs"]
mod preparation;
pub use preparation::*;

pub const HISTORY_MAX_HEADER_BYTES: usize = 4096;
pub const CATALOG_MAX_NODE_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldScopeV1 {
    pub world_id: String,
    pub genesis_config: [u8; 32],
}
impl WorldScopeV1 {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        if self.world_id.is_empty() || self.world_id.len() > 256 {
            return Err(ExecutionError::Invalid("history world ID bound"));
        }
        Ok(())
    }
    pub fn matches(&self, roots: &RootSetV1) -> bool {
        self.world_id == roots.world_id && self.genesis_config == roots.genesis_config
    }
}

#[cfg(test)]
#[path = "incremental_history_tests.rs"]
mod tests;
