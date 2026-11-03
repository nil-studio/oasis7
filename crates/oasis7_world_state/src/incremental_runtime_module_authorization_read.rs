use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use std::io::{self, Cursor, Write};
pub(super) fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("module authorization schema")
}
pub struct ModuleAuthorizationReadWorkV1 {
    /// Cell visits and external output attempts share this finite allowance.
    pub cells_remaining: u64,
    pub max_payload_bytes: u64,
}
struct BudgetOutput<'a, W> {
    output: &'a mut W,
    remaining: &'a mut u64,
    exhausted: bool,
}
impl<W: Write> Write for BudgetOutput<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(left) = self.remaining.checked_sub(1) else {
            self.exhausted = true;
            return Err(io::Error::other("module authorization output work"));
        };
        *self.remaining = left;
        self.output.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}
impl ModuleAuthorizationReadWorkV1 {
    pub(super) fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "module authorization cells",
                ))?;
        Ok(())
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum ModuleAuthorizationValueV1 {
    Unit,
    Unsigned(u64),
    Text { utf8_bytes: u64 },
    Label(&'static str),
}
const LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "module authorization UTF8",
    ledger_utf8: "module authorization unused ledger UTF8",
    ledger_frame: "module authorization unused ledger",
    boolean: "module authorization Bool",
    fixed: "module authorization fixed",
    profile: "module authorization profile",
    integer: "module authorization U64",
};
pub(super) fn key<R: ValueNodeReader, C: FnMut() -> bool>(
    field: ModuleAuthorizationFieldV1,
    address: ModuleAuthorizationAddressV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        &address.semantic(field),
    )
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    field: ModuleAuthorizationFieldV1,
    address: ModuleAuthorizationAddressV1,
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    output: &mut impl Write,
) -> Result<ModuleAuthorizationValueV1, ValueTreeError> {
    let spec = column(field, address).ok_or_else(invalid)?;
    let profile = RecordReadProfileV1 {
        codec: spec.codec,
        nullable: spec.nullable,
        integer_max: u64::MAX,
        labels: LABELS,
    };
    if !spec.labels.is_empty() {
        let mut bytes = [0; 12];
        let mut out = Cursor::new(bytes.as_mut_slice());
        let result = read_record_value_v1(id, key, records, max.min(12), &mut out, profile)?;
        if result == RecordValueV1::Unit {
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled);
            }
            return Ok(ModuleAuthorizationValueV1::Unit);
        }
        let n = out.position() as usize;
        let label = spec
            .labels
            .iter()
            .copied()
            .find(|label| label.as_bytes() == &bytes[..n])
            .ok_or_else(invalid)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        return Ok(ModuleAuthorizationValueV1::Label(label));
    }
    let result = read_record_value_v1(id, key, records, max, output, profile)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    match result {
        RecordValueV1::Unit => Ok(ModuleAuthorizationValueV1::Unit),
        RecordValueV1::Unsigned(v) => Ok(ModuleAuthorizationValueV1::Unsigned(v)),
        RecordValueV1::Text { utf8_bytes } => Ok(ModuleAuthorizationValueV1::Text { utf8_bytes }),
        _ => Err(invalid()),
    }
}
pub(super) fn cell<M, R, E, C>(
    field: ModuleAuthorizationFieldV1,
    address: ModuleAuthorizationAddressV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ModuleAuthorizationReadWorkV1,
    output: &mut impl Write,
) -> Result<ModuleAuthorizationValueV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    work.cell()?;
    column(field, address).ok_or_else(invalid)?;
    let key = key(field, address, &mut ctx.records())?;
    let id = ctx.lookup(&key.0)?.ok_or_else(invalid)?;
    let max = work.max_payload_bytes;
    let (result, exhausted) = {
        let mut bounded = BudgetOutput {
            output,
            remaining: &mut work.cells_remaining,
            exhausted: false,
        };
        let result = value(
            field,
            address,
            &id,
            &key,
            &mut ctx.records(),
            max,
            &mut bounded,
        );
        (result, bounded.exhausted)
    };
    ctx.check()?;
    if exhausted {
        return Err(ValueTreeError::ResourceLimited(
            "module authorization output work",
        ));
    }
    result
}
/// Root-bound fixed-size body metadata. This is data closure, not a permit.
pub struct ModuleAuthorizationBodyQualificationV1 {
    pub(super) root: StateRootV1,
    pub(super) field: ModuleAuthorizationFieldV1,
    pub(super) class: oasis7_proto::memory_budget::MemoryClass,
}
impl ModuleAuthorizationBodyQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn field(&self) -> ModuleAuthorizationFieldV1 {
        self.field
    }
    pub fn allocation_class(&self) -> oasis7_proto::memory_budget::MemoryClass {
        self.class
    }
}
/// Root-bound field contents only. Output is provisional until whole success.
pub fn read_qualified_module_authorization_cell_v1<M, R, E, C>(
    proof: &oasis7_proto::memory_budget::ReservedValue<ModuleAuthorizationBodyQualificationV1>,
    address: ModuleAuthorizationAddressV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ModuleAuthorizationReadWorkV1,
    output: &mut impl Write,
) -> Result<ModuleAuthorizationValueV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if !proof.uses_budget(ctx.values.memory)
        || proof.reserved_bytes() < 512
        || proof.class != ctx.values.class
        || &proof.root != ctx.root
    {
        return Err(invalid());
    }
    let result = cell(proof.field, address, ctx, work, output)?;
    ctx.check()?;
    Ok(result)
}
