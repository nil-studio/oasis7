//! Borrowed GEN contribution. Emitted rows and original sink success remain
//! provisional until the complete caller validates and publishes its closure.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1, incremental_state::StateNamespaceV1,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeArtifactContentFieldV1 {
    Registered,
    Original,
}
impl RuntimeArtifactContentFieldV1 {
    pub const ALL: [Self; 2] = [Self::Registered, Self::Original];
    pub fn descriptor(self) -> FieldDescriptor {
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: match self {
                Self::Registered => "module_artifacts",
                Self::Original => "module_artifact_bytes",
            },
            destination: FieldDestination::State(StateNamespaceV1::Artifacts),
            shape: match self {
                Self::Registered => FieldShape::Set,
                Self::Original => FieldShape::ByteMap,
            },
        }
    }
}
fn spend_record(remaining: &mut u64) -> Result<(), ValueTreeError> {
    *remaining = remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "Runtime artifact GEN records",
        ))?;
    Ok(())
}
/// `persist` must persist/read back the exact bytes and original ID before it
/// reports success. Arbitrary callback success is not a physical capability.
pub fn prepare_runtime_artifact_content_genesis_v1<W, C, P, S>(
    registered: &BTreeSet<String>,
    originals: &BTreeMap<String, Arc<[u8]>>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    records_remaining: &mut u64,
    work: &mut RuntimeArtifactOriginalWorkV1,
    persist: &mut P,
    sink: &mut S,
) -> Result<(), MemberRecordError>
where
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    P: FnMut(OriginalArtifactRefV1, &[u8]) -> Result<(), ValueTreeError>,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
{
    for field in RuntimeArtifactContentFieldV1::ALL {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        spend_record(records_remaining)?;
        let metadata = write_field_metadata_v1(
            field.descriptor(),
            &mut PresenceRecordWriteContext {
                sink: &mut *ctx.sink,
                memory: ctx.memory,
                io: &mut *ctx.io,
                key_work: &mut *ctx.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: ctx.limits.semantic_bytes,
                    record_bytes: ctx.limits.record_bytes,
                },
                cancelled: &mut ctx.cancelled,
            },
        )?;
        crate::incremental_field_write::emit(metadata.field_key, metadata.value_root, ctx, sink)?;
    }
    for key in registered {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        spend_record(records_remaining)?;
        let row = write_member_record_v1(
            RuntimeArtifactContentFieldV1::Registered.descriptor(),
            WorldFieldRecordKindV1::Entry,
            key.as_str(),
            MemberPayloadRefV1::Unit,
            ctx,
        )?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    for (key, bytes) in originals {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        spend_record(records_remaining)?;
        let length = bytes.len() as u64;
        if work.objects_remaining == 0
            || length > work.max_object_bytes
            || length > work.bytes_remaining
        {
            return Err(
                ValueTreeError::ResourceLimited("Runtime artifact GEN original allowance").into(),
            );
        }
        let _scratch = ctx
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        work.objects_remaining -= 1;
        work.bytes_remaining -= length;
        let mut hash = blake3::Hasher::new();
        for chunk in bytes.chunks(VALUE_CHUNK_BYTES) {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            hash.update(chunk);
        }
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let original = OriginalArtifactRefV1 {
            id: *hash.finalize().as_bytes(),
            byte_len: length,
        };
        persist(original, bytes)?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let binding = runtime_artifact_original_bytes_v1(original);
        let row = write_member_record_v1(
            RuntimeArtifactContentFieldV1::Original.descriptor(),
            WorldFieldRecordKindV1::Entry,
            key.as_str(),
            MemberPayloadRefV1::Bytes(&binding),
            ctx,
        )?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
