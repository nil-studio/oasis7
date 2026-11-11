//! Current ChainResourceManifest pointer using the original fixed reference.
//! Child data is not a dispatch or execution qualification.
use super::*;
use crate::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup,
};
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard};
#[allow(clippy::too_many_arguments)]
pub fn open_current_chain_resource_manifest_v1<M, R, E, C>(
    parent: &ReservedValue<
        crate::incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1,
    >,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<RuntimeArtifactChildRootV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !parent.uses_budget(ctx.values.memory)
            || parent.reserved_bytes()
                < std::mem::size_of::<
                    crate::incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1,
                >()
        {
            return Err(invalid().into());
        }
        let fee = ctx
            .values
            .memory
            .reserve(
                8192 + std::mem::size_of::<RuntimeArtifactChildRootV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        parent.validate_binding(&tx.parent_fence().parent_roots, MemoryClass::Ordinary)?;
        let field = RuntimeArtifactFieldV1::ChainResourceManifest;
        let descriptor = field.descriptor();
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let value =
            lookup(tx, &metadata, &mut records, &mut ctx.take_state_error)?.ok_or_else(invalid)?;
        records.presence(&value, &metadata, 0)?;
        let (key, semantic_bytes) = records.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
        let value =
            lookup(tx, &key, &mut records, &mut ctx.take_state_error)?.ok_or_else(invalid)?;
        let mut bytes = [0; RECORD_ROOT_BYTES];
        let info = read_member_record_v1(
            &value,
            &key,
            records.values,
            MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: records.record_bytes,
            },
            &mut io::sink(),
            &mut &mut bytes[..],
            &mut *records.cancelled,
        )?;
        if info.semantic_bytes != semantic_bytes
            || info.codec != MemberPayloadCodecV1::RecordRoot
            || info.payload_bytes != RECORD_ROOT_BYTES as u64
        {
            return Err(invalid().into());
        }
        let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
        let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let root = reference
            .validate_binding(&field.schema(), &owner, StateNamespaceV1::Artifacts)
            .map_err(|_| invalid())?
            .clone();
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        Ok(fee.retain(RuntimeArtifactChildRootV1 {
            binding: RuntimeArtifactRootBindingV1::Field(field),
            root,
            class: MemoryClass::Ordinary,
        }))
    })
}
