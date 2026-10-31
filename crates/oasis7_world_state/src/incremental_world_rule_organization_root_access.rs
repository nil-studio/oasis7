//! Shared exact current-root resolution and publication for typed successors.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::incremental_execution::ExecutionTransaction;
use oasis7_proto::memory_budget::ReservedValue;
pub(super) fn current<
    'p,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: OrganizationRootKeyV1<'_>,
    proof: &'p ReservedValue<OrganizationRecordQualificationV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(WorldFieldKeyV1, &'p StateRootV1), WorldFieldOverlayErrorV1> {
    if records.values.class != MemoryClass::Ordinary
        || !tx.uses_budget(records.values.memory)
        || !proof.uses_budget(records.values.memory)
    {
        return Err(ValueTreeError::Invalid("organization successor pool/class").into());
    }
    let (_, present) =
        crate::incremental_world_rule_parent::read_parent_overlay(tx, records, error)?;
    if !present {
        return Err(ValueTreeError::Invalid("organization successor requires rules").into());
    }
    let key = owner.outer().overlay_path(RecordPathV1::Record, records)?;
    let value = lookup(tx, &key.0, records, error)?.ok_or(ValueTreeError::Invalid(
        "organization successor requires record",
    ))?;
    let _scratch = records
        .values
        .memory
        .reserve(RECORD_ROOT_BYTES + 4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        &value,
        &key.0,
        records.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: key.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: key.1,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: records.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut *records.cancelled,
    )?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("organization successor root reference"))?;
    let digest = WorldFieldKeyV1::from_bytes(key.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid(
            "organization successor owner digest",
        ))?;
    let root = reference
        .validate_binding(&owner.schema(), &digest, StateNamespaceV1::Authorization)
        .map_err(|_| ValueTreeError::Invalid("organization successor root binding"))?;
    proof.validate_binding(&owner.schema(), &digest, root)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((key.0, proof.root()))
}
pub(super) fn store<
    W: oasis7_formal_protocol::incremental_state::StatePageWriter,
    C: FnMut() -> bool,
>(
    owner: OrganizationRootKeyV1<'_>,
    outer: &WorldFieldKeyV1,
    delta: &ReservedValue<oasis7_formal_protocol::incremental_state::StateDeltaV1>,
    write: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    inner_tree::persist(delta, write)?;
    let row = write_organization_root_reference_v1(owner, &delta.after, write)?;
    if row.field_key != *outer {
        return Err(MemberRecordError::Invalid(
            "organization successor outer identity",
        ));
    }
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(row)
}
