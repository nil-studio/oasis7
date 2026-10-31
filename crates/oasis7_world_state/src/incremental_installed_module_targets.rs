//! Original installation-target contents, never installation permission.
pub use crate::incremental_module_instance_fields::ModuleInstallTargetRefV1;
use crate::{
    incremental_field_member::*, incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1, incremental_projection::WORLD_STATE_FIELDS,
    incremental_utf8_output::Utf8Output, state::WorldState,
};
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::Write;
fn descriptor() -> Result<FieldDescriptor, ValueTreeError> {
    WORLD_STATE_FIELDS
        .iter()
        .copied()
        .find(|field| {
            field.name == "installed_module_targets"
                && field.owner == FieldOwner::WorldState
                && field.shape == FieldShape::Map
                && field.destination == FieldDestination::State(StateNamespaceV1::Artifacts)
        })
        .ok_or(ValueTreeError::Invalid("installed targets registration"))
}
fn start<W, C: FnMut() -> bool>(
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    *records = records
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("installed targets records"))?;
    Ok(())
}
/// Source and batch/index residency are caller-admitted. Discard every prefix
/// on failure; success covers only this registered field, not a whole World.
pub fn prepare_installed_module_targets_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    let field = descriptor()?;
    start(context, records)?;
    let metadata = write_field_metadata_v1(
        field,
        &mut PresenceRecordWriteContext {
            sink: &mut *context.sink,
            memory: context.memory,
            io: &mut *context.io,
            key_work: &mut *context.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: context.limits.semantic_bytes,
                record_bytes: context.limits.record_bytes,
            },
            cancelled: &mut context.cancelled,
        },
    )?;
    crate::incremental_field_write::emit(metadata.field_key, metadata.value_root, context, sink)?;
    let mut count = 0u64;
    for (key, target) in &source.installed_module_targets {
        start(context, records)?;
        let payload = match target {
            ModuleInstallTarget::SelfAgent => MemberPayloadRefV1::Unit,
            ModuleInstallTarget::LocationInfrastructure { location_id } => {
                MemberPayloadRefV1::Text(location_id)
            }
        };
        let record = write_member_record_v1(
            field,
            WorldFieldRecordKindV1::Entry,
            key.as_str(),
            payload,
            context,
        )?;
        crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)?;
        count = count
            .checked_add(1)
            .ok_or(MemberRecordError::Invalid("installed targets count"))?;
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(count)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstalledModuleTargetReadV1 {
    SelfAgent,
    LocationInfrastructure { utf8_bytes: u64 },
}
/// None is authenticated absence; SelfAgent differs from an empty location.
/// Streamed bytes remain provisional until full success and final cancellation.
/// The caller admits the query key and any retained location output.
pub fn read_installed_module_target_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &str,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_location_bytes: u64,
    output: &mut impl Write,
) -> Result<Option<InstalledModuleTargetReadV1>, ValueTreeError> {
    context.check()?;
    let field = descriptor()?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = context.lookup(&metadata)?.ok_or(ValueTreeError::Invalid(
        "installed targets missing metadata",
    ))?;
    context.records().presence(&root, &metadata, 0)?;
    let (key, semantic_bytes) = context.key(field, WorldFieldRecordKindV1::Entry, key)?;
    let Some(root) = context.lookup(&key)? else {
        return Ok(None);
    };
    read_target_value(
        &root,
        &key,
        &mut context.values,
        MemberRecordLimitsV1 {
            semantic_bytes: context.semantic_bytes,
            payload_bytes: max_location_bytes,
            record_bytes: context.record_bytes,
        },
        semantic_bytes,
        output,
        &mut context.cancelled,
    )
    .map(Some)
}
fn read_target_value<R: ValueNodeReader>(
    root: &[u8; 32],
    key: &WorldFieldKeyV1,
    context: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    limits: MemberRecordLimitsV1,
    semantic_bytes: u64,
    output: &mut impl Write,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<InstalledModuleTargetReadV1, ValueTreeError> {
    let mut checked = Utf8Output::new(output, "installed target UTF-8");
    let result = read_member_record_v1(
        root,
        key,
        context,
        limits,
        &mut std::io::sink(),
        &mut checked,
        &mut *cancelled,
    );
    if checked.is_invalid() {
        return Err(ValueTreeError::Invalid("installed target UTF-8"));
    }
    let info = result?;
    if info.semantic_bytes != semantic_bytes {
        return Err(ValueTreeError::Invalid("installed target key length"));
    }
    let target = match info.codec {
        MemberPayloadCodecV1::Unit => InstalledModuleTargetReadV1::SelfAgent,
        MemberPayloadCodecV1::Text if checked.is_complete() => {
            InstalledModuleTargetReadV1::LocationInfrastructure {
                utf8_bytes: info.payload_bytes,
            }
        }
        MemberPayloadCodecV1::Text => {
            return Err(ValueTreeError::Invalid("installed target UTF-8"));
        }
        _ => return Err(ValueTreeError::Invalid("installed target codec")),
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(target)
}
#[path = "incremental_installed_module_targets_overlay.rs"]
mod overlay;
pub use overlay::{
    InstalledModuleTargetChangeV1, change_installed_module_target_overlay_v1,
    read_installed_module_target_overlay_v1,
};
