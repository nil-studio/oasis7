use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionTransaction, RootSetV1},
    incremental_state::StateDeltaV1,
};
pub(super) fn mint(
    field: DiplomacyFieldV1,
    owner: [u8; 32],
    root: &StateRootV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<DiplomacyRecordQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(DiplomacyRecordQualificationV1 {
            field,
            owner,
            root: root.clone(),
        }))
}
pub(super) fn digest(key: &WorldFieldKeyV1) -> Result<[u8; 32], ValueTreeError> {
    WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(closure::invalid)
}
pub fn open_qualified_diplomacy_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    roots: &RootSetV1,
    family: &ReservedValue<DiplomacyFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<DiplomacyRecordQualificationV1>>, ValueTreeError> {
    ctx.check()?;
    if !family.uses_budget(ctx.values.memory) {
        return Err(closure::invalid());
    }
    family.validate_binding(&roots.digest().map_err(|_| closure::invalid())?)?;
    let business = roots
        .state
        .iter()
        .find(|r| r.namespace == StateNamespaceV1::Business)
        .ok_or_else(closure::invalid)?;
    if ctx.root != business {
        return Err(closure::invalid());
    }
    let Some(root) = inspect_diplomacy_root_v1(field, id, ctx)? else {
        ctx.check()?;
        return Ok(None);
    };
    let key = ctx.key(field.descriptor()?, WorldFieldRecordKindV1::Entry, id)?;
    let result = mint(
        field,
        digest(&key.0)?,
        &root,
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(Some(result))
}
pub(super) fn current<
    'p,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    proof: &'p ReservedValue<DiplomacyRecordQualificationV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(WorldFieldKeyV1, &'p StateRootV1), WorldFieldOverlayErrorV1> {
    if records.values.class != MemoryClass::Ordinary
        || !tx.uses_budget(records.values.memory)
        || !proof.uses_budget(records.values.memory)
    {
        return Err(closure::invalid().into());
    }
    metadata(field, tx, records, error)?;
    let key = records.key(field.descriptor()?, WorldFieldRecordKindV1::Entry, id)?;
    let value = lookup(tx, &key.0, records, error)?.ok_or_else(closure::invalid)?;
    let child = root::decode(
        field,
        &key,
        value,
        records.values,
        records.record_bytes,
        records.cancelled,
    )?;
    proof.validate_binding(field, &digest(&key.0)?, &child)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((key.0, proof.root()))
}
pub(super) fn metadata<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let key = WorldFieldKeyV1::new(field.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let value = lookup(tx, &key, records, error)?.ok_or_else(closure::invalid)?;
    records.presence(&value, &key, 0)?;
    Ok(())
}
pub(super) fn publish<
    W: oasis7_formal_protocol::incremental_state::StatePageWriter,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    outer: &WorldFieldKeyV1,
    delta: &ReservedValue<StateDeltaV1>,
    write: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    inner_tree::persist(delta, write)?;
    let row = write_diplomacy_root_reference_v1(field, id, &delta.after, write)?;
    if row.field_key != *outer {
        return Err(closure::invalid().into());
    }
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(row)
}
pub fn read_qualified_diplomacy_scalar_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    field: DiplomacyFieldV1,
    column: DiplomacyScalarV1,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max: u64,
    output: &mut dyn std::io::Write,
) -> Result<Option<RecordValueV1>, ValueTreeError> {
    ctx.check()?;
    if !proof.uses_budget(ctx.values.memory) {
        return Err(closure::invalid());
    }
    let outer = ctx.key(field.descriptor()?, WorldFieldRecordKindV1::Entry, id)?;
    proof.validate_binding(field, &digest(&outer.0)?, ctx.root)?;
    let slot = column.slot(field)?;
    let spec = &profile::specs(field)[slot];
    let key = spec.path.read(field, ctx)?;
    let root = ctx.lookup(&key.0)?;
    if let Some(flag) = spec.flag {
        let flag = profile::specs(field)[flag].path.read(field, ctx)?;
        let value = ctx.lookup(&flag.0)?.ok_or_else(closure::invalid)?;
        let RecordValueV1::Bool(present) = closure::primitive(
            Kind::Bool,
            &value,
            &flag,
            &mut ctx.records(),
            1,
            &mut std::io::sink(),
        )?
        else {
            return Err(closure::invalid());
        };
        if present != root.is_some() {
            return Err(closure::invalid());
        }
        if !present {
            ctx.check()?;
            return Ok(None);
        }
    }
    let value = closure::primitive(
        spec.kind,
        &root.ok_or_else(closure::invalid)?,
        &key,
        &mut ctx.records(),
        max,
        output,
    )?;
    ctx.check()?;
    Ok(Some(value))
}
