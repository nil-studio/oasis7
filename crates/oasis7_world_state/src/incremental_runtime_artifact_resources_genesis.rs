//! Bounded borrowed genesis for the original Runtime Artifacts fields.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_group_v1,
        write_field_metadata_v1,
    },
    incremental_field_write,
    incremental_record_tree::{self, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

#[path = "incremental_runtime_artifact_resources_genesis_delta.rs"]
mod delta;
#[path = "incremental_runtime_artifact_resources_genesis_identity.rs"]
mod identity;

#[path = "incremental_runtime_artifact_resources_genesis_builtin.rs"]
mod builtin;
#[path = "incremental_runtime_artifact_resources_genesis_common.rs"]
mod common;
#[path = "incremental_runtime_artifact_resources_genesis_delta_records.rs"]
mod delta_records;
#[path = "incremental_runtime_artifact_resources_genesis_manifest.rs"]
mod manifest;
pub use common::RuntimeArtifactResourcesWorkV1;
use common::*;
/// Emit the three original Runtime Artifacts fields as typed provisional
/// records. Every write remains uncommitted until its owning transaction seals.
pub fn prepare_runtime_artifact_resources_genesis_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    B: BuiltinReleaseManifestSourceV1,
>(
    source: &RuntimeArtifactResourcesRefV1<'_, B>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    let starting_cells = work.cells_remaining;
    let manifest = manifest::record_manifest(source.chain_resource_manifest, context, work)?;
    emit_top(
        RuntimeArtifactFieldV1::ChainResourceManifest,
        Some(manifest),
        context,
        sink,
        work,
    )?;
    let delta = match source.latest_chain_resource_delta {
        Some(value) => Some(delta_records::record_delta(value, context, work)?),
        None => None,
    };
    emit_top(
        RuntimeArtifactFieldV1::LatestChainResourceDelta,
        delta,
        context,
        sink,
        work,
    )?;
    let builtin = builtin::record_builtin(source.builtin_release_manifest, context, work)?;
    emit_top(
        RuntimeArtifactFieldV1::BuiltinReleaseManifest,
        Some(builtin),
        context,
        sink,
        work,
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(starting_cells - work.cells_remaining)
}
