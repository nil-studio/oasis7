//! Exact authenticated edges. Inspection alone is not a closed record proof.
use super::*;
pub use crate::incremental_record_value_read::RecordValueV1 as RuntimeCapabilityValueV1;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_key_read::{OriginalKeyKind, OriginalSemanticKeyProbe},
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, read_record_value_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;
pub(super) fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime capability binding/profile")
}
pub(super) fn cell_key<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RuntimeCapabilityRecordKindV1,
    column: Option<RuntimeCapabilityColumnV1>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    if column.is_some_and(|c| c.kind() != kind) {
        return Err(invalid());
    }
    match column {
        None => records.key(kind.descriptor(), WorldFieldRecordKindV1::Entry, &()),
        Some(c) => records.key(
            kind.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &((), c.name()),
        ),
    }
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    column: Option<RuntimeCapabilityColumnV1>,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<RuntimeCapabilityValueV1, ValueTreeError> {
    read_record_value_v1(
        root,
        key,
        records,
        max,
        output,
        RecordReadProfileV1 {
            codec: column.map_or(MemberPayloadCodecV1::Unit, RuntimeCapabilityColumnV1::codec),
            nullable: column.is_some_and(RuntimeCapabilityColumnV1::nullable),
            integer_max: u64::MAX,
            labels: RecordReadLabelsV1 {
                utf8: "Runtime capability UTF8",
                ledger_utf8: "Runtime capability ledger UTF8",
                ledger_frame: "Runtime capability ledger",
                boolean: "Runtime capability bool",
                fixed: "Runtime capability fixed",
                profile: "Runtime capability profile",
                integer: "Runtime capability integer",
            },
        },
    )
}
pub(super) fn outer_key<R: ValueNodeReader, C: FnMut() -> bool>(
    key: RuntimeCapabilityRecordKeyV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        key.kind.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &key.id,
    )
}
pub(super) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RuntimeCapabilityRecordKindV1,
    key: &(WorldFieldKeyV1, u64),
    root: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let mut sink = std::io::sink();
    let mut probe = OriginalSemanticKeyProbe::new(OriginalKeyKind::Text, &mut sink);
    let result = read_member_record_profile_v1(
        root,
        &key.0,
        records.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: key.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: records.record_bytes,
            },
        },
        &mut probe,
        &mut std::io::Cursor::new(bytes.as_mut_slice()),
        &mut *records.cancelled,
    );
    if let Some(error) = probe.error() {
        return Err(ValueTreeError::Invalid(error));
    }
    result?;
    probe.finish()?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let binding = WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
    Ok(reference
        .validate_binding(
            &kind.schema(),
            &binding.semantic_key.ok_or_else(invalid)?,
            kind.namespace(),
        )
        .map_err(|_| invalid())?
        .clone())
}

pub(super) fn marker<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RuntimeCapabilityRecordKindV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        kind.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(genesis::FORMAT_KEY, ()),
    )
}
pub(super) fn inspect<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: RuntimeCapabilityRecordKeyV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<StateRootV1>, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != key.kind.namespace() {
        return Err(invalid());
    }
    let meta = WorldFieldKeyV1::new(
        key.kind.descriptor(),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let format = marker(key.kind, &mut ctx.records())?;
    let entry = outer_key(key, &mut ctx.records())?;
    let m = ctx.lookup(&meta)?.ok_or_else(invalid)?;
    let f = ctx.lookup(&format.0)?.ok_or_else(invalid)?;
    let root = ctx.lookup(&entry.0)?;
    ctx.records().presence(&m, &meta, 0)?;
    value(
        None,
        &f,
        &format,
        &mut ctx.records(),
        0,
        &mut std::io::sink(),
    )?;
    let child = root
        .map(|id| outer(key.kind, &entry, &id, &mut ctx.records()))
        .transpose()?;
    ctx.check()?;
    Ok(child)
}
fn family_binding<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    family: &ReservedValue<RuntimeCapabilityFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if !family.uses_budget(ctx.values.memory) || !roots.state.iter().any(|r| r == ctx.root) {
        return Err(invalid());
    }
    family.validate_binding(&roots.digest().map_err(|_| invalid())?)
}
pub fn open_qualified_runtime_capability_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: RuntimeCapabilityRecordKeyV1<'_>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    family: &ReservedValue<RuntimeCapabilityFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<RuntimeCapabilityRecordQualificationV1>>, ValueTreeError> {
    family_binding(roots, family, ctx)?;
    let child = inspect(key, ctx)?;
    let owner = WorldFieldKeyV1::from_bytes(outer_key(key, &mut ctx.records())?.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let token = child
        .map(|child| {
            closure::mint(
                key.kind,
                owner,
                child,
                roots.digest().map_err(|_| invalid())?,
                ctx.values.memory,
                ctx.values.class,
            )
        })
        .transpose()?;
    ctx.check()?;
    Ok(token)
}
pub struct RuntimeCapabilityCellRequestV1<'a> {
    pub key: RuntimeCapabilityRecordKeyV1<'a>,
    pub column: RuntimeCapabilityColumnV1,
    pub qualification: &'a ReservedValue<RuntimeCapabilityRecordQualificationV1>,
}
pub fn read_qualified_runtime_capability_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RuntimeCapabilityCellRequestV1<'_>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<RuntimeCapabilityValueV1, ValueTreeError> {
    ctx.check()?;
    if request.column.kind() != request.key.kind
        || !request.qualification.uses_budget(ctx.values.memory)
        || !roots.state.iter().any(|r| r == ctx.root)
    {
        return Err(invalid());
    }
    let child = inspect(request.key, ctx)?.ok_or_else(invalid)?;
    let owner =
        WorldFieldKeyV1::from_bytes(outer_key(request.key, &mut ctx.records())?.0.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
    request.qualification.validate_binding(
        request.key.kind,
        &owner,
        &child,
        &roots.digest().map_err(|_| invalid())?,
    )?;
    let key = cell_key(request.key.kind, Some(request.column), &mut ctx.records())?;
    let id = crate::incremental_record_tree::lookup(
        &child,
        &key.0,
        &mut ctx.records(),
        genesis::LABELS,
    )?
    .ok_or_else(invalid)?;
    let decoded = value(
        Some(request.column),
        &id,
        &key,
        &mut ctx.records(),
        max,
        output,
    )?;
    ctx.check()?;
    Ok(decoded)
}
