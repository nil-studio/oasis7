use super::*;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{StateDeltaV1, StatePageWriter, StateWriteV1},
};
use std::cell::RefCell;
fn body<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    owner: SupplyRootKeyV1<'_>,
    source: SupplyRootSourceV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    max_pending: usize,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _operation = context
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut root = StateRootV1::empty(StateNamespaceV1::Authorization)
        .map_err(|_| MemberRecordError::Invalid("supply genesis empty tree"))?;
    profile::visit(source, &mut |record, path, payload| {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        *remaining = remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("supply genesis cells"))?;
        let row = record.write_path(path, payload, context)?;
        let admission = inner_tree::workspace(context.memory, 1)?;
        let delta = {
            let source = inner_tree::Source {
                source: &*context.sink,
                memory: context.memory,
                class: MemoryClass::Ordinary,
                io: RefCell::new(&mut *context.io),
                cancelled: RefCell::new(&mut context.cancelled),
                error: RefCell::new(None),
            };
            // Every original relative path is unique in the closed visitor;
            // previous inner pages are already durable and readable by this sink.
            admission.retain(source.finish(StateDeltaV1::build(
                root.clone(),
                vec![StateWriteV1 {
                    key: row.field_key.as_bytes().to_vec(),
                    before: None,
                    after: Some(row.value_root),
                }],
                &source,
                max_pending,
            ))?)
        };
        inner_tree::persist(&delta, context)?;
        root = delta.after.clone();
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })?;
    write_supply_root_reference_v1(owner, &root, context)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SupplyRootGenesisCountsV1 {
    pub authorizations: u64,
    pub batches: u64,
}
pub fn prepare_supply_root_genesis_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &crate::state::WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    max_pending: usize,
    sink: &mut S,
) -> Result<SupplyRootGenesisCountsV1, MemberRecordError> {
    let mut counts = SupplyRootGenesisCountsV1::default();
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(rules) = &source.world_rules {
        for (key, value) in &rules.supply_authorizations {
            let row = body(
                SupplyRootKeyV1::Authorization(key),
                SupplyRootSourceV1::Authorization(value),
                context,
                remaining,
                max_pending,
            )?;
            *remaining = remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "supply genesis outer cells",
                ))?;
            crate::incremental_field_write::emit(row.field_key, row.value_root, context, sink)?;
            counts.authorizations = counts
                .authorizations
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid("supply authorization count"))?;
        }
        for (key, value) in &rules.supply_batches {
            let row = body(
                SupplyRootKeyV1::Batch(key),
                SupplyRootSourceV1::Batch(value),
                context,
                remaining,
                max_pending,
            )?;
            *remaining = remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "supply genesis outer cells",
                ))?;
            crate::incremental_field_write::emit(row.field_key, row.value_root, context, sink)?;
            counts.batches = counts
                .batches
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid("supply batch count"))?;
        }
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(counts)
}
