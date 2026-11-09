use super::*;
pub use crate::incremental_record_value_read::RecordValueV1 as RewardValueV1;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_key_read::{OriginalKeyInfo, OriginalKeyKind, OriginalSemanticKeyProbe},
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
pub(super) fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("reward record schema/binding")
}
pub(super) fn cell_key<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RewardRecordKindV1,
    cell: RewardCellV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    if !cell.accepts(kind) {
        return Err(invalid());
    }
    with_reward_cell_key!(cell,encoded=>records.key(kind.descriptor()?,WorldFieldRecordKindV1::Entry,encoded))
}
pub(super) fn primitive<R: ValueNodeReader, C: FnMut() -> bool>(
    codec: MemberPayloadCodecV1,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<RecordValueV1, ValueTreeError> {
    read_record_value_v1(
        root,
        key,
        records,
        max,
        output,
        RecordReadProfileV1 {
            codec,
            nullable: false,
            integer_max: u64::MAX,
            labels: RecordReadLabelsV1 {
                utf8: "reward UTF8",
                ledger_utf8: "reward ledger UTF8",
                ledger_frame: "reward ledger",
                boolean: "reward bool",
                fixed: "reward fixed",
                profile: "reward profile",
                integer: "reward integer",
            },
        },
    )
}
pub(super) fn outer_key<R: ValueNodeReader, C: FnMut() -> bool>(
    key: RewardRecordKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        key.kind.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &key.index,
    )
}
pub(super) fn format_key<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RewardRecordKindV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        kind.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &(FORMAT, ()),
    )
}
pub(super) fn length_key<R: ValueNodeReader, C: FnMut() -> bool>(
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        RewardRecordKindV1::Mint.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &(FORMAT, "length"),
    )
}
pub(super) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RewardRecordKindV1,
    key: &WorldFieldKeyV1,
    root: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(u64, StateRootV1), ValueTreeError> {
    let _workspace = records.values.memory.reserve(8192, records.values.class)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let mut sink = std::io::sink();
    let mut probe = OriginalSemanticKeyProbe::new(OriginalKeyKind::Unsigned, &mut sink);
    let result = read_member_record_v1(
        root,
        key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: records.record_bytes,
        },
        &mut probe,
        &mut std::io::Cursor::new(bytes.as_mut_slice()),
        &mut *records.cancelled,
    );
    if let Some(reason) = probe.error() {
        return Err(ValueTreeError::Invalid(reason));
    }
    let info = result?;
    let OriginalKeyInfo::Unsigned(index) = probe.finish()? else {
        return Err(invalid());
    };
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let binding = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let child = reference
        .validate_binding(
            &kind.schema(),
            &binding.semantic_key.ok_or_else(invalid)?,
            kind.namespace(),
        )
        .map_err(|_| invalid())?
        .clone();
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok((index, child))
}
/// Integrity-only edge; this does not establish a complete record/family closure.
pub fn inspect_reward_record_root_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: RewardRecordKeyV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<StateRootV1>>, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != key.kind.namespace() {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let meta = WorldFieldKeyV1::new(
        key.kind.descriptor()?,
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let format = format_key(key.kind, &mut ctx.records())?;
    let outer = outer_key(key, &mut ctx.records())?;
    let meta_root = ctx.lookup(&meta)?.ok_or_else(invalid)?;
    let marker = ctx.lookup(&format.0)?.ok_or_else(invalid)?;
    let value = ctx.lookup(&outer.0)?;
    let length = if key.kind == RewardRecordKindV1::Mint {
        let path = length_key(&mut ctx.records())?;
        let root = ctx.lookup(&path.0)?.ok_or_else(invalid)?;
        let RewardValueV1::Unsigned(n) = primitive(
            MemberPayloadCodecV1::U64,
            &root,
            &path,
            &mut ctx.records(),
            8,
            &mut std::io::sink(),
        )?
        else {
            return Err(invalid());
        };
        Some(n)
    } else {
        None
    };
    let mut records = ctx.records();
    records.presence(&meta_root, &meta, 0)?;
    primitive(
        MemberPayloadCodecV1::Unit,
        &marker,
        &format,
        &mut records,
        0,
        &mut std::io::sink(),
    )?;
    let value = value
        .map(|root| self::outer(key.kind, &outer.0, &root, &mut records))
        .transpose()?;
    if let Some((index, _)) = &value
        && *index != key.index
    {
        return Err(invalid());
    }
    if length.is_some_and(|n| (key.index < n) != value.is_some()) {
        return Err(invalid());
    }
    ctx.check()?;
    Ok(value.map(|(_, child)| retained.retain(child)))
}
pub struct RewardCellRequestV1<'a> {
    pub key: RewardRecordKeyV1,
    pub cell: RewardCellV1<'a>,
    pub qualification: &'a ReservedValue<RewardRecordQualificationV1>,
}
pub fn read_qualified_reward_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RewardCellRequestV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<Option<RewardValueV1>, ValueTreeError> {
    if !request.cell.accepts(request.key.kind)
        || !request.qualification.uses_budget(ctx.values.memory)
    {
        return Err(invalid());
    }
    let child = inspect_reward_record_root_v1(request.key, ctx)?.ok_or_else(invalid)?;
    request
        .qualification
        .validate_binding(request.key, &child)?;
    let key = cell_key(request.key.kind, request.cell, &mut ctx.records())?;
    let mut inner = WorldFieldReadContextV1 {
        root: &child,
        state: ctx.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    if let RewardCellV1::Member(c, _) = request.cell {
        let marker = cell_key(
            request.key.kind,
            RewardCellV1::Collection(c),
            &mut inner.records(),
        )?;
        let value = inner.lookup(&marker.0)?.ok_or_else(invalid)?;
        primitive(
            MemberPayloadCodecV1::Unit,
            &value,
            &marker,
            &mut inner.records(),
            0,
            &mut std::io::sink(),
        )?;
    }
    let value = inner.lookup(&key.0)?;
    let result = match value {
        Some(root) => Some(primitive(
            request.cell.codec(),
            &root,
            &key,
            &mut inner.records(),
            max,
            output,
        )?),
        None if matches!(request.cell, RewardCellV1::Member(..)) => None,
        None => return Err(invalid()),
    };
    inner.check()?;
    Ok(result)
}
