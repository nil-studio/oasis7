use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use std::io;
pub(super) struct Outer {
    pub key: WorldFieldKeyV1,
    pub value_root: Option<[u8; 32]>,
    pub child: Option<StateRootV1>,
}
pub(super) fn overlay<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: LogisticsRecordKeyV1<'_>,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Outer, WorldFieldOverlayErrorV1> {
    let field = key.descriptor()?;
    let metadata = (
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?,
        0,
    );
    let marker = records.key(
        field,
        WorldFieldRecordKindV1::Entry,
        &(RECORD_FORMAT_KEY, ()),
    )?;
    let outer = records.key(field, WorldFieldRecordKindV1::Entry, &key.id)?;
    let meta = lookup(transaction, &metadata.0, records, error)?
        .ok_or(ValueTreeError::Invalid("logistics missing metadata"))?;
    let format = lookup(transaction, &marker.0, records, error)?
        .ok_or(ValueTreeError::Invalid("logistics missing format"))?;
    let value_root = lookup(transaction, &outer.0, records, error)?;
    records.presence(&meta, &metadata.0, 0)?;
    read::value(
        LogisticsCellV1::Record,
        &format,
        &marker,
        records,
        0,
        &mut io::sink(),
    )?;
    let child = value_root
        .map(|root| read::outer(key, &outer, &root, records))
        .transpose()?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Outer {
        key: outer.0,
        value_root,
        child,
    })
}
pub(super) fn write<W: ValueNodeWriter, C: FnMut() -> bool>(
    key: LogisticsRecordKeyV1<'_>,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    genesis::write_root(key, child, context)
}
