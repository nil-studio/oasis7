use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::Cursor;
pub(super) fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("fixed economy record binding/profile")
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedEconomyValueV1 {
    U64(u64),
    I64(i64),
    Bool(bool),
}
impl FixedEconomyValueV1 {
    pub(super) fn payload(
        self,
        column: FixedEconomyColumnV1,
    ) -> Result<MemberPayloadRefV1<'static>, ValueTreeError> {
        match (self, column.codec()) {
            (Self::U64(v), MemberPayloadCodecV1::U64) => Ok(MemberPayloadRefV1::U64(v)),
            (Self::I64(v), MemberPayloadCodecV1::I64) => Ok(MemberPayloadRefV1::I64(v)),
            (Self::Bool(v), MemberPayloadCodecV1::Bool) => Ok(MemberPayloadRefV1::Bool(v)),
            _ => Err(invalid()),
        }
    }
    pub(super) fn from_record(value: RecordValueV1) -> Result<Self, ValueTreeError> {
        match value {
            RecordValueV1::Unsigned(v) => Ok(Self::U64(v)),
            RecordValueV1::Signed(v) => Ok(Self::I64(v)),
            RecordValueV1::Bool(v) => Ok(Self::Bool(v)),
            _ => Err(invalid()),
        }
    }
}
pub(super) fn cell_key<R: ValueNodeReader, C: FnMut() -> bool>(
    field: FixedEconomyFieldV1,
    column: Option<FixedEconomyColumnV1>,
    ctx: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    if column.is_some_and(|column| column.field() != field) {
        return Err(invalid());
    }
    with_cell_key!(column,key=>ctx.key(field.descriptor()?,WorldFieldRecordKindV1::Entry,key))
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    column: Option<FixedEconomyColumnV1>,
    value: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    ctx: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<RecordValueV1, ValueTreeError> {
    read_record_value_v1(
        value,
        key,
        ctx,
        8,
        &mut std::io::sink(),
        RecordReadProfileV1 {
            codec: column.map_or(MemberPayloadCodecV1::Unit, FixedEconomyColumnV1::codec),
            nullable: false,
            integer_max: u64::MAX,
            labels: RecordReadLabelsV1 {
                utf8: "fixed economy UTF8",
                ledger_utf8: "fixed economy ledger UTF8",
                ledger_frame: "fixed economy ledger",
                boolean: "fixed economy bool",
                fixed: "fixed economy fixed",
                profile: "fixed economy profile",
                integer: "fixed economy integer",
            },
        },
    )
}
pub(super) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    field: FixedEconomyFieldV1,
    key: &(WorldFieldKeyV1, u64),
    root: &[u8; 32],
    ctx: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError> {
    let charge = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        root,
        &key.0,
        ctx.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: key.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: key.1,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: ctx.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut *ctx.cancelled,
    )?;
    let binding = WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let child = reference
        .validate_binding(
            &field.schema(),
            &binding.semantic_key.ok_or_else(invalid)?,
            field.namespace(),
        )
        .map_err(|_| invalid())?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(child.clone()))
}
/// Integrity/binding only; complete qualification is a separate operation.
pub fn inspect_fixed_economy_root_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: FixedEconomyFieldV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError> {
    ctx.check()?;
    let metadata =
        WorldFieldKeyV1::new(field.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
    let meta_root = ctx.lookup(&metadata)?.ok_or_else(invalid)?;
    ctx.records().presence(&meta_root, &metadata, 0)?;
    let key = cell_key(field, None, &mut ctx.records())?;
    let root = ctx.lookup(&key.0)?.ok_or_else(invalid)?;
    outer(field, &key, &root, &mut ctx.records())
}

pub struct FixedEconomyCellRequestV1<'a> {
    pub column: FixedEconomyColumnV1,
    pub qualification: &'a ReservedValue<FixedEconomyRecordQualificationV1>,
}
pub fn read_qualified_fixed_economy_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: FixedEconomyCellRequestV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<FixedEconomyValueV1, ValueTreeError> {
    ctx.check()?;
    if !request.qualification.uses_budget(ctx.values.memory) {
        return Err(invalid());
    }
    let field = request.column.field();
    let child = inspect_fixed_economy_root_v1(field, ctx)?;
    request.qualification.validate_binding(field, &child)?;
    let key = cell_key(field, Some(request.column), &mut ctx.records())?;
    let root = crate::incremental_record_tree::lookup(
        &child,
        &key.0,
        &mut ctx.records(),
        genesis::LABELS,
    )?
    .ok_or_else(invalid)?;
    let result = FixedEconomyValueV1::from_record(value(
        Some(request.column),
        &root,
        &key,
        &mut ctx.records(),
    )?)?;
    ctx.check()?;
    Ok(result)
}
