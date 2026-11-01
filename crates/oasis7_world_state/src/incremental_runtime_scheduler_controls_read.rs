use super::*;
pub use crate::incremental_record_value_read::RecordValueV1 as RuntimeSchedulerControlValueV1;
use crate::incremental_world_field_key::WorldFieldKeyV1;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, read_record_value_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;
const LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "runtime scheduler cursor UTF-8",
    ledger_utf8: "runtime controls unexpected ledger",
    ledger_frame: "runtime controls unexpected ledger frame",
    boolean: "runtime controls unexpected bool",
    fixed: "runtime controls primitive",
    profile: "runtime controls profile",
    integer: "runtime controls usize range",
};
pub(super) fn outer_key<R: ValueNodeReader, C: FnMut() -> bool>(
    name: &'static str,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(descriptor(name), WorldFieldRecordKindV1::Entry, &())
}
pub(super) fn routing_key<R: ValueNodeReader, C: FnMut() -> bool>(
    column: Option<RuntimeRoutingColumnV1>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    match column {
        None => records.key(descriptor(ROUTING), WorldFieldRecordKindV1::Entry, &()),
        Some(c) => records.key(
            descriptor(ROUTING),
            WorldFieldRecordKindV1::Entry,
            &((), c.name()),
        ),
    }
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: (MemberPayloadCodecV1, bool, u64),
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<RuntimeSchedulerControlValueV1, ValueTreeError> {
    read_record_value_v1(
        root,
        key,
        records,
        max,
        output,
        RecordReadProfileV1 {
            codec: profile.0,
            nullable: profile.1,
            integer_max: profile.2,
            labels: LABELS,
        },
    )
}
pub(super) fn routing_value<R: ValueNodeReader, C: FnMut() -> bool>(
    column: Option<RuntimeRoutingColumnV1>,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<RuntimeSchedulerControlValueV1, ValueTreeError> {
    value(
        column.map_or(
            (MemberPayloadCodecV1::Unit, false, u64::MAX),
            RuntimeRoutingColumnV1::profile,
        ),
        root,
        key,
        records,
        8,
        &mut std::io::sink(),
    )
}
pub(super) fn routing_root<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    let _admission = records.values.memory.reserve(8192, records.values.class)?;
    let mut bytes = [0u8; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
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
        &mut std::io::sink(),
        &mut std::io::Cursor::new(bytes.as_mut_slice()),
        &mut *records.cancelled,
    )?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("runtime routing root frame"))?;
    let binding = WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let child = reference
        .validate_binding(
            &routing_schema(),
            &binding
                .semantic_key
                .ok_or(ValueTreeError::Invalid("runtime routing owner"))?,
            StateNamespaceV1::Scheduler,
        )
        .map_err(|_| ValueTreeError::Invalid("runtime routing root binding"))?
        .clone();
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(child)
}
pub(super) fn scalar_at<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: RuntimeSchedulerScalarV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<RuntimeSchedulerControlValueV1, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != StateNamespaceV1::Scheduler {
        return Err(ValueTreeError::Invalid("runtime controls namespace"));
    }
    let meta = WorldFieldKeyV1::new(
        descriptor(field.name()),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let root = ctx
        .lookup(&meta)?
        .ok_or(ValueTreeError::Invalid("runtime controls metadata missing"))?;
    ctx.records().presence(&root, &meta, 0)?;
    let key = outer_key(field.name(), &mut ctx.records())?;
    let root = ctx
        .lookup(&key.0)?
        .ok_or(ValueTreeError::Invalid("runtime control value missing"))?;
    let decoded = value(
        field.profile(),
        &root,
        &key,
        &mut ctx.records(),
        max,
        output,
    )?;
    ctx.check()?;
    Ok(decoded)
}

pub(super) fn family_binding<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    proof: &ReservedValue<RuntimeSchedulerControlsQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if !proof.uses_budget(ctx.values.memory)
        || roots
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Scheduler)
            != Some(ctx.root)
    {
        return Err(ValueTreeError::Invalid("runtime controls family binding"));
    }
    proof.validate_binding(
        &roots
            .digest()
            .map_err(|_| ValueTreeError::Invalid("runtime controls roots"))?,
    )?;
    Ok(())
}
pub fn read_qualified_runtime_scheduler_scalar_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: RuntimeSchedulerScalarV1,
    roots: &RootSetV1,
    proof: &ReservedValue<RuntimeSchedulerControlsQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl std::io::Write,
) -> Result<RuntimeSchedulerControlValueV1, ValueTreeError> {
    family_binding(roots, proof, ctx)?;
    scalar_at(field, ctx, max_payload_bytes, output)
}
pub(super) fn routing_at<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<StateRootV1, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != StateNamespaceV1::Scheduler {
        return Err(ValueTreeError::Invalid("runtime routing namespace"));
    }
    let meta = WorldFieldKeyV1::new(descriptor(ROUTING), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let value = ctx
        .lookup(&meta)?
        .ok_or(ValueTreeError::Invalid("runtime routing metadata"))?;
    ctx.records().presence(&value, &meta, 0)?;
    let key = outer_key(ROUTING, &mut ctx.records())?;
    let value = ctx
        .lookup(&key.0)?
        .ok_or(ValueTreeError::Invalid("runtime routing edge"))?;
    let root = routing_root(&value, &key, &mut ctx.records())?;
    ctx.check()?;
    Ok(root)
}
pub fn open_qualified_runtime_routing_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    proof: &ReservedValue<RuntimeSchedulerControlsQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<ReservedValue<RuntimeRoutingQualificationV1>, ValueTreeError> {
    family_binding(roots, proof, ctx)?;
    let root = routing_at(ctx)?;
    if &root != proof.routing_root() {
        return Err(ValueTreeError::Invalid("runtime routing qualified root"));
    }
    let token = qualification::mint_routing(root, ctx.values.memory, ctx.values.class)?;
    ctx.check()?;
    Ok(token)
}
pub fn read_qualified_runtime_routing_column_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    column: RuntimeRoutingColumnV1,
    roots: &RootSetV1,
    proof: &ReservedValue<RuntimeSchedulerControlsQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<RuntimeSchedulerControlValueV1, ValueTreeError> {
    family_binding(roots, proof, ctx)?;
    let root = routing_at(ctx)?;
    if &root != proof.routing_root() {
        return Err(ValueTreeError::Invalid("runtime routing qualified root"));
    }
    let key = routing_key(Some(column), &mut ctx.records())?;
    let id =
        crate::incremental_record_tree::lookup(&root, &key.0, &mut ctx.records(), genesis::LABELS)?
            .ok_or(ValueTreeError::Invalid("runtime routing required column"))?;
    let decoded = routing_value(Some(column), &id, &key, &mut ctx.records())?;
    ctx.check()?;
    Ok(decoded)
}
