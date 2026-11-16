//! Current lifecycle epoch input from the original closed six-column policy.
//! The returned scalar is data, not governance or publication authority.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::ReservedValue;

#[allow(clippy::too_many_arguments)]
pub fn read_current_governance_record_v1<M:MerkleNodeReader,S:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(
    parent:&ReservedValue<RuntimeGovernanceQualificationV1>,roots:&RootSetV1,key:RuntimeGovernanceKeyV1,state:&S,
    context:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,scan:&mut StateScanBudget,max_heap:usize,guard:&PreparedGuard,live:&ExecutionFence,
)->Result<Option<crate::incremental_record_columns_read::RecoveredColumnsV1<RecoveredRuntimeGovernanceRecordV1>>,WorldFieldOverlayErrorV1>{
    context.transaction.contribute(|tx|{
        tx.validate_guard(guard,live)?;parent.validate_binding(roots)?;
        if parent.allocation_class()!=MemoryClass::Ordinary||!parent.uses_budget(context.values.memory)||context.values.class!=MemoryClass::Ordinary||!tx.uses_budget(context.values.memory)||tx.parent_fence().parent_roots!=roots.digest()?{return Err(ValueTreeError::Invalid("Current governance record binding").into())}
        let kind=overlay::kind(key);
        let (outer,value)={let mut records=InventoryValueReader{values:&mut context.values,key_work:&mut*context.key_work,semantic_bytes:context.semantic_bytes,record_bytes:context.record_bytes,cancelled:&mut context.cancelled};
            let metadata=WorldFieldKeyV1::new(kind.descriptor(),WorldFieldRecordKindV1::Metadata,None).map_err(ValueTreeError::Invalid)?;
            let meta=lookup(tx,&metadata,&mut records,&mut context.take_state_error)?.ok_or(ValueTreeError::Invalid("Current governance metadata"))?;records.presence(&meta,&metadata,0)?;
            let outer=overlay::outer(key,&mut records)?;let value=lookup(tx,&outer.0,&mut records,&mut context.take_state_error)?;(outer,value)};
        let Some(value)=value else{return Ok(None)};
        let mut read=WorldFieldReadContextV1{root:roots.state_root(StateNamespaceV1::Authorization),state,values:ValueReadContext{source:context.values.source,memory:context.values.memory,class:context.values.class,io:&mut*context.values.io},key_work:&mut*context.key_work,semantic_bytes:context.semantic_bytes,record_bytes:context.record_bytes,take_state_error:&mut context.take_state_error,cancelled:&mut context.cancelled};
        let (actual,body)=family::child(kind,&outer.0,&value,&mut read)?;
        if actual!=key{return Err(ValueTreeError::Invalid("Current governance original key").into())}
        let value=recover_runtime_governance_record_v1(kind,&body,&mut read,scan,max_heap)?;
        read.check()?;tx.validate_guard(&tx.guard(),live)?;Ok(Some(value))
    })
}

#[allow(clippy::too_many_arguments)]
pub fn read_current_governance_epoch_length_v1<M, S, R, E, C>(
    parent: &ReservedValue<RuntimeGovernanceQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    max_record_heap: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<u64, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    read_current_governance_execution_policy_v1(parent,roots,state,context,scan,max_record_heap,guard,live).map(|policy|policy.epoch_length_ticks)
}

#[allow(clippy::too_many_arguments)]
pub fn read_current_governance_execution_policy_v1<M, S, R, E, C>(
    parent: &ReservedValue<RuntimeGovernanceQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    max_record_heap: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<GovernanceExecutionPolicy, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        context.transaction.validate_guard(guard, live)?;
        let bad = || ValueTreeError::Invalid("Current governance lifecycle epoch");
        if context.values.class != MemoryClass::Ordinary
            || parent.allocation_class() != MemoryClass::Ordinary
            || !parent.uses_budget(context.values.memory)
            || !context.transaction.uses_budget(context.values.memory)
        {
            return Err(bad().into());
        }
        let metadata = roots
            .state
            .iter()
            .try_fold(roots.world_id.len(), |n, root| {
                n.checked_add(root.node_root.len())?.checked_add(256)
            })
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| n.checked_add(32 << 10))
            .ok_or_else(bad)?;
        let _phase = context
            .values
            .memory
            .reserve(metadata, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        parent.validate_binding(roots)?;
        if roots.digest().map_err(|_| bad())? != context.transaction.parent_fence().parent_roots {
            return Err(bad().into());
        }
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        let key = overlay::outer(RuntimeGovernanceKeyV1::Policy, &mut records)?;
        let value = lookup(
            context.transaction,
            &key.0,
            &mut records,
            &mut context.take_state_error,
        )?
        .ok_or_else(bad)?;
        let mut read = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Authorization),
            state,
            values: ValueReadContext {
                source: records.values.source,
                memory: records.values.memory,
                class: records.values.class,
                io: &mut *records.values.io,
            },
            key_work: &mut *records.key_work,
            semantic_bytes: records.semantic_bytes,
            record_bytes: records.record_bytes,
            take_state_error: &mut context.take_state_error,
            cancelled: &mut records.cancelled,
        };
        // Never read the old parent policy after a same-tick governance edit.
        // The original outer codec binds the exact Current child and key.
        let (identity, body) =
            family::child(RuntimeGovernanceKindV1::Policy, &key.0, &value, &mut read)?;
        if identity != RuntimeGovernanceKeyV1::Policy {
            return Err(bad().into());
        }
        let policy = recover_runtime_governance_record_v1(
            RuntimeGovernanceKindV1::Policy,
            &body,
            &mut read,
            scan,
            max_record_heap,
        )?;
        let RecoveredRuntimeGovernanceRecordV1::Policy(policy) = policy.value() else {
            return Err(bad().into());
        };
        let policy = policy.clone();
        read.check()?;
        context
            .transaction
            .validate_guard(&context.transaction.guard(), live)?;
        Ok(policy)
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
