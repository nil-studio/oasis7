use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_ledger_value::{LEDGER_VALUE_HEADER_BYTES, LedgerValueInfoV1, header_v1},
    incremental_utf8_output::Utf8Output,
};
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader, incremental_value::ValueNodeReader,
};
use std::io::{self, Write};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryValueV1 {
    Unit,
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
    Text { utf8_bytes: u64 },
    Ledger(LedgerValueInfoV1),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryCellReadV1 {
    RecordAbsent,
    MemberAbsent,
    Value(FactoryValueV1),
}
pub(super) type Keys = [Option<(WorldFieldKeyV1, u64)>; 5];
pub(super) type Roots = [Option<[u8; 32]>; 5];
fn collection(cell: FactoryCellV1<'_>) -> Option<FactoryCollectionV1> {
    match cell {
        FactoryCellV1::TextItem(c, _)
        | FactoryCellV1::StackItem(c, _, _)
        | FactoryCellV1::Balance(c, _) => Some(c),
        _ => None,
    }
}
pub(super) fn optional_parent(cell: FactoryCellV1<'_>) -> Option<FactoryColumnV1> {
    use FactoryColumnV1 as C;
    match cell {
        FactoryCellV1::Column(
            C::SnapshotRecipeId
            | C::SnapshotConsumeLedger
            | C::SnapshotPowerRequired
            | C::SnapshotOutputLedger,
        ) => Some(C::ProductionSnapshotPresent),
        FactoryCellV1::Column(c)
            if c.build_only()
                && !matches!(
                    c,
                    C::BuildJobId
                        | C::BuildBuilderAgentId
                        | C::BuildSiteId
                        | C::BuildConsumeLedger
                        | C::BuildReadyAt
                        | C::BuildContractVersion
                        | C::BuildSiteAuthorityRevision
                        | C::BuildSiteLocationId
                        | C::BuildLocationAnchorRevision
                        | C::ObligationPresent
                ) =>
        {
            Some(C::ObligationPresent)
        }
        FactoryCellV1::Collection(c)
        | FactoryCellV1::TextItem(c, _)
        | FactoryCellV1::StackItem(c, _, _)
        | FactoryCellV1::Balance(c, _) => match c {
            FactoryCollectionV1::SpecTags | FactoryCollectionV1::SpecBuildCost => None,
            FactoryCollectionV1::ObligationBalancesBefore
            | FactoryCollectionV1::ObligationBalancesAfter => Some(C::ObligationPresent),
            _ => Some(C::ProductionSnapshotPresent),
        },
        _ => None,
    }
}
pub(super) fn required_nested(cell: FactoryCellV1<'_>) -> bool {
    match cell {
        FactoryCellV1::Column(c) => matches!(
            c,
            FactoryColumnV1::SnapshotRecipeId
                | FactoryColumnV1::SnapshotConsumeLedger
                | FactoryColumnV1::SnapshotPowerRequired
                | FactoryColumnV1::SnapshotOutputLedger
                | FactoryColumnV1::ObligationPayerAgentId
                | FactoryColumnV1::ObligationProfileKey
                | FactoryColumnV1::ObligationProfileRevision
                | FactoryColumnV1::ObligationElectricityAmount
                | FactoryColumnV1::ObligationMode
        ),
        FactoryCellV1::Collection(c) => !matches!(
            c,
            FactoryCollectionV1::ObligationBalancesBefore
                | FactoryCollectionV1::ObligationBalancesAfter
        ),
        _ => false,
    }
}
pub(super) fn keys<R: ValueNodeReader, C: FnMut() -> bool>(
    key: FactoryRecordKeyV1<'_>,
    cell: FactoryCellV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<Keys, ValueTreeError> {
    valid_cell(key, cell)?;
    let field = key.descriptor()?;
    let mut keys = std::array::from_fn(|_| None);
    keys[0] = Some((
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?,
        0,
    ));
    for (index, cell) in [
        (1, Some(FactoryCellV1::Record)),
        (2, Some(cell)),
        (3, collection(cell).map(FactoryCellV1::Collection)),
        (4, optional_parent(cell).map(FactoryCellV1::Column)),
    ] {
        if let Some(cell) = cell {
            keys[index] = Some(
                with_factory_cell_key!(key, cell, encoded => records.key(field, WorldFieldRecordKindV1::Entry, encoded))?,
            );
        }
    }
    Ok(keys)
}
struct LedgerOutput<'a, W> {
    header: [u8; LEDGER_VALUE_HEADER_BYTES],
    used: usize,
    text: Utf8Output<'a, W>,
}
impl<W: Write> Write for LedgerOutput<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let take = bytes.len().min(self.header.len() - self.used);
        self.header[self.used..self.used + take].copy_from_slice(&bytes[..take]);
        self.used += take;
        // Report consumed header bytes before delegating to a fallible sink.
        // Interrupted then retries only the ID slice, never an eaten header.
        if take != 0 {
            return Ok(take);
        }
        self.text.write(&bytes[take..]).map(|n| n + take)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.text.flush()
    }
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    cell: FactoryCellV1<'_>,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<FactoryValueV1, ValueTreeError> {
    let (codec, nullable) = cell.profile();
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: max_payload_bytes,
        record_bytes: records.record_bytes,
    };
    let mut fixed = [0u8; 8];
    let (info, decoded) = match codec {
        MemberPayloadCodecV1::Text => {
            let mut checked = Utf8Output::new(output, "factory UTF-8");
            let result = read_member_record_v1(
                root,
                &key.0,
                records.values,
                limits,
                &mut io::sink(),
                &mut checked,
                &mut *records.cancelled,
            );
            if checked.is_invalid() {
                return Err(ValueTreeError::Invalid("factory UTF-8"));
            }
            let info = result?;
            if !checked.is_complete() {
                return Err(ValueTreeError::Invalid("factory UTF-8"));
            }
            (
                info,
                FactoryValueV1::Text {
                    utf8_bytes: info.payload_bytes,
                },
            )
        }
        MemberPayloadCodecV1::Ledger => {
            let mut checked = LedgerOutput {
                header: [0; LEDGER_VALUE_HEADER_BYTES],
                used: 0,
                text: Utf8Output::new(output, "factory ledger UTF-8"),
            };
            let result = read_member_record_v1(
                root,
                &key.0,
                records.values,
                limits,
                &mut io::sink(),
                &mut checked,
                &mut *records.cancelled,
            );
            if checked.text.is_invalid() {
                return Err(ValueTreeError::Invalid("factory ledger UTF-8"));
            }
            let info = result?;
            let decoded = if info.codec == MemberPayloadCodecV1::Unit {
                FactoryValueV1::Unit
            } else {
                if !checked.text.is_complete() || checked.used != LEDGER_VALUE_HEADER_BYTES {
                    return Err(ValueTreeError::Invalid("factory ledger frame"));
                }
                FactoryValueV1::Ledger(
                    header_v1(&checked.header, info.payload_bytes)
                        .map_err(ValueTreeError::Invalid)?,
                )
            };
            (info, decoded)
        }
        _ => {
            let mut cursor = io::Cursor::new(fixed.as_mut_slice());
            let info = read_member_record_v1(
                root,
                &key.0,
                records.values,
                MemberRecordLimitsV1 {
                    payload_bytes: max_payload_bytes.min(8),
                    ..limits
                },
                &mut io::sink(),
                &mut cursor,
                &mut *records.cancelled,
            )?;
            let decoded = match info.codec {
                MemberPayloadCodecV1::Unit => FactoryValueV1::Unit,
                MemberPayloadCodecV1::U64 => FactoryValueV1::Unsigned(u64::from_be_bytes(fixed)),
                MemberPayloadCodecV1::I64 => FactoryValueV1::Signed(i64::from_be_bytes(fixed)),
                MemberPayloadCodecV1::Bool => match fixed[0] {
                    0 => FactoryValueV1::Bool(false),
                    1 => FactoryValueV1::Bool(true),
                    _ => return Err(ValueTreeError::Invalid("factory bool")),
                },
                _ => return Err(ValueTreeError::Invalid("factory fixed codec")),
            };
            (info, decoded)
        }
    };
    if info.semantic_bytes != key.1
        || !(info.codec == codec || nullable && info.codec == MemberPayloadCodecV1::Unit)
    {
        return Err(ValueTreeError::Invalid("factory expected profile"));
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if info.codec == MemberPayloadCodecV1::Unit {
        return Ok(FactoryValueV1::Unit);
    }
    if matches!(decoded, FactoryValueV1::Unsigned(v) if v > cell.integer_max()) {
        return Err(ValueTreeError::Invalid("factory integer range"));
    }
    Ok(decoded)
}
pub(super) fn resolved<R: ValueNodeReader, C: FnMut() -> bool>(
    cell: FactoryCellV1<'_>,
    keys: &Keys,
    roots: &Roots,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<FactoryCellReadV1, ValueTreeError> {
    let meta = roots[0].ok_or(ValueTreeError::Invalid("factory missing metadata"))?;
    records.presence(&meta, &keys[0].as_ref().unwrap().0, 0)?;
    let Some(parent) = roots[1] else {
        if roots[2..].iter().any(Option::is_some) {
            return Err(ValueTreeError::Invalid("factory orphan cell"));
        }
        return Ok(FactoryCellReadV1::RecordAbsent);
    };
    value(
        FactoryCellV1::Record,
        &parent,
        keys[1].as_ref().unwrap(),
        records,
        0,
        &mut io::sink(),
    )?;
    let mut absent_parent = false;
    if let Some(key) = &keys[4] {
        let root = roots[4].ok_or(ValueTreeError::Invalid("factory missing optional parent"))?;
        let parent = optional_parent(cell).unwrap();
        match value(
            FactoryCellV1::Column(parent),
            &root,
            key,
            records,
            1,
            &mut io::sink(),
        )? {
            FactoryValueV1::Bool(v) => absent_parent = !v,
            _ => return Err(ValueTreeError::Invalid("factory optional parent type")),
        }
    }
    let mut indexed = false;
    let mut allowed_member = true;
    if let Some(key) = &keys[3] {
        let root = roots[3].ok_or(ValueTreeError::Invalid("factory missing collection"))?;
        let c = collection(cell).unwrap();
        match value(
            FactoryCellV1::Collection(c),
            &root,
            key,
            records,
            8,
            &mut io::sink(),
        )? {
            FactoryValueV1::Unit => allowed_member = false,
            FactoryValueV1::Unsigned(length) => {
                if absent_parent {
                    return Err(ValueTreeError::Invalid(
                        "factory collection under absent parent",
                    ));
                }
                match cell {
                    FactoryCellV1::TextItem(_, index) | FactoryCellV1::StackItem(_, index, _) => {
                        indexed = true;
                        allowed_member = index < length;
                    }
                    FactoryCellV1::Balance(..) => allowed_member = length != 0,
                    _ => unreachable!(),
                }
            }
            _ => return Err(ValueTreeError::Invalid("factory collection count")),
        }
    }
    if roots[2].is_some() && (!allowed_member || absent_parent && keys[3].is_some()) {
        return Err(ValueTreeError::Invalid("factory orphan collection member"));
    }
    let Some(root) = roots[2] else {
        if keys[3].is_none() || indexed && allowed_member {
            return Err(ValueTreeError::Invalid("factory missing required cell"));
        }
        return Ok(FactoryCellReadV1::MemberAbsent);
    };
    let result = value(
        cell,
        &root,
        keys[2].as_ref().unwrap(),
        records,
        max_payload_bytes,
        output,
    )?;
    let required_nested = required_nested(cell);
    if !absent_parent && keys[4].is_some() && required_nested && result == FactoryValueV1::Unit {
        return Err(ValueTreeError::Invalid(
            "factory missing present-parent value",
        ));
    }
    if absent_parent && result != FactoryValueV1::Unit {
        return Err(ValueTreeError::Invalid(
            "factory value under absent optional parent",
        ));
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(FactoryCellReadV1::Value(result))
}
/// Authenticates only this explicit cell and its necessary ancestors. Dynamic
/// outputs stream without collecting sibling records; all output is provisional.
/// This is not a full-record/World qualification or a business authorization.
pub fn read_factory_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: FactoryRecordKeyV1<'_>,
    cell: FactoryCellV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<FactoryCellReadV1, ValueTreeError> {
    context.check()?;
    let _workspace = context.values.memory.reserve(8192, context.values.class)?;
    valid_cell(key, cell)?;
    let outer = container::fixed(key, context)?;
    let Some(child) = outer.child else {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        return Ok(FactoryCellReadV1::RecordAbsent);
    };
    let keys = keys(key, cell, &mut context.records())?;
    let roots = tree::roots(&child, &keys, &mut context.records())?;
    let result = resolved(
        cell,
        &keys,
        &roots,
        &mut context.records(),
        max_payload_bytes,
        output,
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(result)
}
