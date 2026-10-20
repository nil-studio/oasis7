use oasis7_runtime::blake3_hex;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(super) struct ExecutionHashPayload<'a> {
    pub(super) world_id: &'a str,
    pub(super) height: u64,
    pub(super) prev_execution_block_hash: &'a str,
    pub(super) execution_state_root: &'a str,
    pub(super) journal_len: usize,
}

/// Read-only compatibility verification uses the original execution hash shape.
pub(crate) fn legacy_migration_execution_hash(
    world_id: &str,
    height: u64,
    previous_execution_block_hash: &str,
    execution_state_root: &str,
    journal_len: usize,
) -> Result<String, String> {
    Ok(blake3_hex(&super::to_cbor(ExecutionHashPayload {
        world_id,
        height,
        prev_execution_block_hash: previous_execution_block_hash,
        execution_state_root,
        journal_len,
    })?))
}

pub(super) fn execution_resource_created_at_height(height: u64) -> u64 {
    if height == 0 { 0 } else { 1 }
}

pub(super) fn execution_resource_context_hash(world_id: &str) -> String {
    format!("execution_bridge_runtime_context_v1:{world_id}")
}

pub(super) fn execution_resource_commit_hash(world_id: &str, height: u64) -> String {
    blake3_hex(format!("execution_bridge_resource_commit_v1:{world_id}:{height}").as_bytes())
}
