//! Original map strings, not identity/account authorization or World qualification.
use crate::{
    incremental_field_member::*, incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1, incremental_projection::WORLD_STATE_FIELDS,
    incremental_utf8_output::read_text_value, state::WorldState,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::Write;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldTextMapFieldV1 {
    ModuleArtifactOwners,
    NodeIdentityBindings,
    NodeMainTokenAccountBindings,
}
impl WorldTextMapFieldV1 {
    pub const ALL: [Self; 3] = [
        Self::ModuleArtifactOwners,
        Self::NodeIdentityBindings,
        Self::NodeMainTokenAccountBindings,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::ModuleArtifactOwners => "module_artifact_owners",
            Self::NodeIdentityBindings => "node_identity_bindings",
            Self::NodeMainTokenAccountBindings => "node_main_token_account_bindings",
        }
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::ModuleArtifactOwners => StateNamespaceV1::Business,
            _ => StateNamespaceV1::Authorization,
        }
    }
    fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|field| {
                field.name == self.name()
                    && field.owner == FieldOwner::WorldState
                    && field.shape == FieldShape::Map
                    && field.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(ValueTreeError::Invalid("world text map registration"))
    }
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
        .ok_or(ValueTreeError::ResourceLimited("world text map records"))?;
    Ok(())
}
/// Caller admits source/batch storage and discards every emitted prefix on error.
pub fn prepare_world_text_maps_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    let mut count = 0u64;
    for field in WorldTextMapFieldV1::ALL {
        start(context, records)?;
        let descriptor = field.descriptor()?;
        let metadata = write_field_metadata_v1(
            descriptor,
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
        crate::incremental_field_write::emit(
            metadata.field_key,
            metadata.value_root,
            context,
            sink,
        )?;
        let values = match field {
            WorldTextMapFieldV1::ModuleArtifactOwners => &source.module_artifact_owners,
            WorldTextMapFieldV1::NodeIdentityBindings => &source.node_identity_bindings,
            WorldTextMapFieldV1::NodeMainTokenAccountBindings => {
                &source.node_main_token_account_bindings
            }
        };
        for (key, value) in values {
            start(context, records)?;
            let record = write_member_record_v1(
                descriptor,
                WorldFieldRecordKindV1::Entry,
                key.as_str(),
                MemberPayloadRefV1::Text(value),
                context,
            )?;
            crate::incremental_field_write::emit(
                record.field_key,
                record.value_root,
                context,
                sink,
            )?;
            count = count
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid("world text map count"))?;
        }
    }
    Ok(count)
}
/// Stream a fully authenticated UTF-8 member without materializing it. None is
/// absence only after field Metadata authentication; Some(0) is an empty value.
/// Retained output is caller-admitted and provisional until whole-call success.
pub fn read_world_text_map_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldTextMapFieldV1,
    key: &str,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<Option<u64>, ValueTreeError> {
    context.check()?;
    let descriptor = field.descriptor()?;
    let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = context
        .lookup(&metadata)?
        .ok_or(ValueTreeError::Invalid("world text map missing metadata"))?;
    context.records().presence(&root, &metadata, 0)?;
    let (key, bytes) = context.key(descriptor, WorldFieldRecordKindV1::Entry, key)?;
    let Some(root) = context.lookup(&key)? else {
        return Ok(None);
    };
    read_text_value(
        &root,
        &key,
        &mut context.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Text,
            semantic_bytes: bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: context.semantic_bytes,
                payload_bytes: max_payload_bytes,
                record_bytes: context.record_bytes,
            },
        },
        output,
        "world text map UTF-8",
        &mut context.cancelled,
    )
    .map(Some)
}
#[path = "incremental_world_text_maps_overlay.rs"]
mod overlay;
pub use overlay::{
    WorldTextMapChangeV1, change_world_text_map_overlay_v1, read_world_text_map_overlay_v1,
};
