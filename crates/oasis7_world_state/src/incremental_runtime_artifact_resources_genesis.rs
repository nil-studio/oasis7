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

/// Build a replacement manifest body in the caller's already admitted
/// writer. This is used only when the Runtime snapshot derivation has produced
/// a complete new manifest; ordinary map-member changes use point operations.
pub(super) fn record_manifest_body<W, C>(
    source: &ChainResourceManifest,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError>
where
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
{
    manifest::record_manifest(source, context, work)
}

/// Build a complete new delta body supplied by the Runtime snapshot/event
/// derivation. `None` is represented by the outer Unit entry and does not call
/// this helper.
pub(super) fn record_delta_body<W, C>(
    source: &ChainResourceDelta,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError>
where
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
{
    delta_records::record_delta(source, context, work)
}

/// Build one full builtin entry supplied by the existing Runtime upsert API.
pub(super) fn record_builtin_entry_body<W, C>(
    source: &BuiltinReleaseEntryRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError>
where
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
{
    builtin::record_builtin_entry(source, context, work)
}

pub(super) fn record_builtin_set_prefix<W, C>(
    root: &mut StateRootV1,
    module_count: u64,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError>
where
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
{
    let kind = RuntimeArtifactRecordV1::BuiltinModuleSet;
    common::metadata(root, kind, context, work)?;
    common::group(root, kind, &("modules", ()), context, work)?;
    common::u64_value(root, kind, "modules.count", module_count, context, work)
}

pub(super) fn root_reference_for<W, C, K>(
    record: RuntimeArtifactRecordV1,
    key: &K,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<RecordRootRefV1, MemberRecordError>
where
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
{
    common::root_reference(record, key, child, context)
}

pub(super) fn append_root_member<W, C, K>(
    root: &mut StateRootV1,
    parent: RuntimeArtifactRecordV1,
    key: &K,
    child_record: RuntimeArtifactRecordV1,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError>
where
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
{
    common::root_member(root, parent, key, child_record, child, context, work)
}
