use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_world_rule_parent::read_parent_overlay,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::MemoryClass;
type Keys = [(WorldFieldKeyV1, u64); 4];
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &str,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(Keys, WorldRuleLineageValueV1), WorldFieldOverlayErrorV1> {
    let (_, present) = read_parent_overlay(transaction, records, error)?;
    let mut make = |part| {
        SupplyRecordKeyV1::Lineage(key).overlay_key(
            if matches!(part, Part::Present) {
                None
            } else {
                Some(part.name())
            },
            records,
        )
    };
    let keys = [
        make(Part::Present)?,
        make(Part::Target)?,
        make(Part::Fulfilled)?,
        make(Part::Reserved)?,
    ];
    let roots = [
        lookup(transaction, &keys[0].0, records, error)?,
        lookup(transaction, &keys[1].0, records, error)?,
        lookup(transaction, &keys[2].0, records, error)?,
        lookup(transaction, &keys[3].0, records, error)?,
    ];
    if let Some(value) = shape(present, &roots)? {
        return Ok((keys, value));
    }
    marker(
        &roots[0].unwrap(),
        &keys[0].0,
        keys[0].1,
        records.values,
        records.record_bytes,
        records.cancelled,
    )?;
    let [target, fulfilled, reserved] = [
        records.quantity(&roots[1].unwrap(), &keys[1].0, keys[1].1)?,
        records.quantity(&roots[2].unwrap(), &keys[2].0, keys[2].1)?,
        records.quantity(&roots[3].unwrap(), &keys[3].0, keys[3].1)?,
    ];
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((
        keys,
        WorldRuleLineageValueV1::Value(SupplyLineageV1 {
            target,
            fulfilled,
            reserved,
        }),
    ))
}
pub fn read_world_rule_lineage_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &str,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<WorldRuleLineageValueV1, WorldFieldOverlayErrorV1> {
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        if records.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(records.values.memory)
        {
            return Err(ValueTreeError::Invalid("world rule lineage overlay pool/class").into());
        }
        resolve(key, transaction, &mut records, error).map(|(_, value)| value)
    })
}
#[derive(Debug, Clone, Copy)]
pub enum WorldRuleLineageExpectedV1<'a> {
    RulesAbsent,
    LineageAbsent,
    Value(&'a SupplyLineageV1),
}
#[derive(Debug, Clone, Copy)]
pub struct WorldRuleLineageChangeV1<'a> {
    pub key: &'a str,
    pub expected: WorldRuleLineageExpectedV1<'a>,
    pub after: Option<&'a SupplyLineageV1>,
}
/// Mutates only a complete frozen three-field record. Parent creation and all
/// policy/authority checks remain with the original owner and qualification.
pub fn change_world_rule_lineage_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: WorldRuleLineageChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let WorldRuleLineageChangeV1 {
        key,
        expected,
        after,
    } = change;
    let cancelled = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction|{
        transaction.validate_guard(guard,live)?;
        if context.values.class!=MemoryClass::Ordinary || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory){return Err(ValueTreeError::Invalid("world rule lineage overlay pool/class").into());}
        let (keys,current)={
            let mut cancel=||cancelled() || (write.cancelled)();
            let mut records=InventoryValueReader {values:&mut context.values,key_work:context.key_work,
                semantic_bytes:context.semantic_bytes,record_bytes:context.record_bytes,cancelled:&mut cancel};
            resolve(key,transaction,&mut records,error)?
        };
        let valid=match (&current,expected){
            (WorldRuleLineageValueV1::RulesAbsent,WorldRuleLineageExpectedV1::RulesAbsent)
            | (WorldRuleLineageValueV1::LineageAbsent,WorldRuleLineageExpectedV1::LineageAbsent)=>true,
            (WorldRuleLineageValueV1::Value(current),WorldRuleLineageExpectedV1::Value(expected))=>amounts(current)==amounts(expected),
            _=>false,
        };
        if !valid{return Err(ValueTreeError::Invalid("world rule lineage expected value").into());}
        if matches!(current,WorldRuleLineageValueV1::RulesAbsent) && after.is_some(){
            return Err(ValueTreeError::Invalid("world rule lineage cannot create parent").into());}
        let before=match &current{WorldRuleLineageValueV1::Value(value)=>Some(amounts(value)),_=>None};
        let after=after.map(amounts);
        if before==after{
            if cancelled() || (write.cancelled)(){return Err(ValueTreeError::Cancelled.into());}
            return Ok(false);
        }
        for (index,part) in Part::ALL.into_iter().enumerate(){
            let changed=if index==0{before.is_some()!=after.is_some()}
                else{before.map(|v|v[index-1])!=after.map(|v|v[index-1])};
            if !changed{continue;}
            if cancelled() || (write.cancelled)(){return Err(ValueTreeError::Cancelled.into());}
            let root=if let Some(after)=after{
                let mut combined=MemberRecordWriteContext {sink:&mut *write.sink,memory:write.memory,
                    io:&mut *write.io,key_work:&mut *write.key_work,limits:write.limits,
                    cancelled:||cancelled() || (write.cancelled)()};
                let record=write_part(key,part,if index==0{None}else{Some(after[index-1])},&mut combined)?;
                if record.field_key!=keys[index].0{return Err(ValueTreeError::Invalid("world rule lineage writer key").into());}
                Some(record.value_root)
            }else{None};
            if cancelled() || (write.cancelled)(){return Err(ValueTreeError::Cancelled.into());}
            transaction.write(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,keys[index].0.as_bytes(),root)?;
            if cancelled() || (write.cancelled)(){return Err(ValueTreeError::Cancelled.into());}
        }
        Ok(true)
    })
}
