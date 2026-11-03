//! The only global record entry is a bound typed reference to a closed child.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::ValueNodeReader,
};
use std::io;
pub(super) struct Outer {
    pub key: WorldFieldKeyV1,
    pub value_root: Option<[u8; 32]>,
    pub child: Option<StateRootV1>,
}
type OuterKeys = [(WorldFieldKeyV1, u64); 3];
fn keys<R: ValueNodeReader, C: FnMut() -> bool>(
    key: FactoryRecordKeyV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<OuterKeys, ValueTreeError> {
    let field = key.descriptor()?;
    Ok([
        (
            WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?,
            0,
        ),
        records.key(
            field,
            WorldFieldRecordKindV1::Entry,
            &(RECORD_FORMAT_KEY, ()),
        )?,
        with_factory_outer_key!(key,encoded=>records.key(field,WorldFieldRecordKindV1::Entry,encoded))?,
    ])
}
fn decode<R: ValueNodeReader, C: FnMut() -> bool>(
    key: FactoryRecordKeyV1<'_>,
    keys: OuterKeys,
    roots: [Option<[u8; 32]>; 3],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<Outer, ValueTreeError> {
    let metadata = roots[0].ok_or(ValueTreeError::Invalid("factory missing metadata"))?;
    records.presence(&metadata, &keys[0].0, 0)?;
    let format = roots[1].ok_or(ValueTreeError::Invalid(
        "factory missing record-root profile",
    ))?;
    read::value(
        FactoryCellV1::Record,
        &format,
        &keys[1],
        records,
        0,
        &mut io::sink(),
    )?;
    let child = if let Some(root) = roots[2] {
        let mut bytes = [0u8; RECORD_ROOT_BYTES];
        read_member_record_profile_v1(
            &root,
            &keys[2].0,
            records.values,
            MemberReadProfileV1 {
                codec: MemberPayloadCodecV1::RecordRoot,
                semantic_bytes: keys[2].1,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: records.semantic_bytes,
                    payload_bytes: RECORD_ROOT_BYTES as u64,
                    record_bytes: records.record_bytes,
                },
            },
            &mut io::sink(),
            &mut &mut bytes[..],
            &mut *records.cancelled,
        )?;
        let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
            .map_err(|_| ValueTreeError::Invalid("factory record root framing"))?;
        let binding =
            WorldFieldKeyV1::from_bytes(keys[2].0.as_bytes()).map_err(ValueTreeError::Invalid)?;
        Some(
            reference
                .validate_binding(
                    &key.schema(),
                    &binding.semantic_key.unwrap(),
                    key.namespace(),
                )
                .map_err(|_| ValueTreeError::Invalid("factory record root binding"))?
                .clone(),
        )
    } else {
        None
    };
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(Outer {
        key: keys[2].0.clone(),
        value_root: roots[2],
        child,
    })
}
pub(super) fn fixed<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: FactoryRecordKeyV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Outer, ValueTreeError> {
    let keys = keys(key, &mut context.records())?;
    let mut roots = [None; 3];
    for (index, (key, _)) in keys.iter().enumerate() {
        roots[index] = context.lookup(key)?;
    }
    decode(key, keys, roots, &mut context.records())
}
pub(super) fn overlay<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: FactoryRecordKeyV1<'_>,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Outer, WorldFieldOverlayErrorV1> {
    let keys = keys(key, records)?;
    let mut roots = [None; 3];
    for (index, (key, _)) in keys.iter().enumerate() {
        roots[index] = lookup(transaction, key, records, error)?;
    }
    Ok(decode(key, keys, roots, records)?)
}
pub(super) fn write<W: ValueNodeWriter, C: FnMut() -> bool>(
    key: FactoryRecordKeyV1<'_>,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _workspace = context
        .memory
        .reserve(8192, oasis7_proto::memory_budget::MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let field = key.descriptor()?;
    let semantic=with_factory_outer_key!(key,encoded=>crate::incremental_semantic_key::hash_semantic_key_v1(encoded,context.memory,oasis7_proto::memory_budget::MemoryClass::Ordinary,context.key_work,&mut context.cancelled)).map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(key.schema(), semantic.digest, child)
        .map_err(|_| MemberRecordError::Invalid("factory record root"))?;
    with_factory_outer_key!(key,encoded=>write_member_record_v1(field,WorldFieldRecordKindV1::Entry,encoded,MemberPayloadRefV1::RecordRoot(&reference),context))
}
pub(super) fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    key: FactoryRecordKeyV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, MemberRecordError> {
    write_field_metadata_v1(
        key.descriptor()?,
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
    )
}
pub(super) fn genesis_body<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    key: FactoryRecordKeyV1<'_>,
    value: FactoryRecordRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    work: &mut FactoryWorkBudgetV1,
) -> Result<StateRootV1, MemberRecordError> {
    let _workspace = context
        .memory
        .reserve(16 << 10, oasis7_proto::memory_budget::MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut root = StateRootV1::empty(key.namespace())
        .map_err(|_| MemberRecordError::Invalid("factory child namespace"))?;
    work.cell()?;
    spend(records)?;
    let metadata = metadata(key, context)?;
    root = tree::append(
        root,
        &metadata.field_key,
        metadata.value_root,
        context,
        work,
    )?;
    visit_factory_record_v1(key, value, &mut |cell,
                                              payload|
     -> Result<(), MemberRecordError> {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        work.cell()?;
        spend(records)?;
        let record = write_cell(key, cell, payload, context)?;
        root = tree::append(
            root.clone(),
            &record.field_key,
            record.value_root,
            context,
            work,
        )?;
        Ok(())
    })?;
    Ok(root)
}

pub(super) fn body<
    R: ValueNodeReader,
    W: StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    key: FactoryRecordKeyV1<'_>,
    value: FactoryRecordRefV1<'_>,
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    work: &mut FactoryWorkBudgetV1,
) -> Result<StateRootV1, WorldFieldOverlayErrorV1> {
    let _workspace = write
        .memory
        .reserve(16 << 10, oasis7_proto::memory_budget::MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut root = StateRootV1::empty(key.namespace())
        .map_err(|_| ValueTreeError::Invalid("factory child namespace"))?;
    work.cell()?;
    let metadata = {
        let mut combined = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || cancelled() || (write.cancelled)(),
        };
        metadata(key, &mut combined)?
    };
    root = tree::edit(
        root,
        &metadata.field_key,
        (None, Some(metadata.value_root)),
        values,
        write,
        cancelled,
        work,
    )?;
    visit_factory_record_v1(key, value, &mut |cell,
                                              payload|
     -> Result<
        (),
        WorldFieldOverlayErrorV1,
    > {
        work.cell()?;
        let record = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled() || (write.cancelled)(),
            };
            write_cell(key, cell, payload, &mut combined)?
        };
        root = tree::edit(
            root.clone(),
            &record.field_key,
            (None, Some(record.value_root)),
            values,
            write,
            cancelled,
            work,
        )?;
        Ok(())
    })?;
    Ok(root)
}
