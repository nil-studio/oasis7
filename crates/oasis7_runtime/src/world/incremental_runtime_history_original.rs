//! Compatibility exports and the actual private Runtime World borrowing adapter.
use oasis7_world_state::consensus::{TickConsensusRecord, TickConsensusRejectionAuditEvent};
pub use oasis7_world_state::incremental_runtime_history_original::*;
/// The five actual Runtime History owners, borrowed from the execution World.
/// The archived prefix is read through the Formal typed index; `ticks` is only
/// the current hot suffix and must never be relabeled as the complete prefix.
pub struct RuntimeHistoryBorrowedV1<'a> {
    pub snapshot_catalog: &'a oasis7_world_state::snapshot::SnapshotCatalog,
    pub journal: &'a oasis7_world_state::snapshot::Journal,
    pub ticks: &'a [TickConsensusRecord],
    pub archived_tick_count: u64,
    pub rejection_audit: &'a [TickConsensusRejectionAuditEvent],
}
impl super::World {
    pub fn borrow_iws_runtime_history_v1(&self) -> RuntimeHistoryBorrowedV1<'_> {
        RuntimeHistoryBorrowedV1 {
            snapshot_catalog: &self.snapshot_catalog,
            journal: &self.journal,
            ticks: &self.tick_consensus_records,
            archived_tick_count: self.formal_archived_tick_consensus_count,
            rejection_audit: &self.tick_consensus_rejection_audit_events,
        }
    }
}

#[cfg(test)]
use oasis7_proto::memory_budget::MemoryBudget;
#[cfg(test)]
use oasis7_wasm_abi::CanonicalCborWorkV1;
#[cfg(test)]
use oasis7_world_state::{incremental_original_producer as original, snapshot::SnapshotRecord};
#[cfg(test)]
#[path = "incremental_runtime_history_original_tests.rs"]
mod tests;
