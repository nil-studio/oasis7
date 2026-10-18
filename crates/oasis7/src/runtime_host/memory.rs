//! Native stale-parent memory correction adapter. Preparation provenance stays immutable.
use crate::simulator::{CognitionError, ContinuousAgentTurnContextV1, MemoryWriteStore};

pub fn rebind_stale_parent_corrections(
    store: &mut MemoryWriteStore,
    parent_request_id: &str,
    parent_digest: &str,
    turn: &ContinuousAgentTurnContextV1,
) -> Result<(), CognitionError> {
    store.rebind_corrections_after_stale_parent(parent_request_id, parent_digest, turn)
}
